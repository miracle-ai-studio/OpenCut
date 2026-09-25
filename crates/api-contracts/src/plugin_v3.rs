//! Portable preset plans for schema 3, without changing planner-v1 contracts.
use opencut_contracts::project_v3_ops::{VisualOperation, VisualPlan};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetPlanV3 {
    pub package_sha256: String,
    pub spec_sha256: String,
    pub grant_revision: String,
    pub operations: Vec<VisualOperation>,
    pub plan: VisualPlan,
}
