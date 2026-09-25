//! Schema 3 history and branch results; legacy schema 2 tools retain their wire contracts.
use crate::{
    branching::BranchView,
    collaboration::{CollaborationConflict, Direction, PropertyChange},
};
use opencut_contracts::{
    TransactionResult, project_v3::ProjectDocumentV3, project_v3_ops::ApplyV3Project,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct V3CollaborationPlan {
    pub direction: Direction,
    pub expected_revision: String,
    pub target_transaction_id: String,
    pub undo_transaction_id: Option<String>,
    pub changes: Vec<PropertyChange>,
    pub conflicts: Vec<CollaborationConflict>,
    pub proposed_project: Option<ProjectDocumentV3>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V3BranchEditRequest {
    pub project_id: String,
    pub branch_name: String,
    pub request: ApplyV3Project,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct V3BranchComparePlan {
    pub project_id: String,
    pub source_branch: String,
    pub target_branch: String,
    pub base_revision: String,
    pub source_revision: String,
    pub target_revision: String,
    pub changes: Vec<PropertyChange>,
    pub conflicts: Vec<CollaborationConflict>,
    pub proposed_project: Option<ProjectDocumentV3>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct V3BranchMergeCommit {
    pub source_branch: String,
    pub target_branch: String,
    pub base_revision: String,
    pub source_revision: String,
    pub transaction: TransactionResult,
}

/// Branch refs are shared across schema versions; all v3 branch heads must be schema 3.
pub type V3BranchView = BranchView;
