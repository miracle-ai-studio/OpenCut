//! Public selective history operations. These create new revisions; they never rewrite history.
pub use opencut_contracts::project_history::{Direction, RedoRequest, UndoRequest};
use opencut_contracts::{ProjectDocument, TransactionResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PropertyChange {
    /// JSON pointer through stable entity IDs, for example /sequences/main/tracks/voice/params/volume.
    pub path: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CollaborationConflict {
    pub path: String,
    pub code: String,
    pub expected: Option<Value>,
    pub actual: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CollaborationPlan {
    pub direction: Direction,
    pub expected_revision: String,
    /// Original edit transaction whose effect is being undone or redone.
    pub target_transaction_id: String,
    /// The undo transaction required by a redo.
    pub undo_transaction_id: Option<String>,
    pub changes: Vec<PropertyChange>,
    pub conflicts: Vec<CollaborationConflict>,
    /// Present only when conflict-free and valid as a project document.
    pub proposed_project: Option<ProjectDocument>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CollaborationCommit {
    pub direction: Direction,
    pub target_transaction_id: String,
    pub undo_transaction_id: Option<String>,
    pub transaction: TransactionResult,
}
