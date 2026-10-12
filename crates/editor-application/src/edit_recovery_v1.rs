//! Recovery ports retain reviewed public transactions, never media bytes or a
//! second mutable project. Adapters own authorization, quotas and atomic proof.
pub use opencut_contracts::edit_recovery_v1::*;
use opencut_contracts::{EditorError, project_v4_public::ApplyPublicV4Project};
use sha2::{Digest, Sha256};

pub const MAX_RECORD_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_UNRESOLVED_RECORDS: u64 = 32;
pub const MAX_UNRESOLVED_BYTES: u64 = 64 * 1024 * 1024;

pub trait EditRecoveryStoreV1 {
    /// Serialize with an immediate write lock: authenticate the original scope,
    /// compare an existing record before head checks, then reserve bounded space.
    fn stage_recovery_v1(
        &mut self,
        request: &StageEditRecoveryV1,
        prepared: &PreparedEditRecoveryV1,
    ) -> Result<EditRecoveryRecordV1, EditorError>;
    /// Read only. Neither absence nor a changed head is proof of successful edit.
    fn read_recovery_v1(
        &self,
        request: &ReadEditRecoveryV1,
    ) -> Result<Option<EditRecoveryRecordV1>, EditorError>;
    /// Atomically reauthorize, read the exact original idempotency receipt first,
    /// then delete only if saved or the original fixed-head write is impossible.
    fn ack_recovery_v1(
        &mut self,
        request: &AckEditRecoveryV1,
    ) -> Result<EditRecoveryAckV1, EditorError>;
}

pub struct PreparedEditRecoveryV1 {
    payload: String,
    digest: String,
}
impl PreparedEditRecoveryV1 {
    pub fn payload(&self) -> &str {
        &self.payload
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

fn invalid() -> EditorError {
    EditorError::new(
        "INVALID_ARGUMENT",
        "Invalid fixed timeline recovery context",
    )
}
fn id(value: &str) -> Result<(), EditorError> {
    if value.trim().is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        return Err(invalid());
    }
    Ok(())
}

pub fn transaction(context: &EditRecoveryContextV1) -> Result<&ApplyPublicV4Project, EditorError> {
    let EditRecoveryContextV1::TimelineV4 { plan, .. } = context;
    plan.transaction.as_ref().ok_or_else(invalid)
}

pub fn exact_payload_digest(payload: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"opencut.edit-recovery.v1\0");
    hash.update(payload.as_bytes());
    format!("sha256:{:x}", hash.finalize())
}

pub fn prepare(request: &StageEditRecoveryV1) -> Result<PreparedEditRecoveryV1, EditorError> {
    let EditRecoveryContextV1::TimelineV4 {
        request: timeline,
        plan,
    } = &request.context;
    let tx = transaction(&request.context)?;
    id(&timeline.project_id)?;
    id(&timeline.sequence_id)?;
    id(&timeline.expected_revision)?;
    id(&timeline.idempotency_key)?;
    crate::require_idempotency_key(&timeline.idempotency_key)?;
    if tx.project_id != timeline.project_id
        || tx.expected_revision != timeline.expected_revision
        || tx.idempotency_key != timeline.idempotency_key
        || tx.label != timeline.label
        || tx.operations.is_empty()
        || tx.operations.len() > 128
        || tx.label.trim().is_empty()
        || tx.label.len() > 4096
    {
        return Err(invalid());
    }
    let identity =
        crate::identity::request_identity("project_v4_timeline", &timeline.project_id, timeline)?;
    if plan.original_request_digest != identity.current_digest() {
        return Err(invalid());
    }
    let payload = serde_json::to_string(request).map_err(|_| invalid())?;
    if payload.len() > MAX_RECORD_BYTES {
        return Err(EditorError::new(
            "RECOVERY_QUOTA_EXCEEDED",
            "Recovery context exceeds the 2 MiB record budget",
        ));
    }
    let digest = exact_payload_digest(&payload);
    Ok(PreparedEditRecoveryV1 { payload, digest })
}

pub fn stage(
    store: &mut impl EditRecoveryStoreV1,
    request: &StageEditRecoveryV1,
) -> Result<EditRecoveryRecordV1, EditorError> {
    let prepared = prepare(request)?;
    store.stage_recovery_v1(request, &prepared)
}
pub fn read(
    store: &impl EditRecoveryStoreV1,
    request: &ReadEditRecoveryV1,
) -> Result<Option<EditRecoveryRecordV1>, EditorError> {
    id(&request.project_id)?;
    if let Some(recovery) = &request.recovery_id {
        id(recovery)?;
    }
    store.read_recovery_v1(request)
}
pub fn ack(
    store: &mut impl EditRecoveryStoreV1,
    request: &AckEditRecoveryV1,
) -> Result<EditRecoveryAckV1, EditorError> {
    id(&request.project_id)?;
    id(&request.recovery_id)?;
    store.ack_recovery_v1(request)
}
