//! Project-scoped planner contracts. Installation and global grants remain owner-only.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PluginTarget {
    pub project_id: String,
    pub id: String,
    pub version: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetGrant {
    pub target: PluginTarget,
    pub package_sha256: String,
    pub operations: BTreeSet<String>,
    pub enabled: bool,
    pub expected_revision: String,
    pub idempotency_key: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Grant {
    pub target: PluginTarget,
    pub package_sha256: String,
    pub operations: BTreeSet<String>,
    pub enabled: bool,
    pub revision: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectInput {
    pub target: PluginTarget,
    pub operation_name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OperationView {
    pub grant: Grant,
    pub operation_name: String,
    pub operation_version: u32,
    pub input_schema: Value,
    pub output_schema: Value,
    pub limits: crate::plugins::PluginLimits,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanProjectPlugin {
    pub plugin: crate::plugins::PlanInput,
    pub idempotency_key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_name: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectPluginPlan {
    pub grant: Grant,
    pub transaction: opencut_contracts::project_public::ApplyProject,
    pub preview: opencut_contracts::project_public::PublicProjectDocument,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Capabilities {
    pub grant_tool: String,
    pub list_tool: String,
    pub inspect_tool: String,
    pub plan_tool: String,
    pub max_grants_per_project: usize,
    pub max_operations_per_grant: usize,
    pub max_parameter_bytes: usize,
    pub max_snapshot_bytes: usize,
    pub max_schema_bytes: usize,
    pub max_component_bytes: usize,
    pub max_concurrent_per_process: usize,
    pub ceilings: crate::plugins::PluginLimits,
}
