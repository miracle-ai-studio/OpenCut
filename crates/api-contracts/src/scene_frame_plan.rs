//! One additive visual stack profile over the frozen raster/decode/blend plans.
use crate::blend_frame_plan::BlendFramePlan;
use opencut_contracts::{scene_frame::*, text_frame::TextFrameIdentity};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneFramePlan {
    pub identity: TextFrameIdentity,
    pub execution: BlendFramePlan,
    pub stacks: Vec<SceneLayerStack>,
    pub mask_fonts: Vec<SceneTextMaskFont>,
    pub font_synthesis: Vec<SceneFontSynthesis>,
    pub steps: Vec<SceneStep>,
    pub background: SceneBackground,
}
