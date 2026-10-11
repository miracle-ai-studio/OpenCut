//! Portable application use cases and ports; native storage and host I/O are adapters.
pub mod access;
pub mod access_audit;
pub mod analysis;
pub mod analysis_evidence;
pub mod analysis_query;
pub mod artifact_gc;
pub mod artifact_retention;
pub mod audit;
pub mod branching;
pub mod caption_generation;
pub mod classic_import;
pub mod collaboration;
pub mod events;
pub mod history_navigation;
pub mod identity;
pub mod job_views;
pub mod jobs;
pub mod lifecycle;
pub mod maintenance;
pub mod media;
pub mod media_library;
pub mod plugin_v2;
pub mod plugins;
pub mod preview;
pub mod project_edit;
pub mod project_library;
pub mod project_package;
pub mod project_package_v3;
pub mod project_v3;
pub mod project_v3_collaboration;
pub mod project_v3_jobs;
pub mod projects;
pub mod queries;
pub mod render;
pub mod render_v3;
pub mod resources;
pub mod speaker_turns;
pub mod subtitle_exchange;
pub mod timeline_edit;
use identity::content_id;
pub use opencut_api_contracts::{
    JobEvent, JobProgress, JobRecord, JobState, RestoreRevisionRequest, RevisionPage,
    RevisionSummary, StoredEvent,
};
use opencut_contracts::EditorError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobRequest {
    pub idempotency_key: String,
    pub kind: String,
    pub input: Value,
}

fn encode_json<T: Serialize>(value: &T, description: &str) -> Result<String, EditorError> {
    serde_json::to_string(value).map_err(|error| {
        EditorError::new(
            "SERIALIZATION_ERROR",
            format!("could not encode {description}: {error}"),
        )
    })
}

fn require_idempotency_key(key: &str) -> Result<(), EditorError> {
    if key.trim().is_empty() {
        Err(EditorError::new(
            "INVALID_ARGUMENT",
            "idempotency key must not be empty",
        ))
    } else {
        Ok(())
    }
}

fn revision_conflict(expected: &str, actual: &str) -> EditorError {
    EditorError {
        code: "REVISION_CONFLICT".into(),
        message: "project has changed; refresh the snapshot and re-plan".into(),
        details: json!({
            "expected_revision": expected,
            "actual_revision": actual,
            "recovery_action": "refresh_and_replan"
        }),
    }
}

pub mod uploads;

pub mod project_plugins;

pub mod denials;

pub mod storage;

pub mod gc;

pub mod budget;

pub mod database;

pub mod plugin_v3;
pub mod reproducibility;

pub mod tool_execution;

pub mod workspace_port;
