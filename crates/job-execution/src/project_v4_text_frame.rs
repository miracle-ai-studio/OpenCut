//! Private fixed-revision complete frame preparation. No paths enter public DTOs.
use crate::{
    project_v4_audio::TrustedJobAuthorityV1, project_v4_frame::LocalVisualExecutionPolicyV1,
};
use opencut_contracts::{
    AssetLockManifest, Time, project_v4::ProjectSnapshotV4, text_frame_v2::FontLockV2,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreparedV4TextFramePng {
    pub snapshot: ProjectSnapshotV4,
    pub sequence_id: String,
    pub time: Time,
    pub asset_lock: AssetLockManifest,
    pub font_lock: Vec<FontLockV2>,
    pub authority: TrustedJobAuthorityV1,
    pub processing_policy: LocalVisualExecutionPolicyV1,
}
