//! Trusted adapters report a denial after the rejected operation has unwound.
use crate::{access::Caller, audit::CallSource};
pub use opencut_api_contracts::denials::*;
use opencut_contracts::EditorError;
pub const MAX_RETAINED: usize = 10_000;
pub const FAILURE_WARNING: &str = "DENIAL_AUDIT_UNAVAILABLE";
/// Not Deserialize: callers and IDs originate at a host boundary, never request arguments.
pub struct Denial<'a> {
    pub request_id: &'a str,
    pub source: CallSource,
    pub caller: Option<&'a Caller>,
    pub operation: DeniedOperation,
    pub reason: DenialReason,
}
pub trait DenialStore {
    fn record_denial(&self, denial: &Denial<'_>) -> Result<(), EditorError>;
    fn read_denials(&self, query: &DenialQuery) -> Result<DenialPage, EditorError>;
}
pub fn operation(name: &str) -> DeniedOperation {
    if opencut_api_contracts::tools::is_tool(name) {
        DeniedOperation::Tool { name: name.into() }
    } else {
        DeniedOperation::UnknownTool
    }
}
pub fn is_denial(error: &EditorError) -> bool {
    matches!(error.code.as_str(), "NOT_AUTHORIZED" | "PERMISSION_DENIED")
}
pub fn validate(denial: &Denial<'_>) -> Result<(), EditorError> {
    if uuid::Uuid::parse_str(denial.request_id).is_err() {
        return Err(invalid());
    }
    if let DeniedOperation::Tool { name } = &denial.operation
        && matches!(operation(name), DeniedOperation::UnknownTool)
    {
        return Err(invalid());
    }
    Ok(())
}
fn invalid() -> EditorError {
    EditorError::new(
        "INVALID_ARGUMENT",
        "Invalid denial audit query or host context",
    )
}
pub fn validate_query(query: &DenialQuery) -> Result<i64, EditorError> {
    let after = query.after_sequence.parse::<i64>().map_err(|_| invalid())?;
    let id = match &query.target {
        DenialTarget::Workspace => None,
        DenialTarget::Request(id) | DenialTarget::Principal(id) | DenialTarget::Credential(id) => {
            Some(id)
        }
    };
    if after < 0
        || after.to_string() != query.after_sequence
        || !(1..=200).contains(&query.limit)
        || id.is_some_and(|s| s.is_empty() || s.len() > 1024 || s.chars().any(char::is_control))
    {
        return Err(invalid());
    }
    Ok(after)
}
