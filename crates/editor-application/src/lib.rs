//! Portable application use cases and ports; native storage and host I/O are adapters.
pub mod access;
pub mod access_audit;
pub mod artifact_retention;
pub mod audit;
pub mod events;
pub mod identity;
pub mod job_views;
pub mod jobs;
pub mod lifecycle;
pub mod maintenance;
pub mod media;
pub mod plugins;
pub mod projects;
pub mod queries;
pub mod render;
pub mod resources;
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
