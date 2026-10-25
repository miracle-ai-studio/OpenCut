//! Fixed-source render parameters, shared with the private durable codecs.
use opencut_contracts::Time;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrameRenderInput {
    pub idempotency_key: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub time: Time,
}
/// Half-open sample interval at the target sample rate; mastering precedes cropping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioSampleRange {
    pub start_sample: String,
    pub frame_count: String,
}
/// Half-open interval on the original scene frame grid, preserving animation clocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VideoFrameRange {
    pub start_frame: String,
    pub frame_count: String,
}
