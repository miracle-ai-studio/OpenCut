//! One additive visual stack profile over the frozen raster/decode/blend plans.
use crate::visual_blend_frame_v1::ClassicBlendFramePlanV1;
use opencut_contracts::{scene_frame_v1::*, text_frame_v2::TextFrameIdentityV2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicSceneFramePlanV1 {
    pub identity: TextFrameIdentityV2,
    pub execution: ClassicBlendFramePlanV1,
    pub stacks: Vec<SceneLayerStackV1>,
    pub mask_fonts: Vec<SceneTextMaskFontV1>,
    pub font_synthesis: Vec<SceneFontSynthesisV1>,
    pub steps: Vec<SceneStepV1>,
    pub background: SceneBackgroundV1,
}
