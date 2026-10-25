//! Acquire the unmodified normal source for an explicit browser synthesis binding.
//! Static routes never invoke Python. Derived routes obtain the original
//! verified output through injected adapters; synthesis never changes bytes.
use opencut_contracts::{EditorError, font_synthesis::FontSynthesisSource};
use opencut_editor_application::{
    font_derived::{DerivedFontSource, PinnedDerivedFont},
    font_import::{
        FontImportCheckpoint, MAX_FONT_BYTES,
        catalog::{FontFetchPolicy, PinnedFontSource, validate_font_fetch},
    },
    font_synthesis::{self as application, SynthesisBaseSource},
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub struct NativeSynthesisFontSource {
    cache: Option<PathBuf>,
}
impl NativeSynthesisFontSource {
    pub fn new(cache: Option<PathBuf>) -> Self {
        Self { cache }
    }
}
fn integrity() -> EditorError {
    EditorError::new(
        "FONT_SOURCE_INTEGRITY_ERROR",
        "Synthesis source cache differs from the pinned actual face",
    )
}
fn unavailable() -> EditorError {
    EditorError::new(
        "FONT_SOURCE_UNAVAILABLE",
        "Pinned synthesis source cache is unavailable",
    )
}
fn cache_path(actual: &PinnedFontSource, cache: &Path) -> Result<PathBuf, EditorError> {
    let google = format!(
        "https://raw.githubusercontent.com/google/fonts/{}/",
        actual.source_commit
    );
    let official = format!(
        "https://raw.githubusercontent.com/google/material-design-icons/{}/",
        actual.source_commit
    );
    let (relative, prefix) = if let Some(relative) = actual.source_url.strip_prefix(&google) {
        let prefix = if actual.source_commit
            == opencut_editor_application::font_import::catalog::GOOGLE_FONTS_COMMIT
        {
            cache.to_path_buf()
        } else {
            cache.join("fixed").join(actual.source_commit)
        };
        (relative, prefix)
    } else if let Some(relative) = actual.source_url.strip_prefix(&official) {
        (
            relative,
            cache
                .join("official/google-material-design-icons")
                .join(actual.source_commit),
        )
    } else {
        return Err(integrity());
    };
    // Only compiled plain repository-relative filenames; no locator or URL
    // from the public opaque proof is used to form a filesystem path.
    if !relative
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"/-_.".contains(&b))
        || relative
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(integrity());
    }
    Ok(prefix.join(relative))
}
fn original_historical_cache_path(
    actual: &PinnedFontSource,
    cache: &Path,
) -> Result<Option<PathBuf>, EditorError> {
    let primary = cache_path(actual, cache)?;
    let prefix = cache.join("fixed").join(actual.source_commit);
    Ok(primary
        .strip_prefix(prefix)
        .ok()
        .map(|relative| cache.join(relative)))
}
fn static_candidate(actual: &PinnedFontSource, cache: &Path) -> bool {
    let Ok(primary) = cache_path(actual, cache) else {
        return false;
    };
    match fs::symlink_metadata(&primary) {
        Ok(_) => return candidate(&primary, actual.size_bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return false,
    }
    // The native reader only consults the old location when the primary file
    // is absent. A known malformed primary must not be hidden by another hint.
    original_historical_cache_path(actual, cache)
        .ok()
        .flatten()
        .is_some_and(|path| candidate(&path, actual.size_bytes))
}
fn candidate(path: &Path, size: u64) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() == size
    })
}
fn read_cached(
    path: &Path,
    size: u64,
    hash: &str,
    deadline: Instant,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<Option<Vec<u8>>, EditorError> {
    checkpoint.check()?;
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(unavailable()),
        Ok(metadata)
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() != size =>
        {
            return Err(integrity());
        }
        Ok(_) => {}
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
    }
    let file = options.open(path).map_err(|_| unavailable())?;
    let metadata = file.metadata().map_err(|_| unavailable())?;
    if !metadata.is_file() || metadata.len() != size {
        return Err(integrity());
    }
    crate::font_import::fetch::read_pinned_bytes(file, size, hash, deadline, checkpoint).map(Some)
}
/// Catalog availability is only a cheap local candidate hint. Import reopens
/// the file safely, checks cancellation and hashes every byte before publication.
pub fn cached_candidate(source: &FontSynthesisSource, cache: Option<&Path>) -> bool {
    let Some(cache) = cache else { return false };
    let Ok(expected) = application::source(&source.source.face.id) else {
        return false;
    };
    if source != &expected {
        return false;
    }
    let Ok(base) = application::base_source(&source.source.face.id) else {
        return false;
    };
    match base {
        SynthesisBaseSource::RepositoryStatic(actual) => static_candidate(actual, cache),
        SynthesisBaseSource::ApprovedStatic(actual) => static_candidate(&actual.source, cache),
        SynthesisBaseSource::Outline(pin)
        | SynthesisBaseSource::RenamedOutline(pin)
        | SynthesisBaseSource::CompleteColor(pin) => {
            pin.source.face.size_bytes.parse().ok().is_some_and(|size| {
                candidate(
                    &cache.join("derived").join(&pin.source.face.content_hash),
                    size,
                )
            })
        }
    }
}
fn base_input_cache_paths(
    source: &FontSynthesisSource,
    cache: &Path,
) -> Result<Option<(Vec<PathBuf>, u64)>, EditorError> {
    if application::source(&source.source.face.id)? != *source {
        return Err(integrity());
    }
    let (pin, v2) = match application::base_source(&source.source.face.id)? {
        SynthesisBaseSource::Outline(pin) => (pin, false),
        SynthesisBaseSource::RenamedOutline(pin) => (pin, true),
        SynthesisBaseSource::CompleteColor(pin) => (pin, true),
        _ => return Ok(None),
    };
    // Read only the original compiled recipe, never the synthesis envelope's
    // output size or a caller-provided locator. These are the paths used by
    // NativeConfiguredOutlineFontSource::obtain (including its V2 wrapper).
    let descriptor = &pin.source;
    let proof: serde_json::Value =
        serde_json::from_str(&descriptor.proof_json).map_err(|_| integrity())?;
    let recipe = &proof["source"];
    let size = recipe["source_size"].as_u64().ok_or_else(integrity)?;
    let path = recipe["source_path"].as_str().ok_or_else(integrity)?;
    if size == 0
        || size > if v2 { 64 * 1024 * 1024 } else { MAX_FONT_BYTES }
        || descriptor.source_size_bytes != size.to_string()
        || path.contains('\\')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !Path::new(path)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
    {
        return Err(integrity());
    }
    let repository = recipe["source_repository"]
        .as_str()
        .unwrap_or("google/fonts");
    let historical = v2
        && descriptor.source_commit
            != opencut_editor_application::font_import::catalog::GOOGLE_FONTS_COMMIT;
    let mut paths = Vec::with_capacity(2);
    match repository {
        "google/material-design-icons" if v2 => paths.push(
            cache
                .join("official/google-material-design-icons")
                .join(&descriptor.source_commit)
                .join(path),
        ),
        "google/fonts" if historical => {
            paths.push(cache.join(path));
            paths.push(
                cache
                    .join("fixed")
                    .join(&descriptor.source_commit)
                    .join(path),
            );
        }
        "google/fonts" => paths.push(cache.join(path)),
        _ => return Err(integrity()),
    }
    Ok(Some((paths, size)))
}
/// A raw source cache is only an input hint, never a completed font. Hosts must
/// separately admit the selected derivation runtime; import still verifies the
/// original source hash, runs the pinned recipe and verifies its output hash.
pub fn base_input_cached_candidate(source: &FontSynthesisSource, cache: Option<&Path>) -> bool {
    let Some(cache) = cache else { return false };
    base_input_cache_paths(source, cache)
        .ok()
        .flatten()
        .is_some_and(|(paths, size)| paths.iter().any(|path| candidate(path, size)))
}
/// Host assembly supplies lazy original derivation ports. Exact output-cache
/// hits are verified here before either port is called, so no Python discovery
/// occurs for cached outputs or any static source.
pub struct RoutedSynthesisFontSource<'a> {
    cache: Option<PathBuf>,
    outline_source: Option<&'a dyn DerivedFontSource>,
    named_source: Option<&'a dyn DerivedFontSource>,
    color_source: Option<&'a dyn DerivedFontSource>,
}
impl<'a> RoutedSynthesisFontSource<'a> {
    pub fn new(
        cache: Option<PathBuf>,
        outline_source: Option<&'a dyn DerivedFontSource>,
        named_source: Option<&'a dyn DerivedFontSource>,
    ) -> Self {
        Self {
            cache,
            outline_source,
            named_source,
            color_source: None,
        }
    }
    pub fn with_color_static(mut self, port: &'a dyn DerivedFontSource) -> Self {
        self.color_source = Some(port);
        self
    }
}
impl DerivedFontSource for RoutedSynthesisFontSource<'_> {
    fn obtain(
        &self,
        source: &PinnedDerivedFont,
        policy: &FontFetchPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        application::validate_pinned_source(source)?;
        let limits = application::acquisition_limits(&source.source.face.id)?;
        if !(1..=limits.max_source_bytes).contains(&policy.max_bytes)
            || !(1..=limits.max_deadline_ms).contains(&policy.deadline_ms)
        {
            return Err(EditorError::new(
                "FONT_SYNTHESIS_INVALID",
                "Synthesis acquisition policy exceeds its selected route",
            ));
        }
        let output_size = source
            .source
            .face
            .size_bytes
            .parse::<u64>()
            .map_err(|_| integrity())?;
        if output_size > limits.max_output_bytes || output_size > policy.max_bytes {
            return Err(EditorError::new(
                "FONT_BUDGET_EXCEEDED",
                "Base output exceeds the synthesis envelope",
            ));
        }
        let base = application::base_source(&source.source.face.id)?;
        let (pin, port) = match base {
            SynthesisBaseSource::RepositoryStatic(_) | SynthesisBaseSource::ApprovedStatic(_) => {
                return NativeSynthesisFontSource::new(self.cache.clone())
                    .obtain(source, policy, checkpoint);
            }
            SynthesisBaseSource::Outline(pin) => (pin, self.outline_source),
            SynthesisBaseSource::RenamedOutline(pin) => (pin, self.named_source),
            SynthesisBaseSource::CompleteColor(pin) => (pin, self.color_source),
        };
        let deadline = Instant::now() + Duration::from_millis(policy.deadline_ms);
        if let Some(cache) = &self.cache {
            if let Some(bytes) = read_cached(
                &cache.join("derived").join(&pin.source.face.content_hash),
                output_size,
                &pin.source.face.content_hash,
                deadline,
                checkpoint,
            )? {
                return Ok(bytes);
            }
        }
        let port = port.ok_or_else(|| {
            EditorError::new(
                "FONT_DERIVATION_UNAVAILABLE",
                "The selected original base derivation port is not configured",
            )
        })?;
        let mut bounded = || {
            checkpoint.check()?;
            if Instant::now() >= deadline {
                return Err(EditorError::new(
                    "FONT_SOURCE_TIMEOUT",
                    "Synthesis base acquisition exceeded its deadline",
                ));
            }
            Ok(())
        };
        bounded.check()?;
        let bytes = port.obtain(&pin, policy, &mut bounded)?;
        bounded.check()?;
        // The injected port still must return the original compiled output,
        // never variable source bytes or another derivative with matching names.
        crate::font_import::fetch::read_pinned_bytes(
            std::io::Cursor::new(bytes),
            output_size,
            &pin.source.face.content_hash,
            deadline,
            &mut bounded,
        )
    }
}
impl DerivedFontSource for NativeSynthesisFontSource {
    fn obtain(
        &self,
        source: &PinnedDerivedFont,
        policy: &FontFetchPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        let actual = application::actual_source(source)?;
        if !(1..=MAX_FONT_BYTES).contains(&policy.max_bytes)
            || !(1..=30_000).contains(&policy.deadline_ms)
        {
            return Err(EditorError::new(
                "FONT_SYNTHESIS_INVALID",
                "Synthesis acquisition policy exceeds its bounded envelope",
            ));
        }
        if actual.size_bytes > policy.max_bytes {
            return Err(EditorError::new(
                "FONT_BUDGET_EXCEEDED",
                "Actual font exceeds the acquisition byte budget",
            ));
        }
        let deadline = Instant::now() + Duration::from_millis(policy.deadline_ms);
        if let Some(cache) = &self.cache {
            if let Some(bytes) = read_cached(
                &cache_path(actual, cache)?,
                actual.size_bytes,
                actual.content_hash,
                deadline,
                checkpoint,
            )? {
                return Ok(bytes);
            }
            // Earlier fixed-source acquisition may have populated the original
            // repository path. It can also hold another commit: only an exact
            // size/hash match is reusable, and no existing file is overwritten.
            if let Some(path) = original_historical_cache_path(actual, cache)? {
                match read_cached(
                    &path,
                    actual.size_bytes,
                    actual.content_hash,
                    deadline,
                    checkpoint,
                ) {
                    Ok(Some(bytes)) => return Ok(bytes),
                    Ok(None) => {}
                    Err(error) if error.code == "FONT_SOURCE_INTEGRITY_ERROR" => {}
                    Err(error) => return Err(error),
                }
            }
        }
        checkpoint.check()?;
        match application::base_source(&source.source.face.id)? {
            SynthesisBaseSource::RepositoryStatic(_) => {
                validate_font_fetch(actual.catalog_id, policy)?;
            }
            SynthesisBaseSource::ApprovedStatic(_) => {
                opencut_editor_application::font_catalog::validate_fetch(
                    actual.catalog_id,
                    policy,
                )?;
            }
            _ => return Err(integrity()),
        }
        let remaining_ms = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(u128::from(policy.deadline_ms)) as u64;
        if remaining_ms == 0 {
            return Err(EditorError::new(
                "FONT_SOURCE_TIMEOUT",
                "Synthesis source acquisition exceeded its deadline",
            ));
        }
        let remaining_policy = FontFetchPolicy {
            deadline_ms: remaining_ms,
            ..*policy
        };
        crate::font_import::fetch::fetch_pinned_source(actual, &remaining_policy, checkpoint)
    }
}

#[cfg(test)]
#[path = "font_synthesis_tests.rs"]
mod tests;
