//! Trusted private preparation; native paths never cross the public boundary.
use crate::project_v4_audio::{LocalAudioExecutionPolicyV1, TrustedJobAuthorityV1};
use crate::project_v4_frame::LocalVisualExecutionPolicyV1;
use opencut_contracts::{
    AssetLockManifest, project_v4::ProjectSnapshotV4, text_frame_v2::FontLockV2,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalVideoExecutionPolicyV1 {
    pub visual: LocalVisualExecutionPolicyV1,
    pub audio: LocalAudioExecutionPolicyV1,
    pub max_frames: u64,
    pub max_output_bytes: u64,
    pub deadline_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedV4VideoMp4V1 {
    pub snapshot: ProjectSnapshotV4,
    pub sequence_id: String,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLockV2>,
    pub authority: TrustedJobAuthorityV1,
    pub processing_policy: LocalVideoExecutionPolicyV1,
}
