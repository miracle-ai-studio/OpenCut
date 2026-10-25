//! Explicit project branches and bounded three-way merge contracts.
use crate::collaboration::{CollaborationConflict, PropertyChange};
pub use opencut_contracts::branching::*;
use opencut_contracts::{ProjectDocument, TransactionRequest};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalBranchEditRequest {
    pub project_id: String,
    pub branch_name: String,
    pub request: TransactionRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HistoricalBranchComparePlan {
    pub project_id: String,
    pub source_branch: String,
    pub target_branch: String,
    pub base_revision: String,
    pub source_revision: String,
    pub target_revision: String,
    pub changes: Vec<PropertyChange>,
    pub conflicts: Vec<CollaborationConflict>,
    pub proposed_project: Option<ProjectDocument>,
}
