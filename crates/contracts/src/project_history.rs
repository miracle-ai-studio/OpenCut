//! Causal editing history. Public diffs expose digests,never native provenance.
use crate::TransactionResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Undo,
    Redo,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UndoRequest {
    pub project_id: String,
    pub expected_revision: String,
    pub target_transaction_id: String,
    pub idempotency_key: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RedoRequest {
    pub project_id: String,
    pub expected_revision: String,
    pub undo_transaction_id: String,
    pub idempotency_key: String,
}
/// A missing digest means an absent property; a JSON null has its own digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryChange {
    pub path: String,
    pub before_sha256: Option<String>,
    pub after_sha256: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HistoryConflictReason {
    DependentChange,
    ProjectConstraint,
    LockedTrack,
    NoEffect,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryConflict {
    pub path: String,
    pub reason: HistoryConflictReason,
    pub expected_sha256: Option<String>,
    pub actual_sha256: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryPlan {
    pub direction: Direction,
    pub expected_revision: String,
    pub target_transaction_id: String,
    pub undo_transaction_id: Option<String>,
    pub changes: Vec<HistoryChange>,
    pub conflicts: Vec<HistoryConflict>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryCommit {
    pub direction: Direction,
    pub target_transaction_id: String,
    pub undo_transaction_id: Option<String>,
    pub transaction: TransactionResult,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryNavigationInput {
    pub project_id: String,
    pub revision: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UndoTarget {
    pub transaction_id: String,
    pub label: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RedoTarget {
    pub transaction_id: String,
    pub undo_transaction_id: String,
    pub label: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryNavigation {
    pub project_id: String,
    pub revision: String,
    pub undo: Option<UndoTarget>,
    pub redo: Option<RedoTarget>,
}
/// Durable causal data is typed and independent of the request identity hash.
/// Revision documents remain the authority for reconstructing original values.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum CausalJournal {
    #[serde(rename = "project_v4_selective_undo")]
    Undo {
        schema_version: u32,
        request: UndoRequest,
        changes: Vec<HistoryChange>,
    },
    #[serde(rename = "project_v4_selective_redo")]
    Redo {
        schema_version: u32,
        request: RedoRequest,
        target_transaction_id: String,
        changes: Vec<HistoryChange>,
    },
}
