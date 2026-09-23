//! Shared admission arithmetic and owner policy. No filesystem or database ownership here.
use crate::{
    access::{Caller, denied},
    audit::ExecutionContext,
};
pub use opencut_api_contracts::budget::*;
use opencut_contracts::EditorError;
pub const MAX_RESERVATIONS: usize = 4096;
pub const MAX_POLICY_CHANGES: usize = 1024;
pub fn decimal(value: &str) -> Result<u64, EditorError> {
    let n = value.parse::<u64>().map_err(|_| invalid())?;
    if n.to_string() != value || n > i64::MAX as u64 {
        return Err(invalid());
    }
    Ok(n)
}
fn invalid() -> EditorError {
    EditorError::new(
        "INVALID_ARGUMENT",
        "Budget bytes must be canonical nonnegative decimal integers at most i64::MAX",
    )
}
pub fn validate(input: &Configure) -> Result<(), EditorError> {
    decimal(&input.payload_limit_bytes)?;
    decimal(&input.minimum_available_bytes)?;
    if let Some(limit) = &input.media_output_limit_bytes
        && decimal(limit)? == 0
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Media output file limit must be positive",
        ));
    }
    if input.idempotency_key.trim().is_empty()
        || input.idempotency_key.len() > 1024
        || input.idempotency_key.chars().any(char::is_control)
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Invalid budget policy idempotency key",
        ));
    }
    Ok(())
}
pub fn admit(
    policy: &Policy,
    used: u64,
    held: u64,
    additional: u64,
    available: u64,
) -> Result<(), EditorError> {
    let total = used
        .checked_add(held)
        .and_then(|x| x.checked_add(additional));
    if total.is_none_or(|n| n > decimal(&policy.payload_limit_bytes).unwrap_or(0)) {
        return Err(EditorError::new(
            "STORAGE_BUDGET_EXCEEDED",
            "Managed payload usage and outstanding reservations exceed the configured admission limit",
        ));
    }
    let required = held
        .checked_add(additional)
        .and_then(|n| n.checked_add(decimal(&policy.minimum_available_bytes).ok()?));
    if required.is_none_or(|n| n > available) {
        return Err(EditorError::new(
            "STORAGE_CAPACITY_LOW",
            "Observed volume headroom is insufficient for the reservation and configured margin",
        ));
    }
    Ok(())
}
pub trait BudgetNative {
    fn inspect(&self, recover: bool) -> Result<Report, EditorError>;
    fn configure(
        &self,
        input: Configure,
        context: &ExecutionContext,
    ) -> Result<PolicyChange, EditorError>;
}
pub fn inspect(
    caller: &Caller,
    native: &impl BudgetNative,
    recover: bool,
) -> Result<Report, EditorError> {
    if !caller.is_owner() {
        return Err(denied());
    }
    native.inspect(recover)
}
pub fn configure(
    context: &ExecutionContext,
    native: &impl BudgetNative,
    input: Configure,
) -> Result<PolicyChange, EditorError> {
    if !context.caller().is_owner() {
        return Err(denied());
    }
    validate(&input)?;
    native.configure(input, context)
}
