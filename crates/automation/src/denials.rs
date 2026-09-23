//! Native reporting facade. No request arguments, tokens or paths enter a record.
use crate::{CallSource, Caller, Response};
use opencut_contracts::EditorError;
pub use opencut_editor_application::denials::*;
use opencut_sqlite_store::EditorApplication;
use std::path::Path;
pub fn record_at(database: &Path, denial: &Denial<'_>) -> Result<(), EditorError> {
    EditorApplication::open_managed(database)?.record_denial(denial)
}
pub fn observe_response<T>(
    store: &impl DenialStore,
    caller: Option<&Caller>,
    source: CallSource,
    operation: DeniedOperation,
    reason: DenialReason,
    mut response: Response<T>,
) -> Response<T> {
    if response.error.as_ref().is_some_and(is_denial)
        && store
            .record_denial(&Denial {
                request_id: &response.request_id,
                source,
                caller,
                operation,
                reason,
            })
            .is_err()
        && !response.warnings.iter().any(|w| w == FAILURE_WARNING)
    {
        response.warnings.push(FAILURE_WARNING.into());
    }
    response
}
