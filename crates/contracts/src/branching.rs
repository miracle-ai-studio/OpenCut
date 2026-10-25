//! Branch references and complete-model merge plans. Main and named edits
//! share the ordinary public transaction kernel;there is no branch edit ABI.
use crate::{
    AssetVersion, TransactionResult,
    project_history::{HistoryChange, HistoryConflict},
    project_public::PublicProjectDocument,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BranchCompareRequest {
    pub project_id: String,
    pub source_branch: String,
    pub target_branch: String,
    pub expected_source_revision: String,
    pub expected_target_revision: String,
    /// Explicit observations for an unpinned historical merge base only.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub base_asset_versions: BTreeMap<String, AssetVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BranchMergeRequest {
    pub project_id: String,
    pub source_branch: String,
    pub target_branch: String,
    pub expected_source_revision: String,
    pub expected_target_revision: String,
    /// Explicit observations for an unpinned historical merge base only.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub base_asset_versions: BTreeMap<String, AssetVersion>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BranchView {
    pub project_id: String,
    pub branch_name: String,
    pub head_revision: String,
    pub created_from_revision: String,
    pub is_main: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BranchMergeCommit {
    pub source_branch: String,
    pub target_branch: String,
    pub base_revision: String,
    pub source_revision: String,
    pub transaction: TransactionResult,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BranchComparePlan {
    pub project_id: String,
    pub source_branch: String,
    pub target_branch: String,
    pub base_revision: String,
    pub source_revision: String,
    pub target_revision: String,
    pub changes: Vec<HistoryChange>,
    pub conflicts: Vec<HistoryConflict>,
    pub proposed_project: Option<PublicProjectDocument>,
}
/// Durable causal data;the exact request and both parents survive packaging.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BranchJournal {
    BranchMerge {
        request: BranchMergeRequest,
        base_revision: String,
        changes: Vec<HistoryChange>,
    },
}
