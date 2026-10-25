//! Fixed-revision read samples; times use the selected channel's declared local
//! clock. No browser tangent solver or media/source clock enters this contract.
use crate::Time;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyframeSampleRequest {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub channel_id: String,
    pub times: Vec<Time>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyframeSample {
    pub time: Time,
    pub value: f64,
    pub physical_value: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyframeSampleResult {
    pub revision: String,
    pub channel_id: String,
    pub samples: Vec<KeyframeSample>,
}
