//! A paused, fixed-head transform draft. Never creates a revision or job.
use crate::visual_preview_scene_v1::{VisualPreviewSceneFrameResultV1, VisualPreviewSceneFrameV1};
use opencut_contracts::project_v4_public::ApplyPublicV4Project;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualTransformDraftFrameV1 {
    pub draft_id: String,
    pub transaction: ApplyPublicV4Project,
    pub frame: VisualPreviewSceneFrameV1,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualTransformDraftFrameResultV1 {
    pub draft_id: String,
    pub transaction_digest: String,
    pub frame: VisualPreviewSceneFrameResultV1,
}
