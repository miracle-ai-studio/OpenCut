//! Additive final blend plan. Existing normal execution plans remain frozen.
use crate::visual_graphic_frame_v1::ClassicGraphicFramePlanV1;
use opencut_contracts::{blend_frame_v1::LayerBlendV1, text_frame_v2::TextFrameIdentityV2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicBlendFramePlanV1 {
    pub identity: TextFrameIdentityV2,
    pub execution: ClassicGraphicFramePlanV1,
    pub blends: Vec<LayerBlendV1>,
}
