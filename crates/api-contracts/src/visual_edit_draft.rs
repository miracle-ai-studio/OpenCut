//! One ephemeral paused frame for an explicit gesture or static visual property.
use crate::visual_preview::VisualPreviewFrame;
use opencut_contracts::{project_public::ApplyProject, visual_edit_draft::VisualEditDraftIntent};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualEditDraftFrame {
    pub draft_id: String,
    pub intent: VisualEditDraftIntent,
    pub transaction: ApplyProject,
    pub frame: VisualPreviewFrame,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualEditDraftFrameResult {
    pub draft_id: String,
    pub transaction_digest: String,
    pub frame: crate::visual_preview::VisualPreviewFrameResult,
}
