//! Ephemeral preview control. Producer progress is never presentation evidence.
use opencut_contracts::Time;
use opencut_media_contracts::FrameAlignment;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Create {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub start_time: Time,
    pub alignment: FrameAlignment,
    pub audio_enabled: bool,
    pub idempotency_key: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionInput {
    pub session_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Acquire {
    pub session_id: String,
    pub expected_generation: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StreamInput {
    pub session_id: String,
    pub generation: String,
    pub owner_id: String,
    pub lease_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Control {
    pub session_id: String,
    pub generation: String,
    pub owner_id: String,
    pub lease_id: String,
    pub command: Command,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Play,
    Pause,
    Seek {
        sequence_time: Time,
        alignment: FrameAlignment,
    },
    Stop,
    Heartbeat,
    ReleaseOwner,
    ReportPresentation {
        telemetry: Telemetry,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Telemetry {
    pub sequence: String,
    pub presented_time: Time,
    pub media_clock_time: Time,
    pub playing: bool,
    pub stalled: bool,
    pub client_observed_at_ms: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Presentation {
    pub telemetry: Telemetry,
    pub server_received_monotonic_ms: String,
    pub staleness_ms: String,
    pub fresh: bool,
    pub client_timestamp_status: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Ready,
    Producing,
    Paused,
    Ended,
    Failed,
    Stopped,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Session {
    pub session_id: String,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub generation: String,
    pub state: State,
    pub actual_start: Time,
    pub duration: Time,
    pub owner_id: String,
    pub lease_id: String,
    pub lease_remaining_ms: String,
    pub expires_in_ms: String,
    pub presentation: Option<Presentation>,
    pub produced_through: Option<Time>,
    pub buffered_bytes: String,
    pub failure_code: Option<String>,
    /// The resume anchor is never inferred from encoded output progress.
    pub resume_position: Time,
    pub resume_position_confirmed: bool,
    pub next_event_cursor: String,
    pub restart_recoverable: bool,
    pub asset_locking: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventsInput {
    pub session_id: String,
    pub cursor: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Event {
    pub cursor: String,
    pub generation: String,
    pub kind: String,
    pub server_monotonic_ms: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Events {
    pub events: Vec<Event>,
    pub next_cursor: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Capabilities {
    pub backend: String,
    pub control_transports: Vec<String>,
    pub stream_path: String,
    pub restart_recoverable: bool,
    pub browser_presentation_verified: bool,
    pub max_sessions_per_process: usize,
    pub max_width: u32,
    pub max_height: u32,
    pub owner_lease_ms: u64,
    pub presentation_freshness_ms: u64,
}
