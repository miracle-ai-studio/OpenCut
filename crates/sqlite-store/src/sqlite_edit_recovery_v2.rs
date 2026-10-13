//! Recovery retains the exact reviewed public intent. Receipt reads never run
//! an edit, restore or history use case and never rewrite frozen V1 records.
use super::sqlite_edit_recovery_shared::{Scope, quota, scope};
use super::*;
use opencut_contracts::project_v4_history::{CausalJournalV4, HistoryCommitV4, HistoryDirectionV4};
use opencut_editor_application::{
    audit::ExecutionContext,
    edit_recovery_v2::{self as recovery, *},
    project_v4::V4Intent,
};

pub(super) const MIGRATION: &str = "
CREATE TABLE edit_recovery_v2(
 recovery_id TEXT PRIMARY KEY,
 project_id TEXT NOT NULL REFERENCES projects(project_id),
 principal_id TEXT NOT NULL,
 credential_id TEXT NOT NULL,
 owner INTEGER NOT NULL CHECK(owner IN (0,1)),
 request_digest TEXT NOT NULL,
 context_json TEXT NOT NULL,
 stored_bytes INTEGER NOT NULL CHECK(stored_bytes > 0 AND stored_bytes <= 2097152),
 UNIQUE(project_id,principal_id,credential_id,owner)
);
CREATE TRIGGER recovery_v1_excludes_v2 BEFORE INSERT ON edit_recovery_v1
WHEN EXISTS(SELECT 1 FROM edit_recovery_v2 WHERE project_id=NEW.project_id AND principal_id=NEW.principal_id AND credential_id=NEW.credential_id AND owner=NEW.owner)
BEGIN SELECT RAISE(ABORT,'unresolved recovery scope conflict'); END;
CREATE TRIGGER recovery_v2_excludes_v1 BEFORE INSERT ON edit_recovery_v2
WHEN EXISTS(SELECT 1 FROM edit_recovery_v1 WHERE project_id=NEW.project_id AND principal_id=NEW.principal_id AND credential_id=NEW.credential_id AND owner=NEW.owner)
BEGIN SELECT RAISE(ABORT,'unresolved recovery scope conflict'); END;
UPDATE metadata SET value='29' WHERE key='schema_version' AND value='28';
";

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
    connection.query_row("SELECT recovery_id,project_id,request_digest,context_json FROM edit_recovery_v2
      WHERE project_id=?1 AND principal_id=?2 AND credential_id=?3 AND owner=?4 AND (?5 IS NULL OR recovery_id=?5)",
      params![project, identity.principal, identity.credential, identity.owner, id],
      |row| Ok(Stored { id:row.get(0)?, project:row.get(1)?, digest:row.get(2)?, payload:row.get(3)? }))
      .optional().map_err(storage_error)
}
fn corrupt(message: &str) -> EditorError {
    EditorError::new("STORAGE_ERROR", message)
}
fn require_original_font_authority(
    connection: &Connection,
    context: &ExecutionContext,
    stored: &Stored,
) -> Result<(), EditorError> {
    if matches!(
        sqlite_edit_recovery_shared::classify_v2_payload(
            &stored.payload,
            &stored.digest,
            &stored.project
        )?,
        sqlite_edit_recovery_shared::V2SlotKind::Font { .. }
    ) {
        let original = opencut_editor_application::font_import_v1::recovery::decode_payload(
            &stored.payload,
            &stored.digest,
            &stored.project,
        )?;
        sqlite_project_v4::authorize(
            connection,
            context,
            V4Intent::FontImport(&original.context.request),
        )?;
    }
    Ok(())
}
fn decode(stored: &Stored) -> Result<StageEditRecoveryV2, EditorError> {
    if matches!(
        sqlite_edit_recovery_shared::classify_v2_payload(
            &stored.payload,
            &stored.digest,
            &stored.project
        )?,
        sqlite_edit_recovery_shared::V2SlotKind::Font { .. }
    ) {
        return Err(sqlite_edit_recovery_shared::font_protocol_required());
    }
    if stored.payload.len() > opencut_editor_application::edit_recovery_v1::MAX_RECORD_BYTES
        || recovery::exact_payload_digest(&stored.payload) != stored.digest
    {
        return Err(corrupt("Stored recovery context identity differs"));
    }
    let input: StageEditRecoveryV2 = decode_json(&stored.payload, "Stored V2 recovery context")?;
    // Check typed intent invariants without replacing the authoritative raw digest.
    recovery::prepare(&input)?;
    if recovery::intent(&input.context.intent).project_id() != stored.project {
        return Err(corrupt("Stored recovery project scope differs"));
    }
    Ok(input)
}

/// Verify the receipt's actual transaction/revision edges, not the present head.
pub(super) fn receipt_journal(
    connection: &Connection,
    intent: V4Intent<'_>,
    receipt: &TransactionResult,
) -> Result<String, EditorError> {
    if Some(receipt.base_revision.as_str()) != intent.expected_revision()
        || receipt.transaction_id.is_empty()
        || receipt.revision.is_empty()
    {
        return Err(corrupt("Original recovery receipt scope differs"));
    }
    let raw: Option<(String, String)> = connection.query_row("SELECT t.request_json,t.result_json FROM editor_transactions t JOIN revisions r
      ON r.project_id=t.project_id AND r.transaction_id=t.transaction_id AND r.revision=t.revision AND r.parent_revision=t.base_revision
      WHERE t.project_id=?1 AND t.transaction_id=?2 AND t.base_revision=?3 AND t.revision=?4",
      params![intent.project_id(),receipt.transaction_id,receipt.base_revision,receipt.revision], |row| Ok((row.get(0)?, row.get(1)?)))
      .optional().map_err(storage_error)?;
    let (raw, stored_result) =
        raw.ok_or_else(|| corrupt("Original recovery receipt has no matching revision edge"))?;
    if raw.len() > opencut_editor_application::gc::MAX_REFERENCE_DOCUMENT_BYTES
        || stored_result.len() > opencut_editor_application::gc::MAX_REFERENCE_DOCUMENT_BYTES
    {
        return Err(EditorError::new(
            "RESOURCE_TOO_LARGE",
            "Recovery journal exceeds the reference budget",
        ));
    }
    let stored_result: TransactionResult =
        decode_json(&stored_result, "Original transaction journal result")?;
    if &stored_result != receipt {
        return Err(corrupt(
            "Original receipt differs from its complete transaction result",
        ));
    }
    Ok(raw)
}
fn original_receipt(
    connection: &Connection,
    context: &EditRecoveryContextV2,
) -> Result<Option<EditRecoveryReceiptV2>, EditorError> {
    let intent = recovery::intent(&context.intent);
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
    let transaction: TransactionResult = decode_json(&raw, "Original V2 recovery receipt")?;
    let journal = receipt_journal(connection, intent, &transaction)?;
    let receipt = match &context.intent {
        EditRecoveryIntentV2::PublicApply { .. } => {
            let stored: opencut_contracts::project_v4_public::ApplyPublicV4Project =
                decode_json(&journal, "Original public apply journal")?;
            if V4Intent::PublicApply(&stored).identity()?.current_digest()
                != intent.identity()?.current_digest()
            {
                return Err(corrupt("Original public apply journal identity differs"));
            }
            EditRecoveryReceiptV2::PublicApply { transaction }
        }
        EditRecoveryIntentV2::Restore { .. } => {
            let stored: opencut_contracts::project_v4_ops::RestoreV4Revision =
                decode_json(&journal, "Original restore journal")?;
            if V4Intent::Restore(&stored).identity()?.current_digest()
                != intent.identity()?.current_digest()
            {
                return Err(corrupt("Original restore journal identity differs"));
            }
            EditRecoveryReceiptV2::Restore { transaction }
        }
        EditRecoveryIntentV2::Undo { request } => {
            let CausalJournalV4::Undo {
                schema_version,
                request: stored,
                ..
            } = decode_json(&journal, "Original undo journal")?
            else {
                return Err(corrupt(
                    "Original undo receipt has a different causal journal",
                ));
            };
            if schema_version != 4 || &stored != request {
                return Err(corrupt("Original undo journal identity differs"));
            }
            EditRecoveryReceiptV2::Undo {
                commit: HistoryCommitV4 {
                    direction: HistoryDirectionV4::Undo,
                    target_transaction_id: request.target_transaction_id.clone(),
                    undo_transaction_id: None,
                    transaction,
                },
            }
        }
        EditRecoveryIntentV2::Redo { request } => {
            let CausalJournalV4::Redo {
                schema_version,
                request: stored,
                target_transaction_id,
                ..
            } = decode_json(&journal, "Original redo journal")?
            else {
                return Err(corrupt(
                    "Original redo receipt has a different causal journal",
                ));
            };
            if schema_version != 4 || &stored != request {
                return Err(corrupt("Original redo journal identity differs"));
            }
            let predecessor: Option<String> = connection.query_row("SELECT request_json FROM editor_transactions WHERE project_id=?1 AND transaction_id=?2",
                params![request.project_id,request.undo_transaction_id], |row|row.get(0)).optional().map_err(storage_error)?;
            let predecessor = predecessor
                .ok_or_else(|| corrupt("Original redo receipt has no undo predecessor"))?;
            if predecessor.len() > opencut_editor_application::gc::MAX_REFERENCE_DOCUMENT_BYTES {
                return Err(EditorError::new(
                    "RESOURCE_TOO_LARGE",
                    "Recovery predecessor exceeds the reference budget",
                ));
            }
            let CausalJournalV4::Undo {
                schema_version: previous_version,
                request: previous,
                ..
            } = decode_json(&predecessor, "Redo undo predecessor")?
            else {
                return Err(corrupt("Original redo predecessor is not an undo"));
            };
            if previous_version != 4
                || previous.project_id != request.project_id
                || previous.target_transaction_id != target_transaction_id
            {
                return Err(corrupt("Original redo predecessor identity differs"));
            }
            EditRecoveryReceiptV2::Redo {
                commit: HistoryCommitV4 {
                    direction: HistoryDirectionV4::Redo,
                    target_transaction_id,
                    undo_transaction_id: Some(request.undo_transaction_id.clone()),
                    transaction,
                },
            }
        }
    };
    recovery::validate_original_receipt(context, &receipt)?;
    Ok(Some(receipt))
}
fn record(
    connection: &Connection,
    stored: Stored,
    input: StageEditRecoveryV2,
) -> Result<EditRecoveryRecordV2, EditorError> {
    Ok(EditRecoveryRecordV2 {
        schema_version: 2,
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
fn validate_proposal(
    app: &EditorApplication,
    context: &EditRecoveryContextV2,
) -> Result<(), EditorError> {
    match &context.intent {
        EditRecoveryIntentV2::PublicApply { request } => {
            app.plan_public_v4_project(request)?;
        }
        EditRecoveryIntentV2::Undo { request } => {
            if !app.plan_v4_undo(request)?.conflicts.is_empty() {
                return Err(EditorError::new(
                    "SELECTIVE_CONFLICT",
                    "Recovery undo has selective conflicts",
                ));
            }
        }
        EditRecoveryIntentV2::Redo { request } => {
            if !app.plan_v4_redo(request)?.conflicts.is_empty() {
                return Err(EditorError::new(
                    "SELECTIVE_CONFLICT",
                    "Recovery redo has selective conflicts",
                ));
            }
        }
        EditRecoveryIntentV2::Restore { .. } => {} // Fixed target checked in the write snapshot below, never execute restore.
    }
    Ok(())
}

impl EditRecoveryStoreV2 for EditorApplication {
    fn stage_recovery_v2(
        &mut self,
        input: &StageEditRecoveryV2,
        prepared: &PreparedEditRecoveryV2,
    ) -> Result<EditRecoveryRecordV2, EditorError> {
        let checked = recovery::prepare(input)?;
        if checked.payload() != prepared.payload() || checked.digest() != prepared.digest() {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Prepared recovery context differs from the request",
            ));
        }
        let context = self.connection.1.clone();
        let identity = scope(&context);
        let intent = recovery::intent(&input.context.intent);
        sqlite_project_v4::authorize(&self.connection, &context, intent)?;
        sqlite_edit_recovery_shared::require_no_other_version(
            &self.connection,
            &identity,
            intent.project_id(),
            intent.key(),
            2,
        )?;
        let exists = load(&self.connection, &identity, intent.project_id(), None)?.is_some();
        let proposal = if exists {
            Ok(())
        } else {
            validate_proposal(self, &input.context)
        };
        let sql = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_project_v4::authorize(&sql, &context, intent)?;
        sqlite_edit_recovery_shared::require_no_other_version(
            &sql,
            &identity,
            intent.project_id(),
            intent.key(),
            2,
        )?;
        if let Some(stored) = load(&sql, &identity, intent.project_id(), None)? {
            if stored.digest != prepared.digest() || stored.payload != prepared.payload() {
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
                    "Resolve the original staged transaction before replacing it",
                ));
            }
            let result = record(
                &sql,
                stored,
                decode_json(prepared.payload(), "Recovery context")?,
            )?;
            sql.commit().map_err(storage_error)?;
            return Ok(result);
        }
        // Exact concurrent staging/replay precedes an old-head planning error.
        proposal?;
        // Any different family's receipt/key collision is rejected before admitting a new record.
        original_receipt(&sql, &input.context)?;
        let head = sqlite_project_document::project_head_revision(&sql, intent.project_id())?;
        if Some(head.as_str()) != intent.expected_revision() {
            return Err(EditorError::new(
                "REVISION_CONFLICT",
                "Project changed before recovery staging; review a new plan",
            ));
        }
        let source = sqlite_project_v4::load_source(&sql, intent.project_id(), Some(&head))?;
        let opencut_editor_application::project_v4::StoredProjectDocumentV4::V4(project) =
            source.document
        else {
            return Err(EditorError::new(
                "UNSUPPORTED_PROJECT_VERSION",
                "Recovery requires a V4 revision",
            ));
        };
        if input
            .context
            .sequence_id
            .as_ref()
            .is_some_and(|id| !project.sequences.iter().any(|s| &s.id == id))
        {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Recovery scene is not in the captured revision",
            ));
        }
        if let EditRecoveryIntentV2::Restore { request } = &input.context.intent {
            sqlite_project_v4::load_source(
                &sql,
                &request.project_id,
                Some(&request.target_revision),
            )?;
        }
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
        sqlite_edit_recovery_shared::reserve(&sql, bytes)?;
        sql.execute("INSERT INTO edit_recovery_v2(recovery_id,project_id,principal_id,credential_id,owner,request_digest,context_json,stored_bytes) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![id,intent.project_id(),identity.principal,identity.credential,identity.owner,prepared.digest(),prepared.payload(),bytes as u64]).map_err(storage_error)?;
        let result = record(
            &sql,
            Stored {
                id,
                project: intent.project_id().into(),
                digest: prepared.digest().into(),
                payload: prepared.payload().into(),
            },
            input.clone(),
        )?;
        sql.commit().map_err(storage_error)?;
        Ok(result)
    }
    fn inspect_recovery_v2(
        &self,
        input: &ReadEditRecoveryV2,
    ) -> Result<EditRecoveryInspectionV2, EditorError> {
        let sql = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let context = &self.connection.1;
        require_read(&sql, context, &input.project_id)?;
        let legacy = sqlite_edit_recovery_v1::read_on(
            &sql,
            context,
            &opencut_editor_application::edit_recovery_v1::ReadEditRecoveryV1 {
                project_id: input.project_id.clone(),
                recovery_id: input.recovery_id.clone(),
            },
        )?;
        let record = if let Some(record) = legacy {
            Some(DiscoveredEditRecoveryV2::LegacyV1 {
                record: Box::new(record),
            })
        } else if let Some(stored) = load(
            &sql,
            &scope(context),
            &input.project_id,
            input.recovery_id.as_deref(),
        )? {
            require_original_font_authority(&sql, context, &stored)?;
            let decoded = decode(&stored)?;
            sqlite_project_v4::authorize(&sql, context, recovery::intent(&decoded.context.intent))?;
            Some(DiscoveredEditRecoveryV2::CurrentV2 {
                record: Box::new(record(&sql, stored, decoded)?),
            })
        } else {
            None
        };
        sql.commit().map_err(storage_error)?;
        Ok(EditRecoveryInspectionV2 {
            schema_version: 2,
            record,
        })
    }
    fn ack_recovery_v2(
        &mut self,
        input: &AckEditRecoveryV2,
    ) -> Result<EditRecoveryAckV2, EditorError> {
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
                "Recovery record does not exist in the authenticated V2 scope",
            )
        })?;
        require_original_font_authority(&sql, &context, &stored)?;
        let decoded = decode(&stored)?;
        let intent = recovery::intent(&decoded.context.intent);
        sqlite_project_v4::authorize(&sql, &context, intent)?;
        let receipt = original_receipt(&sql, &decoded.context)?;
        let resolution = if receipt.is_some() {
            EditRecoveryResolutionV2::Saved
        } else if Some(
            sqlite_project_document::project_head_revision(&sql, &input.project_id)?.as_str(),
        ) != intent.expected_revision()
        {
            EditRecoveryResolutionV2::NotSaved
        } else {
            return Err(EditorError::new(
                "RECOVERY_UNRESOLVED",
                "The original fixed-head transaction remains possible; recover it before acknowledging",
            ));
        };
        sql.execute(
            "DELETE FROM edit_recovery_v2 WHERE recovery_id=?1",
            [&stored.id],
        )
        .map_err(storage_error)?;
        let result = EditRecoveryAckV2 {
            schema_version: 2,
            recovery_id: stored.id,
            resolution,
            receipt,
        };
        sql.commit().map_err(storage_error)?;
        Ok(result)
    }
}
impl EditorApplication {
    pub fn stage_edit_recovery_v2(
        &mut self,
        input: &StageEditRecoveryV2,
    ) -> Result<EditRecoveryRecordV2, EditorError> {
        recovery::stage(self, input)
    }
    pub fn inspect_edit_recovery_v2(
        &self,
        input: &ReadEditRecoveryV2,
    ) -> Result<EditRecoveryInspectionV2, EditorError> {
        recovery::inspect(self, input)
    }
    pub fn ack_edit_recovery_v2(
        &mut self,
        input: &AckEditRecoveryV2,
    ) -> Result<EditRecoveryAckV2, EditorError> {
        recovery::ack(self, input)
    }
}
#[cfg(test)]
mod tests;
