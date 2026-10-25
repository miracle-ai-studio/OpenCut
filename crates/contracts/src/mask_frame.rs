//! Fixed Classic box-mask facts. Original media/text/graphic/blend wires stay frozen.
use crate::{blend_frame::BoundBlendFrame, scene::Mask, text_frame::TextFrameIdentity};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayerMask {
    pub element_id: String,
    pub mask: Option<Mask>,
}

/// Canvas geometry retains Classic double precision before local pixel rounding.
/// The existing source/GPU quad stays f32 under its original frozen contract.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaskQuad {
    pub center_x: f64,
    pub center_y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation_radians: f64,
    pub flip_x: bool,
    pub flip_y: bool,
    pub opacity: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundLayerMask {
    pub element_id: String,
    pub mask: Option<Mask>,
    /// Logical cropped element bounds, excluding graphic stroke padding.
    pub quad: Option<MaskQuad>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundMaskFrame {
    pub identity: TextFrameIdentity,
    pub execution: BoundBlendFrame,
    pub masks: Vec<BoundLayerMask>,
}
