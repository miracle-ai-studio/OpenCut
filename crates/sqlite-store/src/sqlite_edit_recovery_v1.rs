//! Host-private exact transaction recovery. No media pin, auto-apply or second
//! project aggregate is created. Current authorization precedes every outcome.
use super::sqlite_edit_recovery_shared::{Scope, quota, scope};
use super::*;
use opencut_editor_application::{
    audit::ExecutionContext,
    edit_recovery_v1::{self as recovery, *},
    project_v4::V4Intent,
};

/// Registered by the workspace schema migration, not invoked on first request.
pub(super) const MIGRATION: &str = "
CREATE TABLE edit_recovery_v1(
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
UPDATE metadata SET value='28' WHERE key='schema_version' AND value='27';
";

struct Stored {
    id: String,
    project: String,
    digest: String,
    payload: String,
}
fn load(
    connection: &Connection,
    scope: &Scope,
    project: &str,
    id: Option<&str>,
) -> Result<Option<Stored>, EditorError> {
    connection
        .query_row(
            "SELECT recovery_id,project_id,request_digest,context_json FROM edit_recovery_v1
         WHERE project_id=?1 AND principal_id=?2 AND credential_id=?3 AND owner=?4
         AND (?5 IS NULL OR recovery_id=?5)",
            params![project, scope.principal, scope.credential, scope.owner, id],
            |row| {
                Ok(Stored {
                    id: row.get(0)?,
                    project: row.get(1)?,
                    digest: row.get(2)?,
                    payload: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(storage_error)
}
fn authorize_scope(
    connection: &Connection,
    context: &ExecutionContext,
    project: &str,
) -> Result<(), EditorError> {
    sqlite_access::require(connection, context, project, ProjectAction::Read)?;
    sqlite_access::require(connection, context, project, ProjectAction::Edit)?;
    sqlite_edit_control::require_writable(connection, context, project)
}
fn decode(stored: &Stored) -> Result<StageEditRecoveryV1, EditorError> {
    if stored.payload.len() > MAX_RECORD_BYTES
        || recovery::exact_payload_digest(&stored.payload) != stored.digest
    {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Stored recovery context identity differs",
        ));
    }
    let input: StageEditRecoveryV1 = decode_json(&stored.payload, "Stored recovery context")?;
    // The raw payload digest stays authoritative; do not reserialize decoded
    // floats and silently replace the original exact context identity.
    let tx = recovery::transaction(&input.context)?;
    if tx.project_id != stored.project {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Stored recovery project scope differs",
        ));
    }
    Ok(input)
}
fn original_receipt(
    connection: &Connection,
    context: &EditRecoveryContextV1,
) -> Result<Option<TransactionResult>, EditorError> {
    let tx = recovery::transaction(context)?;
    let intent = V4Intent::PublicApply(tx);
    let raw = load_idempotent(
        connection,
        intent.family(),
        intent.scope(),
        intent.key(),
        &intent.identity()?,
        "transaction",
    )?;
    let receipt = raw
        .map(|raw| decode_json::<TransactionResult>(&raw, "Original recovery receipt"))
        .transpose()?;
    if receipt.as_ref().is_some_and(|r| {
        r.base_revision != tx.expected_revision
            || r.transaction_id.is_empty()
            || r.revision.is_empty()
    }) {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Original recovery receipt scope differs",
        ));
    }
    Ok(receipt)
}
fn record(
    connection: &Connection,
    stored: Stored,
    input: StageEditRecoveryV1,
) -> Result<EditRecoveryRecordV1, EditorError> {
    let EditRecoveryContextV1::TimelineV4 { request, .. } = &input.context;
    Ok(EditRecoveryRecordV1 {
        schema_version: 1,
        recovery_id: stored.id,
        project_id: stored.project,
        sequence_id: request.sequence_id.clone(),
        request_digest: stored.digest,
        receipt: original_receipt(connection, &input.context)?,
        context: input.context,
    })
}

/// Same read snapshot for V2 legacy discovery; no nested transaction or rewrite.
pub(super) fn read_on(
    connection: &Connection,
    context: &ExecutionContext,
    input: &ReadEditRecoveryV1,
) -> Result<Option<EditRecoveryRecordV1>, EditorError> {
    // Empty discovery needs only Read, but legacy V1 entry points keep their
    // published Edit/write-control requirement in their caller below.
    let Some(stored) = load(
        connection,
        &scope(context),
        &input.project_id,
        input.recovery_id.as_deref(),
    )?
    else {
        return Ok(None);
    };
    let decoded = decode(&stored)?;
    sqlite_project_v4::authorize(
        connection,
        context,
        V4Intent::PublicApply(recovery::transaction(&decoded.context)?),
    )?;
    Ok(Some(record(connection, stored, decoded)?))
}

impl EditRecoveryStoreV1 for EditorApplication {
    fn stage_recovery_v1(
        &mut self,
        input: &StageEditRecoveryV1,
        prepared: &PreparedEditRecoveryV1,
    ) -> Result<EditRecoveryRecordV1, EditorError> {
        let checked = recovery::prepare(input)?;
        if checked.payload() != prepared.payload() || checked.digest() != prepared.digest() {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Prepared recovery context differs from the request",
            ));
        }
        let context = self.connection.1.clone();
        let identity = scope(&context);
        let apply = recovery::transaction(&input.context)?;
        authorize_scope(&self.connection, &context, &apply.project_id)?;
        sqlite_edit_recovery_shared::require_no_other_version(
            &self.connection,
            &identity,
            &apply.project_id,
            &apply.idempotency_key,
            1,
        )?;
        // Existing exact records are replayed before checking the current head.
        // New proposals use the existing public planner, without another edit or
        // timeline replan; SQL rechecks the captured head and original authority.
        let exists = load(&self.connection, &identity, &apply.project_id, None)?.is_some();
        let proposal = if exists {
            Ok(())
        } else {
            self.plan_public_v4_project(apply).map(|_| ())
        };
        let sql = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_project_v4::authorize(&sql, &context, V4Intent::PublicApply(apply))?;
        sqlite_edit_recovery_shared::require_no_other_version(
            &sql,
            &identity,
            &apply.project_id,
            &apply.idempotency_key,
            1,
        )?;
        if let Some(stored) = load(&sql, &identity, &apply.project_id, None)? {
            if stored.digest != prepared.digest() || stored.payload != prepared.payload() {
                let old = decode(&stored)?;
                let code = if recovery::transaction(&old.context)?.idempotency_key
                    == apply.idempotency_key
                {
                    "IDEMPOTENCY_KEY_REUSED"
                } else {
                    "RECOVERY_CONFLICT"
                };
                return Err(EditorError::new(
                    code,
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
        // A concurrent stage/apply can change head during read-only planning.
        // Its exact pending record above wins over the old-head planning error.
        proposal?;
        let head = sqlite_project_document::project_head_revision(&sql, &apply.project_id)?;
        if head != apply.expected_revision {
            return Err(EditorError::new(
                "REVISION_CONFLICT",
                "Project changed before recovery staging; review a new plan",
            ));
        }
        let source = sqlite_project_v4::load_source(
            &sql,
            &apply.project_id,
            Some(&apply.expected_revision),
        )?;
        let EditRecoveryContextV1::TimelineV4 { request, .. } = &input.context;
        let opencut_editor_application::project_v4::StoredProjectDocumentV4::V4(project) =
            source.document
        else {
            return Err(EditorError::new(
                "UNSUPPORTED_PROJECT_VERSION",
                "Recovery requires a V4 revision",
            ));
        };
        if !project
            .sequences
            .iter()
            .any(|s| s.id == request.sequence_id)
        {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Recovery scene is not in the captured revision",
            ));
        }
        let id = format!("recovery_{}", Uuid::new_v4().simple());
        let bytes = prepared
            .payload()
            .len()
            .checked_add(
                id.len()
                    + apply.project_id.len()
                    + identity.principal.len()
                    + identity.credential.len()
                    + prepared.digest().len()
                    + 16,
            )
            .ok_or_else(quota)?;
        sqlite_edit_recovery_shared::reserve(&sql, bytes)?;
        sql.execute("INSERT INTO edit_recovery_v1(recovery_id,project_id,principal_id,credential_id,owner,request_digest,context_json,stored_bytes) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![id,apply.project_id,identity.principal,identity.credential,identity.owner,prepared.digest(),prepared.payload(),bytes as u64]).map_err(storage_error)?;
        let result = record(
            &sql,
            Stored {
                id,
                project: apply.project_id.clone(),
                digest: prepared.digest().into(),
                payload: prepared.payload().into(),
            },
            input.clone(),
        )?;
        sql.commit().map_err(storage_error)?;
        Ok(result)
    }

    fn read_recovery_v1(
        &self,
        input: &ReadEditRecoveryV1,
    ) -> Result<Option<EditRecoveryRecordV1>, EditorError> {
        let sql = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let context = &self.connection.1;
        authorize_scope(&sql, context, &input.project_id)?;
        let result = read_on(&sql, context, input)?;
        sql.commit().map_err(storage_error)?;
        Ok(result)
    }

    fn ack_recovery_v1(
        &mut self,
        input: &AckEditRecoveryV1,
    ) -> Result<EditRecoveryAckV1, EditorError> {
        let context = self.connection.1.clone();
        let sql = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        authorize_scope(&sql, &context, &input.project_id)?;
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
        let apply = recovery::transaction(&decoded.context)?;
        sqlite_project_v4::authorize(&sql, &context, V4Intent::PublicApply(apply))?;
        // The immediate lock prevents a racing original apply from inserting a
        // receipt after this check but before a NotSaved resolution. This head
        // inequality proves impossibility only; it never proves a successful edit.
        let receipt = original_receipt(&sql, &decoded.context)?;
        let resolution = if receipt.is_some() {
            EditRecoveryResolutionV1::Saved
        } else if sqlite_project_document::project_head_revision(&sql, &input.project_id)?
            != apply.expected_revision
        {
            EditRecoveryResolutionV1::NotSaved
        } else {
            return Err(EditorError::new(
                "RECOVERY_UNRESOLVED",
                "The original fixed-head transaction remains possible; recover it before acknowledging",
            ));
        };
        sql.execute(
            "DELETE FROM edit_recovery_v1 WHERE recovery_id=?1",
            [&stored.id],
        )
        .map_err(storage_error)?;
        let result = EditRecoveryAckV1 {
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
    pub fn stage_edit_recovery_v1(
        &mut self,
        input: &StageEditRecoveryV1,
    ) -> Result<EditRecoveryRecordV1, EditorError> {
        recovery::stage(self, input)
    }
    pub fn read_edit_recovery_v1(
        &self,
        input: &ReadEditRecoveryV1,
    ) -> Result<Option<EditRecoveryRecordV1>, EditorError> {
        recovery::read(self, input)
    }
    pub fn ack_edit_recovery_v1(
        &mut self,
        input: &AckEditRecoveryV1,
    ) -> Result<EditRecoveryAckV1, EditorError> {
        recovery::ack(self, input)
    }
}

#[cfg(test)]
mod tests;
