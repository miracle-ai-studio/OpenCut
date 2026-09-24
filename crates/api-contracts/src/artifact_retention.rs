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
