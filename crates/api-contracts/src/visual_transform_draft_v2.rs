//! Explicit Classic gesture semantics; older transform-only drafts stay strict.
use crate::visual_preview_scene_v1::VisualPreviewSceneFrameV1;
use opencut_contracts::{
    element_transform_v2::TransformGestureIntentV2, project_v4_public::ApplyPublicV4Project,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualTransformDraftFrameV2 {
    pub draft_id: String,
    pub gesture_intent: TransformGestureIntentV2,
    pub transaction: ApplyPublicV4Project,
    pub frame: VisualPreviewSceneFrameV1,
}
pub use crate::visual_transform_draft_v1::VisualTransformDraftFrameResultV1 as VisualTransformDraftFrameResultV2;
