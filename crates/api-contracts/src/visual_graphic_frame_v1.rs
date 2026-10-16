//! Additive graphics execution plan; existing media/text/caption plans stay frozen.
use crate::visual_text_frame_v3::ClassicTextFramePlanV3;
use opencut_contracts::{graphic_frame_v1::GraphicLayerPlanV1, text_frame_v2::TextFrameIdentityV2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicGraphicFramePlanV1 {
    pub identity: TextFrameIdentityV2,
    pub execution: ClassicTextFramePlanV3,
    pub graphics: Vec<GraphicLayerPlanV1>,
    /// Complete original bottom-to-top order across all partitions.
    pub order: Vec<String>,
}
