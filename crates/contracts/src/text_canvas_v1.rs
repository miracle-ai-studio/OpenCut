//! Read-only, fixed-revision canvas editing facts. This never authorizes a write
//! or exposes a resource locator; font bytes use the existing scoped read API.
use crate::{
    Time,
    text_frame_v2::{TextFrameIdentityV2, TextLayerPlanV2},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextCanvasPlanRequestV1 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub element_id: String,
    pub time: Time,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextCanvasPlanResultV1 {
    pub schema_version: u32,
    pub identity: TextFrameIdentityV2,
    pub layer: TextLayerPlanV2,
}
