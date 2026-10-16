//! Request identity and host-owned persistence port. No system clock, actors,
//! leases, tokens or native resource paths are introduced into project content.
use crate::audio_preview_v4::{
    AudioPreviewCommandV4, AudioPreviewCreateV4, AudioPreviewSessionV4,
    validate_audio_preview_create,
};
pub use opencut_api_contracts::audio_preview_recovery_v1::*;
use opencut_contracts::EditorError;
use sha2::{Digest, Sha256};

pub const MAX_RECORD_BYTES: usize = 16 * 1024;
pub const MAX_RECORDS: u64 = 32;
pub fn invalid() -> EditorError {
    EditorError::new(
        "INVALID_ARGUMENT",
        "Invalid original preview recovery request",
    )
}
pub fn identifier(value: &str) -> Result<(), EditorError> {
    if value.trim().is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        Err(invalid())
    } else {
        Ok(())
    }
}
pub fn create(request: &AudioPreviewRecoveryRequestV1) -> &AudioPreviewCreateV4 {
    match request {
        AudioPreviewRecoveryRequestV1::Create { request } => request,
        AudioPreviewRecoveryRequestV1::Control { create, .. } => create,
    }
}
pub fn kind(request: &AudioPreviewRecoveryRequestV1) -> AudioPreviewRecoveryKindV1 {
    match request {
        AudioPreviewRecoveryRequestV1::Create { .. } => AudioPreviewRecoveryKindV1::Create,
        AudioPreviewRecoveryRequestV1::Control { .. } => AudioPreviewRecoveryKindV1::Control,
    }
}
pub fn payload(input: &StageAudioPreviewRecoveryV1) -> Result<String, EditorError> {
    validate_audio_preview_create(create(&input.request))?;
    if let AudioPreviewRecoveryRequestV1::Control { request, .. } = &input.request {
        for id in [
            &request.session_id,
            &request.owner_id,
            &request.lease_id,
            &request.control_id,
        ] {
            identifier(id)?;
        }
        let generation = request.generation.parse::<u64>().map_err(|_| invalid())?;
        if generation.to_string() != request.generation
            || matches!(
                request.command,
                AudioPreviewCommandV4::Heartbeat | AudioPreviewCommandV4::ReportConsumed { .. }
            )
        {
            return Err(invalid());
        }
    }
    let raw = serde_json::to_string(input).map_err(|_| invalid())?;
    if raw.len() > MAX_RECORD_BYTES {
        return Err(EditorError::new(
            "RECOVERY_QUOTA_EXCEEDED",
            "Preview recovery request exceeds 16 KiB",
        ));
    }
    Ok(raw)
}
pub fn digest(raw: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"opencut.audio-preview-recovery.v1\0");
    h.update(raw.as_bytes());
    format!("sha256:{:x}", h.finalize())
}

/// Internal prepared record. Only the redacted view crosses the public wire.
pub struct StoredAudioPreviewRecoveryV1 {
    pub view: AudioPreviewRecoveryRecordV1,
    pub original: StageAudioPreviewRecoveryV1,
    pub host_instance: String,
    pub receipt: Option<AudioPreviewSessionV4>,
    pub dispatched: bool,
}
pub trait AudioPreviewRecoveryStoreV1 {
    fn stage_audio_preview_recovery_v1(
        &mut self,
        input: &StageAudioPreviewRecoveryV1,
        host_instance: &str,
    ) -> Result<AudioPreviewRecoveryRecordV1, EditorError>;
    fn read_audio_preview_recovery_v1(
        &self,
        project: &str,
        id: Option<&str>,
    ) -> Result<Option<StoredAudioPreviewRecoveryV1>, EditorError>;
    fn begin_audio_preview_recovery_v1(
        &mut self,
        project: &str,
        id: &str,
    ) -> Result<(), EditorError>;
    fn complete_audio_preview_recovery_v1(
        &mut self,
        project: &str,
        id: &str,
        receipt: &AudioPreviewSessionV4,
    ) -> Result<(), EditorError>;
    /// The retired bit commits before any cleanup, fencing delayed dispatch.
    fn retire_audio_preview_recovery_v1(
        &mut self,
        project: &str,
        id: &str,
    ) -> Result<(), EditorError>;
    /// Delete only completed/retired records. Unknown staged requests cannot ack.
    fn ack_audio_preview_recovery_v1(&mut self, project: &str, id: &str)
    -> Result<(), EditorError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_original_control_identity_includes_generation_and_lease() {
        let input:StageAudioPreviewRecoveryV1=serde_json::from_value(serde_json::json!({"request":{"kind":"control","create":{"project_id":"p","revision":"r","sequence_id":"s","sample_rate":48000,"start_time":{"ticks":"0","timebase":120000},"idempotency_key":"original"},"request":{"session_id":"session","generation":"4","owner_id":"owner","lease_id":"secret","control_id":"control","command":{"kind":"seek","sequence_time":{"ticks":"20","timebase":120000}}}}})).unwrap();
        let raw = payload(&input).unwrap();
        let mut changed = input.clone();
        if let AudioPreviewRecoveryRequestV1::Control { request, .. } = &mut changed.request {
            request.generation = "5".into();
        }
        assert_ne!(digest(&raw), digest(&payload(&changed).unwrap()));
        assert_eq!(create(&input.request).idempotency_key, "original");
    }
}
