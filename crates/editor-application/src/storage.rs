//! Storage observation policy; native descriptor traversal belongs to the adapter.
use crate::access::{Caller, denied};
pub use opencut_api_contracts::storage::*;
use opencut_contracts::EditorError;
pub const MAX_DEPTH: usize = 64;
pub const TIME_LIMIT_MS: u64 = 5_000;
pub const MAX_CONCURRENT: usize = 2;
pub trait StorageInspector {
    fn inspect(&self, input: &InspectInput) -> Result<UsageReport, EditorError>;
}
pub fn inspect(
    caller: &Caller,
    native: &impl StorageInspector,
    input: InspectInput,
) -> Result<UsageReport, EditorError> {
    if !caller.is_owner() {
        return Err(denied());
    }
    if input.max_entries == 0 || input.max_entries > MAX_ENTRIES {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "max_entries must be in 1..100000",
        ));
    }
    native.inspect(&input)
}
