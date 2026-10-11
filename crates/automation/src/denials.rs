//! Shared refusal reporting. Request arguments, tokens and paths stay out of records.
use crate::{CallSource, Caller, Response};
pub use opencut_editor_application::denials::*;
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
