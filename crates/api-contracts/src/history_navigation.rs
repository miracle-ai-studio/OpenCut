//! Durable editor navigation over immutable history; applying targets uses existing undo/redo tools.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryNavigationInput {
    pub project_id: String,
    pub revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UndoTarget {
    pub transaction_id: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RedoTarget {
    pub transaction_id: String,
    pub undo_transaction_id: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HistoryNavigation {
    pub project_id: String,
    pub revision: String,
    pub undo: Option<UndoTarget>,
    pub redo: Option<RedoTarget>,
}
