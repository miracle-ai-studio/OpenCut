//! Owner administration history. Query policy is independent of SQL/transports.
pub use crate::access::{
    AccessAuditCoverage, AccessAuditPage, AccessAuditQuery, AccessAuditRecord, AccessAuditTarget,
};
use crate::access::{Caller, denied};
use opencut_contracts::EditorError;

/// Reserve envelope space below a 4 MiB response budget. Large policy snapshots
/// can produce a shorter page even when the requested row limit is not reached.
pub const RECORD_BYTES_LIMIT: usize = 4_000_000;
pub trait AccessAuditRead {
    /// Visit at most `limit` ascending records from one consistent read view.
    /// Stop immediately when the visitor returns false; never materialize the
    /// whole history. Adapters also enforce their trusted caller's owner status.
    fn visit_access_audit(
        &self,
        target: &AccessAuditTarget,
        after: i64,
        limit: usize,
        visitor: &mut dyn FnMut(AccessAuditRecord) -> Result<bool, EditorError>,
    ) -> Result<(), EditorError>;
}
pub fn query(
    store: &impl AccessAuditRead,
    caller: &Caller,
    input: AccessAuditQuery,
) -> Result<AccessAuditPage, EditorError> {
    if !caller.is_owner() {
        return Err(denied());
    }
    let invalid = || {
        EditorError::new(
            "INVALID_ARGUMENT",
            "Invalid access audit target, sequence or page size",
        )
    };
    let after = input.after_sequence.parse::<i64>().map_err(|_| invalid())?;
    let id = match &input.target {
        AccessAuditTarget::Workspace => None,
        AccessAuditTarget::Principal(id)
        | AccessAuditTarget::Credential(id)
        | AccessAuditTarget::Request(id) => Some(id),
    };
    if after < 0
        || after.to_string() != input.after_sequence
        || !(1..=500).contains(&input.limit)
        || id.is_some_and(|id| id.is_empty() || id.len() > 1024 || id.chars().any(char::is_control))
    {
        return Err(invalid());
    }
    let mut page = AccessAuditPage {
        records: vec![],
        last_sequence: input.after_sequence,
        has_more: false,
        coverage: AccessAuditCoverage::RecordedAdministrationSinceSchema5,
    };
    let mut bytes = 0;
    store.visit_access_audit(&input.target, after, input.limit + 1, &mut |record| {
        if page.records.len() == input.limit {
            page.has_more = true;
            return Ok(false);
        }
        let size = serde_json::to_vec(&record)
            .map_err(|_| {
                EditorError::new("INTERNAL_ERROR", "Could not encode access audit record")
            })?
            .len();
        if size > RECORD_BYTES_LIMIT && page.records.is_empty() {
            return Err(EditorError::new(
                "RESOURCE_TOO_LARGE",
                "Access audit record exceeds the control response budget",
            ));
        }
        if bytes + size > RECORD_BYTES_LIMIT {
            page.has_more = true;
            return Ok(false);
        }
        bytes += size;
        page.last_sequence = record.sequence.clone();
        page.records.push(record);
        Ok(true)
    })?;
    Ok(page)
}
