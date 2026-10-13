//! Durable recovery of the exact original import; recovery never acquires fonts.
pub use opencut_contracts::font_recovery_v1::*;
use opencut_contracts::{EditorError, TransactionResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub trait FontRecoveryStoreV1 {
    fn stage_recovery_font_v1(
        &mut self,
        request: &StageFontRecoveryV1,
        prepared: &PreparedFontRecoveryV1,
    ) -> Result<FontRecoveryRecordV1, EditorError>;
    fn inspect_recovery_font_v1(
        &self,
        request: &ReadFontRecoveryV1,
    ) -> Result<FontRecoveryInspectionV1, EditorError>;
    fn ack_recovery_font_v1(
        &mut self,
        request: &AckFontRecoveryV1,
    ) -> Result<FontRecoveryAckV1, EditorError>;
}

/// Private storage format, distinct from both frozen recovery wire protocols.
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Envelope {
    FontImportV1 { context: FontRecoveryContextV1 },
}
pub struct PreparedFontRecoveryV1 {
    payload: String,
    digest: String,
}
impl PreparedFontRecoveryV1 {
    pub fn payload(&self) -> &str {
        &self.payload
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
}
fn invalid() -> EditorError {
    EditorError::new("INVALID_ARGUMENT", "Invalid font recovery context")
}
fn corrupt() -> EditorError {
    EditorError::new("STORAGE_ERROR", "Font recovery context or receipt differs")
}
fn id(value: &str) -> Result<(), EditorError> {
    if value.trim().is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        return Err(invalid());
    }
    Ok(())
}
fn validate(context: &FontRecoveryContextV1) -> Result<(), EditorError> {
    super::validate_import_request_v1(&context.request)?;
    id(&context.request.expected_revision)?;
    if let Some(sequence) = &context.sequence_id {
        id(sequence)?;
    }
    crate::project_v4::V4Intent::FontImport(&context.request).identity()?;
    Ok(())
}
pub fn exact_payload_digest(payload: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"opencut.font-recovery.v1\0");
    hash.update(payload.as_bytes());
    format!("sha256:{:x}", hash.finalize())
}
pub fn prepare(request: &StageFontRecoveryV1) -> Result<PreparedFontRecoveryV1, EditorError> {
    validate(&request.context)?;
    let payload = serde_json::to_string(&Envelope::FontImportV1 {
        context: request.context.clone(),
    })
    .map_err(|_| invalid())?;
    if payload.len() > crate::edit_recovery_v1::MAX_RECORD_BYTES {
        return Err(EditorError::new(
            "RECOVERY_QUOTA_EXCEEDED",
            "Font recovery context exceeds the record budget",
        ));
    }
    let digest = exact_payload_digest(&payload);
    Ok(PreparedFontRecoveryV1 { payload, digest })
}
/// Validate exact stored bytes, then typed invariants, without normalizing the
/// original digest. SQLite uses this when sharing the V2 table's exclusive slot.
pub fn decode_payload(
    payload: &str,
    digest: &str,
    project: &str,
) -> Result<StageFontRecoveryV1, EditorError> {
    if payload.len() > crate::edit_recovery_v1::MAX_RECORD_BYTES
        || exact_payload_digest(payload) != digest
    {
        return Err(corrupt());
    }
    let Envelope::FontImportV1 { context } =
        serde_json::from_str(payload).map_err(|_| corrupt())?;
    validate(&context).map_err(|_| corrupt())?;
    if context.request.project_id != project {
        return Err(corrupt());
    }
    Ok(StageFontRecoveryV1 { context })
}
pub fn validate_receipt(
    context: &FontRecoveryContextV1,
    receipt: &TransactionResult,
) -> Result<(), EditorError> {
    for value in [
        &receipt.transaction_id,
        &receipt.base_revision,
        &receipt.revision,
    ] {
        id(value).map_err(|_| corrupt())?;
    }
    if receipt.base_revision != context.request.expected_revision {
        return Err(corrupt());
    }
    Ok(())
}
fn validate_record(record: &FontRecoveryRecordV1, project: &str) -> Result<(), EditorError> {
    if record.schema_version != 1
        || record.project_id != project
        || record.context.request.project_id != project
        || !record.request_digest.starts_with("sha256:")
        || record.request_digest.len() != 71
        || !record.request_digest[7..]
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
    {
        return Err(corrupt());
    }
    id(&record.recovery_id).map_err(|_| corrupt())?;
    validate(&record.context).map_err(|_| corrupt())?;
    if let Some(receipt) = &record.receipt {
        validate_receipt(&record.context, receipt)?;
    }
    Ok(())
}
pub fn stage(
    store: &mut impl FontRecoveryStoreV1,
    request: &StageFontRecoveryV1,
) -> Result<FontRecoveryRecordV1, EditorError> {
    let prepared = prepare(request)?;
    let result = store.stage_recovery_font_v1(request, &prepared)?;
    validate_record(&result, &request.context.request.project_id)?;
    if result.context != request.context || result.request_digest != prepared.digest {
        return Err(corrupt());
    }
    Ok(result)
}
pub fn inspect(
    store: &impl FontRecoveryStoreV1,
    request: &ReadFontRecoveryV1,
) -> Result<FontRecoveryInspectionV1, EditorError> {
    id(&request.project_id)?;
    if let Some(recovery) = &request.recovery_id {
        id(recovery)?;
    }
    let result = store.inspect_recovery_font_v1(request)?;
    if result.schema_version != 1 {
        return Err(corrupt());
    }
    if let Some(record) = &result.record {
        validate_record(record, &request.project_id)?;
        if request
            .recovery_id
            .as_ref()
            .is_some_and(|id| id != &record.recovery_id)
        {
            return Err(corrupt());
        }
    }
    Ok(result)
}
pub fn ack(
    store: &mut impl FontRecoveryStoreV1,
    request: &AckFontRecoveryV1,
) -> Result<FontRecoveryAckV1, EditorError> {
    id(&request.project_id)?;
    id(&request.recovery_id)?;
    let result = store.ack_recovery_font_v1(request)?;
    if result.schema_version != 1
        || result.recovery_id != request.recovery_id
        || (result.resolution == FontRecoveryResolutionV1::Saved) != result.receipt.is_some()
    {
        return Err(corrupt());
    }
    if let Some(receipt) = &result.receipt {
        for value in [
            &receipt.transaction_id,
            &receipt.base_revision,
            &receipt.revision,
        ] {
            id(value).map_err(|_| corrupt())?;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> StageFontRecoveryV1 {
        StageFontRecoveryV1 {
            context: FontRecoveryContextV1 {
                request: opencut_contracts::font_import_v1::ImportFontV1 {
                    project_id: "p".into(),
                    expected_revision: "r".into(),
                    idempotency_key: "k".into(),
                    catalog_id: "google-lato-regular-v1".into(),
                    font_id: "f".into(),
                    required_text: "AV To".into(),
                },
                sequence_id: Some("s".into()),
            },
        }
    }
    #[test]
    fn font_recovery_digest_has_a_distinct_domain_and_preserves_exact_payload() {
        let request = request();
        let prepared = prepare(&request).unwrap();
        assert_ne!(
            prepared.digest(),
            crate::edit_recovery_v2::exact_payload_digest(prepared.payload())
        );
        assert_eq!(
            decode_payload(prepared.payload(), prepared.digest(), "p").unwrap(),
            request
        );
        let raw = format!(" {}", prepared.payload());
        assert_eq!(
            decode_payload(&raw, prepared.digest(), "p")
                .unwrap_err()
                .code,
            "STORAGE_ERROR"
        );
        assert_eq!(
            decode_payload(&raw, &exact_payload_digest(&raw), "p").unwrap(),
            request
        );
        assert_eq!(
            decode_payload(prepared.payload(), prepared.digest(), "other")
                .unwrap_err()
                .code,
            "STORAGE_ERROR"
        );
    }
    #[test]
    fn font_recovery_context_and_current_text_are_part_of_the_identity() {
        let initial = request();
        let original = prepare(&initial).unwrap();
        for change in 0..3 {
            let mut changed = initial.clone();
            match change {
                0 => changed.context.request.required_text = "Different".into(),
                1 => changed.context.request.catalog_id = "google-mashanzheng-regular-v1".into(),
                _ => changed.context.sequence_id = None,
            }
            assert_ne!(prepare(&changed).unwrap().digest(), original.digest());
        }
        let mut changed = initial;
        changed.context.request.catalog_id = "unreviewed".into();
        assert!(prepare(&changed).is_err());
    }
}
