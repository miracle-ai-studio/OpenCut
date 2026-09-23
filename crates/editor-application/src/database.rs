//! Database maintenance policy. SQLite, native paths and checkpointing live in the adapter.
use crate::access::{Caller, denied};
pub use opencut_api_contracts::database::*;
use opencut_contracts::EditorError;

pub trait DatabaseMaintenance {
    fn inspect_database_storage(&self) -> Result<Report, EditorError>;
    fn checkpoint_database(&self, input: &CheckpointInput)
    -> Result<CheckpointReport, EditorError>;
}
pub fn inspect(caller: &Caller, store: &impl DatabaseMaintenance) -> Result<Report, EditorError> {
    if !caller.is_owner() {
        return Err(denied());
    }
    store.inspect_database_storage()
}
pub fn checkpoint(
    caller: &Caller,
    store: &impl DatabaseMaintenance,
    input: CheckpointInput,
) -> Result<CheckpointReport, EditorError> {
    if !caller.is_owner() {
        return Err(denied());
    }
    if input.busy_timeout_ms > 5000 {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Checkpoint busy_timeout_ms must be in 0..5000",
        ));
    }
    store.checkpoint_database(&input)
}
