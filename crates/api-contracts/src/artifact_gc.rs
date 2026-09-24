//! Owner-reviewed observation plans for published artifact collection.
//! A plan is durable but never authorizes deletion without a fresh execution check.
use crate::{artifact_retention::ReferenceRoot, audit::Actor};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanInput {
    pub artifact_id: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanId {
    pub plan_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Query {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Summary {
    pub plan_id: String,
    pub plan_hash: String,
    pub artifact_id: String,
    pub observed_at: String,
    pub default_retained: bool,
    pub root_count: String,
    pub origin_request_id: String,
    pub actor: Actor,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Page {
    pub plans: Vec<Summary>,
    pub next_cursor: Option<String>,
    pub examined: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Plan {
    pub plan_id: String,
    pub plan_hash: String,
    pub artifact_id: String,
    pub size_bytes: String,
    pub observed_at: String,
    pub default_retained: bool,
    pub roots: Vec<ReferenceRoot>,
    pub root_count: String,
    pub roots_truncated: bool,
    pub native_alias_root_count: String,
    pub examined_rows: String,
    pub native_paths_examined: String,
    pub origin_request_id: String,
    pub actor: Actor,
    pub execution_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Capabilities {
    pub references_tool: String,
    pub plan_tool: String,
    pub inspect_tool: String,
    pub query_tool: String,
    pub plan_available: bool,
    pub execute_available: bool,
    pub owner_only: bool,
    pub max_reference_rows: usize,
    pub max_reference_bytes: usize,
}
