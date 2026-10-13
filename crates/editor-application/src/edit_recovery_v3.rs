//! Exact original caption intents. Recovery inspection never executes an edit.
use crate::project_v4::V4Intent;
pub use opencut_contracts::edit_recovery_v3::*;
use opencut_contracts::{EditorError, subtitle_v4::CaptionReceiptV4};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Envelope {
    CaptionEditV3 { context: EditRecoveryContextV3 },
}
pub struct PreparedEditRecoveryV3 {
    payload: String,
    digest: String,
}
impl PreparedEditRecoveryV3 {
    pub fn payload(&self) -> &str {
        &self.payload
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
}
pub trait EditRecoveryStoreV3 {
    fn stage_recovery_v3(
        &mut self,
        input: &StageEditRecoveryV3,
        prepared: &PreparedEditRecoveryV3,
    ) -> Result<EditRecoveryRecordV3, EditorError>;
    fn inspect_recovery_v3(
        &self,
        input: &ReadEditRecoveryV3,
    ) -> Result<EditRecoveryInspectionV3, EditorError>;
    fn ack_recovery_v3(
        &mut self,
        input: &AckEditRecoveryV3,
    ) -> Result<EditRecoveryAckV3, EditorError>;
}
fn invalid() -> EditorError {
    EditorError::new("INVALID_ARGUMENT", "Invalid caption recovery context")
}
fn corrupt() -> EditorError {
    EditorError::new(
        "STORAGE_ERROR",
        "Original caption recovery identity differs",
    )
}
fn id(v: &str) -> Result<(), EditorError> {
    if v.trim().is_empty() || v.len() > 1024 || v.chars().any(char::is_control) {
        return Err(invalid());
    }
    Ok(())
}
pub fn intent(context: &EditRecoveryContextV3) -> V4Intent<'_> {
    match &context.intent {
        EditRecoveryIntentV3::SubtitleGenerateV4 { request } => V4Intent::CaptionGenerate(request),
        EditRecoveryIntentV3::SubtitleImportV4 { request } => V4Intent::CaptionImport(request),
    }
}
fn validate(context: &EditRecoveryContextV3) -> Result<(), EditorError> {
    let original = intent(context);
    id(original.project_id())?;
    id(original.expected_revision().ok_or_else(invalid)?)?;
    crate::require_idempotency_key(original.key())?;
    let scene = match &context.intent {
        EditRecoveryIntentV3::SubtitleGenerateV4 { request } => {
            id(&request.track_id)?;
            id(&request.analysis_job_id)?;
            if !(1..=20).contains(&request.words_per_caption) {
                return Err(invalid());
            }
            &request.sequence_id
        }
        EditRecoveryIntentV3::SubtitleImportV4 { request } => {
            id(&request.track_id)?;
            if request.content.len() > 2 * 1024 * 1024 {
                return Err(invalid());
            }
            &request.sequence_id
        }
    };
    id(scene)?;
    if context.sequence_id.as_ref().is_some_and(|s| s != scene) {
        return Err(invalid());
    }
    original.identity()?;
    Ok(())
}
pub fn exact_payload_digest(payload: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"opencut.edit-recovery.v3\0");
    hash.update(payload.as_bytes());
    format!("sha256:{:x}", hash.finalize())
}
pub fn prepare(input: &StageEditRecoveryV3) -> Result<PreparedEditRecoveryV3, EditorError> {
    validate(&input.context)?;
    let payload = serde_json::to_string(&Envelope::CaptionEditV3 {
        context: input.context.clone(),
    })
    .map_err(|_| invalid())?;
    if payload.len() > crate::edit_recovery_v1::MAX_RECORD_BYTES {
        return Err(EditorError::new(
            "RECOVERY_QUOTA_EXCEEDED",
            "Caption context exceeds recovery record capacity",
        ));
    }
    let digest = exact_payload_digest(&payload);
    Ok(PreparedEditRecoveryV3 { payload, digest })
}
pub fn decode_payload(
    payload: &str,
    digest: &str,
    project: &str,
) -> Result<StageEditRecoveryV3, EditorError> {
    if payload.len() > crate::edit_recovery_v1::MAX_RECORD_BYTES
        || exact_payload_digest(payload) != digest
    {
        return Err(corrupt());
    }
    let Envelope::CaptionEditV3 { context } =
        serde_json::from_str(payload).map_err(|_| corrupt())?;
    validate(&context).map_err(|_| corrupt())?;
    if intent(&context).project_id() != project {
        return Err(corrupt());
    }
    Ok(StageEditRecoveryV3 { context })
}
pub fn validate_receipt(
    context: &EditRecoveryContextV3,
    receipt: &CaptionReceiptV4,
) -> Result<(), EditorError> {
    let tx = &receipt.transaction;
    for v in [&tx.transaction_id, &tx.base_revision, &tx.revision] {
        id(v).map_err(|_| corrupt())?;
    }
    if Some(tx.base_revision.as_str()) != intent(context).expected_revision()
        || receipt.cue_count == 0
        || receipt.cue_count > 5000
    {
        return Err(corrupt());
    }
    Ok(())
}
fn record(record: &EditRecoveryRecordV3, project: &str) -> Result<(), EditorError> {
    if record.schema_version != 3
        || record.project_id != project
        || intent(&record.context).project_id() != project
    {
        return Err(corrupt());
    }
    id(&record.recovery_id).map_err(|_| corrupt())?;
    let prepared = prepare(&StageEditRecoveryV3 {
        context: record.context.clone(),
    })?;
    if prepared.digest() != record.request_digest {
        return Err(corrupt());
    }
    if let Some(receipt) = &record.receipt {
        validate_receipt(&record.context, receipt)?
    }
    Ok(())
}
pub fn stage(
    store: &mut impl EditRecoveryStoreV3,
    input: &StageEditRecoveryV3,
) -> Result<EditRecoveryRecordV3, EditorError> {
    let prepared = prepare(input)?;
    let value = store.stage_recovery_v3(input, &prepared)?;
    record(&value, intent(&input.context).project_id())?;
    if value.context != input.context {
        return Err(corrupt());
    }
    Ok(value)
}
pub fn inspect(
    store: &impl EditRecoveryStoreV3,
    input: &ReadEditRecoveryV3,
) -> Result<EditRecoveryInspectionV3, EditorError> {
    id(&input.project_id)?;
    if let Some(v) = &input.recovery_id {
        id(v)?
    }
    let value = store.inspect_recovery_v3(input)?;
    if value.schema_version != 3 {
        return Err(corrupt());
    }
    if let Some(r) = &value.record {
        record(r, &input.project_id)?;
        if input
            .recovery_id
            .as_ref()
            .is_some_and(|id| id != &r.recovery_id)
        {
            return Err(corrupt());
        }
    }
    Ok(value)
}
pub fn ack(
    store: &mut impl EditRecoveryStoreV3,
    input: &AckEditRecoveryV3,
) -> Result<EditRecoveryAckV3, EditorError> {
    id(&input.project_id)?;
    id(&input.recovery_id)?;
    let value = store.ack_recovery_v3(input)?;
    if value.schema_version != 3
        || value.recovery_id != input.recovery_id
        || (value.resolution == EditRecoveryResolutionV3::Saved) != value.receipt.is_some()
    {
        return Err(corrupt());
    }
    Ok(value)
}
