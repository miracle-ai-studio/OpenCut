//! Explicit release of a successful task artifact's default retention root.
//! The native bytes stay available until a separately reviewed collector acts.
use crate::{access::denied, audit::ExecutionContext, job_views::parse_artifact_id};
pub use opencut_api_contracts::artifact_retention::*;
use opencut_contracts::EditorError;

pub trait Store {
    fn retention_context(&self) -> ExecutionContext;
    fn retention_inspect(&self, artifact_id: &str) -> Result<View, EditorError>;
    fn retention_release(&mut self, input: &ReleaseInput) -> Result<View, EditorError>;
}

fn owner(store: &impl Store) -> Result<(), EditorError> {
    if store.retention_context().caller().is_owner() {
        Ok(())
    } else {
        Err(denied())
    }
}

fn validate_id(id: &str) -> Result<(), EditorError> {
    parse_artifact_id(id).map(|_| ())
}

pub fn inspect(store: &impl Store, input: InspectInput) -> Result<View, EditorError> {
    owner(store)?;
    validate_id(&input.artifact_id)?;
    store.retention_inspect(&input.artifact_id)
}

pub fn release(store: &mut impl Store, input: ReleaseInput) -> Result<View, EditorError> {
    owner(store)?;
    validate_id(&input.artifact_id)?;
    if input.idempotency_key.trim().is_empty()
        || input.idempotency_key.len() > 1024
        || input.idempotency_key.chars().any(char::is_control)
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Invalid retention idempotency key",
        ));
    }
    store.retention_release(&input)
}
