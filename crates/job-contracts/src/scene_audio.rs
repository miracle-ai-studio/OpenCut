//! Additive, path-free fixed-revision Classic audio jobs.
use opencut_contracts::AssetLockManifest;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const AUDIO_WAV_KIND: &str = "render.project_audio.v4";
pub const AUDIO_RANGE_WAV_KIND: &str = "render.project_audio_range.v4";

/// Half-open sample interval at the request's target sample rate. No timeline
/// quantization or decoder seek is implied; mastering precedes this crop.
pub use crate::render_requests::AudioSampleRange;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitAudioRangeWav {
    pub idempotency_key: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub sample_rate: u32,
    pub range: AudioSampleRange,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioRangeWavCapability {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub sample_rates: Vec<u32>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitAudioWav {
    pub idempotency_key: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    /// Hertz; the application accepts only 44100 or 48000.
    pub sample_rate: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioWavCapability {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub sample_rates: Vec<u32>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AudioProfile {
    #[serde(rename = "classic-export-v31-web-audio")]
    ClassicExportV31WebAudio,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudioSampleFormat {
    IeeeFloat32Le,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioWavIdentity {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub profile: AudioProfile,
    pub sample_rate: u32,
    pub channels: u32,
    /// Exact output frame count, canonical unsigned decimal across JS boundaries.
    pub frame_count: String,
    pub sample_format: AudioSampleFormat,
}
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub enum AudioDecoderProfile {
    #[serde(rename = "web-audio-decode-at-target-rate")]
    WebAudioDecodeAtTargetRate,
    #[serde(rename = "mediabunny-1.41.0-concatenate-offline-resample")]
    Mediabunny141ConcatenateOfflineResample,
}
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub enum AudioPitchProfile {
    #[serde(rename = "unprepared-linear")]
    UnpreparedLinear,
    #[serde(rename = "prepared-linear")]
    PreparedLinear,
    #[serde(rename = "prepared-soundtouch")]
    PreparedSoundtouch,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AudioMasteringProfile {
    #[serde(rename = "classic-export-whole-scene-web-audio-v31")]
    ClassicExportWholeSceneWebAudioV31,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioExecutionSummary {
    pub host_sha256: String,
    pub browser_version: String,
    pub browser_sha256: String,
    pub module_sha256: String,
    pub playwright_version: String,
    pub mediabunny_version: String,
    pub soundtouch_version: String,
    /// Unique profiles used by this fixed scene, not claims about unused engines.
    pub decoder_profiles: Vec<AudioDecoderProfile>,
    pub pitch_profiles: Vec<AudioPitchProfile>,
    pub mastering_profile: AudioMasteringProfile,
    pub mastering_applied: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioArtifactMetadata {
    pub mime_type: String,
    pub size_bytes: String,
    pub content_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioWavResult {
    pub schema_version: u32,
    pub identity: AudioWavIdentity,
    pub asset_lock: AssetLockManifest,
    /// SHA-256 of the complete fixed snapshot plus sequence, rate and profile.
    /// This is separate from the public submission/idempotency identity.
    pub render_input_sha256: String,
    pub execution: AudioExecutionSummary,
    pub artifact: AudioArtifactMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioRangeWavIdentity {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub profile: AudioProfile,
    pub sample_rate: u32,
    pub channels: u32,
    pub start_sample: String,
    pub frame_count: String,
    pub whole_scene_frame_count: String,
    pub sample_format: AudioSampleFormat,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioRangeWavResult {
    pub schema_version: u32,
    pub identity: AudioRangeWavIdentity,
    pub asset_lock: AssetLockManifest,
    /// Complete fixed scene, execution profile and exact output sample range.
    pub render_input_sha256: String,
    pub execution: AudioExecutionSummary,
    pub artifact: AudioArtifactMetadata,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn submission_cannot_supply_execution_authority_or_native_locations() {
        let input = serde_json::json!({"idempotency_key":"key", "project_id":"p", "revision":"r", "sequence_id":"s", "sample_rate":48000});
        assert!(serde_json::from_value::<SubmitAudioWav>(input.clone()).is_ok());
        for field in [
            "snapshot",
            "path",
            "authority",
            "processing_policy",
            "asset_lock",
        ] {
            let mut attempt = input.clone();
            attempt[field] = serde_json::json!("injected");
            assert!(serde_json::from_value::<SubmitAudioWav>(attempt).is_err());
        }
        let mut range = input;
        range["range"] = serde_json::json!({"start_sample":"17", "frame_count":"1024"});
        assert!(serde_json::from_value::<SubmitAudioRangeWav>(range.clone()).is_ok());
        assert!(serde_json::from_value::<SubmitAudioWav>(range.clone()).is_err());
        range["range"]["path"] = serde_json::json!("/injected");
        assert!(serde_json::from_value::<SubmitAudioRangeWav>(range).is_err());
    }
}
