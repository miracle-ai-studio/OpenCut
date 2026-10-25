//! Scope, capacity and action classification for one authenticated recovery slot.
use super::*;
use opencut_editor_application::{audit::ExecutionContext, edit_recovery as recovery};

pub(super) struct Scope {
    pub principal: String,
    pub credential: String,
    pub owner: bool,
}
pub(super) fn scope(context: &ExecutionContext) -> Scope {
    let caller = context.caller();
    Scope {
        principal: caller.principal_id().into(),
        credential: match caller {
            access::Caller::LocalOwner => String::new(),
            access::Caller::Credential(c) => c.credential_id().into(),
        },
        owner: caller.is_owner(),
    }
}
pub(super) fn quota() -> EditorError {
    EditorError::new(
        "RECOVERY_QUOTA_EXCEEDED",
        "Unresolved recovery storage is full; resolve existing transactions first",
    )
}
pub(super) fn reserve(connection: &Connection, bytes: usize) -> Result<(), EditorError> {
    let (count, total): (u64, u64) = connection
        .query_row(
            "SELECT count(*),COALESCE(sum(stored_bytes),0) FROM edit_recovery",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(storage_error)?;
    if bytes > recovery::MAX_RECORD_BYTES
        || count >= recovery::MAX_UNRESOLVED_RECORDS
        || total
            .checked_add(bytes as u64)
            .is_none_or(|value| value > recovery::MAX_UNRESOLVED_BYTES)
    {
        return Err(quota());
    }
    Ok(())
}
