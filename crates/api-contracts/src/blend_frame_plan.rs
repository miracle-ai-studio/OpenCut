//! Additive final blend plan. Existing normal execution plans remain frozen.
use crate::graphic_frame_plan::GraphicFramePlan;
use opencut_contracts::{blend_frame::LayerBlend, text_frame::TextFrameIdentity};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlendFramePlan {
    pub identity: TextFrameIdentity,
    pub execution: GraphicFramePlan,
    pub blends: Vec<LayerBlend>,
}
