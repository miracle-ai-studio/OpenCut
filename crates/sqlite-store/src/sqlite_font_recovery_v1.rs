//! Font recovery shares the existing V2 slot, quota and exclusion triggers.
//! It never downloads, publishes CAS bytes, replans against a new head or edits.
use super::sqlite_edit_recovery_shared::{Scope, V2SlotKind, quota, scope};
use super::*;
use opencut_editor_application::{
    audit::ExecutionContext,
    font_import_v1::{
        expected_import_resource_v1,
        recovery::{self as recovery, *},
    },
    project_v4::{StoredProjectDocumentV4, V4Intent},
};

struct Stored {
    id: String,
    project: String,
    digest: String,
    payload: String,
}
fn load(
    connection: &Connection,
    identity: &Scope,
    project: &str,
    id: Option<&str>,
) -> Result<Option<Stored>, EditorError> {
    connection.query_row("SELECT recovery_id,project_id,request_digest,context_json FROM edit_recovery_v2 WHERE project_id=?1 AND principal_id=?2 AND credential_id=?3 AND owner=?4 AND (?5 IS NULL OR recovery_id=?5)",params![project,identity.principal,identity.credential,identity.owner,id],|r|Ok(Stored{id:r.get(0)?,project:r.get(1)?,digest:r.get(2)?,payload:r.get(3)?})).optional().map_err(storage_error)
}
fn decode(stored: &Stored) -> Result<StageFontRecoveryV1, EditorError> {
    recovery::decode_payload(&stored.payload, &stored.digest, &stored.project)
}
fn kind(stored: &Stored) -> Result<V2SlotKind, EditorError> {
    sqlite_edit_recovery_shared::classify_v2_payload(
        &stored.payload,
        &stored.digest,
        &stored.project,
    )
}
fn original_receipt(
    connection: &Connection,
    context: &FontRecoveryContextV1,
) -> Result<Option<TransactionResult>, EditorError> {
    let intent = V4Intent::FontImport(&context.request);
    let Some(raw) = load_idempotent(
        connection,
        intent.family(),
        intent.scope(),
        intent.key(),
        &intent.identity()?,
        "transaction",
    )?
    else {
        return Ok(None);
    };
    let transaction: TransactionResult = decode_json(&raw, "Original font recovery receipt")?;
    let journal = sqlite_edit_recovery_v2::receipt_journal(connection, intent, &transaction)?;
    let request: opencut_contracts::font_import_v1::ImportFontV1 =
        decode_json(&journal, "Original font import journal")?;
    if V4Intent::FontImport(&request).identity()?.current_digest()
        != intent.identity()?.current_digest()
    {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Original font import journal identity differs",
        ));
    }
    recovery::validate_receipt(context, &transaction)?;
    Ok(Some(transaction))
}
fn record(
    connection: &Connection,
    stored: Stored,
    input: StageFontRecoveryV1,
) -> Result<FontRecoveryRecordV1, EditorError> {
    Ok(FontRecoveryRecordV1 {
        schema_version: 1,
        recovery_id: stored.id,
        project_id: stored.project,
        request_digest: stored.digest,
        receipt: original_receipt(connection, &input.context)?,
        context: input.context,
    })
}
fn require_read(
    connection: &Connection,
    context: &ExecutionContext,
    project: &str,
) -> Result<(), EditorError> {
    sqlite_access::require(connection, context, project, ProjectAction::Read)?;
    sqlite_project_library::require_live(connection, project)
}
fn proposal(connection: &Connection, context: &FontRecoveryContextV1) -> Result<(), EditorError> {
    let request = &context.request;
    let head = sqlite_project_document::project_head_revision(connection, &request.project_id)?;
    if head != request.expected_revision {
        return Err(EditorError::new(
            "REVISION_CONFLICT",
            "Project changed before font recovery staging; review a new plan",
        ));
    }
    let source = sqlite_project_v4::load_source(connection, &request.project_id, Some(&head))?;
    let StoredProjectDocumentV4::V4(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Font recovery requires a V4 revision",
        ));
    };
    if context
        .sequence_id
        .as_ref()
        .is_some_and(|id| !project.sequences.iter().any(|s| &s.id == id))
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Recovery scene is not in the captured revision",
        ));
    }
    let font = expected_import_resource_v1(request)?;
    opencut_editor_application::project_v4::plan(
        &opencut_contracts::project_v4::ProjectSnapshotV4 {
            project: *project,
            revision: head,
        },
        &opencut_contracts::project_v4_ops::ApplyV4Project {
            project_id: request.project_id.clone(),
            expected_revision: request.expected_revision.clone(),
            idempotency_key: request.idempotency_key.clone(),
            label: format!("Import {}", font.family),
            operations: vec![
                opencut_contracts::project_v4_ops::ProjectOperationV4::FontPut { font },
            ],
        },
    )?;
    Ok(())
}

impl FontRecoveryStoreV1 for EditorApplication {
    fn stage_recovery_font_v1(
        &mut self,
        input: &StageFontRecoveryV1,
        prepared: &PreparedFontRecoveryV1,
    ) -> Result<FontRecoveryRecordV1, EditorError> {
        let checked = recovery::prepare(input)?;
        if checked.payload() != prepared.payload() || checked.digest() != prepared.digest() {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Prepared font recovery differs from the request",
            ));
        }
        let context = self.connection.1.clone();
        let identity = scope(&context);
        let request = &input.context.request;
        let intent = V4Intent::FontImport(request);
        sqlite_project_v4::authorize(&self.connection, &context, intent)?;
        let sql = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_project_v4::authorize(&sql, &context, intent)?;
        sqlite_edit_recovery_shared::require_no_other_version(
            &sql,
            &identity,
            &request.project_id,
            &request.idempotency_key,
            2,
        )?;
        if let Some(stored) = load(&sql, &identity, &request.project_id, None)? {
            let previous = kind(&stored)?;
            if !matches!(previous, V2SlotKind::Font { .. })
                || stored.digest != prepared.digest()
                || stored.payload != prepared.payload()
            {
                return Err(EditorError::new(
                    if previous.key() == request.idempotency_key {
                        "IDEMPOTENCY_KEY_REUSED"
                    } else {
                        "RECOVERY_CONFLICT"
                    },
                    "Resolve the original staged transaction before replacing it",
                ));
            }
            let decoded = decode(&stored)?;
            let result = record(&sql, stored, decoded)?;
            sql.commit().map_err(storage_error)?;
            return Ok(result);
        }
        original_receipt(&sql, &input.context)?;
        proposal(&sql, &input.context)?;
        let id = format!("recovery_{}", Uuid::new_v4().simple());
        let bytes = prepared
            .payload()
            .len()
            .checked_add(
                id.len()
                    + request.project_id.len()
                    + identity.principal.len()
                    + identity.credential.len()
                    + prepared.digest().len()
                    + 16,
            )
            .ok_or_else(quota)?;
        sqlite_edit_recovery_shared::reserve(&sql, bytes)?;
        sql.execute("INSERT INTO edit_recovery_v2(recovery_id,project_id,principal_id,credential_id,owner,request_digest,context_json,stored_bytes) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![id,request.project_id,identity.principal,identity.credential,identity.owner,prepared.digest(),prepared.payload(),bytes as u64]).map_err(storage_error)?;
        let result = record(
            &sql,
            Stored {
                id,
                project: request.project_id.clone(),
                digest: prepared.digest().into(),
                payload: prepared.payload().into(),
            },
            input.clone(),
        )?;
        sql.commit().map_err(storage_error)?;
        Ok(result)
    }
    fn inspect_recovery_font_v1(
        &self,
        input: &ReadFontRecoveryV1,
    ) -> Result<FontRecoveryInspectionV1, EditorError> {
        let sql = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let context = &self.connection.1;
        require_read(&sql, context, &input.project_id)?;
        let found = load(
            &sql,
            &scope(context),
            &input.project_id,
            input.recovery_id.as_deref(),
        )?;
        let result = if let Some(stored) = found {
            match kind(&stored)? {
                V2SlotKind::Edit { .. }
                | V2SlotKind::DerivedFont { .. }
                | V2SlotKind::CaptionV3 { .. } => None, // The unchanged V2 discovery reports it.
                V2SlotKind::Font { .. } => {
                    let decoded = decode(&stored)?;
                    sqlite_project_v4::authorize(
                        &sql,
                        context,
                        V4Intent::FontImport(&decoded.context.request),
                    )?;
                    Some(record(&sql, stored, decoded)?)
                }
            }
        } else {
            None
        };
        sql.commit().map_err(storage_error)?;
        Ok(FontRecoveryInspectionV1 {
            schema_version: 1,
            record: result,
        })
    }
    fn ack_recovery_font_v1(
        &mut self,
        input: &AckFontRecoveryV1,
    ) -> Result<FontRecoveryAckV1, EditorError> {
        let context = self.connection.1.clone();
        let sql = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        require_read(&sql, &context, &input.project_id)?;
        let stored = load(
            &sql,
            &scope(&context),
            &input.project_id,
            Some(&input.recovery_id),
        )?
        .ok_or_else(|| {
            EditorError::new(
                "NOT_FOUND",
                "Recovery record does not exist in the authenticated font scope",
            )
        })?;
        if !matches!(kind(&stored)?, V2SlotKind::Font { .. }) {
            return Err(EditorError::new(
                "RECOVERY_PROTOCOL_REQUIRED",
                "Use the original edit recovery protocol to resolve this record",
            ));
        }
        let decoded = decode(&stored)?;
        sqlite_project_v4::authorize(
            &sql,
            &context,
            V4Intent::FontImport(&decoded.context.request),
        )?;
        let receipt = original_receipt(&sql, &decoded.context)?;
        let resolution = if receipt.is_some() {
            FontRecoveryResolutionV1::Saved
        } else if sqlite_project_document::project_head_revision(&sql, &input.project_id)?
            != decoded.context.request.expected_revision
        {
            FontRecoveryResolutionV1::NotSaved
        } else {
            return Err(EditorError::new(
                "RECOVERY_UNRESOLVED",
                "The original fixed-head font import remains possible; recover it before acknowledging",
            ));
        };
        sql.execute(
            "DELETE FROM edit_recovery_v2 WHERE recovery_id=?1",
            [&stored.id],
        )
        .map_err(storage_error)?;
        let result = FontRecoveryAckV1 {
            schema_version: 1,
            recovery_id: stored.id,
            resolution,
            receipt,
        };
        sql.commit().map_err(storage_error)?;
        Ok(result)
    }
}
impl EditorApplication {
    pub fn stage_font_recovery_v1(
        &mut self,
        request: &StageFontRecoveryV1,
    ) -> Result<FontRecoveryRecordV1, EditorError> {
        recovery::stage(self, request)
    }
    pub fn inspect_font_recovery_v1(
        &self,
        request: &ReadFontRecoveryV1,
    ) -> Result<FontRecoveryInspectionV1, EditorError> {
        recovery::inspect(self, request)
    }
    pub fn ack_font_recovery_v1(
        &mut self,
        request: &AckFontRecoveryV1,
    ) -> Result<FontRecoveryAckV1, EditorError> {
        recovery::ack(self, request)
    }
}
#[cfg(test)]
mod tests;
