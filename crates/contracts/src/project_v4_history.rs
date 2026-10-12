//! Unpublished V4 history protocol. Diffs expose digests, never native provenance.
use crate::TransactionResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HistoryDirectionV4 {
    Undo,
    Redo,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UndoV4Project {
    pub project_id: String,
    pub expected_revision: String,
    pub target_transaction_id: String,
    pub idempotency_key: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RedoV4Project {
    pub project_id: String,
    pub expected_revision: String,
    pub undo_transaction_id: String,
    pub idempotency_key: String,
}
/// A missing digest means an absent property; a JSON null has its own digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryChangeV4 {
    pub path: String,
    pub before_sha256: Option<String>,
    pub after_sha256: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HistoryConflictReasonV4 {
    DependentChange,
    ProjectConstraint,
    LockedTrack,
    NoEffect,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryConflictV4 {
    pub path: String,
    pub reason: HistoryConflictReasonV4,
    pub expected_sha256: Option<String>,
    pub actual_sha256: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryPlanV4 {
    pub direction: HistoryDirectionV4,
    pub expected_revision: String,
    pub target_transaction_id: String,
    pub undo_transaction_id: Option<String>,
    pub changes: Vec<HistoryChangeV4>,
    pub conflicts: Vec<HistoryConflictV4>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryCommitV4 {
    pub direction: HistoryDirectionV4,
    pub target_transaction_id: String,
    pub undo_transaction_id: Option<String>,
    pub transaction: TransactionResult,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryNavigationInputV4 {
    pub project_id: String,
    pub revision: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryUndoTargetV4 {
    pub transaction_id: String,
    pub label: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryRedoTargetV4 {
    pub transaction_id: String,
    pub undo_transaction_id: String,
    pub label: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryNavigationV4 {
    pub project_id: String,
    pub revision: String,
    pub undo: Option<HistoryUndoTargetV4>,
    pub redo: Option<HistoryRedoTargetV4>,
}
/// Durable causal data is typed and independent of the request identity hash.
/// Revision documents remain the authority for reconstructing original values.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum CausalJournalV4 {
    #[serde(rename = "project_v4_selective_undo")]
    Undo {
        schema_version: u32,
        request: UndoV4Project,
        changes: Vec<HistoryChangeV4>,
    },
    #[serde(rename = "project_v4_selective_redo")]
    Redo {
        schema_version: u32,
        request: RedoV4Project,
        target_transaction_id: String,
        changes: Vec<HistoryChangeV4>,
    },
}
