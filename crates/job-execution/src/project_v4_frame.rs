//! Private persisted scene-frame preparation. No public native configuration.
use crate::project_v4_audio::TrustedJobAuthorityV1;
use opencut_contracts::{AssetLockManifest, Time, project_v4::ProjectSnapshotV4};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalVisualExecutionPolicyV1 {
    pub max_frame_pixels: u64,
    pub max_decoded_pixels: u64,
    pub max_gpu_bytes: u64,
    pub max_rgba_bytes: u64,
    pub max_encoded_bytes: u64,
    pub max_output_bytes: u64,
    pub max_rss_bytes: u64,
    pub deadline_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedV4FramePng {
    pub snapshot: ProjectSnapshotV4,
    pub sequence_id: String,
    pub time: Time,
    pub asset_lock: AssetLockManifest,
    pub authority: TrustedJobAuthorityV1,
    pub processing_policy: LocalVisualExecutionPolicyV1,
}
