//! Additive fixed-revision property intent. Omitted fields preserve authored
//! values and all animation channels; no prepared paths cross this boundary.
use crate::project_v4::{GainV4, RetimeV4, TransformV4};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementPropertiesRequestV1 {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub label: String,
    pub sequence_id: String,
    pub element_id: String,
    pub transform: Option<TransformV4>,
    pub opacity: Option<f64>,
    pub audio_gain: Option<GainV4>,
    pub muted: Option<bool>,
    pub retime: Option<RetimeV4>,
}
