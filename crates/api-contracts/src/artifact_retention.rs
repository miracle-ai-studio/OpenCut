//! Owner-managed retention of a published task artifact. Release is not deletion.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectInput {
    pub artifact_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleaseInput {
    pub artifact_id: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReferencesInput {
    pub artifact_id: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    Revision,
    JobInput,
    JobResult,
    ProjectMedia,
    PluginPublication,
    ProjectPluginGrant,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct ReferenceRoot {
    pub kind: ReferenceKind,
    pub resource_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReferencesView {
    pub artifact_id: String,
    pub default_retained: bool,
    pub roots: Vec<ReferenceRoot>,
    pub root_count: String,
    pub roots_truncated: bool,
    pub examined_rows: String,
    pub native_source_candidates: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RetentionState {
    Retained,
    Released,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct View {
    pub artifact_id: String,
    pub job_id: String,
    pub content_hash: String,
    pub size_bytes: String,
    pub state: RetentionState,
    pub released_at: Option<String>,
    pub release_request_id: Option<String>,
}
