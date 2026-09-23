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
    if let Some(limit) = &input.control_limit_bytes
        && decimal(limit)? == 0
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Database control limit must be positive",
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
    admit_payload(policy, used, held, 0, additional, available)
}
/// Control reservations share physical volume headroom, but never consume the
/// configured logical payload quota.
pub fn admit_payload(
    policy: &Policy,
    used_payload: u64,
    held_payload: u64,
    held_control: u64,
    additional: u64,
    available: u64,
) -> Result<(), EditorError> {
    let total = used_payload
        .checked_add(held_payload)
        .and_then(|x| x.checked_add(additional));
    if total.is_none_or(|n| n > decimal(&policy.payload_limit_bytes).unwrap_or(0)) {
        return Err(EditorError::new(
            "STORAGE_BUDGET_EXCEEDED",
            "Managed payload usage and outstanding reservations exceed the configured admission limit",
        ));
    }
    let required = held_payload
        .checked_add(held_control)
        .and_then(|n| n.checked_add(additional))
        .and_then(|n| n.checked_add(decimal(&policy.minimum_available_bytes).ok()?));
    if required.is_none_or(|n| n > available) {
        return Err(EditorError::new(
            "STORAGE_CAPACITY_LOW",
            "Observed volume headroom is insufficient for the reservation and configured margin",
        ));
    }
    Ok(())
}
/// Independent logical control limit with the same physical reservation pool.
/// The caller supplies the control margin separately from the payload policy.
pub fn admit_control(
    control_limit: u64,
    used_control: u64,
    held_control: u64,
    held_payload: u64,
    additional: u64,
    minimum_available: u64,
    available: u64,
) -> Result<(), EditorError> {
    let logical = used_control
        .checked_add(held_control)
        .and_then(|n| n.checked_add(additional));
    if logical.is_none_or(|n| n > control_limit) {
        return Err(EditorError::new(
            "DATABASE_CAPACITY_EXHAUSTED",
            "Managed database control usage and reservations exceed the configured limit",
        ));
    }
    let physical = held_payload
        .checked_add(held_control)
        .and_then(|n| n.checked_add(additional))
        .and_then(|n| n.checked_add(minimum_available));
    if physical.is_none_or(|n| n > available) {
        return Err(EditorError::new(
            "STORAGE_CAPACITY_LOW",
            "Observed volume headroom is insufficient for database control reservation",
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

#[cfg(test)]
mod admission_partition_tests {
    use super::*;

    #[test]
    fn control_reservations_use_physical_space_without_spending_payload_quota() {
        let policy = Policy {
            revision: 0,
            payload_limit_bytes: "100".into(),
            minimum_available_bytes: "0".into(),
        };
        assert!(admit_payload(&policy, 80, 0, 15, 20, 35).is_ok());
        assert_eq!(
            admit_payload(&policy, 80, 0, 15, 21, 36).unwrap_err().code,
            "STORAGE_BUDGET_EXCEEDED"
        );
        assert_eq!(
            admit_payload(&policy, 80, 0, 15, 20, 34).unwrap_err().code,
            "STORAGE_CAPACITY_LOW"
        );
        assert!(admit_control(200, 100, 20, 10, 50, 5, 85).is_ok());
        assert_eq!(
            admit_control(169, 100, 20, 10, 50, 5, 85).unwrap_err().code,
            "DATABASE_CAPACITY_EXHAUSTED"
        );
        assert_eq!(
            admit_control(200, 100, 20, 10, 50, 5, 84).unwrap_err().code,
            "STORAGE_CAPACITY_LOW"
        );
    }
}
