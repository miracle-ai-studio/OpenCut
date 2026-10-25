//! Additive fixed-revision jobs; legacy JobStartInput is unchanged.
use opencut_contracts::{Time, TimeRange};
use opencut_media_contracts::{Container, FrameAlignment, VideoCodec};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalVisualSubmitRender {
    pub idempotency_key: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub container: Container,
    pub video_codec: VideoCodec,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalVisualSubmitFrame {
    pub idempotency_key: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub requested_time: Time,
    pub alignment: FrameAlignment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalVisualSubmitRange {
    pub idempotency_key: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub requested_range: TimeRange,
    pub alignment: FrameAlignment,
}
