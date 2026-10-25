//! Isolated, bounded fixed-CAS source decoding. Cache and projection live above
//! this adapter; no authoring duration or gain participates in source peaks.
use opencut_contracts::{EditorError, source_art::*};
use opencut_editor_application::source_art::*;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
mod identity;
mod process;
#[cfg(test)]
mod tests;
pub use identity::resolve_identity;

#[derive(Clone)]
pub struct SourceArtProcessConfig {
    pub node: PathBuf,
    pub runner: PathBuf,
    pub chromium: PathBuf,
    pub deadline: Duration,
    pub max_encoded_bytes: u64,
    pub max_pcm_bytes: u64,
    pub max_rss_bytes: u64,
    pub identity: SourceArtIdentity,
}
pub struct SourceArtProcessExecutor {
    config: SourceArtProcessConfig,
    asset_root: PathBuf,
}
fn error(code: &str, message: &str) -> EditorError {
    EditorError::new(code, message)
}
fn protocol(message: &str) -> EditorError {
    error("SOURCE_ART_PROTOCOL_ERROR", message)
}
fn trusted_file(path: &Path) -> Result<(), EditorError> {
    if !path.is_absolute() || !path.is_file() {
        return Err(error(
            "CAPABILITY_UNAVAILABLE",
            "Source-art runtime file is unavailable",
        ));
    }
    Ok(())
}
fn validate_identity(identity: &SourceArtIdentity) -> Result<(), EditorError> {
    if ![
        SOURCE_PEAKS_ALGORITHM,
        THUMBNAIL_ALGORITHM,
        SOURCE_FRAME_ALGORITHM,
    ]
    .contains(&identity.algorithm.as_str())
        || identity.decode_profile != SOURCE_ART_DECODE_PROFILE
        || identity.browser_version.is_empty()
        || identity.browser_version.len() > 128
        || !identity
            .browser_version
            .bytes()
            .all(|c| c.is_ascii_digit() || c == b'.')
        || [
            &identity.browser_sha256,
            &identity.node_sha256,
            &identity.module_sha256,
        ]
        .iter()
        .any(|s| {
            s.len() != 64
                || !s
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        })
    {
        return Err(protocol("Invalid source-art runtime identity"));
    }
    Ok(())
}
impl SourceArtProcessExecutor {
    pub fn new(config: SourceArtProcessConfig, asset_root: PathBuf) -> Result<Self, EditorError> {
        for path in [&config.node, &config.runner, &config.chromium] {
            trusted_file(path)?;
        }
        validate_identity(&config.identity)?;
        let policy = SourceArtPolicy {
            max_encoded_bytes: config.max_encoded_bytes,
            max_pcm_bytes: config.max_pcm_bytes,
            max_rss_bytes: config.max_rss_bytes,
            deadline_ms: u64::try_from(config.deadline.as_millis())
                .map_err(|_| protocol("Invalid source-art deadline"))?,
        };
        validate_policy(&policy)?;
        if config.deadline.as_millis() != u128::from(policy.deadline_ms)
            || config.deadline != Duration::from_millis(policy.deadline_ms)
        {
            return Err(protocol("Source-art deadline must be whole milliseconds"));
        }
        Ok(Self { config, asset_root })
    }
}
impl SourceArtExecutor for SourceArtProcessExecutor {
    fn execute(
        &self,
        prepared: &PreparedSourceArt,
        checkpoint: &mut dyn SourceArtCheckpoint,
    ) -> Result<DecodedSourceArt, EditorError> {
        #[cfg(unix)]
        {
            process::execute(self, prepared, checkpoint)
        }
        #[cfg(not(unix))]
        {
            let _ = (prepared, checkpoint);
            Err(error(
                "CAPABILITY_UNAVAILABLE",
                "Source-art process isolation requires Unix",
            ))
        }
    }
}
