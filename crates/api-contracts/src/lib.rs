//! Version 1 wire contracts; no database, native paths, codecs, or plugin runtime.
pub mod access;
pub mod analysis_evidence;
pub mod analysis_query;
pub mod artifact_gc;
pub mod artifact_retention;
pub mod audio_preview_v4;
pub mod audit;
pub mod branching;
pub mod capabilities;
pub mod classic_import;
pub mod collaboration;
pub mod context;
pub mod edit_control;
pub mod edit_recovery_v1;
pub mod edit_recovery_v2;
pub mod event_stream;
pub mod history_navigation;
pub mod job_views;
pub mod maintenance;
pub mod media;
pub mod media_library;
pub mod plugin_publication;
pub mod plugin_v2;
pub mod plugins;
pub mod preview;
pub mod project_library;
pub mod project_package;
pub mod project_package_v3;
pub mod project_v3_collaboration;
pub mod project_v3_jobs;
pub mod project_v4_audio;
pub mod project_v4_frame;
pub mod render;
pub mod stream_uploads;
pub mod visual_preview_v4;
pub use opencut_contracts::source_art_v1;
pub use opencut_contracts::source_art_waveform_display_v1;
pub mod subtitle_exchange;
pub mod timeline_clipboard_v4;
pub mod timeline_v4;
pub mod tools;
use opencut_contracts::{CreateProject, EditorError, TransactionRequest};
pub use opencut_job_contracts as jobs;
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
#[serde(deny_unknown_fields)]
pub struct RestoreRevisionRequest {
    pub expected_revision: String,
    pub target_revision: String,
    pub idempotency_key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RevisionSummary {
    pub revision: String,
    pub parent_revision: Option<String>,
    pub transaction_id: Option<String>,
    pub label: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RevisionPage {
    pub project_id: String,
    pub head_revision: String,
    pub revisions: Vec<RevisionSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectCreateInput {
    pub project: CreateProject,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectInput {
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryInput {
    pub project_id: String,
    pub from_revision: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevisionInput {
    pub project_id: String,
    pub revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RestoreInput {
    pub project_id: String,
    pub request: RestoreRevisionRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditInput {
    pub project_id: String,
    pub request: TransactionRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventsInput {
    #[serde(default)]
    pub after_sequence: i64,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobEventsInput {
    #[serde(default)]
    pub after_sequence: i64,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub job_id: Option<String>,
}

fn default_limit() -> usize {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmptyInput {}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Response<T = Value> {
    pub api_version: String,
    pub request_id: String,
    pub data: Option<T>,
    pub error: Option<EditorError>,
    pub warnings: Vec<String>,
}

impl Response {
    pub fn from_result_with_id(request_id: String, result: Result<Value, EditorError>) -> Self {
        let (data, error) = match result {
            Ok(data) => (Some(data), None),
            Err(error) => (None, Some(error)),
        };
        Self {
            api_version: "1".into(),
            request_id,
            data,
            error,
            warnings: vec![],
        }
    }
}

pub mod uploads;

pub mod project_plugins;

pub mod denials;
pub mod domains;

pub mod storage;

pub mod gc;

pub mod budget;

pub mod database;

pub mod plugin_v3;
pub mod reproducibility;

pub mod execution_receipt;
