//! Fixed Kenney acquisition. The shared process transport owns child teardown;
//! the existing upload probe and CAS adapter own binary media admission.
use opencut_contracts::{Asset, AssetKind, AssetVersion, EditorError, project_v4::*};
use opencut_editor_application::{catalog_resources_v1::*, font_import_v1::FontImportCheckpointV1};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct CatalogArchiveRuntimeV1 {
    pub node: PathBuf,
    pub runner: PathBuf,
}
pub struct NativeCatalogSoundV1 {
    pub root: PathBuf,
    pub runtime: Option<CatalogArchiveRuntimeV1>,
    pub allow_download: bool,
}
fn error(message: &str) -> EditorError {
    EditorError::new("RESOURCE_SOURCE_INVALID", message)
}
// This is a metadata-only hint for the one compiled archive in the existing CAS
// layout, never proof of its bytes or authority to acquire a resource.
fn cached_archive_path(root: &Path) -> Result<Option<PathBuf>, EditorError> {
    let shard = root.join(&KENNEY_SHA256[..2]);
    for directory in [root, shard.as_path()] {
        match std::fs::symlink_metadata(directory) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(error(
                    "Pinned archive cache directory is not a regular directory",
                ));
            }
            Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(error("Cannot inspect pinned archive cache directory")),
        }
    }
    let path = shard.join(KENNEY_SHA256);
    match std::fs::symlink_metadata(&path) {
        Ok(meta)
            if meta.is_file()
                && !meta.file_type().is_symlink()
                && meta.len() == KENNEY_BYTES as u64 =>
        {
            Ok(Some(path))
        }
        Ok(_) => Err(error("Pinned archive cache has an invalid type or size")),
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(error("Cannot inspect pinned archive cache")),
    }
}
pub fn cached_archive_candidate(root: &Path) -> bool {
    matches!(cached_archive_path(root), Ok(Some(_)))
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 15) as usize] as char);
    }
    out
}
fn unhex(value: &str, size: usize) -> Result<Vec<u8>, EditorError> {
    if size == 0
        || size > MAX_SOUND_BYTES
        || value.len() != size * 2
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(error("Invalid archive output byte envelope"));
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|p| {
            u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16)
                .map_err(|_| error("Invalid archive bytes"))
        })
        .collect()
}
fn check(
    deadline: Instant,
    checkpoint: &mut dyn FontImportCheckpointV1,
) -> Result<(), EditorError> {
    checkpoint.check()?;
    if Instant::now() >= deadline {
        return Err(EditorError::new(
            "RESOURCE_SOURCE_TIMEOUT",
            "Catalog acquisition deadline exceeded",
        ));
    }
    Ok(())
}
fn pin_bytes(
    root: &std::path::Path,
    bytes: &[u8],
    name: &str,
    kind: AssetKind,
) -> Result<(AssetVersion, opencut_asset_store::AssetStore), EditorError> {
    let mut temporary = tempfile::NamedTempFile::new_in(
        root.parent()
            .ok_or_else(|| error("Missing CAS workspace"))?,
    )
    .map_err(|_| error("Could not stage catalog resource"))?;
    temporary
        .write_all(bytes)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|_| error("Could not stage catalog bytes"))?;
    let store = opencut_asset_store::AssetStore::open_managed(root)?;
    let asset = store.pin_asset(&Asset {
        id: "catalog-source".into(),
        name: name.into(),
        kind,
        uri: temporary
            .path()
            .to_str()
            .ok_or_else(|| error("Invalid local path"))?
            .into(),
        duration: None,
        version: None,
        metadata: Default::default(),
    })?;
    Ok((
        asset
            .version
            .ok_or_else(|| error("CAS did not capture catalog source"))?,
        store,
    ))
}
impl NativeCatalogSoundV1 {
    fn archive(
        &self,
        deadline: Instant,
        checkpoint: &mut dyn FontImportCheckpointV1,
    ) -> Result<Vec<u8>, EditorError> {
        check(deadline, checkpoint)?;
        let expected = AssetVersion {
            id: format!("av_{KENNEY_SHA256}"),
            content_hash: KENNEY_SHA256.into(),
            size: KENNEY_BYTES as u64,
        };
        if cached_archive_path(&self.root)?.is_some() {
            let store = opencut_asset_store::AssetStore::read_only(&self.root)
                .map_err(|_| error("Cannot open pinned archive cache"))?;
            let (mut file, _) = store
                .open_version_verified(&expected)
                .map_err(|_| error("Pinned archive cache failed immutable byte verification"))?;
            let mut bytes = Vec::with_capacity(KENNEY_BYTES);
            Read::by_ref(&mut file)
                .take(KENNEY_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| error("Cannot read pinned archive cache"))?;
            if bytes.len() != KENNEY_BYTES || sha(&bytes) != KENNEY_SHA256 {
                return Err(error("Pinned archive cache differs from its fixed source"));
            }
            check(deadline, checkpoint)?;
            return Ok(bytes);
        }
        if !self.allow_download {
            return Err(EditorError::new(
                "CAPABILITY_UNAVAILABLE",
                "Public resource downloads are disabled by this host",
            ));
        }
        let duration = deadline.saturating_duration_since(Instant::now());
        let proxy =
            crate::font_import_v1::fetch::source_proxy(KENNEY_URL, |key| std::env::var(key).ok())?;
        let agent = ureq::Agent::config_builder()
            .proxy(proxy)
            .https_only(true)
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(duration))
            .timeout_resolve(Some(duration.min(Duration::from_secs(5))))
            .timeout_connect(Some(duration.min(Duration::from_secs(5))))
            .timeout_recv_response(Some(duration.min(Duration::from_secs(10))))
            .build()
            .new_agent();
        check(deadline, checkpoint)?;
        let mut response = agent
            .get(KENNEY_URL)
            .header("Accept", "application/zip")
            .header("Accept-Encoding", "identity")
            .header("User-Agent", "OpenCut-Catalog-Resources-V1")
            .call()
            .map_err(|_| {
                EditorError::new(
                    "RESOURCE_SOURCE_UNAVAILABLE",
                    "Pinned Kenney source could not be downloaded",
                )
            })?;
        check(deadline, checkpoint)?;
        if response.status().as_u16() != 200
            || response
                .headers()
                .get("content-encoding")
                .is_some_and(|v| v.as_bytes() != b"identity")
            || response.headers().get("content-length").is_some_and(|v| {
                v.to_str().ok().and_then(|s| s.parse::<usize>().ok()) != Some(KENNEY_BYTES)
            })
        {
            return Err(error("Pinned source status or size changed"));
        }
        let bytes = crate::font_import_v1::fetch::read_pinned_bytes(
            response
                .body_mut()
                .with_config()
                .limit(KENNEY_BYTES as u64 + 1)
                .reader(),
            KENNEY_BYTES as u64,
            KENNEY_SHA256,
            deadline,
            checkpoint,
        )?;
        let (version, _publication) = pin_bytes(
            &self.root,
            &bytes,
            "kenney_ui-audio.zip",
            AssetKind::Graphic,
        )?;
        if version != expected {
            return Err(error("Archive changed during CAS capture"));
        }
        check(deadline, checkpoint)?;
        Ok(bytes)
    }
    #[cfg(unix)]
    fn extract(
        &self,
        id: &str,
        archive: &[u8],
        deadline: Instant,
        checkpoint: &mut dyn FontImportCheckpointV1,
    ) -> Result<Vec<u8>, EditorError> {
        let expected = sound_source(id)?;
        let runtime = self.runtime.as_ref().ok_or_else(|| {
            EditorError::new(
                "CAPABILITY_UNAVAILABLE",
                "Catalog archive runtime is unavailable",
            )
        })?;
        let execution = format!("catalog_{}", uuid::Uuid::new_v4().simple());
        let mut process = crate::browser_process::BrowserProcess::start(
            &runtime.node,
            &runtime.runner,
            deadline.saturating_duration_since(Instant::now()),
        )?;
        let result = (|| {
            crate::classic_audio::transport::packet(
                process
                    .input
                    .as_mut()
                    .ok_or_else(|| error("Missing archive input"))?,
                &execution,
                serde_json::json!({"type":"catalog_archive_v1","catalogId":id,"archiveHex":hex(archive)}),
            )?;
            process.input.take();
            loop {
                check(deadline, checkpoint)?;
                let Some(packet) = process.receive()? else {
                    continue;
                };
                if packet["protocolVersion"] != 1
                    || packet["executionId"] != execution
                    || packet["type"] != "complete"
                    || packet["catalogId"] != id
                    || packet["archiveSha256"] != KENNEY_SHA256
                {
                    return Err(error("Archive response changed its fixed identity"));
                }
                let size = packet["size"]
                    .as_u64()
                    .filter(|n| *n <= MAX_SOUND_BYTES as u64)
                    .ok_or_else(|| error("Invalid archive entry size"))?
                    as usize;
                let bytes = unhex(
                    packet["hex"]
                        .as_str()
                        .ok_or_else(|| error("Missing archive bytes"))?,
                    size,
                )?;
                if packet["sha256"] != sha(&bytes)
                    || packet["crc32"].as_u64() != Some(u64::from(crc32(&bytes)))
                    || bytes.len() != expected.size
                    || sha(&bytes) != expected.sha256
                    || crc32(&bytes) != expected.crc32
                {
                    return Err(error("Archive output digest changed"));
                }
                check(deadline, checkpoint)?;
                return Ok(bytes);
            }
        })();
        if result.is_err() {
            process.guard.terminate();
        }
        let cleanup = process.finish(result.is_ok(), &execution);
        match (result, cleanup) {
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Ok(bytes), Ok(())) => Ok(bytes),
        }
    }
    #[cfg(not(unix))]
    fn extract(
        &self,
        _: &str,
        _: &[u8],
        _: Instant,
        _: &mut dyn FontImportCheckpointV1,
    ) -> Result<Vec<u8>, EditorError> {
        Err(EditorError::new(
            "CAPABILITY_UNAVAILABLE",
            "Catalog archive process groups are unavailable",
        ))
    }
}
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb88320 } else { 0 };
        }
    }
    !crc
}
impl CatalogSoundAcquireV1 for NativeCatalogSoundV1 {
    type Lease = opencut_asset_store::AssetStore;
    fn acquire(
        &self,
        id: &str,
        checkpoint: &mut dyn FontImportCheckpointV1,
    ) -> Result<AcquiredCatalogResourceV1<Self::Lease>, EditorError> {
        let source = entry(id)?;
        if source.category
            != opencut_contracts::catalog_resources_v1::CatalogResourceCategoryV1::Sounds
            || self.runtime.is_none()
        {
            return Err(EditorError::new(
                "CAPABILITY_UNAVAILABLE",
                "Pinned sound runtime is unavailable",
            ));
        }
        let deadline = Instant::now() + Duration::from_secs(45);
        let workspace = self
            .root
            .parent()
            .ok_or_else(|| error("Missing CAS workspace"))?;
        let _reservation = opencut_storage_budget::Budget::new(workspace).reserve(
            "catalog_resource",
            KENNEY_BYTES as u64 * 2 + MAX_SOUND_BYTES as u64 * 2,
        )?;
        let archive = self.archive(deadline, checkpoint)?;
        let bytes = self.extract(id, &archive, deadline, checkpoint)?;
        check(deadline, checkpoint)?;
        let mut temporary = tempfile::Builder::new()
            .suffix(".ogg")
            .tempfile_in(workspace)
            .map_err(|_| error("Cannot stage pinned audio"))?;
        temporary
            .write_all(&bytes)
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|_| error("Cannot stage pinned audio bytes"))?;
        let backend = opencut_media::FfmpegBackend::discover().map_err(|_| {
            EditorError::new(
                "CAPABILITY_UNAVAILABLE",
                "Pinned sound probe is unavailable",
            )
        })?;
        let cancel = opencut_media::CancellationToken::new();
        let metadata = std::thread::scope(|scope| {
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            let token = cancel.clone();
            let path = temporary.path();
            scope.spawn(move || {
                let _ = sender.send(backend.probe_upload(path, &token));
            });
            loop {
                if let Err(error) = check(deadline, checkpoint) {
                    cancel.cancel();
                    return Err(error);
                }
                match receiver.recv_timeout(Duration::from_millis(25)) {
                    Ok(value) => return value.map_err(|_| error("Pinned sound probe failed")),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => return Err(error("Pinned sound probe ended without an observation")),
                }
            }
        })?;
        let observed = crate::media::describe(
            &metadata,
            temporary.path(),
            &source.name,
            &sha(&bytes),
            bytes.len() as u64,
        )?;
        let (version, lease) = pin_bytes(&self.root, &bytes, &source.name, AssetKind::Audio)?;
        check(deadline, checkpoint)?;
        // Reuse the same legacy probe projection used by authorized uploads.
        let mut observed = observed;
        observed.version = Some(version.clone());
        let converted = opencut_editor_core_projection(&observed, &version)?;
        Ok(AcquiredCatalogResourceV1 {
            resource: converted,
            lease,
        })
    }
}
fn opencut_editor_core_projection(
    asset: &Asset,
    version: &AssetVersion,
) -> Result<AssetV4, EditorError> {
    // Native services does not depend directly on editor-core. The application
    // owns this already-used typed metadata projection, exposed below as a port helper.
    opencut_editor_application::catalog_resources_v1::observed_asset(asset, version)
}

/// New local SFX extension, not a Classic renderer algorithm. The versioned
/// recipe fixes source, envelope, sample format and seed; no user filter or
/// executable expression is accepted. Exact cross-platform PCM is not claimed.
fn synthesize(
    recipe: &opencut_contracts::catalog_resources_v1::LocalSoundRecipeV1,
    checkpoint: &mut dyn FontImportCheckpointV1,
) -> Result<Vec<u8>, EditorError> {
    use opencut_contracts::catalog_resources_v1::LocalSoundKindV1;
    validate_recipe(recipe)?;
    checkpoint.check()?;
    let frames = recipe.duration_ms * 48;
    let mut wave = Vec::with_capacity(44 + frames as usize * 2);
    wave.extend_from_slice(b"RIFF");
    wave.extend_from_slice(&(36 + frames * 2).to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend_from_slice(&16u32.to_le_bytes());
    wave.extend_from_slice(&1u16.to_le_bytes());
    wave.extend_from_slice(&1u16.to_le_bytes());
    wave.extend_from_slice(&48000u32.to_le_bytes());
    wave.extend_from_slice(&96000u32.to_le_bytes());
    wave.extend_from_slice(&2u16.to_le_bytes());
    wave.extend_from_slice(&16u16.to_le_bytes());
    wave.extend_from_slice(b"data");
    wave.extend_from_slice(&(frames * 2).to_le_bytes());
    let gain = 10.0f64.powf(recipe.level_db / 20.0);
    let mut random = if recipe.seed == 0 {
        0x6d2b79f5
    } else {
        recipe.seed
    };
    let mut low = 0.0;
    let mut phase = 0.0;
    for index in 0..frames {
        if index % 2048 == 0 {
            checkpoint.check()?;
        }
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        let noise = f64::from(random) / f64::from(u32::MAX) * 2.0 - 1.0;
        let x = f64::from(index) / f64::from(frames - 1);
        let seconds = f64::from(index) / 48000.0;
        let sample = match recipe.kind {
            LocalSoundKindV1::Whoosh => {
                let sweep = (std::f64::consts::PI * x).sin();
                low += (0.025 + 0.25 * sweep) * (noise - low);
                (0.6 * (noise - low) + 0.4 * low) * sweep * sweep
            }
            LocalSoundKindV1::Click => {
                phase += std::f64::consts::TAU * (350.0 + 1450.0 * (1.0 - x)) / 48000.0;
                (0.65 * phase.sin() + 0.35 * noise)
                    * (-30.0 * x).exp()
                    * (1.0 - x)
                    * (seconds / 0.001).min(1.0)
            }
            LocalSoundKindV1::Impact => {
                phase += std::f64::consts::TAU * (40.0 + 150.0 * (-8.0 * x).exp()) / 48000.0;
                low += 0.25 * (noise - low);
                (0.7 * phase.sin() + 0.3 * low)
                    * (-4.0 * x).exp()
                    * (1.0 - x).powi(3)
                    * (seconds / 0.002).min(1.0)
            }
        };
        let sample = if index == 0 || index + 1 == frames {
            0
        } else {
            (sample * gain * 32767.0).round().clamp(-32767.0, 32767.0) as i16
        };
        wave.extend_from_slice(&sample.to_le_bytes());
    }
    checkpoint.check()?;
    Ok(wave)
}
impl CatalogSoundGenerateV1 for NativeCatalogSoundV1 {
    type Lease = opencut_asset_store::AssetStore;
    fn generate(
        &self,
        recipe: &opencut_contracts::catalog_resources_v1::LocalSoundRecipeV1,
        checkpoint: &mut dyn FontImportCheckpointV1,
    ) -> Result<AcquiredCatalogResourceV1<Self::Lease>, EditorError> {
        validate_recipe(recipe)?;
        checkpoint.check()?;
        let workspace = self
            .root
            .parent()
            .ok_or_else(|| error("Missing CAS workspace"))?;
        let _reservation = opencut_storage_budget::Budget::new(workspace).reserve(
            "catalog_generated_sound",
            (44 + u64::from(recipe.duration_ms) * 96) * 2,
        )?;
        let bytes = synthesize(recipe, checkpoint)?;
        let (version, lease) =
            pin_bytes(&self.root, &bytes, "generated-sound.wav", AssetKind::Audio)?;
        checkpoint.check()?;
        Ok(AcquiredCatalogResourceV1 {
            lease,
            resource: AssetV4 {
                id: "generated-sound".into(),
                name: "Local sound".into(),
                kind: AssetKind::Audio,
                version,
                duration: Some(opencut_contracts::Time {
                    ticks: (recipe.duration_ms * 48).to_string(),
                    timebase: 48000,
                }),
                origin: ResourceOriginV4::Generated {
                    provider: "opencut-local-sfx-v1".into(),
                    generation_id: String::new(),
                },
                license: None,
                metadata: MediaMetadataV4 {
                    mime_type: Some("audio/wav".into()),
                    intrinsic_size: None,
                    frame_rate: None,
                    rotation_degrees: None,
                    audio: AudioPresenceV4::Present {
                        metadata: Some(AudioMetadataV4 {
                            sample_rate: 48000,
                            channels: 1,
                            channel_layout: Some("mono".into()),
                        }),
                    },
                },
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_candidate_is_only_a_regular_exact_size_hint_and_never_creates_cache_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("cas");
        assert!(!cached_archive_candidate(&root));
        assert!(!root.exists());
        let path = root.join(&KENNEY_SHA256[..2]).join(KENNEY_SHA256);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        assert!(!cached_archive_candidate(&root));
        std::fs::write(&path, vec![0u8; KENNEY_BYTES - 1]).unwrap();
        assert!(!cached_archive_candidate(&root));
        std::fs::write(&path, vec![0u8; KENNEY_BYTES]).unwrap();
        assert!(cached_archive_candidate(&root));
        // The candidate deliberately does not claim this wrong digest is valid.
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(!cached_archive_candidate(&root));
    }
    #[cfg(unix)]
    #[test]
    fn archive_candidate_refuses_symlink_files_shards_and_roots() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("cas");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        let bytes = outside.join(KENNEY_SHA256);
        std::fs::write(&bytes, vec![0u8; KENNEY_BYTES]).unwrap();
        let shard = root.join(&KENNEY_SHA256[..2]);
        symlink(&outside, &shard).unwrap();
        assert!(!cached_archive_candidate(&root));
        std::fs::remove_file(&shard).unwrap();
        std::fs::create_dir(&shard).unwrap();
        symlink(&bytes, shard.join(KENNEY_SHA256)).unwrap();
        assert!(!cached_archive_candidate(&root));
        let alias = temp.path().join("alias");
        symlink(&root, &alias).unwrap();
        assert!(!cached_archive_candidate(&alias));
    }
    #[test]
    fn corrupt_local_archive_never_grants_bytes_or_silently_falls_back_to_network() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("cas");
        drop(opencut_asset_store::AssetStore::open(&root).unwrap());
        let path = root.join(&KENNEY_SHA256[..2]).join(KENNEY_SHA256);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![0u8; KENNEY_BYTES]).unwrap();
        assert!(cached_archive_candidate(&root));
        for allow_download in [false, true] {
            let source = NativeCatalogSoundV1 {
                root: root.clone(),
                runtime: None,
                allow_download,
            };
            let error = source
                .archive(Instant::now() + Duration::from_secs(1), &mut || Ok(()))
                .unwrap_err();
            assert_eq!(error.code, "RESOURCE_SOURCE_INVALID");
            assert!(error.message.contains("verification"));
        }
    }
    #[test]
    fn offline_missing_archive_and_cancelled_candidate_are_deterministic() {
        let temp = tempfile::tempdir().unwrap();
        let source = NativeCatalogSoundV1 {
            root: temp.path().join("missing"),
            runtime: None,
            allow_download: false,
        };
        assert_eq!(
            source
                .archive(Instant::now() + Duration::from_secs(1), &mut || Ok(()))
                .unwrap_err()
                .code,
            "CAPABILITY_UNAVAILABLE"
        );
        let mut cancelled = || Err(EditorError::new("JOB_CANCELLED", "cancelled"));
        assert_eq!(
            source
                .archive(Instant::now() + Duration::from_secs(1), &mut cancelled)
                .unwrap_err()
                .code,
            "JOB_CANCELLED"
        );
        assert!(!source.root.exists());
    }
    #[test]
    fn archive_output_checks_real_crc_and_canonical_bounded_hex() {
        assert_eq!(crc32(b"123456789"), 0xcbf43926);
        let bytes = b"pinned source bytes";
        assert_eq!(unhex(&hex(bytes), bytes.len()).unwrap(), bytes);
        assert!(unhex("FF", 1).is_err());
        assert!(unhex("0000", 1).is_err());
        assert!(unhex("", 0).is_err());
        assert!(unhex("00", MAX_SOUND_BYTES + 1).is_err());
    }
    #[test]
    fn cancelled_or_expired_checkpoint_preserves_failure_before_any_io() {
        let mut cancelled = || Err(EditorError::new("JOB_CANCELLED", "cancelled"));
        assert_eq!(
            check(Instant::now() + Duration::from_secs(1), &mut cancelled)
                .unwrap_err()
                .code,
            "JOB_CANCELLED"
        );
        assert_eq!(
            check(Instant::now(), &mut || Ok(())).unwrap_err().code,
            "RESOURCE_SOURCE_TIMEOUT"
        );
    }
    #[test]
    fn local_recipes_emit_bounded_wav_distinct_pcm_seed_and_level_with_zero_edges() {
        use opencut_contracts::catalog_resources_v1::{LocalSoundKindV1, LocalSoundRecipeV1};
        let mut digests = std::collections::BTreeSet::new();
        for kind in [
            LocalSoundKindV1::Whoosh,
            LocalSoundKindV1::Click,
            LocalSoundKindV1::Impact,
        ] {
            let recipe = LocalSoundRecipeV1 {
                kind,
                duration_ms: 200,
                seed: 1,
                level_db: -6.0,
            };
            let bytes = synthesize(&recipe, &mut || Ok(())).unwrap();
            assert_eq!(bytes.len(), 44 + 9600 * 2);
            assert_eq!(&bytes[0..4], b"RIFF");
            assert_eq!(&bytes[8..16], b"WAVEfmt ");
            assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 48000);
            assert_eq!(&bytes[44..46], &[0, 0]);
            assert_eq!(&bytes[bytes.len() - 2..], &[0, 0]);
            let samples = bytes[44..]
                .chunks_exact(2)
                .map(|p| i16::from_le_bytes([p[0], p[1]]))
                .collect::<Vec<_>>();
            assert!(samples.iter().any(|v| v.abs() > 100));
            assert!(samples.iter().all(|v| i32::from(*v).abs() <= 16423));
            assert_eq!(bytes, synthesize(&recipe, &mut || Ok(())).unwrap());
            let mut changed = recipe.clone();
            changed.seed += 1;
            assert_ne!(bytes, synthesize(&changed, &mut || Ok(())).unwrap());
            changed = recipe;
            changed.level_db = -12.0;
            let quieter = synthesize(&changed, &mut || Ok(())).unwrap();
            let energy = |b: &[u8]| {
                b[44..]
                    .chunks_exact(2)
                    .map(|p| {
                        let n = f64::from(i16::from_le_bytes([p[0], p[1]]));
                        n * n
                    })
                    .sum::<f64>()
            };
            assert!((energy(&quieter) / energy(&bytes) - 0.2511886).abs() < 0.001);
            digests.insert(sha(&bytes));
        }
        assert_eq!(digests.len(), 3);
    }
    struct FixedSource {
        root: PathBuf,
        calls: std::cell::Cell<usize>,
    }
    impl CatalogSoundGenerateV1 for FixedSource {
        type Lease = opencut_asset_store::AssetStore;
        fn generate(
            &self,
            recipe: &opencut_contracts::catalog_resources_v1::LocalSoundRecipeV1,
            checkpoint: &mut dyn FontImportCheckpointV1,
        ) -> Result<AcquiredCatalogResourceV1<Self::Lease>, EditorError> {
            self.calls.set(self.calls.get() + 1);
            NativeCatalogSoundV1 {
                root: self.root.clone(),
                runtime: None,
                allow_download: false,
            }
            .generate(recipe, checkpoint)
        }
    }
    fn fixture() -> (
        tempfile::TempDir,
        opencut_sqlite_store::EditorApplication,
        opencut_contracts::catalog_resources_v1::GenerateCatalogSoundV1,
        FixedSource,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let mut app = opencut_sqlite_store::EditorApplication::open_managed(
            directory.path().join("workspace.sqlite"),
        )
        .unwrap();
        let created = app
            .create_v4_project(&opencut_contracts::project_v4_ops::CreateV4Project {
                project: opencut_contracts::CreateProject {
                    id: "project".into(),
                    name: "Catalog".into(),
                    sequence_id: "scene".into(),
                    width: 320,
                    height: 180,
                    fps: opencut_contracts::FrameRate {
                        numerator: 30,
                        denominator: 1,
                    },
                },
                idempotency_key: "create".into(),
            })
            .unwrap();
        let source = FixedSource {
            root: app.asset_store_root().to_path_buf(),
            calls: std::cell::Cell::new(0),
        };
        let request = opencut_contracts::catalog_resources_v1::GenerateCatalogSoundV1 {
            project_id: created.project.id,
            expected_revision: created.revision,
            idempotency_key: "original-catalog-generate".into(),
            asset_id: "catalog-sound".into(),
            recipe: opencut_contracts::catalog_resources_v1::LocalSoundRecipeV1 {
                kind: opencut_contracts::catalog_resources_v1::LocalSoundKindV1::Click,
                duration_ms: 100,
                seed: 123,
                level_db: -6.0,
            },
        };
        (directory, app, request, source)
    }
    #[test]
    fn real_cas_commit_reopen_replays_original_request_without_reacquisition() {
        let (directory, mut app, request, source) = fixture();
        let commit = opencut_editor_application::project_v4::catalog_resources_v1::generate(
            &mut app,
            &source,
            &request,
            &mut || Ok(()),
        )
        .unwrap();
        assert_eq!(source.calls.get(), 1);
        assert!(commit.created_ids.contains(&request.asset_id));
        drop(app);
        let mut app = opencut_sqlite_store::EditorApplication::open_managed(
            directory.path().join("workspace.sqlite"),
        )
        .unwrap();
        assert_eq!(
            opencut_editor_application::project_v4::catalog_resources_v1::generate(
                &mut app,
                &source,
                &request,
                &mut || Ok(())
            )
            .unwrap(),
            commit
        );
        assert_eq!(source.calls.get(), 1);
        let mut changed = request.clone();
        changed.recipe.seed += 1;
        assert_eq!(
            opencut_editor_application::project_v4::catalog_resources_v1::generate(
                &mut app,
                &source,
                &changed,
                &mut || Ok(())
            )
            .unwrap_err()
            .code,
            "IDEMPOTENCY_KEY_REUSED"
        );
        assert_eq!(source.calls.get(), 1);
    }
    #[test]
    fn generated_resource_history_reopens_and_undo_redo_preserve_recipe_and_bytes() {
        use opencut_contracts::{project_v4_history::*, project_v4_ops::InspectV4Project};
        let (directory, mut app, request, source) = fixture();
        let saved = opencut_editor_application::project_v4::catalog_resources_v1::generate(
            &mut app,
            &source,
            &request,
            &mut || Ok(()),
        )
        .unwrap();
        let read = |app: &opencut_sqlite_store::EditorApplication| {
            app.inspect_v4_project(&InspectV4Project {
                project_id: request.project_id.clone(),
                revision: None,
            })
            .unwrap()
        };
        let expected = read(&app).project;
        drop(app);
        let mut app = opencut_sqlite_store::EditorApplication::open_managed(
            directory.path().join("workspace.sqlite"),
        )
        .unwrap();
        app.v4_history_navigation(&HistoryNavigationInputV4 {
            project_id: request.project_id.clone(),
            revision: None,
        })
        .unwrap();
        let undone = app
            .apply_v4_undo(&UndoV4Project {
                project_id: request.project_id.clone(),
                expected_revision: read(&app).revision,
                target_transaction_id: saved.transaction_id,
                idempotency_key: "undo-generated".into(),
            })
            .unwrap();
        assert!(read(&app).project.assets.is_empty());
        app.apply_v4_redo(&RedoV4Project {
            project_id: request.project_id.clone(),
            expected_revision: read(&app).revision,
            undo_transaction_id: undone.transaction.transaction_id,
            idempotency_key: "redo-generated".into(),
        })
        .unwrap();
        assert_eq!(read(&app).project, expected);
        assert_eq!(source.calls.get(), 1);
    }
    #[test]
    fn cancellation_or_stale_head_never_acquires_or_publishes() {
        use opencut_editor_application::project_v4::ProjectV4Read;
        let (_directory, mut app, mut request, source) = fixture();
        let prior = app.head_v4_source(&request.project_id).unwrap();
        assert_eq!(
            opencut_editor_application::project_v4::catalog_resources_v1::generate(
                &mut app,
                &source,
                &request,
                &mut || Err(EditorError::new("JOB_CANCELLED", "cancelled"))
            )
            .unwrap_err()
            .code,
            "JOB_CANCELLED"
        );
        assert_eq!(source.calls.get(), 0);
        assert_eq!(app.head_v4_source(&request.project_id).unwrap(), prior);
        request.expected_revision = "stale".into();
        assert_eq!(
            opencut_editor_application::project_v4::catalog_resources_v1::generate(
                &mut app,
                &source,
                &request,
                &mut || Ok(())
            )
            .unwrap_err()
            .code,
            "REVISION_CONFLICT"
        );
        assert_eq!(source.calls.get(), 0);
        assert_eq!(app.head_v4_source(&request.project_id).unwrap(), prior);
    }
    #[test]
    fn cancellation_after_cas_capture_leaves_project_and_original_command_uncommitted() {
        use opencut_editor_application::project_v4::ProjectV4Read;
        struct CancelAfterCapture<'a> {
            source: &'a FixedSource,
            cancelled: &'a std::cell::Cell<bool>,
        }
        impl CatalogSoundGenerateV1 for CancelAfterCapture<'_> {
            type Lease = opencut_asset_store::AssetStore;
            fn generate(
                &self,
                recipe: &opencut_contracts::catalog_resources_v1::LocalSoundRecipeV1,
                checkpoint: &mut dyn FontImportCheckpointV1,
            ) -> Result<AcquiredCatalogResourceV1<Self::Lease>, EditorError> {
                let result = self.source.generate(recipe, checkpoint)?;
                self.cancelled.set(true);
                Ok(result)
            }
        }
        let (_directory, mut app, request, source) = fixture();
        let prior = app.head_v4_source(&request.project_id).unwrap();
        let cancelled = std::cell::Cell::new(false);
        let native = CancelAfterCapture {
            source: &source,
            cancelled: &cancelled,
        };
        let mut checkpoint = || {
            if cancelled.get() {
                Err(EditorError::new("JOB_CANCELLED", "cancelled after capture"))
            } else {
                Ok(())
            }
        };
        assert_eq!(
            opencut_editor_application::project_v4::catalog_resources_v1::generate(
                &mut app,
                &native,
                &request,
                &mut checkpoint
            )
            .unwrap_err()
            .code,
            "JOB_CANCELLED"
        );
        assert_eq!(source.calls.get(), 1);
        assert_eq!(app.head_v4_source(&request.project_id).unwrap(), prior);
        let committed = opencut_editor_application::project_v4::catalog_resources_v1::generate(
            &mut app,
            &source,
            &request,
            &mut || Ok(()),
        )
        .unwrap();
        assert!(committed.created_ids.contains(&request.asset_id));
        assert_eq!(source.calls.get(), 2);
    }
}
