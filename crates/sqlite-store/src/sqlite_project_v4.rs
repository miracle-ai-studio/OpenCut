//! V4 uses the existing aggregate revision, journal, receipt and audit authority.
//! No mutable V4 content tables or bookmark sidecars are introduced.
use super::*;
use opencut_asset_store::AssetStore;
use opencut_contracts::{project_v4::*, project_v4_ops::*};
use opencut_editor_application::project_v4::{
    self, PreparedV4Commit, PreparedV4Create, ProjectV4LibraryRead, ProjectV4Read, ProjectV4Store,
    StoredProjectDocumentV4, StoredProjectRevisionV4, V4Intent,
};
use sqlite_project_document::{VersionedProjectDocument, decode_versioned_project};

mod public;
pub(super) mod resources;
#[cfg(test)]
mod tests;

/// Protocol barrier: older hosts must reject this workspace before any write.
/// Historical document bytes are deliberately not changed by this migration.
pub(super) const MIGRATION: &str = "
UPDATE metadata SET value='27' WHERE key='schema_version' AND value='26';
";

pub(super) fn load_source(
    connection: &Connection,
    project: &str,
    revision: Option<&str>,
) -> Result<StoredProjectRevisionV4, EditorError> {
    sqlite_project_library::require_live(connection, project)?;
    let row: Option<(String, String)> = match revision {
        Some(revision) => connection
            .query_row(
                "SELECT document_json,revision FROM revisions WHERE project_id=?1 AND revision=?2",
                params![project, revision],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage_error)?,
        None => connection
            .query_row(
                "SELECT r.document_json,p.head_revision FROM projects p JOIN revisions r
             ON r.project_id=p.project_id AND r.revision=p.head_revision WHERE p.project_id=?1",
                [project],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage_error)?,
    };
    let (raw, revision) =
        row.ok_or_else(|| EditorError::new("NOT_FOUND", "Project revision does not exist"))?;
    let (id, document) = match decode_versioned_project(&raw)? {
        VersionedProjectDocument::Legacy(document) => (
            document.id.clone(),
            StoredProjectDocumentV4::Legacy(Box::new(document)),
        ),
        VersionedProjectDocument::V3(document) => (
            document.base.id.clone(),
            StoredProjectDocumentV4::V3(Box::new(document)),
        ),
        VersionedProjectDocument::V4(document) => (
            document.id.clone(),
            StoredProjectDocumentV4::V4(Box::new(document)),
        ),
    };
    if id != project {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Project identity differs from revision scope",
        ));
    }
    Ok(StoredProjectRevisionV4 { document, revision })
}

pub(super) fn authorize(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    intent: V4Intent<'_>,
) -> Result<(), EditorError> {
    let id = intent.project_id();
    sqlite_project_library::require_live(connection, id)?;
    if matches!(intent, V4Intent::Create(_) | V4Intent::Copy(_)) {
        sqlite_access::owner(context)?;
        if let V4Intent::Copy(input) = intent {
            sqlite_access::require(
                connection,
                context,
                &input.source_project_id,
                ProjectAction::Read,
            )?;
            load_source(
                connection,
                &input.source_project_id,
                Some(&input.source_revision),
            )?;
        }
    } else {
        sqlite_access::require(connection, context, id, ProjectAction::Read)?;
        sqlite_access::require(
            connection,
            context,
            id,
            match intent {
                V4Intent::Restore(_) | V4Intent::Undo(_) | V4Intent::Redo(_) => {
                    ProjectAction::Restore
                }
                _ => ProjectAction::Edit,
            },
        )?;
        sqlite_project_document::project_head_revision(connection, id)?;
    }
    sqlite_edit_control::require_writable(connection, context, id)?;
    resources::authorize_intent(connection, context, intent)
}

impl project_v4::resources::ProjectV4ResourceRead for EditorApplication {
    fn require_resource_revision_read(
        &self,
        project: &str,
        revision: &str,
    ) -> Result<(), EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_access::require(&tx, &self.connection.1, project, ProjectAction::Read)?;
        sqlite_project_library::require_live(&tx, project)?;
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM revisions WHERE project_id=?1 AND revision=?2)",
                params![project, revision],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if !exists {
            return Err(EditorError::new(
                "NOT_FOUND",
                "Project revision does not exist",
            ));
        }
        tx.commit().map_err(storage_error)
    }
}

impl ProjectV4Read for EditorApplication {
    fn authorize_v4(&self, intent: V4Intent<'_>) -> Result<(), EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        authorize(&tx, &self.connection.1, intent)?;
        tx.commit().map_err(storage_error)
    }
    fn head_v4_source(&self, project: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_access::require(&tx, &self.connection.1, project, ProjectAction::Read)?;
        let source = load_source(&tx, project, None)?;
        tx.commit().map_err(storage_error)?;
        Ok(source)
    }
    fn revision_v4_source(
        &self,
        project: &str,
        revision: &str,
    ) -> Result<StoredProjectRevisionV4, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_access::require(&tx, &self.connection.1, project, ProjectAction::Read)?;
        let source = load_source(&tx, project, Some(revision))?;
        tx.commit().map_err(storage_error)?;
        Ok(source)
    }
    fn replay_v4_create(
        &self,
        input: &CreateV4Project,
    ) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let intent = V4Intent::Create(input);
        authorize(&tx, &self.connection.1, intent)?;
        let value = replay(&tx, intent, "project_v4")?
            .map(|raw| decode_json(&raw, "V4 creation receipt"))
            .transpose()?;
        tx.commit().map_err(storage_error)?;
        Ok(value)
    }
    fn replay_v4_copy(
        &self,
        input: &CopyV4Project,
    ) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let intent = V4Intent::Copy(input);
        authorize(&tx, &self.connection.1, intent)?;
        let value = replay(&tx, intent, "project_v4")?
            .map(|raw| decode_json(&raw, "V4 copy receipt"))
            .transpose()?;
        tx.commit().map_err(storage_error)?;
        Ok(value)
    }
    fn replay_v4_edit(
        &self,
        intent: V4Intent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        authorize(&tx, &self.connection.1, intent)?;
        let value = replay(&tx, intent, "transaction")?
            .map(|raw| decode_edit_receipt(intent, &raw))
            .transpose()?;
        tx.commit().map_err(storage_error)?;
        Ok(value)
    }
}

fn replay(
    connection: &Connection,
    intent: V4Intent<'_>,
    kind: &str,
) -> Result<Option<String>, EditorError> {
    load_idempotent(
        connection,
        intent.family(),
        intent.scope(),
        intent.key(),
        &intent.identity()?,
        kind,
    )
}

impl EditorApplication {
    pub fn plan_v4_timeline(
        &self,
        request: &opencut_contracts::timeline_v4::TimelineEditRequestV4,
    ) -> Result<opencut_contracts::timeline_v4::TimelineEditPlanV4, EditorError> {
        opencut_editor_application::timeline_v4::plan(self, request)
    }

    pub fn plan_v4_audio_gain(
        &self,
        request: &opencut_contracts::audio_gain_v4::AudioGainRequestV4,
    ) -> Result<opencut_contracts::audio_gain_v4::AudioGainPlanV4, EditorError> {
        opencut_editor_application::audio_gain_v4::plan(self, request)
    }

    pub fn plan_v4_source_audio(
        &self,
        request: &opencut_contracts::source_audio_v4::SourceAudioRequestV4,
    ) -> Result<opencut_contracts::source_audio_v4::SourceAudioPlanV4, EditorError> {
        opencut_editor_application::source_audio_v4::plan(self, request)
    }

    pub fn revalidate_v4_resource_read(
        &self,
        request: &opencut_contracts::project_v4_resources::InspectResourceV4,
    ) -> Result<(), EditorError> {
        project_v4::resources::revalidate_resource_read(self, request)
    }

    pub fn plan_v4_media_import(
        &self,
        request: &opencut_contracts::media_import_v4::MediaImportRequestV4,
    ) -> Result<opencut_contracts::media_import_v4::MediaImportPlanV4, EditorError> {
        opencut_editor_application::media_import_v4::plan(self, request)
    }

    pub fn plan_v4_media_workspace(
        &self,
        request: &opencut_contracts::media_import_v4::MediaWorkspaceRequestV4,
    ) -> Result<opencut_contracts::media_import_v4::MediaImportPlanV4, EditorError> {
        opencut_editor_application::media_import_v4::plan_workspace(self, request)
    }

    pub fn inspect_v4_resource(
        &self,
        request: &opencut_contracts::project_v4_resources::InspectResourceV4,
    ) -> Result<opencut_contracts::project_v4_resources::ResourceViewV4, EditorError> {
        project_v4::resources::inspect_resource(self, request)
    }

    fn initial_v4_atomic(
        &mut self,
        intent: V4Intent<'_>,
        prepared: PreparedV4Create,
    ) -> Result<ProjectSnapshotV4, EditorError> {
        self.authorize_v4(intent)?;
        require_prepared_request(intent, prepared.request_json())?;
        // Hash/descriptor reads precede the SQLite write lock. Its shared CAS
        // gate remains held until publication, fencing cooperative collection.
        let _cas =
            resources::verify_document(&self.asset_store_root, &prepared.snapshot().project, None)?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        authorize(&tx, &context, intent)?;
        require_prepared_request(intent, prepared.request_json())?;
        if let Some(raw) = replay(&tx, intent, "project_v4")? {
            return decode_json(&raw, "V4 creation receipt");
        }
        if tx
            .query_row(
                "SELECT 1 FROM projects WHERE project_id=?1",
                [intent.project_id()],
                |_| Ok(()),
            )
            .optional()
            .map_err(storage_error)?
            .is_some()
        {
            return Err(EditorError::new("ALREADY_EXISTS", "Project already exists"));
        }
        let snapshot = prepared.snapshot();
        if snapshot.project.id != intent.project_id() {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Prepared project identity differs from request",
            ));
        }
        opencut_editor_core::project_v4::validate_project_v4(&snapshot.project)?;
        let document = encode_json(&snapshot.project, "V4 project document")?;
        let receipt = encode_json(snapshot, "V4 creation receipt")?;
        tx.execute(
            "INSERT INTO projects(project_id,name,head_revision) VALUES (?1,?2,?3)",
            params![
                snapshot.project.id,
                snapshot.project.name,
                snapshot.revision
            ],
        )
        .map_err(constraint_or_storage)?;
        tx.execute("INSERT INTO revisions(revision,project_id,parent_revision,transaction_id,document_json) VALUES (?1,?2,NULL,NULL,?3)", params![snapshot.revision,snapshot.project.id,document]).map_err(storage_error)?;
        if let V4Intent::Copy(input) = intent {
            tx.execute("INSERT INTO project_media_bindings(project_id,media_id) SELECT ?1,media_id FROM available_project_media WHERE project_id=?2", params![snapshot.project.id,input.source_project_id]).map_err(storage_error)?;
        }
        insert_receipt(&tx, intent, "project_v4", &receipt)?;
        let event = if matches!(intent, V4Intent::Copy(_)) {
            "project.v4_copied"
        } else {
            "project.v4_created"
        };
        // Stored receipts preserve the aggregate; observable events carry only
        // revision identity, never legacy locators or resource provenance.
        let event_payload = encode_json(
            &serde_json::json!({"project_id": snapshot.project.id,
                "revision": snapshot.revision, "schema_version": 4}),
            "V4 project event summary",
        )?;
        tx.execute("INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json) VALUES (?1,?2,?3,?4,?5)", params![prepared.event_id(),snapshot.project.id,snapshot.revision,event,event_payload]).map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            event,
            sqlite_audit::Links {
                project_id: Some(&snapshot.project.id),
                revision: Some(&snapshot.revision),
                ..Default::default()
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(snapshot.clone())
    }
}
impl ProjectV4Store for EditorApplication {
    fn create_v4_atomic(
        &mut self,
        input: &CreateV4Project,
        prepared: PreparedV4Create,
    ) -> Result<ProjectSnapshotV4, EditorError> {
        self.initial_v4_atomic(V4Intent::Create(input), prepared)
    }
    fn copy_v4_atomic(
        &mut self,
        input: &CopyV4Project,
        prepared: PreparedV4Create,
    ) -> Result<ProjectSnapshotV4, EditorError> {
        self.initial_v4_atomic(V4Intent::Copy(input), prepared)
    }
    fn commit_v4_atomic(
        &mut self,
        intent: V4Intent<'_>,
        prepared: PreparedV4Commit,
    ) -> Result<TransactionResult, EditorError> {
        self.authorize_v4(intent)?;
        require_prepared_request(intent, prepared.request_json())?;
        resources::authorize_document(
            &self.connection,
            &self.connection.1,
            intent,
            &prepared.snapshot().project,
        )?;
        let fixed = load_source(
            &self.connection,
            intent.project_id(),
            intent.expected_revision(),
        )?;
        let inherited = if matches!(intent, V4Intent::Promote(_)) {
            None
        } else {
            Some(&fixed.document)
        };
        let _cas = resources::verify_document(
            &self.asset_store_root,
            &prepared.snapshot().project,
            inherited,
        )?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        authorize(&tx, &context, intent)?;
        require_prepared_request(intent, prepared.request_json())?;
        // Original analysis authority and the exact caption transformation are
        // rechecked under this same IMMEDIATE publication lock, including replay.
        sqlite_caption_v4::require_caption_proposal(&tx, &context, intent, prepared.snapshot())?;
        if let Some(raw) = replay(&tx, intent, "transaction")? {
            return decode_edit_receipt(intent, &raw);
        }
        let current = load_source(&tx, intent.project_id(), None)?;
        let expected = intent.expected_revision().ok_or_else(|| {
            EditorError::new("INVALID_ARGUMENT", "Creation is not an edit intent")
        })?;
        if current.revision != expected {
            return Err(revision_conflict(expected, &current.revision));
        }
        let snapshot = prepared.snapshot();
        let result = prepared.result();
        if snapshot.project.id != intent.project_id()
            || result.base_revision != current.revision
            || result.revision != snapshot.revision
        {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Prepared commit differs from its fixed revision or scope",
            ));
        }
        opencut_editor_core::project_v4::validate_project_v4(&snapshot.project)?;
        resources::authorize_document(&tx, &context, intent, &snapshot.project)?;
        let document = encode_json(&snapshot.project, "V4 committed document")?;
        let receipt = encode_json(result, "V4 transaction receipt")?;
        tx.execute("INSERT INTO revisions(revision,project_id,parent_revision,transaction_id,document_json) VALUES (?1,?2,?3,?4,?5)", params![result.revision,intent.project_id(),result.base_revision,result.transaction_id,document]).map_err(storage_error)?;
        let updated = tx.execute("UPDATE projects SET head_revision=?1,name=?2 WHERE project_id=?3 AND head_revision=?4", params![result.revision,snapshot.project.name,intent.project_id(),result.base_revision]).map_err(storage_error)?;
        if updated != 1 {
            return Err(EditorError::new(
                "REVISION_CONFLICT",
                "Project head moved before V4 publication",
            ));
        }
        tx.execute("INSERT INTO editor_transactions(transaction_id,project_id,base_revision,revision,request_json,result_json) VALUES (?1,?2,?3,?4,?5,?6)", params![result.transaction_id,intent.project_id(),result.base_revision,result.revision,prepared.journal_json(),receipt]).map_err(storage_error)?;
        let original_receipt = if matches!(
            intent,
            V4Intent::CaptionGenerate(_) | V4Intent::CaptionImport(_)
        ) {
            encode_json(
                &sqlite_caption_v4::caption_receipt(&tx, intent, result)?,
                "Original caption receipt",
            )?
        } else {
            receipt.clone()
        };
        insert_receipt(&tx, intent, "transaction", &original_receipt)?;
        let event = match intent {
            V4Intent::Promote(_) => "project.v4_promoted",
            V4Intent::Restore(_) => "project.v4_restored",
            V4Intent::Undo(_) => "project.v4_selective_undo",
            V4Intent::Redo(_) => "project.v4_selective_redo",
            V4Intent::CaptionGenerate(_) => "subtitle.v4_generated",
            V4Intent::CaptionImport(_) => "subtitle.v4_imported",
            _ => "project.v4_edited",
        };
        tx.execute("INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json) VALUES (?1,?2,?3,?4,?5)", params![prepared.event_id(),intent.project_id(),result.revision,event,receipt]).map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            event,
            sqlite_audit::Links {
                project_id: Some(intent.project_id()),
                revision: Some(&result.revision),
                transaction_id: Some(&result.transaction_id),
                ..Default::default()
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(result.clone())
    }
}

fn insert_receipt(
    connection: &Connection,
    intent: V4Intent<'_>,
    kind: &str,
    receipt: &str,
) -> Result<(), EditorError> {
    connection.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json) VALUES (?1,?2,?3,?4,?5,?6)", params![intent.family(),intent.scope(),intent.key(),intent.identity()?.current_digest(),kind,receipt]).map_err(idempotency_insert_error)?;
    Ok(())
}

fn require_prepared_request(intent: V4Intent<'_>, prepared: &str) -> Result<(), EditorError> {
    if prepared != intent.request_json()? {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Prepared V4 payload belongs to another request",
        ));
    }
    Ok(())
}

impl EditorApplication {
    pub fn create_v4_project(
        &mut self,
        input: &CreateV4Project,
    ) -> Result<ProjectSnapshotV4, EditorError> {
        project_v4::create(self, input)
    }
    pub fn promote_v4_project(
        &mut self,
        input: &PromoteV4Project,
    ) -> Result<TransactionResult, EditorError> {
        project_v4::promote(self, input)
    }
    pub fn inspect_v4_project(
        &self,
        input: &InspectV4Project,
    ) -> Result<ProjectSnapshotV4, EditorError> {
        project_v4::inspect(self, input)
    }
    pub fn apply_v4_project(
        &mut self,
        input: &ApplyV4Project,
    ) -> Result<TransactionResult, EditorError> {
        project_v4::apply(self, input)
    }
    pub fn restore_v4_revision(
        &mut self,
        input: &RestoreV4Revision,
    ) -> Result<TransactionResult, EditorError> {
        project_v4::restore(self, input)
    }
    pub fn copy_v4_project(
        &mut self,
        input: &CopyV4Project,
    ) -> Result<ProjectSnapshotV4, EditorError> {
        project_v4::copy(self, input)
    }
    pub fn list_v4_projects(&self, input: &ListV4Projects) -> Result<V4ProjectPage, EditorError> {
        project_v4::list(self, input)
    }
}

impl ProjectV4LibraryRead for EditorApplication {
    fn list_v4_projects(&self, input: &ListV4Projects) -> Result<V4ProjectPage, EditorError> {
        if !(1..=100).contains(&input.limit) {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Project page limit must be 1..100",
            ));
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let principal = sqlite_access::principal(&tx, &self.connection.1)?;
        use sha2::{Digest, Sha256};
        let caller_hash = format!(
            "{:x}",
            Sha256::digest(principal.as_deref().unwrap_or("local-owner").as_bytes())
        );
        let prefix = format!("v4:{caller_hash}:");
        let after = match input.cursor.as_deref() {
            None => "",
            Some(cursor) if cursor.len() <= 1200 => cursor
                .strip_prefix(&prefix)
                .filter(|id| !id.is_empty())
                .ok_or_else(|| {
                    EditorError::new(
                        "INVALID_CURSOR",
                        "Project cursor belongs to another caller or library format",
                    )
                })?,
            Some(_) => {
                return Err(EditorError::new(
                    "INVALID_CURSOR",
                    "Project cursor is too long",
                ));
            }
        };
        let mut statement = tx.prepare("SELECT p.project_id,p.name,p.head_revision,
            json_extract(r.document_json,'$.schema_version'),
            CASE WHEN json_extract(r.document_json,'$.schema_version')=3 THEN json_extract(r.document_json,'$.base.id') ELSE json_extract(r.document_json,'$.id') END
            FROM projects p JOIN revisions r ON r.project_id=p.project_id AND r.revision=p.head_revision
            WHERE p.project_id>?2 AND NOT EXISTS(SELECT 1 FROM deleted_projects d WHERE d.project_id=p.project_id)
            AND (?1 IS NULL OR EXISTS(SELECT 1 FROM access_grants g WHERE g.principal_id=?1 AND g.project_id=p.project_id AND g.action_mask & 1 != 0))
            ORDER BY p.project_id LIMIT ?3").map_err(storage_error)?;
        let rows = statement
            .query_map(params![principal, after, (input.limit + 1) as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<u32>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(storage_error)?;
        let mut items = Vec::new();
        for row in rows {
            let (project_id, name, revision, schema_version, document_id) =
                row.map_err(storage_error)?;
            let schema_version = schema_version
                .filter(|version| (1..=4).contains(version))
                .ok_or_else(|| {
                    EditorError::new(
                        "UNSUPPORTED_PROJECT_VERSION",
                        "Library contains an unsupported outer project schema",
                    )
                })?;
            if document_id.as_deref() != Some(project_id.as_str()) {
                return Err(EditorError::new(
                    "STORAGE_ERROR",
                    "Library revision differs from project scope",
                ));
            }
            items.push(V4ProjectSummary {
                project_id,
                name,
                revision,
                schema_version,
            });
        }
        let next_cursor = if items.len() > input.limit {
            items.truncate(input.limit);
            items
                .last()
                .map(|item| format!("{prefix}{}", item.project_id))
        } else {
            None
        };
        drop(statement);
        tx.commit().map_err(storage_error)?;
        Ok(V4ProjectPage { items, next_cursor })
    }
}

fn decode_edit_receipt(intent: V4Intent<'_>, raw: &str) -> Result<TransactionResult, EditorError> {
    if matches!(
        intent,
        V4Intent::CaptionGenerate(_) | V4Intent::CaptionImport(_)
    ) {
        Ok(
            decode_json::<opencut_api_contracts::subtitle_v4::CaptionReceiptV4>(
                raw,
                "Original caption receipt",
            )?
            .transaction,
        )
    } else {
        decode_json(raw, "V4 edit receipt")
    }
}
