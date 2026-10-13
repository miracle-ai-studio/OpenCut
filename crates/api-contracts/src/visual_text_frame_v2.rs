//! New media/free-text frame plan; the original media-only plan remains frozen.
use crate::visual_preview_v4::ClassicVisualPlan;
use opencut_contracts::text_frame_v2::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicTextFramePlanV2 {
    pub identity: TextFrameIdentityV2,
    pub media: ClassicVisualPlan,
    pub texts: Vec<TextLayerPlanV2>,
    pub order: Vec<String>,
}
