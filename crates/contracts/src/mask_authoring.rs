//! Read-only fixed-head logical bounds for Classic box-mask editing.
use crate::{Time, mask_frame::MaskQuad, scene::BoxMaskGeometry};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaskGeometryRequest {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub element_id: String,
    pub time: Time,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaskGeometryResult {
    pub revision: String,
    pub element_id: String,
    pub quad: MaskQuad,
    pub default_geometry: BoxMaskGeometry,
}
