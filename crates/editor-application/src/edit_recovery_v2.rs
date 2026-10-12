//! Recover original public write intents without re-planning or auto-dispatch.
//! Storage owns live authorization, shared quotas and atomic resolution proof.
pub use crate::edit_recovery_v1::{MAX_RECORD_BYTES, MAX_UNRESOLVED_BYTES, MAX_UNRESOLVED_RECORDS};
use crate::project_v4::V4Intent;
pub use opencut_contracts::edit_recovery_v2::*;
use opencut_contracts::{EditorError, TransactionResult, project_v4_history::HistoryDirectionV4};
use sha2::{Digest, Sha256};

pub trait EditRecoveryStoreV2 {
    /// Revalidate the original action/resource authority before preparation and
    /// under an Immediate lock. Exact existing records precede head checks.
    /// V1/V2 share 32 records/64 MiB and one unresolved record per authenticated
    /// principal/credential/owner/project; never replace the pending intent.
    fn stage_recovery_v2(
        &mut self,
        request: &StageEditRecoveryV2,
        prepared: &PreparedEditRecoveryV2,
    ) -> Result<EditRecoveryRecordV2, EditorError>;
    /// Read only; discover V1 before V2. Empty discovery requires Read/live,
    /// while a loaded record requires its original Edit or Restore action and
    /// current write control. Query exact family/scope/key/full-intent receipts;
    /// do not invoke apply, follow head or manufacture success from a revision.
    fn inspect_recovery_v2(
        &self,
        request: &ReadEditRecoveryV2,
    ) -> Result<EditRecoveryInspectionV2, EditorError>;
    /// Under an Immediate lock reauthorize and read the original receipt first.
    /// Delete only when it exists, or when no receipt exists and the original
    /// fixed-head request is impossible. Permission/lease/transport errors and
    /// an unchanged head retain the record. This port acknowledges V2 only;
    /// discovered V1 records use the unchanged V1 acknowledgement operation.
    fn ack_recovery_v2(
        &mut self,
        request: &AckEditRecoveryV2,
    ) -> Result<EditRecoveryAckV2, EditorError>;
}

pub struct PreparedEditRecoveryV2 {
    payload: String,
    digest: String,
}
impl PreparedEditRecoveryV2 {
    pub fn payload(&self) -> &str {
        &self.payload
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

fn invalid() -> EditorError {
    EditorError::new("INVALID_ARGUMENT", "Invalid fixed write recovery context")
}
fn corrupt() -> EditorError {
    EditorError::new(
        "STORAGE_ERROR",
        "Recovery record or original receipt differs",
    )
}
fn id(value: &str) -> Result<(), EditorError> {
    if value.trim().is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        return Err(invalid());
    }
    Ok(())
}

pub fn intent(request: &EditRecoveryIntentV2) -> V4Intent<'_> {
    match request {
        EditRecoveryIntentV2::PublicApply { request } => V4Intent::PublicApply(request),
        EditRecoveryIntentV2::Undo { request } => V4Intent::Undo(request),
        EditRecoveryIntentV2::Redo { request } => V4Intent::Redo(request),
        EditRecoveryIntentV2::Restore { request } => V4Intent::Restore(request),
    }
}

pub fn exact_payload_digest(payload: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"opencut.edit-recovery.v2\0");
    hash.update(payload.as_bytes());
    format!("sha256:{:x}", hash.finalize())
}

fn validate_context(context: &EditRecoveryContextV2) -> Result<(), EditorError> {
    let original = intent(&context.intent);
    id(original.project_id())?;
    id(original.expected_revision().ok_or_else(invalid)?)?;
    id(original.key())?;
    crate::require_idempotency_key(original.key())?;
    if let Some(sequence) = &context.sequence_id {
        id(sequence)?;
    }
    match &context.intent {
        EditRecoveryIntentV2::PublicApply { request } => {
            if request.operations.is_empty()
                || request.operations.len() > 128
                || request.label.trim().is_empty()
                || request.label.len() > 4096
            {
                return Err(invalid());
            }
        }
        EditRecoveryIntentV2::Undo { request } => id(&request.target_transaction_id)?,
        EditRecoveryIntentV2::Redo { request } => id(&request.undo_transaction_id)?,
        EditRecoveryIntentV2::Restore { request } => id(&request.target_revision)?,
    }
    // Use the executable family's shared typed identity, never a client family
    // string or resolved private resource record. Storage uses this same intent.
    original.identity()?;
    Ok(())
}

pub fn prepare(request: &StageEditRecoveryV2) -> Result<PreparedEditRecoveryV2, EditorError> {
    validate_context(&request.context)?;
    let payload = serde_json::to_string(request).map_err(|_| invalid())?;
    if payload.len() > MAX_RECORD_BYTES {
        return Err(EditorError::new(
            "RECOVERY_QUOTA_EXCEEDED",
            "Recovery context exceeds the 2 MiB record budget",
        ));
    }
    let digest = exact_payload_digest(&payload);
    Ok(PreparedEditRecoveryV2 { payload, digest })
}

pub fn receipt_transaction(receipt: &EditRecoveryReceiptV2) -> &TransactionResult {
    match receipt {
        EditRecoveryReceiptV2::PublicApply { transaction }
        | EditRecoveryReceiptV2::Restore { transaction } => transaction,
        EditRecoveryReceiptV2::Undo { commit } | EditRecoveryReceiptV2::Redo { commit } => {
            &commit.transaction
        }
    }
}
fn validate_receipt_shape(receipt: &EditRecoveryReceiptV2) -> Result<(), EditorError> {
    let transaction = receipt_transaction(receipt);
    for value in [
        &transaction.transaction_id,
        &transaction.base_revision,
        &transaction.revision,
    ] {
        id(value).map_err(|_| corrupt())?;
    }
    match receipt {
        EditRecoveryReceiptV2::Undo { commit } => {
            if commit.direction != HistoryDirectionV4::Undo || commit.undo_transaction_id.is_some()
            {
                return Err(corrupt());
            }
            id(&commit.target_transaction_id).map_err(|_| corrupt())?;
        }
        EditRecoveryReceiptV2::Redo { commit } => {
            if commit.direction != HistoryDirectionV4::Redo {
                return Err(corrupt());
            }
            id(&commit.target_transaction_id).map_err(|_| corrupt())?;
            id(commit.undo_transaction_id.as_deref().ok_or_else(corrupt)?)
                .map_err(|_| corrupt())?;
        }
        _ => {}
    }
    Ok(())
}

pub fn validate_original_receipt(
    context: &EditRecoveryContextV2,
    receipt: &EditRecoveryReceiptV2,
) -> Result<(), EditorError> {
    validate_receipt_shape(receipt)?;
    if Some(receipt_transaction(receipt).base_revision.as_str())
        != intent(&context.intent).expected_revision()
    {
        return Err(corrupt());
    }
    match (&context.intent, receipt) {
        (EditRecoveryIntentV2::PublicApply { .. }, EditRecoveryReceiptV2::PublicApply { .. })
        | (EditRecoveryIntentV2::Restore { .. }, EditRecoveryReceiptV2::Restore { .. }) => Ok(()),
        (EditRecoveryIntentV2::Undo { request }, EditRecoveryReceiptV2::Undo { commit })
            if commit.target_transaction_id == request.target_transaction_id =>
        {
            Ok(())
        }
        (EditRecoveryIntentV2::Redo { request }, EditRecoveryReceiptV2::Redo { commit })
            if commit.undo_transaction_id.as_deref()
                == Some(request.undo_transaction_id.as_str()) =>
        {
            Ok(())
        }
        _ => Err(corrupt()),
    }
}

fn validate_record(record: &EditRecoveryRecordV2, project: &str) -> Result<(), EditorError> {
    if record.schema_version != 2
        || record.project_id != project
        || intent(&record.context.intent).project_id() != project
        || !record.request_digest.starts_with("sha256:")
        || record.request_digest.len() != 71
        || !record.request_digest[7..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(corrupt());
    }
    id(&record.recovery_id).map_err(|_| corrupt())?;
    validate_context(&record.context).map_err(|_| corrupt())?;
    if let Some(receipt) = &record.receipt {
        validate_original_receipt(&record.context, receipt)?;
    }
    Ok(())
}

pub fn stage(
    store: &mut impl EditRecoveryStoreV2,
    request: &StageEditRecoveryV2,
) -> Result<EditRecoveryRecordV2, EditorError> {
    let prepared = prepare(request)?;
    let record = store.stage_recovery_v2(request, &prepared)?;
    validate_record(&record, intent(&request.context.intent).project_id())?;
    if record.context != request.context || record.request_digest != prepared.digest() {
        return Err(corrupt());
    }
    Ok(record)
}

pub fn inspect(
    store: &impl EditRecoveryStoreV2,
    request: &ReadEditRecoveryV2,
) -> Result<EditRecoveryInspectionV2, EditorError> {
    id(&request.project_id)?;
    if let Some(recovery) = &request.recovery_id {
        id(recovery)?;
    }
    let inspection = store.inspect_recovery_v2(request)?;
    if inspection.schema_version != 2 {
        return Err(corrupt());
    }
    let found = match &inspection.record {
        Some(DiscoveredEditRecoveryV2::CurrentV2 { record }) => {
            validate_record(record, &request.project_id)?;
            Some(record.recovery_id.as_str())
        }
        Some(DiscoveredEditRecoveryV2::LegacyV1 { record }) => {
            let transaction = crate::edit_recovery_v1::transaction(&record.context)?;
            let opencut_contracts::edit_recovery_v1::EditRecoveryContextV1::TimelineV4 {
                request: original,
                ..
            } = &record.context;
            if record.schema_version != 1
                || record.project_id != request.project_id
                || transaction.project_id != request.project_id
                || original.project_id != request.project_id
                || original.sequence_id != record.sequence_id
                || record.receipt.as_ref().is_some_and(|receipt| {
                    receipt.base_revision != transaction.expected_revision
                        || receipt.transaction_id.is_empty()
                        || receipt.revision.is_empty()
                })
            {
                return Err(corrupt());
            }
            id(&record.recovery_id).map_err(|_| corrupt())?;
            Some(record.recovery_id.as_str())
        }
        None => None,
    };
    if request
        .recovery_id
        .as_deref()
        .is_some_and(|id| found.is_some_and(|found| found != id))
    {
        return Err(corrupt());
    }
    Ok(inspection)
}

pub fn ack(
    store: &mut impl EditRecoveryStoreV2,
    request: &AckEditRecoveryV2,
) -> Result<EditRecoveryAckV2, EditorError> {
    id(&request.project_id)?;
    id(&request.recovery_id)?;
    let acknowledgement = store.ack_recovery_v2(request)?;
    if acknowledgement.schema_version != 2
        || acknowledgement.recovery_id != request.recovery_id
        || (acknowledgement.resolution == EditRecoveryResolutionV2::Saved)
            != acknowledgement.receipt.is_some()
    {
        return Err(corrupt());
    }
    if let Some(receipt) = &acknowledgement.receipt {
        validate_receipt_shape(receipt)?;
    }
    Ok(acknowledgement)
}

#[cfg(test)]
mod tests;
