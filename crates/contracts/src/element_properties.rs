//! Additive fixed-revision property intent. Omitted fields preserve authored
//! values and all animation channels; no prepared paths cross this boundary.
use crate::scene::{Gain, Retime, Transform};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementPropertiesRequest {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub label: String,
    pub sequence_id: String,
    pub element_id: String,
    pub transform: Option<Transform>,
    pub opacity: Option<f64>,
    pub audio_gain: Option<Gain>,
    pub muted: Option<bool>,
    pub retime: Option<Retime>,
}
