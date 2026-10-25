//! New media/free-text frame plan; the original media-only plan remains frozen.
use crate::visual_preview::ClassicVisualPlan;
use opencut_contracts::text_frame::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextFramePlan {
    pub identity: TextFrameIdentity,
    pub media: ClassicVisualPlan,
    pub texts: Vec<TextLayerPlan>,
    pub order: Vec<String>,
}
