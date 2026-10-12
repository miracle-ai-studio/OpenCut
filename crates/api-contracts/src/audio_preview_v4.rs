//! Ephemeral fixed-revision audio playback. Counts are canonical decimal strings;
//! producer progress is never evidence of presentation or audible output.
use opencut_contracts::Time;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AudioPreviewProfileV4 {
    ClassicRealtimeV31,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewIdentityV4 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub profile: AudioPreviewProfileV4,
    pub sample_rate: u32,
    pub channels: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewCreateV4 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub sample_rate: u32,
    /// Nonnegative presentation time, floored once to the output sample grid.
    pub start_time: Time,
    pub idempotency_key: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewInspectV4 {
    pub session_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewAcquireV4 {
    pub session_id: String,
    pub expected_generation: String,
    pub control_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewStreamV4 {
    pub session_id: String,
    pub generation: String,
    pub overlay_epoch: String,
    pub owner_id: String,
    pub lease_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporaryAudioGainV4 {
    pub track_id: String,
    pub element_id: String,
    /// Static element dB only: -600..200 tenths. Mute/source/track are unchanged.
    pub decibels_tenths: i32,
    pub gesture_id: String,
    pub update_sequence: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewControlV4 {
    pub session_id: String,
    pub generation: String,
    pub owner_id: String,
    pub lease_id: String,
    pub control_id: String,
    pub command: AudioPreviewCommandV4,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioPreviewCommandV4 {
    Play,
    Pause,
    Seek {
        sequence_time: Time,
    },
    Stop,
    Heartbeat,
    ReleaseOwner,
    SetTemporaryGain {
        gain: TemporaryAudioGainV4,
    },
    ClearTemporaryGain {
        gesture_id: String,
    },
    ReportConsumed {
        sequence: String,
        /// Cumulative actually consumed frames from this generation's anchor.
        consumed_frames: String,
        playing: bool,
        stalled: bool,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudioPreviewStateV4 {
    Ready,
    Playing,
    Paused,
    Ended,
    Failed,
    Stopped,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewSessionV4 {
    pub schema_version: u32,
    pub session_id: String,
    pub identity: AudioPreviewIdentityV4,
    pub generation: String,
    pub overlay_epoch: String,
    pub owner_id: String,
    pub lease_id: String,
    pub state: AudioPreviewStateV4,
    pub actual_start_sample: String,
    pub total_frames: String,
    /// Server mapping of the last confirmed consumer frame count to Time.
    pub authoritative_time: Time,
    pub consumed_frames: String,
    pub produced_through_sample: String,
    pub buffered_frames: String,
    pub presentation_fresh: bool,
    pub stalled: bool,
    pub lease_remaining_ms: String,
    pub expires_in_ms: String,
    pub temporary_gain: Option<TemporaryAudioGainV4>,
    pub failure_code: Option<String>,
    pub next_event_cursor: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewEventsInputV4 {
    pub session_id: String,
    pub cursor: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewEventV4 {
    pub cursor: String,
    pub generation: String,
    pub overlay_epoch: String,
    pub kind: String,
    pub server_monotonic_ms: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewEventsV4 {
    pub events: Vec<AudioPreviewEventV4>,
    pub next_cursor: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewCapabilityV4 {
    pub schema_version: u32,
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub profile: AudioPreviewProfileV4,
    pub sample_rates: Vec<u32>,
    pub stream_path: String,
    pub max_sessions_per_database: u32,
    pub max_block_frames: u32,
    pub max_buffered_frames: u32,
    pub owner_lease_ms: u32,
    pub authorization_heartbeat_ms: u32,
    pub session_ttl_ms: u32,
}
/// One header precedes one interleaved stereo Float32 little-endian payload.
/// The transport framing is versioned independently from project schemas.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioPreviewPcmHeaderV4 {
    pub version: u32,
    pub session_id: String,
    pub identity: AudioPreviewIdentityV4,
    pub generation: String,
    pub overlay_epoch: String,
    pub sequence: String,
    pub start_sample: String,
    pub frame_count: u32,
    pub content_hash: String,
}
/// Binary transport types come from these same reviewed Rust declarations.
pub fn stream_schemas() -> serde_json::Value {
    serde_json::json!({
        "input": schemars::schema_for!(AudioPreviewStreamV4),
        "header": schemars::schema_for!(AudioPreviewPcmHeaderV4),
    })
}
