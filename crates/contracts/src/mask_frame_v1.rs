//! Fixed Classic box-mask facts. Original media/text/graphic/blend wires stay frozen.
use crate::{
    blend_frame_v1::BoundBlendFrameV1, project_v4::MaskV4, text_frame_v2::TextFrameIdentityV2,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayerMaskV1 {
    pub element_id: String,
    pub mask: Option<MaskV4>,
}

/// Canvas geometry retains Classic double precision before local pixel rounding.
/// The existing source/GPU quad stays f32 under its original frozen contract.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaskQuadV1 {
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
pub struct BoundLayerMaskV1 {
    pub element_id: String,
    pub mask: Option<MaskV4>,
    /// Logical cropped element bounds, excluding graphic stroke padding.
    pub quad: Option<MaskQuadV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundMaskFrameV1 {
    pub identity: TextFrameIdentityV2,
    pub execution: BoundBlendFrameV1,
    pub masks: Vec<BoundLayerMaskV1>,
}
