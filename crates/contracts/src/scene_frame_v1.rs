//! Versioned full Classic visual stacks; previous narrow profiles stay frozen.
use crate::{
    blend_frame_v1::BoundBlendFrameV1,
    mask_frame_v1::MaskQuadV1,
    project_v4::{ColorV4, EffectV4, MaskV4},
    text_frame_v2::TextFrameIdentityV2,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneLayerStackV1 {
    pub element_id: String,
    pub masks: Vec<MaskV4>,
    pub effects: Vec<EffectV4>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneStepV1 {
    Layer {
        element_id: String,
    },
    Effect {
        element_id: String,
        effects: Vec<EffectV4>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneBackgroundV1 {
    Color {
        color: ColorV4,
    },
    MainTrackBlur {
        intensity: f64,
        source_element_ids: Vec<String>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundSceneLayerV1 {
    pub stack: SceneLayerStackV1,
    /// Double-precision local mask bounds; text rasters use a full-frame quad.
    pub quad: Option<MaskQuadV1>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundSceneFrameV1 {
    pub identity: TextFrameIdentityV2,
    pub execution: BoundBlendFrameV1,
    pub layers: Vec<BoundSceneLayerV1>,
    pub steps: Vec<SceneStepV1>,
    pub background: SceneBackgroundV1,
}

/// Native cmap proof belongs to this fixed mask owner and exact UTF-8 content.
/// Private font handles remain outside the plan, as in normal text frames.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneTextMaskFontV1 {
    pub mask_id: String,
    pub font: crate::text_frame_v2::ValidatedTextFontV2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SceneFontSynthesisOwnerV1 {
    Text,
    TextMask,
}

/// Explicit requested typography is separate from the unchanged native face.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneFontSynthesisV1 {
    pub element_id: String,
    pub owner_id: String,
    pub owner_kind: SceneFontSynthesisOwnerV1,
    pub font_id: String,
    pub binding: crate::font_synthesis_v1::FontSynthesisBindingV1,
}
