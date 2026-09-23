//! Trusted host context and provenance query policy; no transport or storage I/O.
pub use opencut_api_contracts::audit::*;
use opencut_contracts::EditorError;

/// Constructed by a trusted host, not deserialized from a request body/header.
/// Current local authority represents an OS owner/shared bearer, not a human ID.
#[derive(Clone)]
pub struct ExecutionContext {
    request_id: String,
    actor: Actor,
    caller: crate::access::Caller,
}
impl ExecutionContext {
    pub fn local(source: CallSource) -> Self {
        Self::for_caller(source, crate::access::Caller::LocalOwner)
    }
    pub fn for_caller(source: CallSource, caller: crate::access::Caller) -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            actor: Actor {
                principal_id: caller.principal_id().into(),
                source,
                worker_id: None,
            },
            caller,
        }
    }
    pub fn worker(worker_id: Option<&str>) -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            actor: Actor {
                principal_id: "local-runtime".into(),
                source: CallSource::Worker,
                worker_id: worker_id.map(str::to_owned),
            },
            caller: crate::access::Caller::LocalOwner,
        }
    }
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    pub fn actor(&self) -> &Actor {
        &self.actor
    }
    pub fn caller(&self) -> &crate::access::Caller {
        &self.caller
    }
}

pub trait AuditRead {
    /// Return ascending sequence order, selecting `limit` rows in one read view.
    fn read_audit(
        &self,
        target: &AuditTarget,
        after: i64,
        limit: usize,
    ) -> Result<Vec<AuditRecord>, EditorError>;
}
pub fn query(store: &impl AuditRead, input: AuditQuery) -> Result<AuditPage, EditorError> {
    let invalid = || {
        EditorError::new(
            "INVALID_ARGUMENT",
            "Invalid audit target, sequence or page size",
        )
    };
    let after = input.after_sequence.parse::<i64>().map_err(|_| invalid())?;
    if after < 0 || after.to_string() != input.after_sequence || !(1..=500).contains(&input.limit) {
        return Err(invalid());
    }
    let id = match &input.target {
        AuditTarget::Workspace => None,
        AuditTarget::Project(id)
        | AuditTarget::Transaction(id)
        | AuditTarget::Job(id)
        | AuditTarget::Request(id) => Some(id),
    };
    if id.is_some_and(|id| id.is_empty() || id.len() > 1024) {
        return Err(invalid());
    }
    let mut records = store.read_audit(&input.target, after, input.limit + 1)?;
    let has_more = records.len() > input.limit;
    records.truncate(input.limit);
    let last_sequence = records
        .last()
        .map(|r| r.sequence.clone())
        .unwrap_or(input.after_sequence);
    Ok(AuditPage {
        records,
        last_sequence,
        has_more,
        coverage: AuditCoverage::RecordedMutationsSinceSchema4,
    })
}
