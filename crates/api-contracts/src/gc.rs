//! Explicit, owner-only CAS garbage collection with reviewed plans and durable receipts.
use crate::audit::Actor;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const MAX_OBJECTS: usize = 100;
pub const MAX_REFERENCE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_REFERENCE_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_REFERENCE_ROWS: usize = 100_000;
pub const MAX_NAMESPACE_ENTRIES: usize = 100_000;
pub const MAX_REFERENCE_MS: u64 = 5_000;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Object {
    Content { content_hash: String },
    Capture { capture_id: String },
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanInput {
    pub idempotency_key: String,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default = "default_limit")]
    #[schemars(range(min=1,max=MAX_OBJECTS))]
    pub limit: usize,
}
fn default_limit() -> usize {
    MAX_OBJECTS
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanId {
    pub plan_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecuteInput {
    pub plan_id: String,
    pub expected_plan_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    Planned,
    Removing,
    Removed,
    AlreadyAbsent,
    Retained,
    Changed,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Item {
    pub object: Object,
    pub size_bytes: String,
    pub state: ItemState,
    pub request_id: Option<String>,
    pub actor: Option<Actor>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanState {
    Planned,
    Executing,
    Complete,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Plan {
    pub plan_id: String,
    pub plan_hash: String,
    pub state: PlanState,
    pub origin_request_id: String,
    pub actor: Actor,
    pub examined: usize,
    pub protected: usize,
    pub next_cursor: Option<String>,
    pub items: Vec<Item>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Capabilities {
    pub plan_tool: String,
    pub execute_tool: String,
    pub inspect_tool: String,
    pub query_tool: String,
    pub available: bool,
    pub owner_only: bool,
    pub max_objects: usize,
    pub max_reference_bytes: usize,
    pub max_reference_document_bytes: usize,
    pub max_reference_rows: usize,
    pub max_namespace_entries: usize,
    pub reference_cooperative_time_limit_ms: u64,
    pub protects_all_revisions: bool,
    pub protects_all_jobs: bool,
    pub deletes_published_artifacts: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Query {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub include_complete: bool,
    #[serde(default = "query_limit")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: usize,
}
fn query_limit() -> usize {
    50
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Summary {
    pub plan_id: String,
    pub plan_hash: String,
    pub state: PlanState,
    pub origin_request_id: String,
    pub actor: Actor,
    pub object_count: usize,
    pub remaining: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Page {
    pub plans: Vec<Summary>,
    pub next_cursor: Option<String>,
    pub examined: usize,
}
