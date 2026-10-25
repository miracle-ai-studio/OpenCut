//! Recover original public write intents without re-planning or auto-dispatch.
//! Storage owns live authorization, shared quotas and atomic resolution proof.
pub const MAX_RECORD_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_UNRESOLVED_RECORDS: u64 = 32;
pub const MAX_UNRESOLVED_BYTES: u64 = 64 * 1024 * 1024;
#[path = "edit_recovery/stored.rs"]
mod stored;
use crate::scene::SceneCommitIntent;
pub use opencut_contracts::edit_recovery::*;
use opencut_contracts::{EditorError, TransactionResult, project_history::Direction};
use sha2::{Digest, Sha256};
pub use stored::decode_payload;

pub trait EditRecoveryStore {
    /// Revalidate the original action/resource authority before preparation and
    /// under an Immediate lock. Exact existing records precede head checks.
    /// All edit intents share32records/64MiB and one unresolved record per
    /// authenticated principal/credential/owner/project.
    fn stage_recovery(
        &mut self,
        request: &StageEditRecovery,
        prepared: &PreparedEditRecovery,
    ) -> Result<EditRecoveryRecord, EditorError>;
    /// Read only; verify original intent authority and typed receipt without
    /// dispatching writes or inferring success from the current head.
    fn inspect_recovery(
        &self,
        request: &ReadEditRecovery,
    ) -> Result<EditRecoveryInspection, EditorError>;
    /// Under an Immediate lock reauthorize and read the original receipt first.
    /// Delete only when it exists, or when no receipt exists and the original
    /// fixed-head request is impossible. Permission/lease/transport failures or
    /// an unchanged head retain the record. Resolution and deletion are atomic.
    fn ack_recovery(&mut self, request: &AckEditRecovery) -> Result<EditRecoveryAck, EditorError>;
}

pub struct PreparedEditRecovery {
    payload: String,
    digest: String,
}
impl PreparedEditRecovery {
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

pub fn intent(request: &EditRecoveryIntent) -> Result<SceneCommitIntent<'_>, EditorError> {
    Ok(match request {
        EditRecoveryIntent::PublicApply { request } => SceneCommitIntent::PublicApply(request),
        EditRecoveryIntent::Undo { request } => SceneCommitIntent::Undo(request),
        EditRecoveryIntent::Redo { request } => SceneCommitIntent::Redo(request),
        EditRecoveryIntent::Restore { request } => SceneCommitIntent::Restore(request),
        EditRecoveryIntent::SubtitleGenerate { request } => {
            SceneCommitIntent::CaptionGenerate(request)
        }
        EditRecoveryIntent::SubtitleImport { request } => SceneCommitIntent::CaptionImport(request),
        EditRecoveryIntent::FontImport { request } => {
            if crate::font_catalog::selection::validate(request)?.derived() {
                SceneCommitIntent::DerivedFontImport(request)
            } else {
                SceneCommitIntent::FontImport(request)
            }
        }
    })
}

pub fn exact_payload_digest(payload: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"opencut.edit-recovery\0");
    hash.update(payload.as_bytes());
    format!("sha256:{:x}", hash.finalize())
}

fn validate_context(context: &EditRecoveryContext) -> Result<(), EditorError> {
    let original = intent(&context.intent)?;
    id(original.project_id())?;
    id(original.expected_revision().ok_or_else(invalid)?)?;
    id(original.key())?;
    crate::require_idempotency_key(original.key())?;
    if let Some(sequence) = &context.sequence_id {
        id(sequence)?;
    }
    match &context.intent {
        EditRecoveryIntent::PublicApply { request } => {
            if request.operations.is_empty()
                || request.operations.len() > 128
                || request.label.trim().is_empty()
                || request.label.len() > 4096
            {
                return Err(invalid());
            }
        }
        EditRecoveryIntent::FontImport { .. } => {}
        EditRecoveryIntent::Undo { request } => id(&request.target_transaction_id)?,
        EditRecoveryIntent::Redo { request } => id(&request.undo_transaction_id)?,
        EditRecoveryIntent::Restore { request } => id(&request.target_revision)?,
        EditRecoveryIntent::SubtitleGenerate { request } => {
            id(&request.track_id)?;
            id(&request.analysis_job_id)?;
            id(&request.sequence_id)?;
            if !(1..=20).contains(&request.words_per_caption)
                || context
                    .sequence_id
                    .as_ref()
                    .is_some_and(|s| s != &request.sequence_id)
            {
                return Err(invalid());
            }
        }
        EditRecoveryIntent::SubtitleImport { request } => {
            id(&request.track_id)?;
            id(&request.sequence_id)?;
            if request.content.len() > MAX_RECORD_BYTES
                || context
                    .sequence_id
                    .as_ref()
                    .is_some_and(|s| s != &request.sequence_id)
            {
                return Err(invalid());
            }
        }
    }
    // Use the executable family's shared typed identity, never a client family
    // string or resolved private resource record. Storage uses this same intent.
    original.identity()?;
    Ok(())
}

pub fn prepare(request: &StageEditRecovery) -> Result<PreparedEditRecovery, EditorError> {
    validate_context(&request.context)?;
    let payload = serde_json::to_string(request).map_err(|_| invalid())?;
    if payload.len() > MAX_RECORD_BYTES {
        return Err(EditorError::new(
            "RECOVERY_QUOTA_EXCEEDED",
            "Recovery context exceeds the 2 MiB record budget",
        ));
    }
    let digest = exact_payload_digest(&payload);
    Ok(PreparedEditRecovery { payload, digest })
}

pub fn receipt_transaction(receipt: &EditRecoveryReceipt) -> &TransactionResult {
    match receipt {
        EditRecoveryReceipt::PublicApply { transaction }
        | EditRecoveryReceipt::Restore { transaction }
        | EditRecoveryReceipt::FontImport { transaction } => transaction,
        EditRecoveryReceipt::SubtitleGenerate { caption }
        | EditRecoveryReceipt::SubtitleImport { caption } => &caption.transaction,
        EditRecoveryReceipt::Undo { commit } | EditRecoveryReceipt::Redo { commit } => {
            &commit.transaction
        }
    }
}
fn validate_receipt_shape(receipt: &EditRecoveryReceipt) -> Result<(), EditorError> {
    let transaction = receipt_transaction(receipt);
    for value in [
        &transaction.transaction_id,
        &transaction.base_revision,
        &transaction.revision,
    ] {
        id(value).map_err(|_| corrupt())?;
    }
    match receipt {
        EditRecoveryReceipt::Undo { commit } => {
            if commit.direction != Direction::Undo || commit.undo_transaction_id.is_some() {
                return Err(corrupt());
            }
            id(&commit.target_transaction_id).map_err(|_| corrupt())?;
        }
        EditRecoveryReceipt::Redo { commit } => {
            if commit.direction != Direction::Redo {
                return Err(corrupt());
            }
            id(&commit.target_transaction_id).map_err(|_| corrupt())?;
            id(commit.undo_transaction_id.as_deref().ok_or_else(corrupt)?)
                .map_err(|_| corrupt())?;
        }
        EditRecoveryReceipt::SubtitleGenerate { caption }
        | EditRecoveryReceipt::SubtitleImport { caption } => {
            if caption.cue_count == 0 || caption.cue_count > 5000 {
                return Err(corrupt());
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn validate_original_receipt(
    context: &EditRecoveryContext,
    receipt: &EditRecoveryReceipt,
) -> Result<(), EditorError> {
    validate_receipt_shape(receipt)?;
    if Some(receipt_transaction(receipt).base_revision.as_str())
        != intent(&context.intent)?.expected_revision()
    {
        return Err(corrupt());
    }
    match (&context.intent, receipt) {
        (EditRecoveryIntent::PublicApply { .. }, EditRecoveryReceipt::PublicApply { .. })
        | (EditRecoveryIntent::FontImport { .. }, EditRecoveryReceipt::FontImport { .. })
        | (EditRecoveryIntent::Restore { .. }, EditRecoveryReceipt::Restore { .. })
        | (
            EditRecoveryIntent::SubtitleGenerate { .. },
            EditRecoveryReceipt::SubtitleGenerate { .. },
        )
        | (EditRecoveryIntent::SubtitleImport { .. }, EditRecoveryReceipt::SubtitleImport { .. }) => {
            Ok(())
        }
        (EditRecoveryIntent::Undo { request }, EditRecoveryReceipt::Undo { commit })
            if commit.target_transaction_id == request.target_transaction_id =>
        {
            Ok(())
        }
        (EditRecoveryIntent::Redo { request }, EditRecoveryReceipt::Redo { commit })
            if commit.undo_transaction_id.as_deref()
                == Some(request.undo_transaction_id.as_str()) =>
        {
            Ok(())
        }
        _ => Err(corrupt()),
    }
}

fn validate_record(record: &EditRecoveryRecord, project: &str) -> Result<(), EditorError> {
    if record.project_id != project
        || intent(&record.context.intent)?.project_id() != project
        || !record.request_digest.starts_with("sha256:")
        || record.request_digest.len() != 71
        || !record.request_digest[7..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(corrupt());
    }
    id(&record.recovery_id).map_err(|_| corrupt())?;
    let canonical = prepare(&StageEditRecovery {
        context: record.context.clone(),
    })
    .map_err(|_| corrupt())?;
    if canonical.digest() != record.request_digest {
        return Err(corrupt());
    }
    if let Some(receipt) = &record.receipt {
        validate_original_receipt(&record.context, receipt)?;
    }
    Ok(())
}

pub fn stage(
    store: &mut impl EditRecoveryStore,
    request: &StageEditRecovery,
) -> Result<EditRecoveryRecord, EditorError> {
    let prepared = prepare(request)?;
    let record = store.stage_recovery(request, &prepared)?;
    validate_record(&record, intent(&request.context.intent)?.project_id())?;
    if record.context != request.context || record.request_digest != prepared.digest() {
        return Err(corrupt());
    }
    Ok(record)
}

pub fn inspect(
    store: &impl EditRecoveryStore,
    request: &ReadEditRecovery,
) -> Result<EditRecoveryInspection, EditorError> {
    id(&request.project_id)?;
    if let Some(recovery) = &request.recovery_id {
        id(recovery)?;
    }
    let inspection = store.inspect_recovery(request)?;
    let found = if let Some(record) = &inspection.record {
        validate_record(record, &request.project_id)?;
        Some(record.recovery_id.as_str())
    } else {
        None
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
    store: &mut impl EditRecoveryStore,
    request: &AckEditRecovery,
) -> Result<EditRecoveryAck, EditorError> {
    id(&request.project_id)?;
    id(&request.recovery_id)?;
    let acknowledgement = store.ack_recovery(request)?;
    if acknowledgement.recovery_id != request.recovery_id
        || (acknowledgement.resolution == EditRecoveryResolution::Saved)
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
