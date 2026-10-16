//! Host-configured, fixed-recipe static derivation. The application owns resource
//! authority; this adapter never receives project content or caller URLs/axes.
use opencut_contracts::EditorError;
use opencut_editor_application::{
    font_derived_v1::{DerivedFontSourceV1, PinnedDerivedFontV1},
    font_import_v1::{FontImportCheckpointV1, catalog::FontFetchPolicyV1},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
const MAX_BYTES: u64 = 32 * 1024 * 1024;
const WRAPPER: &[u8] = include_bytes!("../../../../scripts/font-catalog/run_derived.py");
const DERIVE: &[u8] = include_bytes!("../../../../scripts/font-catalog/derive_static.py");
const GENERATE: &[u8] = include_bytes!("../../../../scripts/font-catalog/generate.py");
const RUNTIME_LOCK: &[u8] =
    include_bytes!("../../../../scripts/font-catalog/derived-runtime-lock.json");
const TOOL_LOCK: &[u8] = include_bytes!("../../../../scripts/font-catalog/fonttools-lock.json");
fn err(code: &str, message: &str) -> EditorError {
    EditorError::new(code, message)
}
fn integrity() -> EditorError {
    err(
        "FONT_SOURCE_INTEGRITY_ERROR",
        "Derived font bytes or proof differ from the fixed recipe",
    )
}
fn unavailable() -> EditorError {
    err(
        "FONT_DERIVATION_UNAVAILABLE",
        "Pinned font derivation runtime is unavailable",
    )
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn check(
    deadline: Instant,
    checkpoint: &mut dyn FontImportCheckpointV1,
) -> Result<(), EditorError> {
    checkpoint.check()?;
    if Instant::now() >= deadline {
        return Err(err("FONT_SOURCE_TIMEOUT", "Derived font deadline exceeded"));
    }
    Ok(())
}
fn bounded(path: &Path, size: u64, hash: &str) -> Result<Vec<u8>, EditorError> {
    bounded_with_limit(path, size, hash, MAX_BYTES)
}
fn bounded_with_limit(
    path: &Path,
    size: u64,
    hash: &str,
    limit: u64,
) -> Result<Vec<u8>, EditorError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| unavailable())?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != size
        || size > limit
    {
        return Err(integrity());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| unavailable())?
        .take(size + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unavailable())?;
    if bytes.len() as u64 != size || digest(&bytes) != hash {
        return Err(integrity());
    }
    Ok(bytes)
}
pub struct NativeConfiguredDerivedFontSourceV1 {
    python: PathBuf,
    script: PathBuf,
    cache: Option<PathBuf>,
    renamed_v2: bool,
}
impl NativeConfiguredDerivedFontSourceV1 {
    pub fn new(python: PathBuf, script: PathBuf, cache: Option<PathBuf>) -> Self {
        Self {
            python,
            script,
            cache,
            renamed_v2: false,
        }
    }
    /// New wrapper selects independently embedded V2 bytes. The public V1
    /// constructor, V1 files and V1 runtime identity remain unchanged.
    pub(crate) fn new_renamed_v2(python: PathBuf, script: PathBuf, cache: Option<PathBuf>) -> Self {
        Self {
            python,
            script,
            cache,
            renamed_v2: true,
        }
    }
    /// Read-only installation admission; the child independently verifies every
    /// fontTools Python file/version and Python version against the recipe lock.
    pub fn availability(&self) -> Result<(), EditorError> {
        if !self.python.is_file() {
            return Err(unavailable());
        }
        let parent = self.script.parent().ok_or_else(unavailable)?;
        let mut files = vec![
            (self.script.clone(), WRAPPER),
            (parent.join("derive_static.py"), DERIVE),
            (parent.join("generate.py"), GENERATE),
            (parent.join("fonttools-lock.json"), TOOL_LOCK),
        ];
        if self.renamed_v2 {
            files[0].1 = crate::font_derived_v2::WRAPPER;
            files.push((
                parent.join("derive_renamed_v2.py"),
                crate::font_derived_v2::DERIVE,
            ));
        }
        for (path, expected) in files {
            bounded(&path, expected.len() as u64, &digest(expected)).map_err(|_| unavailable())?;
        }
        let mut child = Command::new(&self.python)
            .arg("-I")
            .arg("-B")
            .arg(&self.script)
            .arg("--identity")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| unavailable())?;
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(unavailable());
            }
            match child.try_wait() {
                Ok(Some(status)) if status.success() => break,
                Ok(Some(_)) | Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(unavailable());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            }
        }
        let mut bytes = Vec::new();
        child
            .stdout
            .take()
            .ok_or_else(unavailable)?
            .take(8193)
            .read_to_end(&mut bytes)
            .map_err(|_| unavailable())?;
        let observed: Value = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
        let expected: Value = serde_json::from_slice(if self.renamed_v2 {
            crate::font_derived_v2::RUNTIME_LOCK
        } else {
            RUNTIME_LOCK
        })
        .map_err(|_| unavailable())?;
        if bytes.len() > 8192 || observed != expected {
            return Err(unavailable());
        }
        Ok(())
    }
    pub fn cached(&self, source: &PinnedDerivedFontV1) -> bool {
        source
            .source
            .face
            .size_bytes
            .parse::<u64>()
            .ok()
            .is_some_and(|size| {
                self.cache.as_ref().is_some_and(|cache| {
                    bounded(
                        &cache.join("derived").join(&source.source.face.content_hash),
                        size,
                        &source.source.face.content_hash,
                    )
                    .is_ok()
                })
            })
    }
    /// Bounded catalog availability hint only. Import still hashes every byte;
    /// listing faces must not reread an entire derived-font inventory.
    pub fn cached_candidate(&self, source: &PinnedDerivedFontV1) -> bool {
        let Ok(size) = source.source.face.size_bytes.parse::<u64>() else {
            return false;
        };
        size > 0
            && size <= MAX_BYTES
            && self.cache.as_ref().is_some_and(|cache| {
                fs::symlink_metadata(cache.join("derived").join(&source.source.face.content_hash))
                    .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink() && m.len() == size)
            })
    }
    fn acquire(
        &self,
        url: &str,
        path: &str,
        size: u64,
        hash: &str,
        source_limit: u64,
        policy: &FontFetchPolicyV1,
        deadline: Instant,
        checkpoint: &mut dyn FontImportCheckpointV1,
    ) -> Result<Vec<u8>, EditorError> {
        check(deadline, checkpoint)?;
        if size > source_limit {
            return Err(err(
                "FONT_BUDGET_EXCEEDED",
                "Derived source exceeds host font envelope",
            ));
        }
        let relative = Path::new(path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(integrity());
        }
        let temporary_cache = tempfile::tempdir().map_err(|_| unavailable())?;
        let cached = self
            .cache
            .as_ref()
            .map(|cache| cache.join(relative))
            .unwrap_or_else(|| temporary_cache.path().join(relative));
        if cached.exists() {
            let bytes = bounded_with_limit(&cached, size, hash, source_limit)?;
            check(deadline, checkpoint)?;
            return Ok(bytes);
        }
        if !policy.allow_public_font_download {
            return Err(err(
                "FONT_NETWORK_DENIED",
                "Public font downloads are disabled by this host",
            ));
        }
        let duration = deadline.saturating_duration_since(Instant::now());
        let proxy = crate::font_import_v1::fetch::source_proxy(url, |key| std::env::var(key).ok())?;
        let agent = ureq::Agent::config_builder()
            .proxy(proxy)
            .https_only(true)
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(duration))
            .timeout_resolve(Some(duration.min(Duration::from_secs(5))))
            .timeout_connect(Some(duration.min(Duration::from_secs(5))))
            .build()
            .new_agent();
        let response = agent.get(url).header("Accept-Encoding", "identity").call();
        check(deadline, checkpoint)?;
        let mut response = response.map_err(|_| {
            err(
                "FONT_SOURCE_UNAVAILABLE",
                "Pinned variable font source unavailable",
            )
        })?;
        if response.status().as_u16() != 200
            || response
                .headers()
                .get("content-encoding")
                .is_some_and(|h| h.as_bytes() != b"identity")
        {
            return Err(integrity());
        }
        let reader = response.body_mut().with_config().limit(size + 1).reader();
        let bytes = crate::font_import_v1::fetch::read_pinned_bytes(
            reader, size, hash, deadline, checkpoint,
        )?;
        // Cache publication is atomic; incomplete reads never become reusable bytes.
        let parent = cached.parent().ok_or_else(integrity)?;
        fs::create_dir_all(parent).map_err(|_| unavailable())?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|_| unavailable())?;
        std::io::Write::write_all(&mut temporary, &bytes).map_err(|_| unavailable())?;
        temporary.persist(&cached).map_err(|_| unavailable())?;
        Ok(bytes)
    }
}
impl DerivedFontSourceV1 for NativeConfiguredDerivedFontSourceV1 {
    fn obtain(
        &self,
        source: &PinnedDerivedFontV1,
        policy: &FontFetchPolicyV1,
        checkpoint: &mut dyn FontImportCheckpointV1,
    ) -> Result<Vec<u8>, EditorError> {
        if self.renamed_v2 {
            opencut_editor_application::font_derived_v2::validate_pinned_source_v2(source)?;
        } else {
            opencut_editor_application::font_derived_v1::validate_pinned_source_v1(source)?;
        }
        let deadline = Instant::now() + Duration::from_millis(policy.deadline_ms);
        check(deadline, checkpoint)?;
        let descriptor = &source.source;
        let output_size = descriptor
            .face
            .size_bytes
            .parse::<u64>()
            .map_err(|_| integrity())?;
        if output_size > policy.max_bytes || output_size > MAX_BYTES {
            return Err(err(
                "FONT_BUDGET_EXCEEDED",
                "Derived output exceeds host font envelope",
            ));
        }
        let temporary_cache = tempfile::tempdir().map_err(|_| unavailable())?;
        let output_cache = self
            .cache
            .as_ref()
            .map(|cache| cache.join("derived").join(&descriptor.face.content_hash))
            .unwrap_or_else(|| temporary_cache.path().join("output"));
        if output_cache.exists() {
            let bytes = bounded(&output_cache, output_size, &descriptor.face.content_hash)?;
            check(deadline, checkpoint)?;
            return Ok(bytes);
        }
        self.availability()?;
        let proof: Value = serde_json::from_str(&descriptor.proof_json).map_err(|_| integrity())?;
        let spec = &proof["source"];
        let string = |name: &str| spec[name].as_str().ok_or_else(integrity);
        let size = spec["source_size"].as_u64().ok_or_else(integrity)?;
        let original_path = string("source_path")?;
        let repository = spec["source_repository"].as_str().unwrap_or("google/fonts");
        let historical = self.renamed_v2
            && descriptor.source_commit
                != opencut_editor_application::font_import_v1::catalog::GOOGLE_FONTS_COMMIT_V1;
        let acquisition_path = if self.renamed_v2 && repository == "google/material-design-icons" {
            format!(
                "official/google-material-design-icons/{}/{}",
                descriptor.source_commit, original_path
            )
        } else if historical {
            format!("fixed/{}/{}", descriptor.source_commit, original_path)
        } else {
            original_path.into()
        };
        let source_limit = policy.max_bytes.min(if self.renamed_v2 {
            64 * 1024 * 1024
        } else {
            MAX_BYTES
        });
        // Historical Google inputs may already occupy their previously absent
        // original path in the authorized cache. Reuse only exact verified
        // bytes; a different revision never overwrites that existing object.
        let historical_cached = if historical && repository == "google/fonts" {
            self.cache.as_ref().and_then(|cache| {
                bounded_with_limit(
                    &cache.join(original_path),
                    size,
                    &descriptor.source_sha256,
                    source_limit,
                )
                .ok()
            })
        } else {
            None
        };
        let bytes = if let Some(bytes) = historical_cached {
            bytes
        } else {
            self.acquire(
                &descriptor.source_url,
                &acquisition_path,
                size,
                &descriptor.source_sha256,
                source_limit,
                policy,
                deadline,
                checkpoint,
            )?
        };
        // Full original notice is already a pinned part of this internal catalog.
        let notice = descriptor.license_notice.as_bytes();
        if notice.len() > 65536 || digest(notice) != string("license_sha256")? {
            return Err(integrity());
        }
        let dir = tempfile::tempdir().map_err(|_| unavailable())?;
        let source_file = dir.path().join("source.ttf");
        let license_file = dir.path().join("license.txt");
        let request_file = dir.path().join("request.json");
        let output = dir.path().join("output.ttf");
        fs::write(&source_file, &bytes).map_err(|_| unavailable())?;
        fs::write(&license_file, notice).map_err(|_| unavailable())?;
        let request = json!({"source_file":source_file,"license_file":license_file,"proof":proof});
        fs::write(
            &request_file,
            serde_json::to_vec(&request).map_err(|_| integrity())?,
        )
        .map_err(|_| unavailable())?;
        check(deadline, checkpoint)?;
        let mut child = Command::new(&self.python)
            .arg("-I")
            .arg("-B")
            .arg(&self.script)
            .arg(&request_file)
            .arg(&output)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| unavailable())?;
        let result = (|| {
            loop {
                check(deadline, checkpoint)?;
                if let Some(status) = child.try_wait().map_err(|_| unavailable())? {
                    if !status.success() {
                        let mut budget = status.code() == Some(74);
                        #[cfg(unix)]
                        {
                            use std::os::unix::process::ExitStatusExt;
                            budget |= status.signal().is_some_and(|signal| {
                                signal == libc::SIGXCPU
                                    || signal == libc::SIGXFSZ
                                    || signal == libc::SIGKILL
                            });
                        }
                        return Err(err(
                            if budget {
                                "FONT_BUDGET_EXCEEDED"
                            } else {
                                "FONT_DERIVATION_UNAVAILABLE"
                            },
                            "Pinned font instancing refused its bounded execution or fixed proof",
                        ));
                    }
                    break;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            check(deadline, checkpoint)?;
            bounded(&output, output_size, &descriptor.face.content_hash)
        })();
        if result.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let bytes = result?;
        check(deadline, checkpoint)?;
        let parent = output_cache.parent().ok_or_else(unavailable)?;
        fs::create_dir_all(parent).map_err(|_| unavailable())?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|_| unavailable())?;
        std::io::Write::write_all(&mut temporary, &bytes).map_err(|_| unavailable())?;
        temporary
            .persist(&output_cache)
            .map_err(|_| unavailable())?;
        check(deadline, checkpoint)?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_cache_requires_exact_size_and_hash() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("font");
        fs::write(&path, b"fixed font bytes").unwrap();
        let hash = digest(b"fixed font bytes");
        assert_eq!(bounded(&path, 16, &hash).unwrap(), b"fixed font bytes");
        assert!(bounded(&path, 15, &hash).is_err());
        fs::write(&path, b"other font bytes").unwrap();
        assert!(bounded(&path, 16, &hash).is_err());
    }

    #[test]
    fn absent_runtime_and_linked_cache_fail_closed() {
        let directory = tempfile::tempdir().unwrap();
        let provider = NativeConfiguredDerivedFontSourceV1::new(
            directory.path().join("missing-python"),
            directory.path().join("missing-script"),
            None,
        );
        assert!(provider.availability().is_err());
        #[cfg(unix)]
        {
            let source = directory.path().join("source");
            let link = directory.path().join("link");
            fs::write(&source, b"font").unwrap();
            std::os::unix::fs::symlink(&source, &link).unwrap();
            assert!(bounded(&link, 4, &digest(b"font")).is_err());
        }
    }
    #[test]
    #[ignore = "requires fixed derived output cache in OPENCUT_FONT_DERIVED_CACHE; never fetches or derives"]
    fn every_pinned_derived_face_matches_native_inspector() {
        use opencut_editor_application::font_import_v1::{FontImportPolicyV1, FontInspectorV1};
        let root = PathBuf::from(
            std::env::var_os("OPENCUT_FONT_DERIVED_CACHE").expect("explicit derived fixture cache"),
        );
        let sources = opencut_editor_application::font_derived_v1::catalog().unwrap();
        assert!(!sources.is_empty(), "derived inventory must not be vacuous");
        let mut failures = Vec::new();
        for source in &sources {
            let face = &source.source.face;
            let size = face.size_bytes.parse::<u64>().unwrap();
            let bytes = match bounded(
                &root.join("derived").join(&face.content_hash),
                size,
                &face.content_hash,
            ) {
                Ok(bytes) => bytes,
                Err(error) => {
                    failures.push(format!("{}: {error:?}", face.id));
                    continue;
                }
            };
            // Inventory proves static metadata only. Actual import/render text
            // still receives its own cmap coverage inspection.
            match crate::font_import_v1::StaticFontInspectorV1.inspect(
                &bytes,
                "",
                &FontImportPolicyV1::default(),
                &mut || Ok(()),
            ) {
                Ok(parsed)
                    if parsed.family == face.family
                        && parsed.weight == face.weight
                        && parsed.style == face.style => {}
                Ok(parsed) => failures.push(format!("{}: parsed identity {parsed:?}", face.id)),
                Err(error) => failures.push(format!("{}: {error:?}", face.id)),
            }
        }
        if let Some(path) = std::env::var_os("OPENCUT_FONT_DERIVED_NATIVE_REPORT") {
            let report = json!({"catalog_sha256":digest(include_bytes!("../../../editor-application/src/font_derived_v1/catalog_pins.json")),
                "inspector_source_sha256":digest(include_bytes!("../font_import_v1/mod.rs")),
                "verified_current_pinned_faces":sources.len()-failures.len(),"current_failures":failures});
            fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        }
        assert!(
            failures.is_empty(),
            "{} of {} derived fonts failed native inspection:\n{}",
            failures.len(),
            sources.len(),
            failures.join("\n")
        );
    }
}
