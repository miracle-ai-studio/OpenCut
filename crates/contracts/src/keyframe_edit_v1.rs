//! Additive authoring operations. V4 time/value handles encode Classic Y
//! overshoot; the frozen legacy normalized 0..1 curve contract is unchanged.
use crate::{
    Time,
    project_v4::{AnimationChannelV4, DiscreteKeyV4, ScalarKeyV4},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyframeEditRequestV1 {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub label: String,
    pub sequence_id: String,
    pub edits: Vec<KeyframeEditV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum KeyframeOwnerV1 {
    Element { element_id: String },
    Track { track_id: String },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyframeRefV1 {
    pub channel_id: String,
    pub key_id: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CurvePresetV1 {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum KeyframeEditV1 {
    ChannelPut {
        owner: KeyframeOwnerV1,
        channel: AnimationChannelV4,
    },
    /// All selected keys move by one exact offset; collisions reject the batch.
    Move {
        keys: Vec<KeyframeRefV1>,
        delta: Time,
    },
    Delete {
        keys: Vec<KeyframeRefV1>,
    },
    PutScalar {
        channel_id: String,
        key: ScalarKeyV4,
    },
    PutDiscrete {
        channel_id: String,
        key: DiscreteKeyV4,
    },
    /// Raw time/value tangents are generated in Rust. Flat segments require an
    /// explicit nonzero reference span instead of silently discarding the bump.
    Curve {
        channel_id: String,
        left_key_id: String,
        right_key_id: String,
        curve: CurvePresetV1,
        reference_span_value: Option<f64>,
    },
    /// Clipboard keys retain curve handles and extrapolation remains owned by
    /// the destination channel. Callers allocate fresh IDs once before retries.
    PasteScalar {
        channel_id: String,
        keys: Vec<ScalarKeyV4>,
        source_anchor: Time,
        destination_anchor: Time,
    },
    PasteDiscrete {
        channel_id: String,
        keys: Vec<DiscreteKeyV4>,
        source_anchor: Time,
        destination_anchor: Time,
    },
}
