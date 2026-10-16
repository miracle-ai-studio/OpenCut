//! Read-only fixed-head logical bounds for Classic box-mask editing.
use crate::{Time, mask_frame_v1::MaskQuadV1, project_v4::BoxMaskGeometryV4};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaskGeometryRequestV1 {
    pub project_id: String,
    pub expected_revision: String,
    pub sequence_id: String,
    pub element_id: String,
    pub time: Time,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaskGeometryResultV1 {
    pub schema_version: u32,
    pub revision: String,
    pub element_id: String,
    pub quad: MaskQuadV1,
    pub default_geometry: BoxMaskGeometryV4,
}
