//! Private persisted V4 audio input. Trusted native adapters supply authority.
use opencut_contracts::{AssetLockManifest, scene::EditorSnapshot};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TrustedJobAuthority {
    LocalOwner,
    Credential {
        principal_id: String,
        credential_id: String,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocalAudioProcessingProfile {
    LocalClassicAudioV1,
}
/// Host admission captured for this job; execution also checks current policy.
/// Native executable paths and enforced cgroup paths remain host configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalAudioExecutionPolicy {
    pub profile: LocalAudioProcessingProfile,
    pub max_pcm_bytes: u64,
    pub max_encoded_bytes: u64,
    pub max_rss_bytes: u64,
    pub deadline_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedAudioWav {
    pub snapshot: EditorSnapshot,
    pub sequence_id: String,
    pub sample_rate: u32,
    pub asset_lock: AssetLockManifest,
    pub authority: TrustedJobAuthority,
    pub processing_policy: LocalAudioExecutionPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_range: Option<PreparedAudioSampleRange>,
}

/// Private counterpart of the public range DTO, with no dependency on the
/// job-contracts crate. Old persisted whole-scene jobs deserialize to None.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedAudioSampleRange {
    pub start_sample: String,
    pub frame_count: String,
}
