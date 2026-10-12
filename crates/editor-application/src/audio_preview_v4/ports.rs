use super::*;
use opencut_contracts::{EditorError, project_v4::ProjectSnapshotV4};
use opencut_job_execution::project_v4_audio::TrustedJobAuthorityV1;

pub struct AuthorizedAudioPreviewV4 {
    pub snapshot: ProjectSnapshotV4,
    pub authority: TrustedJobAuthorityV1,
    pub policy: AudioPreviewPolicyV4,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioPreviewPolicyV4 {
    pub max_pcm_bytes: u64,
    pub max_encoded_bytes: u64,
    pub max_rss_bytes: u64,
}
pub trait AudioPreviewReadV4 {
    /// Read + RunJobs, live status, fixed revision and trusted identity/policy
    /// must be checked together; no mutable current head substitutes this read.
    fn fixed_audio_preview_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<AuthorizedAudioPreviewV4, EditorError>;
    /// Recheck the original credential and fixed project/resource scope on each
    /// operation/read/block publication and periodically while buffers exist.
    fn revalidate_audio_preview(
        &self,
        project_id: &str,
        revision: &str,
        authority: &TrustedJobAuthorityV1,
    ) -> Result<(), EditorError>;
}
pub fn validate_audio_preview_policy(policy: &AudioPreviewPolicyV4) -> Result<(), EditorError> {
    if policy.max_pcm_bytes == 0
        || policy.max_pcm_bytes > 256 * 1024 * 1024
        || policy.max_encoded_bytes == 0
        || policy.max_encoded_bytes > 32 * 1024 * 1024
        || policy.max_rss_bytes == 0
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Invalid host audio-preview budget",
        ));
    }
    Ok(())
}
pub fn validate_audio_preview_create(input: &AudioPreviewCreateV4) -> Result<(), EditorError> {
    for id in [
        &input.project_id,
        &input.revision,
        &input.sequence_id,
        &input.idempotency_key,
    ] {
        if id.trim().is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Invalid fixed audio-preview identity",
            ));
        }
    }
    if !matches!(input.sample_rate, 44_100 | 48_000) {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Audio preview requires 44100 or 48000 Hz",
        ));
    }
    let time = opencut_time::RationalTime::try_from(&input.start_time)?;
    if time.ticks() < 0 {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Audio preview start must be nonnegative",
        ));
    }
    Ok(())
}
