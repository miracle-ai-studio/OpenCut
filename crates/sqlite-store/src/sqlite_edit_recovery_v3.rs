//! Caption recovery reuses the existing authenticated exclusive slot and quota.
use super::sqlite_edit_recovery_shared::{Scope, V2SlotKind, quota, scope};
use super::*;
use opencut_contracts::subtitle_v4::CaptionReceiptV4;
use opencut_editor_application::{
    audit::ExecutionContext,
    edit_recovery_v3::{self as recovery, *},
};
struct Stored {
    id: String,
    project: String,
    digest: String,
    payload: String,
}
fn load(
    c: &Connection,
    scope: &Scope,
    project: &str,
    id: Option<&str>,
) -> Result<Option<Stored>, EditorError> {
    c.query_row("SELECT recovery_id,project_id,request_digest,context_json FROM edit_recovery_v2 WHERE project_id=?1 AND principal_id=?2 AND credential_id=?3 AND owner=?4 AND (?5 IS NULL OR recovery_id=?5)",params![project,scope.principal,scope.credential,scope.owner,id],|r|Ok(Stored{id:r.get(0)?,project:r.get(1)?,digest:r.get(2)?,payload:r.get(3)?})).optional().map_err(storage_error)
}
fn decode(s: &Stored) -> Result<StageEditRecoveryV3, EditorError> {
    recovery::decode_payload(&s.payload, &s.digest, &s.project)
}
fn receipt(
    c: &Connection,
    context: &EditRecoveryContextV3,
) -> Result<Option<CaptionReceiptV4>, EditorError> {
    let intent = recovery::intent(context);
    let Some(raw) = load_idempotent(
        c,
        intent.family(),
        intent.scope(),
        intent.key(),
        &intent.identity()?,
        "transaction",
    )?
    else {
        return Ok(None);
    };
    let receipt: CaptionReceiptV4 = decode_json(&raw, "Original caption receipt")?;
    recovery::validate_receipt(context, &receipt)?;
    let journal = sqlite_edit_recovery_v2::receipt_journal(c, intent, &receipt.transaction)?;
    let stored_intent = match &context.intent {
        EditRecoveryIntentV3::SubtitleGenerateV4 { request } => {
            let stored: opencut_contracts::subtitle_v4::GenerateCaptionsV4 =
                decode_json(&journal, "Original caption generation journal")?;
            if &stored != request {
                return Err(EditorError::new(
                    "STORAGE_ERROR",
                    "Original caption request differs from journal",
                ));
            }
        }
        EditRecoveryIntentV3::SubtitleImportV4 { request } => {
            let stored: opencut_contracts::subtitle_v4::SubtitleImportV4 =
                decode_json(&journal, "Original subtitle import journal")?;
            if &stored != request {
                return Err(EditorError::new(
                    "STORAGE_ERROR",
                    "Original subtitle request differs from journal",
                ));
            }
        }
    };
    let _ = stored_intent;
    if sqlite_caption_v4::caption_receipt(c, intent, &receipt.transaction)? != receipt {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Original caption receipt differs from revision effect",
        ));
    }
    Ok(Some(receipt))
}
fn require(
    c: &Connection,
    caller: &ExecutionContext,
    context: &EditRecoveryContextV3,
) -> Result<(), EditorError> {
    let intent = recovery::intent(context);
    sqlite_project_v4::authorize(c, caller, intent)?;
    if let EditRecoveryIntentV3::SubtitleGenerateV4 { request } = &context.intent {
        sqlite_caption_v4::require_generation_source(c, caller, request)?;
    }
    Ok(())
}
fn record(
    c: &Connection,
    s: Stored,
    input: StageEditRecoveryV3,
) -> Result<EditRecoveryRecordV3, EditorError> {
    Ok(EditRecoveryRecordV3 {
        schema_version: 3,
        recovery_id: s.id,
        project_id: s.project,
        request_digest: s.digest,
        receipt: receipt(c, &input.context)?,
        context: input.context,
    })
}
fn require_read(c: &Connection, caller: &ExecutionContext, p: &str) -> Result<(), EditorError> {
    sqlite_access::require(c, caller, p, ProjectAction::Read)?;
    sqlite_project_library::require_live(c, p)
}
fn proposal(
    c: &Connection,
    caller: &ExecutionContext,
    context: &EditRecoveryContextV3,
) -> Result<(), EditorError> {
    let original = recovery::intent(context);
    let fixed = sqlite_project_v4_jobs::fixed(
        c,
        original.project_id(),
        original.expected_revision().unwrap(),
    )?;
    match &context.intent {
        EditRecoveryIntentV3::SubtitleGenerateV4 { request } => {
            let result = sqlite_caption_v4::require_generation_source(c, caller, request)?;
            opencut_editor_application::caption_v4::generate(&fixed, request, &result)?;
        }
        EditRecoveryIntentV3::SubtitleImportV4 { request } => {
            opencut_editor_application::caption_v4::import(&fixed, request)?;
        }
    }
    Ok(())
}
impl EditRecoveryStoreV3 for EditorApplication {
    fn stage_recovery_v3(
        &mut self,
        input: &StageEditRecoveryV3,
        prepared: &PreparedEditRecoveryV3,
    ) -> Result<EditRecoveryRecordV3, EditorError> {
        let checked = recovery::prepare(input)?;
        if checked.payload() != prepared.payload() || checked.digest() != prepared.digest() {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Prepared caption context differs",
            ));
        }
        let caller = self.connection.1.clone();
        let identity = scope(&caller);
        let intent = recovery::intent(&input.context);
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        require(&tx, &caller, &input.context)?;
        sqlite_edit_recovery_shared::require_no_other_version(
            &tx,
            &identity,
            intent.project_id(),
            intent.key(),
            2,
        )?;
        if let Some(stored) = load(&tx, &identity, intent.project_id(), None)? {
            if stored.payload != prepared.payload() || stored.digest != prepared.digest() {
                let old = sqlite_edit_recovery_shared::classify_v2_payload(
                    &stored.payload,
                    &stored.digest,
                    &stored.project,
                )?;
                return Err(EditorError::new(
                    if old.key() == intent.key() {
                        "IDEMPOTENCY_KEY_REUSED"
                    } else {
                        "RECOVERY_CONFLICT"
                    },
                    "Resolve the original staged edit before replacing it",
                ));
            }
            let decoded = decode(&stored)?;
            let result = record(&tx, stored, decoded)?;
            tx.commit().map_err(storage_error)?;
            return Ok(result);
        }
        receipt(&tx, &input.context)?;
        let head = sqlite_project_document::project_head_revision(&tx, intent.project_id())?;
        if Some(head.as_str()) != intent.expected_revision() {
            return Err(EditorError::new(
                "REVISION_CONFLICT",
                "Project changed before original caption staging",
            ));
        }
        proposal(&tx, &caller, &input.context)?;
        let id = format!("recovery_{}", Uuid::new_v4().simple());
        let bytes = prepared
            .payload()
            .len()
            .checked_add(
                id.len()
                    + intent.project_id().len()
                    + identity.principal.len()
                    + identity.credential.len()
                    + prepared.digest().len()
                    + 16,
            )
            .ok_or_else(quota)?;
        sqlite_edit_recovery_shared::reserve(&tx, bytes)?;
        tx.execute("INSERT INTO edit_recovery_v2(recovery_id,project_id,principal_id,credential_id,owner,request_digest,context_json,stored_bytes) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![id,intent.project_id(),identity.principal,identity.credential,identity.owner,prepared.digest(),prepared.payload(),bytes as u64]).map_err(storage_error)?;
        let result = record(
            &tx,
            Stored {
                id,
                project: intent.project_id().into(),
                digest: prepared.digest().into(),
                payload: prepared.payload().into(),
            },
            input.clone(),
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
    fn inspect_recovery_v3(
        &self,
        input: &ReadEditRecoveryV3,
    ) -> Result<EditRecoveryInspectionV3, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let caller = &self.connection.1;
        require_read(&tx, caller, &input.project_id)?;
        let record = if let Some(stored) = load(
            &tx,
            &scope(caller),
            &input.project_id,
            input.recovery_id.as_deref(),
        )? {
            if matches!(
                sqlite_edit_recovery_shared::classify_v2_payload(
                    &stored.payload,
                    &stored.digest,
                    &stored.project
                )?,
                V2SlotKind::CaptionV3 { .. }
            ) {
                let decoded = decode(&stored)?;
                require(&tx, caller, &decoded.context)?;
                Some(record(&tx, stored, decoded)?)
            } else {
                None
            }
        } else {
            None
        };
        tx.commit().map_err(storage_error)?;
        Ok(EditRecoveryInspectionV3 {
            schema_version: 3,
            record,
        })
    }
    fn ack_recovery_v3(
        &mut self,
        input: &AckEditRecoveryV3,
    ) -> Result<EditRecoveryAckV3, EditorError> {
        let caller = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        require_read(&tx, &caller, &input.project_id)?;
        let stored = load(
            &tx,
            &scope(&caller),
            &input.project_id,
            Some(&input.recovery_id),
        )?
        .ok_or_else(|| {
            EditorError::new(
                "NOT_FOUND",
                "Original caption recovery is absent in this authenticated scope",
            )
        })?;
        let decoded = decode(&stored)?;
        require(&tx, &caller, &decoded.context)?;
        let receipt = receipt(&tx, &decoded.context)?;
        let resolution = if receipt.is_some() {
            EditRecoveryResolutionV3::Saved
        } else if Some(
            sqlite_project_document::project_head_revision(&tx, &input.project_id)?.as_str(),
        ) != recovery::intent(&decoded.context).expected_revision()
        {
            EditRecoveryResolutionV3::NotSaved
        } else {
            return Err(EditorError::new(
                "RECOVERY_UNRESOLVED",
                "The original caption edit remains possible; recover it before acknowledging",
            ));
        };
        tx.execute(
            "DELETE FROM edit_recovery_v2 WHERE recovery_id=?1",
            [&stored.id],
        )
        .map_err(storage_error)?;
        let result = EditRecoveryAckV3 {
            schema_version: 3,
            recovery_id: stored.id,
            resolution,
            receipt,
        };
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
}
impl EditorApplication {
    pub fn stage_edit_recovery_v3(
        &mut self,
        input: &StageEditRecoveryV3,
    ) -> Result<EditRecoveryRecordV3, EditorError> {
        recovery::stage(self, input)
    }
    pub fn inspect_edit_recovery_v3(
        &self,
        input: &ReadEditRecoveryV3,
    ) -> Result<EditRecoveryInspectionV3, EditorError> {
        recovery::inspect(self, input)
    }
    pub fn ack_edit_recovery_v3(
        &mut self,
        input: &AckEditRecoveryV3,
    ) -> Result<EditRecoveryAckV3, EditorError> {
        recovery::ack(self, input)
    }
}
