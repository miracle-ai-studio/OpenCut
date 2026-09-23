//! Versioned event stream records shared by transports. Cursors are opaque strings.
use crate::job_views::JobChange;
use opencut_contracts::EditorError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventScope {
    Projects { project_id: Option<String> },
    Jobs { job_id: Option<String> },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventChangesInput {
    pub scope: EventScope,
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}
fn default_limit() -> usize {
    100
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EventChangesPage {
    pub events: Vec<EventChange>,
    pub next_cursor: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EventChange {
    pub cursor: String,
    pub event: EventData,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum EventData {
    Project(ProjectEvent),
    Job(JobChange),
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectEvent {
    pub event_id: String,
    pub project_id: String,
    pub revision: String,
    pub event_type: String,
    pub payload: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub enum StreamFormat {
    #[serde(rename = "opencut.events.v1")]
    V1,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct StreamRecord {
    pub format: StreamFormat,
    #[serde(flatten)]
    pub record: StreamRecordData,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamRecordData {
    Ready {
        scope: EventScope,
        cursor: Option<String>,
    },
    Event {
        cursor: String,
        event: EventData,
    },
    End {
        cursor: Option<String>,
        reason: StreamEnd,
    },
    Error {
        cursor: Option<String>,
        error: EditorError,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StreamEnd {
    CaughtUp,
    LimitReached,
}
impl StreamRecord {
    pub fn new(record: StreamRecordData) -> Self {
        Self {
            format: StreamFormat::V1,
            record,
        }
    }
}
pub fn schema() -> Value {
    serde_json::to_value(schemars::schema_for!(StreamRecord)).expect("serializable schema")
}
