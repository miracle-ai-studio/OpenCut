//! Private execution and journal records shared by application ports and native stores.
use crate::JobState;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StoredEvent {
    pub sequence: i64,
    pub event_id: String,
    pub project_id: String,
    pub revision: String,
    pub event_type: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct JobRecord {
    pub job_id: String,
    pub state: JobState,
    pub attempt: u32,
    pub kind: String,
    pub input: Value,
    pub result: Option<Value>,
    pub error: Option<Value>,
    pub lease_owner: Option<String>,
    pub lease_until: Option<i64>,
    pub cancel_requested: bool,
    #[serde(default)]
    pub progress: Option<JobProgress>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobProgress {
    pub stage: String,
    pub fraction: Option<f64>,
    pub media_time: Option<opencut_contracts::Time>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct JobEvent {
    pub sequence: i64,
    pub event_id: String,
    pub job_id: String,
    pub event_type: String,
    pub state: JobState,
    pub attempt: u32,
    pub payload: Value,
    pub created_at: String,
}
