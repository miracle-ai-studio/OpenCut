//! Additive graphics execution plan; existing media/text/caption plans stay frozen.
use crate::caption_frame_plan::CaptionFramePlan;
use opencut_contracts::{graphic_frame::GraphicLayerPlan, text_frame::TextFrameIdentity};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicFramePlan {
    pub identity: TextFrameIdentity,
    pub execution: CaptionFramePlan,
    pub graphics: Vec<GraphicLayerPlan>,
    /// Complete original bottom-to-top order across all partitions.
    pub order: Vec<String>,
}
