//! Schema 3 history and branch results; legacy schema 2 tools retain their wire contracts.
use crate::{
    branching::BranchView,
    collaboration::{CollaborationConflict, Direction, PropertyChange},
};
use opencut_contracts::{
    TransactionResult, historical_visual::HistoricalVisualDocument,
    historical_visual_ops::ApplyHistoricalVisualProject,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HistoricalVisualCollaborationPlan {
    pub direction: Direction,
    pub expected_revision: String,
    pub target_transaction_id: String,
    pub undo_transaction_id: Option<String>,
    pub changes: Vec<PropertyChange>,
    pub conflicts: Vec<CollaborationConflict>,
    pub proposed_project: Option<HistoricalVisualDocument>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalVisualBranchEditRequest {
    pub project_id: String,
    pub branch_name: String,
    pub request: ApplyHistoricalVisualProject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HistoricalVisualBranchComparePlan {
    pub project_id: String,
    pub source_branch: String,
    pub target_branch: String,
    pub base_revision: String,
    pub source_revision: String,
    pub target_revision: String,
    pub changes: Vec<PropertyChange>,
    pub conflicts: Vec<CollaborationConflict>,
    pub proposed_project: Option<HistoricalVisualDocument>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HistoricalVisualBranchMergeCommit {
    pub source_branch: String,
    pub target_branch: String,
    pub base_revision: String,
    pub source_revision: String,
    pub transaction: TransactionResult,
}

/// Branch refs are shared across schema versions; all v3 branch heads must be schema 3.
pub type HistoricalVisualBranchView = BranchView;
