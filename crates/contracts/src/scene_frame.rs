//! Versioned full Classic visual stacks; previous narrow profiles stay frozen.
use crate::{
    blend_frame::BoundBlendFrame,
    mask_frame::MaskQuad,
    scene::{Color, Mask, SceneEffect},
    text_frame::TextFrameIdentity,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneLayerStack {
    pub element_id: String,
    pub masks: Vec<Mask>,
    pub effects: Vec<SceneEffect>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneStep {
    Layer {
        element_id: String,
    },
    Effect {
        element_id: String,
        effects: Vec<SceneEffect>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneBackground {
    Color {
        color: Color,
    },
    MainTrackBlur {
        intensity: f64,
        source_element_ids: Vec<String>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundSceneLayer {
    pub stack: SceneLayerStack,
    /// Double-precision local mask bounds; text rasters use a full-frame quad.
    pub quad: Option<MaskQuad>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundSceneFrame {
    pub identity: TextFrameIdentity,
    pub execution: BoundBlendFrame,
    pub layers: Vec<BoundSceneLayer>,
    pub steps: Vec<SceneStep>,
    pub background: SceneBackground,
}

/// Native cmap proof belongs to this fixed mask owner and exact UTF-8 content.
/// Private font handles remain outside the plan, as in normal text frames.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneTextMaskFont {
    pub mask_id: String,
    pub font: crate::text_frame::ValidatedTextFont,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SceneFontSynthesisOwner {
    Text,
    TextMask,
}

/// Explicit requested typography is separate from the unchanged native face.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneFontSynthesis {
    pub element_id: String,
    pub owner_id: String,
    pub owner_kind: SceneFontSynthesisOwner,
    pub font_id: String,
    pub binding: crate::font_synthesis::FontSynthesisBinding,
}
