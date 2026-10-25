//! Project-scoped upload control plane. Media bytes never enter these DTOs.
/// Exact file addressability shared with browser resources, not a media quota.
/// Actual writes obey volume headroom and the configured storage policy.
pub const MAX_SOURCE_BYTES: u64 = (1u64 << 53) - 1;
pub const MAX_CHUNK_BYTES: u64 = 4 * 1024 * 1024;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BeginUpload {
    pub project_id: String,
    pub idempotency_key: String,
    pub name: String,
    pub size_bytes: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompleteUpload {
    pub upload_id: String,
    pub content_hash: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UploadInput {
    pub upload_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UploadState {
    Receiving,
    Sealed,
    Aborted,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UploadView {
    pub upload_id: String,
    pub project_id: String,
    pub name: String,
    pub size_bytes: String,
    pub content_hash: Option<String>,
    pub next_offset: String,
    pub state: UploadState,
    pub job_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChunkDescriptor {
    pub upload_id: String,
    pub offset: String,
    pub size_bytes: String,
    pub content_hash: String,
}

pub use opencut_job_contracts::MediaImportJobResult;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectMediaInput {
    pub project_id: String,
    pub media_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectMediaQuery {
    pub project_id: String,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_page_size")]
    pub limit: usize,
}
fn default_page_size() -> usize {
    50
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectMediaPage {
    pub media: Vec<MediaImportJobResult>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UploadRetentionState {
    Retained,
    Released,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UploadRetentionView {
    pub upload_id: String,
    pub project_id: String,
    pub state: UploadRetentionState,
    pub reserved_bytes: String,
    /// Observation only. Release rechecks authorization and terminal state.
    pub release_eligible: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UploadQuotaView {
    pub project_id: String,
    pub project_reserved_bytes: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UploadQuery {
    pub project_id: String,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_page_size")]
    pub limit: usize,
    #[serde(default = "retained_only_default")]
    pub retained_only: bool,
}
fn retained_only_default() -> bool {
    true
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UploadEntry {
    pub upload: UploadView,
    pub retention: UploadRetentionView,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UploadPage {
    pub uploads: Vec<UploadEntry>,
    pub next_cursor: Option<String>,
}
