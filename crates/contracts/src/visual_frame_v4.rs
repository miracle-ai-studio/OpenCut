//! Pure Classic visual facts shared by application and versioned transports.
use crate::Time;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisualFrameIdentityV4 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub time: Time,
    pub width: u32,
    pub height: u32,
    pub profile: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VisualNoFrameReasonV4 {
    OutsideReadWindow,
    Eof,
    TimestampGap,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum DecodedVisualSourceV4 {
    #[serde(rename = "readyImage")]
    ReadyImage {
        #[serde(rename = "elementId")]
        element_id: String,
        width: u32,
        height: u32,
    },
    #[serde(rename = "readyVideo")]
    ReadyVideo {
        #[serde(rename = "elementId")]
        element_id: String,
        width: u32,
        height: u32,
        #[serde(rename = "requestedTime")]
        requested_time: Time,
        /// None is primary-track selection, not an invented FFmpeg stream index.
        #[serde(rename = "selectedVideoStream")]
        selected_video_stream: Option<u32>,
        #[serde(rename = "actualTimestampSeconds")]
        actual_timestamp_seconds: f64,
        #[serde(rename = "frameDurationSeconds")]
        frame_duration_seconds: f64,
    },
    #[serde(rename = "noFrame")]
    NoFrame {
        #[serde(rename = "elementId")]
        element_id: String,
        #[serde(rename = "requestedTime")]
        requested_time: Time,
        reason: VisualNoFrameReasonV4,
    },
}
impl DecodedVisualSourceV4 {
    pub fn element_id(&self) -> &str {
        match self {
            Self::ReadyImage { element_id, .. }
            | Self::ReadyVideo { element_id, .. }
            | Self::NoFrame { element_id, .. } => element_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisualQuadV4 {
    /// GPU uniform values match the Classic descriptor's f64→f32 conversion.
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
    /// Classic casts degrees to f32 before its radians conversion.
    pub rotation_radians: f32,
    pub flip_x: bool,
    pub flip_y: bool,
    /// Raw layer multiplier; UNORM render targets perform the later clamp.
    pub opacity: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundVisualLayerV4 {
    pub element_id: String,
    pub asset_id: String,
    /// NoFrame or a zero-sized authored transform draws no quad.
    pub quad: Option<VisualQuadV4>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundVisualFrameV4 {
    pub identity: VisualFrameIdentityV4,
    /// Original compositor clear, followed by one normal background color layer.
    pub clear: [f32; 4],
    pub background: [f32; 4],
    pub layers: Vec<BoundVisualLayerV4>,
    pub decoded: Vec<DecodedVisualSourceV4>,
}
