//! Additive authoring operations. V4 time/value handles encode Classic Y
//! overshoot; the frozen legacy normalized 0..1 curve contract is unchanged.
use crate::{
    Time,
    scene::{AnimationChannel, DiscreteKey, ScalarKey},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyframeEditRequest {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub label: String,
    pub sequence_id: String,
    pub edits: Vec<KeyframeEdit>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum KeyframeOwner {
    Element { element_id: String },
    Track { track_id: String },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyframeRef {
    pub channel_id: String,
    pub key_id: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CurvePreset {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum KeyframeEdit {
    ChannelPut {
        owner: KeyframeOwner,
        channel: AnimationChannel,
    },
    /// All selected keys move by one exact offset; collisions reject the batch.
    Move {
        keys: Vec<KeyframeRef>,
        delta: Time,
    },
    Delete {
        keys: Vec<KeyframeRef>,
    },
    PutScalar {
        channel_id: String,
        key: ScalarKey,
    },
    /// Sample and split the current scalar span in Rust. Pointer insertion never
    /// reuses stale browser samples or invalidates neighboring time handles.
    InsertScalar {
        channel_id: String,
        key_id: String,
        time: Time,
    },
    PutDiscrete {
        channel_id: String,
        key: DiscreteKey,
    },
    /// Raw time/value tangents are generated in Rust. Flat segments require an
    /// explicit nonzero reference span instead of silently discarding the bump.
    Curve {
        channel_id: String,
        left_key_id: String,
        right_key_id: String,
        curve: CurvePreset,
        reference_span_value: Option<f64>,
    },
    /// Clipboard keys retain curve handles and extrapolation remains owned by
    /// the destination channel. Callers allocate fresh IDs once before retries.
    PasteScalar {
        channel_id: String,
        keys: Vec<ScalarKey>,
        source_anchor: Time,
        destination_anchor: Time,
    },
    PasteDiscrete {
        channel_id: String,
        keys: Vec<DiscreteKey>,
        source_anchor: Time,
        destination_anchor: Time,
    },
}
