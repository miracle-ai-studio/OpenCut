//! Explicit project branches and bounded three-way merge contracts.
use crate::collaboration::{CollaborationConflict, PropertyChange};
use opencut_contracts::{ProjectDocument, TransactionRequest, TransactionResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BranchCreateRequest {
    pub project_id: String,
    pub branch_name: String,
    pub from_revision: String,
    pub expected_revision: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BranchListRequest {
    pub project_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BranchEditRequest {
    pub project_id: String,
    pub branch_name: String,
    pub request: TransactionRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BranchCompareRequest {
    pub project_id: String,
    pub source_branch: String,
    pub target_branch: String,
    pub expected_source_revision: String,
    pub expected_target_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BranchMergeRequest {
    pub project_id: String,
    pub source_branch: String,
    pub target_branch: String,
    pub expected_source_revision: String,
    pub expected_target_revision: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BranchView {
    pub project_id: String,
    pub branch_name: String,
    pub head_revision: String,
    pub created_from_revision: String,
    pub is_main: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BranchComparePlan {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BranchMergeCommit {
    pub source_branch: String,
    pub target_branch: String,
    pub base_revision: String,
    pub source_revision: String,
    pub transaction: TransactionResult,
}
