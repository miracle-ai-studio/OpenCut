//! Recovery retains the exact reviewed public intent. Receipt reads never run
//! an edit, restore or history use case. Historical raw payloads stay unchanged.
use super::sqlite_edit_recovery_shared::{Scope, quota, scope};
use super::*;
use opencut_contracts::project_history::{CausalJournal, Direction, HistoryCommit};
use opencut_editor_application::{
    audit::ExecutionContext,
    edit_recovery::{self as recovery, *},
    scene::SceneCommitIntent,
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
    connection.query_row("SELECT recovery_id,project_id,request_digest,context_json FROM edit_recovery
      WHERE project_id=?1 AND principal_id=?2 AND credential_id=?3 AND owner=?4 AND (?5 IS NULL OR recovery_id=?5)",
      params![project, identity.principal, identity.credential, identity.owner, id],
      |row| Ok(Stored { id:row.get(0)?, project:row.get(1)?, digest:row.get(2)?, payload:row.get(3)? }))
      .optional().map_err(storage_error)
}
fn corrupt(message: &str) -> EditorError {
    EditorError::new("STORAGE_ERROR", message)
}
fn require_original_authority(
    connection: &Connection,
    caller: &ExecutionContext,
    context: &EditRecoveryContext,
) -> Result<(), EditorError> {
    let intent = recovery::intent(&context.intent)?;
    sqlite_scene::authorize(connection, caller, intent)?;
    if let EditRecoveryIntent::SubtitleGenerate { request } = &context.intent {
        sqlite_captions::require_generation_source(connection, caller, request)?;
    }
    Ok(())
}
fn decode(stored: &Stored) -> Result<StageEditRecovery, EditorError> {
    recovery::decode_payload(&stored.payload, &stored.digest, &stored.project)
}

/// Verify the receipt's actual transaction/revision edges, not the present head.
pub(super) fn receipt_journal(
    connection: &Connection,
    intent: SceneCommitIntent<'_>,
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
    context: &EditRecoveryContext,
) -> Result<Option<EditRecoveryReceipt>, EditorError> {
    let intent = recovery::intent(&context.intent)?;
    if let EditRecoveryIntent::Restore { request } = &context.intent {
        if let Some(transaction) = sqlite_scene::historical_restore_receipt(connection, request)? {
            let receipt = EditRecoveryReceipt::Restore { transaction };
            recovery::validate_original_receipt(context, &receipt)?;
            return Ok(Some(receipt));
        }
    }

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
    if matches!(
        &context.intent,
        EditRecoveryIntent::SubtitleGenerate { .. } | EditRecoveryIntent::SubtitleImport { .. }
    ) {
        let caption: opencut_contracts::subtitles::CaptionReceipt =
            decode_json(&raw, "Original caption receipt")?;
        let journal = receipt_journal(connection, intent, &caption.transaction)?;
        match &context.intent {
            EditRecoveryIntent::SubtitleGenerate { request } => {
                let stored: opencut_contracts::subtitles::GenerateCaptions =
                    decode_json(&journal, "Original caption generation journal")?;
                if &stored != request {
                    return Err(corrupt("Original caption request differs from journal"));
                }
            }
            EditRecoveryIntent::SubtitleImport { request } => {
                let stored: opencut_contracts::subtitles::SubtitleImport =
                    decode_json(&journal, "Original subtitle import journal")?;
                if &stored != request {
                    return Err(corrupt("Original subtitle request differs from journal"));
                }
            }
            _ => return Err(corrupt("Caption recovery intent changed")),
        }
        if sqlite_captions::caption_receipt(connection, intent, &caption.transaction)? != caption {
            return Err(corrupt(
                "Original caption receipt differs from revision effect",
            ));
        }
        let receipt = match &context.intent {
            EditRecoveryIntent::SubtitleGenerate { .. } => {
                EditRecoveryReceipt::SubtitleGenerate { caption }
            }
            _ => EditRecoveryReceipt::SubtitleImport { caption },
        };
        recovery::validate_original_receipt(context, &receipt)?;
        return Ok(Some(receipt));
    }
    let transaction: TransactionResult = decode_json(&raw, "Original recovery receipt")?;
    let journal = receipt_journal(connection, intent, &transaction)?;
    let receipt = match &context.intent {
        EditRecoveryIntent::FontImport { request } => {
            let stored: opencut_contracts::font_import::ImportFont =
                decode_json(&journal, "Original font import journal")?;
            if &stored != request {
                return Err(corrupt("Original font import request differs from journal"));
            }
            EditRecoveryReceipt::FontImport { transaction }
        }
        EditRecoveryIntent::PublicApply { .. } => {
            let stored: opencut_contracts::project_public::ApplyProject =
                decode_json(&journal, "Original public apply journal")?;
            if SceneCommitIntent::PublicApply(&stored)
                .identity()?
                .current_digest()
                != intent.identity()?.current_digest()
            {
                return Err(corrupt("Original public apply journal identity differs"));
            }
            EditRecoveryReceipt::PublicApply { transaction }
        }
        EditRecoveryIntent::Restore { .. } => {
            let stored: opencut_contracts::scene_ops::RestoreProject =
                decode_json(&journal, "Original restore journal")?;
            if SceneCommitIntent::Restore(&stored)
                .identity()?
                .current_digest()
                != intent.identity()?.current_digest()
            {
                return Err(corrupt("Original restore journal identity differs"));
            }
            EditRecoveryReceipt::Restore { transaction }
        }
        EditRecoveryIntent::Undo { request } => {
            let CausalJournal::Undo {
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
            EditRecoveryReceipt::Undo {
                commit: HistoryCommit {
                    direction: Direction::Undo,
                    target_transaction_id: request.target_transaction_id.clone(),
                    undo_transaction_id: None,
                    transaction,
                },
            }
        }
        EditRecoveryIntent::Redo { request } => {
            let CausalJournal::Redo {
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
            let CausalJournal::Undo {
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
            EditRecoveryReceipt::Redo {
                commit: HistoryCommit {
                    direction: Direction::Redo,
                    target_transaction_id,
                    undo_transaction_id: Some(request.undo_transaction_id.clone()),
                    transaction,
                },
            }
        }
        EditRecoveryIntent::SubtitleGenerate { .. } | EditRecoveryIntent::SubtitleImport { .. } => {
            return Err(corrupt("Caption recovery receipt changed"));
        }
    };
    recovery::validate_original_receipt(context, &receipt)?;
    Ok(Some(receipt))
}
fn record(
    connection: &Connection,
    stored: Stored,
    input: StageEditRecovery,
) -> Result<EditRecoveryRecord, EditorError> {
    Ok(EditRecoveryRecord {
        recovery_id: stored.id,
        project_id: stored.project,
        request_digest: recovery::prepare(&input)?.digest().to_owned(),
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
    context: &EditRecoveryContext,
) -> Result<(), EditorError> {
    match &context.intent {
        EditRecoveryIntent::FontImport { request } => {
            opencut_editor_application::font_catalog::selection::validate(request)?;
        }
        EditRecoveryIntent::PublicApply { request } => {
            app.plan_editor_edit(request)?;
        }
        EditRecoveryIntent::Undo { request } => {
            if !app.plan_project_undo(request)?.conflicts.is_empty() {
                return Err(EditorError::new(
                    "SELECTIVE_CONFLICT",
                    "Recovery undo has selective conflicts",
                ));
            }
        }
        EditRecoveryIntent::Redo { request } => {
            if !app.plan_project_redo(request)?.conflicts.is_empty() {
                return Err(EditorError::new(
                    "SELECTIVE_CONFLICT",
                    "Recovery redo has selective conflicts",
                ));
            }
        }
        EditRecoveryIntent::Restore { .. }
        | EditRecoveryIntent::SubtitleGenerate { .. }
        | EditRecoveryIntent::SubtitleImport { .. } => {} // Validated in the fixed write snapshot below.
    }
    Ok(())
}

impl EditRecoveryStore for EditorApplication {
    fn stage_recovery(
        &mut self,
        input: &StageEditRecovery,
        prepared: &PreparedEditRecovery,
    ) -> Result<EditRecoveryRecord, EditorError> {
        let checked = recovery::prepare(input)?;
        if checked.payload() != prepared.payload() || checked.digest() != prepared.digest() {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Prepared recovery context differs from the request",
            ));
        }
        let context = self.connection.1.clone();
        let identity = scope(&context);
        let intent = recovery::intent(&input.context.intent)?;
        require_original_authority(&self.connection, &context, &input.context)?;
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
        require_original_authority(&sql, &context, &input.context)?;
        if let Some(stored) = load(&sql, &identity, intent.project_id(), None)? {
            let original = decode(&stored)?;
            let old_intent = recovery::intent(&original.context.intent)?;
            if original.context != input.context {
                return Err(EditorError::new(
                    if old_intent.key() == intent.key() {
                        "IDEMPOTENCY_KEY_REUSED"
                    } else {
                        "RECOVERY_CONFLICT"
                    },
                    "Resolve the original staged transaction before replacing it",
                ));
            }
            let result = record(&sql, stored, input.clone())?;
            sql.commit().map_err(storage_error)?;
            return Ok(result);
        }
        // Exact concurrent staging/replay precedes an old-head planning error.
        proposal?;
        // Any different family's receipt/key collision is rejected before admitting a new record.
        original_receipt(&sql, &input.context)?;
        let source = sqlite_scene::intent_source(&sql, intent)?;
        let head = source.revision.clone();
        if Some(head.as_str()) != intent.expected_revision() {
            return Err(EditorError::new(
                "REVISION_CONFLICT",
                "Project changed before recovery staging; review a new plan",
            ));
        }
        let opencut_editor_application::scene::StoredProjectDocument::Editable(project) =
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
        if let EditRecoveryIntent::Restore { request } = &input.context.intent {
            sqlite_scene::load_source(&sql, &request.project_id, Some(&request.target_revision))?;
        }
        match &input.context.intent {
            EditRecoveryIntent::SubtitleGenerate { request } => {
                let source = sqlite_scene_jobs::fixed(
                    &sql,
                    intent.project_id(),
                    intent.expected_revision().unwrap(),
                )?;
                let analysis = sqlite_captions::require_generation_source(&sql, &context, request)?;
                opencut_editor_application::captions::generate(&source, request, &analysis)?;
            }
            EditRecoveryIntent::SubtitleImport { request } => {
                let source = sqlite_scene_jobs::fixed(
                    &sql,
                    intent.project_id(),
                    intent.expected_revision().unwrap(),
                )?;
                opencut_editor_application::captions::import(&source, request)?;
            }
            _ => {}
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
        sql.execute("INSERT INTO edit_recovery(recovery_id,project_id,principal_id,credential_id,owner,request_digest,context_json,stored_bytes) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![id,intent.project_id(),identity.principal,identity.credential,identity.owner,prepared.digest(),prepared.payload(),bytes as u64]).map_err(storage_error)?;
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
    fn inspect_recovery(
        &self,
        input: &ReadEditRecovery,
    ) -> Result<EditRecoveryInspection, EditorError> {
        let sql = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let context = &self.connection.1;
        require_read(&sql, context, &input.project_id)?;
        let record = if let Some(stored) = load(
            &sql,
            &scope(context),
            &input.project_id,
            input.recovery_id.as_deref(),
        )? {
            let decoded = decode(&stored)?;
            require_original_authority(&sql, context, &decoded.context)?;
            Some(record(&sql, stored, decoded)?)
        } else {
            None
        };
        sql.commit().map_err(storage_error)?;
        Ok(EditRecoveryInspection { record })
    }
    fn ack_recovery(&mut self, input: &AckEditRecovery) -> Result<EditRecoveryAck, EditorError> {
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
                "Recovery record does not exist in the authenticated scope",
            )
        })?;

        let decoded = decode(&stored)?;
        let intent = recovery::intent(&decoded.context.intent)?;
        require_original_authority(&sql, &context, &decoded.context)?;
        let receipt = original_receipt(&sql, &decoded.context)?;
        let resolution = if receipt.is_some() {
            EditRecoveryResolution::Saved
        } else if Some(sqlite_scene::intent_source(&sql, intent)?.revision.as_str())
            != intent.expected_revision()
        {
            EditRecoveryResolution::NotSaved
        } else {
            return Err(EditorError::new(
                "RECOVERY_UNRESOLVED",
                "The original fixed-head transaction remains possible; recover it before acknowledging",
            ));
        };
        sql.execute(
            "DELETE FROM edit_recovery WHERE recovery_id=?1",
            [&stored.id],
        )
        .map_err(storage_error)?;
        let result = EditRecoveryAck {
            recovery_id: stored.id,
            resolution,
            receipt,
        };
        sql.commit().map_err(storage_error)?;
        Ok(result)
    }
}
impl EditorApplication {
    pub fn stage_edit_recovery(
        &mut self,
        input: &StageEditRecovery,
    ) -> Result<EditRecoveryRecord, EditorError> {
        recovery::stage(self, input)
    }
    pub fn inspect_edit_recovery(
        &self,
        input: &ReadEditRecovery,
    ) -> Result<EditRecoveryInspection, EditorError> {
        recovery::inspect(self, input)
    }
    pub fn ack_edit_recovery(
        &mut self,
        input: &AckEditRecovery,
    ) -> Result<EditRecoveryAck, EditorError> {
        recovery::ack(self, input)
    }
}

#[cfg(test)]
mod tests;
