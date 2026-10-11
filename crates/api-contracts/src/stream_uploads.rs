//! Streaming upload control; the complete digest is declared after the byte pass.
use crate::uploads::{UploadRetentionView, UploadState};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BeginUpload {
    pub project_id: String,
    pub idempotency_key: String,
    pub name: String,
    pub size_bytes: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompleteUpload {
    pub upload_id: String,
    pub content_hash: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StreamUploadView {
    pub upload_id: String,
    pub project_id: String,
    pub name: String,
    pub size_bytes: String,
    pub content_hash: Option<String>,
    pub next_offset: String,
    pub state: UploadState,
    pub job_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct StreamUploadEntry {
    pub upload: StreamUploadView,
    pub retention: UploadRetentionView,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct StreamUploadPage {
    pub uploads: Vec<StreamUploadEntry>,
    pub next_cursor: Option<String>,
}
