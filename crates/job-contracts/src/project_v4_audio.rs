//! Additive, path-free fixed-revision Classic audio jobs.
use opencut_contracts::AssetLockManifest;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const V4_AUDIO_WAV_KIND: &str = "render.project_audio.v4";
pub const V4_AUDIO_RANGE_WAV_KIND: &str = "render.project_audio_range.v4";

/// Half-open sample interval at the request's target sample rate. No timeline
/// quantization or decoder seek is implied; mastering precedes this crop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4AudioSampleRangeV1 {
    pub start_sample: String,
    pub frame_count: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitV4AudioRangeWav {
    pub idempotency_key: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub sample_rate: u32,
    pub range: V4AudioSampleRangeV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4AudioRangeWavCapabilityV1 {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub sample_rates: Vec<u32>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitV4AudioWav {
    pub idempotency_key: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    /// Hertz; the application accepts only 44100 or 48000.
    pub sample_rate: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4AudioWavCapabilityV1 {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub sample_rates: Vec<u32>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum V4AudioProfileV1 {
    #[serde(rename = "classic-export-v31-web-audio")]
    ClassicExportV31WebAudio,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum V4AudioSampleFormatV1 {
    IeeeFloat32Le,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4AudioWavIdentityV1 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub profile: V4AudioProfileV1,
    pub sample_rate: u32,
    pub channels: u32,
    /// Exact output frame count, canonical unsigned decimal across JS boundaries.
    pub frame_count: String,
    pub sample_format: V4AudioSampleFormatV1,
}
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub enum V4AudioDecoderProfileV1 {
    #[serde(rename = "web-audio-decode-at-target-rate")]
    WebAudioDecodeAtTargetRate,
    #[serde(rename = "mediabunny-1.41.0-concatenate-offline-resample")]
    Mediabunny141ConcatenateOfflineResample,
}
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub enum V4AudioPitchProfileV1 {
    #[serde(rename = "unprepared-linear")]
    UnpreparedLinear,
    #[serde(rename = "prepared-linear")]
    PreparedLinear,
    #[serde(rename = "prepared-soundtouch")]
    PreparedSoundtouch,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum V4AudioMasteringProfileV1 {
    #[serde(rename = "classic-export-whole-scene-web-audio-v31")]
    ClassicExportWholeSceneWebAudioV31,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4AudioExecutionSummaryV1 {
    pub host_sha256: String,
    pub browser_version: String,
    pub browser_sha256: String,
    pub module_sha256: String,
    pub playwright_version: String,
    pub mediabunny_version: String,
    pub soundtouch_version: String,
    /// Unique profiles used by this fixed scene, not claims about unused engines.
    pub decoder_profiles: Vec<V4AudioDecoderProfileV1>,
    pub pitch_profiles: Vec<V4AudioPitchProfileV1>,
    pub mastering_profile: V4AudioMasteringProfileV1,
    pub mastering_applied: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4AudioArtifactMetadataV1 {
    pub mime_type: String,
    pub size_bytes: String,
    pub content_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4AudioWavResultV1 {
    pub schema_version: u32,
    pub identity: V4AudioWavIdentityV1,
    pub asset_lock: AssetLockManifest,
    /// SHA-256 of the complete fixed snapshot plus sequence, rate and profile.
    /// This is separate from the public submission/idempotency identity.
    pub render_input_sha256: String,
    pub execution: V4AudioExecutionSummaryV1,
    pub artifact: V4AudioArtifactMetadataV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4AudioRangeWavIdentityV1 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub profile: V4AudioProfileV1,
    pub sample_rate: u32,
    pub channels: u32,
    pub start_sample: String,
    pub frame_count: String,
    pub whole_scene_frame_count: String,
    pub sample_format: V4AudioSampleFormatV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V4AudioRangeWavResultV1 {
    pub schema_version: u32,
    pub identity: V4AudioRangeWavIdentityV1,
    pub asset_lock: AssetLockManifest,
    /// Complete fixed scene, execution profile and exact output sample range.
    pub render_input_sha256: String,
    pub execution: V4AudioExecutionSummaryV1,
    pub artifact: V4AudioArtifactMetadataV1,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn submission_cannot_supply_execution_authority_or_native_locations() {
        let input = serde_json::json!({"idempotency_key":"key", "project_id":"p", "revision":"r", "sequence_id":"s", "sample_rate":48000});
        assert!(serde_json::from_value::<SubmitV4AudioWav>(input.clone()).is_ok());
        for field in [
            "snapshot",
            "path",
            "authority",
            "processing_policy",
            "asset_lock",
        ] {
            let mut attempt = input.clone();
            attempt[field] = serde_json::json!("injected");
            assert!(serde_json::from_value::<SubmitV4AudioWav>(attempt).is_err());
        }
        let mut range = input;
        range["range"] = serde_json::json!({"start_sample":"17", "frame_count":"1024"});
        assert!(serde_json::from_value::<SubmitV4AudioRangeWav>(range.clone()).is_ok());
        assert!(serde_json::from_value::<SubmitV4AudioWav>(range.clone()).is_err());
        range["range"]["path"] = serde_json::json!("/injected");
        assert!(serde_json::from_value::<SubmitV4AudioRangeWav>(range).is_err());
    }
}
