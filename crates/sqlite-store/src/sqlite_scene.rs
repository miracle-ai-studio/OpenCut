//! V4 uses the existing aggregate revision, journal, receipt and audit authority.
//! No mutable V4 content tables or bookmark sidecars are introduced.
use super::*;
use opencut_asset_store::AssetStore;
use opencut_contracts::{scene::*, scene_ops::*};
use opencut_editor_application::project_resources;
use opencut_editor_application::scene::{
    self, PreparedCreate, PreparedSceneCommit, SceneCommitIntent, SceneRead, SceneStore,
    StoredProjectDocument, StoredProjectRevision,
};
use sqlite_project_document::{ProjectStorageDocument, decode_stored_project};

mod public;
pub(crate) mod resources;
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
) -> Result<StoredProjectRevision, EditorError> {
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
    let (id, document) = match decode_stored_project(&raw)? {
        ProjectStorageDocument::Legacy(document) => (
            document.id.clone(),
            StoredProjectDocument::Legacy(Box::new(document)),
        ),
        ProjectStorageDocument::HistoricalVisual(document) => (
            document.base.id.clone(),
            StoredProjectDocument::HistoricalVisual(Box::new(document)),
        ),
        ProjectStorageDocument::ReadOnly(document) => {
            return Err(
                opencut_editor_application::project_readonly::export_unavailable(&document),
            );
        }
        ProjectStorageDocument::Editable(document) => (
            document.id.clone(),
            StoredProjectDocument::Editable(Box::new(document)),
        ),
    };
    if id != project {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Project identity differs from revision scope",
        ));
    }
    Ok(StoredProjectRevision { document, revision })
}

pub(super) fn intent_source(
    connection: &Connection,
    intent: SceneCommitIntent<'_>,
) -> Result<StoredProjectRevision, EditorError> {
    let reference =
        sqlite_branching::branch(connection, intent.project_id(), intent.target_branch())?;
    load_source(
        connection,
        intent.project_id(),
        Some(&reference.head_revision),
    )
}

pub(super) fn authorize(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    intent: SceneCommitIntent<'_>,
) -> Result<(), EditorError> {
    authorize_on(connection, context, intent, true)
}
fn authorize_on(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    intent: SceneCommitIntent<'_>,
    write: bool,
) -> Result<(), EditorError> {
    let id = intent.project_id();
    match intent {
        SceneCommitIntent::BranchMerge(input) => {
            opencut_editor_application::branching::validate_compare(
                &opencut_editor_application::branching::comparison(input),
            )?
        }

        SceneCommitIntent::PublicApply(r) => {
            if let Some(name) = &r.branch_name {
                sqlite_branching::branch_name(name, false)?;
            }
        }
        SceneCommitIntent::Promote(r) => {
            if let Some(name) = &r.branch_name {
                sqlite_branching::branch_name(name, false)?;
            }
        }
        SceneCommitIntent::Restore(r) => {
            if let Some(name) = &r.branch_name {
                sqlite_branching::branch_name(name, false)?;
            }
        }
        _ => {}
    }

    sqlite_project_library::require_live(connection, id)?;
    if matches!(
        intent,
        SceneCommitIntent::Create(_) | SceneCommitIntent::Copy(_)
    ) {
        sqlite_access::owner(context)?;
        if let SceneCommitIntent::Copy(input) = intent {
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
                SceneCommitIntent::Restore(_)
                | SceneCommitIntent::Undo(_)
                | SceneCommitIntent::Redo(_) => ProjectAction::Restore,
                _ => ProjectAction::Edit,
            },
        )?;
        sqlite_project_document::project_head_revision(connection, id)?;
    }
    if write {
        sqlite_edit_control::require_writable(connection, context, id)?;
    }
    resources::authorize_intent(connection, context, intent)
}

impl project_resources::ProjectResourceRead for EditorApplication {
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

impl SceneRead for EditorApplication {
    fn authorize_plan(&self, intent: SceneCommitIntent<'_>) -> Result<(), EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        authorize_on(&tx, &self.connection.1, intent, false)?;
        tx.commit().map_err(storage_error)
    }

    fn authorize_project(&self, intent: SceneCommitIntent<'_>) -> Result<(), EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        authorize(&tx, &self.connection.1, intent)?;
        tx.commit().map_err(storage_error)
    }
    fn head_scene_source(&self, project: &str) -> Result<StoredProjectRevision, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_access::require(&tx, &self.connection.1, project, ProjectAction::Read)?;
        let source = load_source(&tx, project, None)?;
        tx.commit().map_err(storage_error)?;
        Ok(source)
    }
    fn head_scene_branch(
        &self,
        project: &str,
        name: &str,
    ) -> Result<StoredProjectRevision, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_access::require(&tx, &self.connection.1, project, ProjectAction::Read)?;
        sqlite_project_library::require_live(&tx, project)?;
        let reference = sqlite_branching::branch(&tx, project, name)?;
        let source = load_source(&tx, project, Some(&reference.head_revision))?;
        tx.commit().map_err(storage_error)?;
        Ok(source)
    }
    fn revision_scene_source(
        &self,
        project: &str,
        revision: &str,
    ) -> Result<StoredProjectRevision, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_access::require(&tx, &self.connection.1, project, ProjectAction::Read)?;
        let source = load_source(&tx, project, Some(revision))?;
        tx.commit().map_err(storage_error)?;
        Ok(source)
    }
    fn replay_scene_create(
        &self,
        input: &CreateProjectRequest,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let intent = SceneCommitIntent::Create(input);
        authorize(&tx, &self.connection.1, intent)?;
        let value = replay(&tx, intent, "project_v4")?
            .map(|raw| decode_json(&raw, "V4 creation receipt"))
            .transpose()?;
        tx.commit().map_err(storage_error)?;
        Ok(value)
    }
    fn replay_scene_copy(
        &self,
        input: &CopySceneProject,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let intent = SceneCommitIntent::Copy(input);
        authorize(&tx, &self.connection.1, intent)?;
        let value = replay(&tx, intent, "project_v4")?
            .map(|raw| decode_json(&raw, "V4 copy receipt"))
            .transpose()?;
        tx.commit().map_err(storage_error)?;
        Ok(value)
    }
    fn replay_scene_edit(
        &self,
        intent: SceneCommitIntent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        authorize(&tx, &self.connection.1, intent)?;
        if let SceneCommitIntent::BranchMerge(input) = intent {
            let receipt = sqlite_project_branches::merge_receipt(&tx, input)?;
            if receipt.is_none() {
                let _ = load_idempotent(
                    &tx,
                    intent.family(),
                    intent.scope(),
                    intent.key(),
                    &intent.identity()?,
                    "branch_merge",
                )?;
            }
            tx.commit().map_err(storage_error)?;
            return Ok(receipt.map(|receipt| receipt.transaction));
        }

        if let SceneCommitIntent::Restore(request) = intent {
            if let Some(result) = historical_restore_receipt(&tx, request)? {
                tx.commit().map_err(storage_error)?;
                return Ok(Some(result));
            }
        }
        let value = replay(&tx, intent, "transaction")?
            .map(|raw| decode_edit_receipt(intent, &raw))
            .transpose()?;
        tx.commit().map_err(storage_error)?;
        Ok(value)
    }
}

/// Original restore journals have an independent historical key space. Reuse
/// the exact original hash/strict receipt reader;never reinterpret a key or
/// write another receipt merely to rename the public operation.
pub(super) fn historical_restore_receipt(
    connection: &Connection,
    request: &RestoreProject,
) -> Result<Option<TransactionResult>, EditorError> {
    if request.branch_name.is_some() {
        return Ok(None);
    }
    let exists:bool=connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM idempotency WHERE operation_family='restore_revision' AND scope=?1 AND idempotency_key=?2)",
        params![request.project_id,request.idempotency_key],|row|row.get(0),
    ).map_err(storage_error)?;
    if !exists {
        return Ok(None);
    }
    let conflict = || {
        EditorError::new(
            "IDEMPOTENCY_KEY_REUSED",
            "Restore key belongs to a different original request",
        )
    };
    if !request.asset_versions.is_empty() {
        return Err(conflict());
    }
    let original = RestoreRevisionRequest {
        expected_revision: request.expected_revision.clone(),
        target_revision: request.target_revision.clone(),
        idempotency_key: request.idempotency_key.clone(),
        label: request.label.clone().ok_or_else(conflict)?,
    };
    let raw = load_idempotent(
        connection,
        "restore_revision",
        &request.project_id,
        &request.idempotency_key,
        &request_hash("restore_revision", &request.project_id, &original)?,
        "transaction",
    )?
    .ok_or_else(|| EditorError::new("STORAGE_ERROR", "Original restore receipt disappeared"))?;
    let result: TransactionResult = decode_json(&raw, "original restore receipt")?;
    let exact:bool=connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM editor_transactions t JOIN revisions r ON r.project_id=t.project_id AND r.revision=t.revision WHERE t.project_id=?1 AND t.transaction_id=?2 AND t.base_revision=?3 AND t.revision=?4)",
        params![request.project_id,result.transaction_id,request.expected_revision,result.revision],|row|row.get(0),
    ).map_err(storage_error)?;
    if !exact || result.base_revision != request.expected_revision {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Original restore receipt differs from its immutable transaction",
        ));
    }
    Ok(Some(result))
}

fn replay(
    connection: &Connection,
    intent: SceneCommitIntent<'_>,
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
    pub fn plan_project_timeline(
        &self,
        request: &opencut_contracts::timeline::TimelineEditRequest,
    ) -> Result<opencut_contracts::timeline::TimelineEditPlan, EditorError> {
        opencut_editor_application::timeline::plan(self, request)
    }

    pub fn plan_scene_audio_gain(
        &self,
        request: &opencut_contracts::audio_gain::AudioGainRequest,
    ) -> Result<opencut_contracts::audio_gain::AudioGainPlan, EditorError> {
        opencut_editor_application::audio_gain::plan(self, request)
    }

    pub fn plan_scene_source_audio(
        &self,
        request: &opencut_contracts::source_audio::SourceAudioRequest,
    ) -> Result<opencut_contracts::source_audio::SourceAudioPlan, EditorError> {
        opencut_editor_application::source_audio::plan(self, request)
    }

    pub fn revalidate_project_resource_read(
        &self,
        request: &opencut_contracts::project_resources::InspectResource,
    ) -> Result<(), EditorError> {
        project_resources::revalidate_resource_read(self, request)
    }

    pub fn plan_scene_media_import(
        &self,
        request: &opencut_contracts::media_import::MediaImportRequest,
    ) -> Result<opencut_contracts::media_import::MediaImportPlan, EditorError> {
        opencut_editor_application::media_import::plan(self, request)
    }

    pub fn inspect_project_resource(
        &self,
        request: &opencut_contracts::project_resources::InspectResource,
    ) -> Result<opencut_contracts::project_resources::ResourceView, EditorError> {
        project_resources::inspect_resource(self, request)
    }

    fn initial_scene_atomic(
        &mut self,
        intent: SceneCommitIntent<'_>,
        prepared: PreparedCreate,
    ) -> Result<EditorSnapshot, EditorError> {
        self.authorize_project(intent)?;
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
        opencut_editor_core::scene::validate_project_scene(&snapshot.project)?;
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
        if let SceneCommitIntent::Copy(input) = intent {
            tx.execute("INSERT INTO project_media_bindings(project_id,media_id) SELECT ?1,media_id FROM available_project_media WHERE project_id=?2", params![snapshot.project.id,input.source_project_id]).map_err(storage_error)?;
        }
        insert_receipt(&tx, intent, "project_v4", &receipt)?;
        let event = if matches!(intent, SceneCommitIntent::Copy(_)) {
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
impl SceneStore for EditorApplication {
    fn create_scene_atomic(
        &mut self,
        input: &CreateProjectRequest,
        prepared: PreparedCreate,
    ) -> Result<EditorSnapshot, EditorError> {
        self.initial_scene_atomic(SceneCommitIntent::Create(input), prepared)
    }
    fn copy_scene_atomic(
        &mut self,
        input: &CopySceneProject,
        prepared: PreparedCreate,
    ) -> Result<EditorSnapshot, EditorError> {
        self.initial_scene_atomic(SceneCommitIntent::Copy(input), prepared)
    }
    fn commit_scene_atomic(
        &mut self,
        intent: SceneCommitIntent<'_>,
        prepared: PreparedSceneCommit,
    ) -> Result<TransactionResult, EditorError> {
        self.authorize_project(intent)?;
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
        let inherited = if matches!(intent, SceneCommitIntent::Promote(_)) {
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
        sqlite_font_derivations::require_derived_proposal(&tx, intent, prepared.snapshot())?;
        // Original analysis authority and the exact caption transformation are
        // rechecked under this same IMMEDIATE publication lock, including replay.
        sqlite_captions::require_caption_proposal(&tx, &context, intent, prepared.snapshot())?;
        if let SceneCommitIntent::BranchMerge(input) = intent {
            if let Some(receipt) = sqlite_project_branches::merge_receipt(&tx, input)? {
                return Ok(receipt.transaction);
            }
            let _ = load_idempotent(
                &tx,
                intent.family(),
                intent.scope(),
                intent.key(),
                &intent.identity()?,
                "branch_merge",
            )?;
        } else if let Some(raw) = replay(&tx, intent, "transaction")? {
            return decode_edit_receipt(intent, &raw);
        }
        let current = intent_source(&tx, intent)?;
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
        opencut_editor_core::scene::validate_project_scene(&snapshot.project)?;
        resources::authorize_document(&tx, &context, intent, &snapshot.project)?;
        let merge = if let SceneCommitIntent::BranchMerge(input) = intent {
            Some(sqlite_project_branches::publication(&tx, input, &prepared)?)
        } else {
            None
        };
        let document = encode_json(&snapshot.project, "V4 committed document")?;
        let receipt = encode_json(result, "V4 transaction receipt")?;
        tx.execute("INSERT INTO revisions(revision,project_id,parent_revision,transaction_id,document_json) VALUES (?1,?2,?3,?4,?5)", params![result.revision,intent.project_id(),result.base_revision,result.transaction_id,document]).map_err(storage_error)?;
        let updated=if intent.target_branch()=="main" {
            tx.execute("UPDATE projects SET head_revision=?1,name=?2 WHERE project_id=?3 AND head_revision=?4",params![result.revision,snapshot.project.name,intent.project_id(),result.base_revision])
        }else{
            tx.execute("UPDATE project_branches SET head_revision=?1 WHERE project_id=?2 AND branch_name=?3 AND head_revision=?4",params![result.revision,intent.project_id(),intent.target_branch(),result.base_revision])
        }.map_err(storage_error)?;
        if updated != 1 {
            return Err(EditorError::new(
                "REVISION_CONFLICT",
                "Project head moved before V4 publication",
            ));
        }
        if let Some(merge) = &merge {
            tx.execute("INSERT INTO project_merge_parents(revision,project_id,source_revision) VALUES (?1,?2,?3)",params![result.revision,intent.project_id(),merge.source_revision]).map_err(storage_error)?;
        }
        tx.execute("INSERT INTO editor_transactions(transaction_id,project_id,base_revision,revision,request_json,result_json) VALUES (?1,?2,?3,?4,?5,?6)", params![result.transaction_id,intent.project_id(),result.base_revision,result.revision,prepared.journal_json(),receipt]).map_err(storage_error)?;
        let original_receipt = if let Some(merge) = &merge {
            encode_json(merge, "branch merge receipt")?
        } else if matches!(
            intent,
            SceneCommitIntent::CaptionGenerate(_) | SceneCommitIntent::CaptionImport(_)
        ) {
            encode_json(
                &sqlite_captions::caption_receipt(&tx, intent, result)?,
                "Original caption receipt",
            )?
        } else {
            receipt.clone()
        };
        insert_receipt(
            &tx,
            intent,
            if merge.is_some() {
                "branch_merge"
            } else {
                "transaction"
            },
            &original_receipt,
        )?;
        sqlite_font_derivations::persist_proof(&tx, intent, prepared.snapshot())?;
        let event = if merge.is_some() {
            if intent.target_branch() == "main" {
                "transaction.committed"
            } else {
                "branch.merged"
            }
        } else if intent.target_branch() != "main" {
            "branch.edited"
        } else {
            match intent {
                SceneCommitIntent::Promote(_) => "project.v4_promoted",
                SceneCommitIntent::Restore(_) => "project.v4_restored",
                SceneCommitIntent::Undo(_) => "project.v4_selective_undo",
                SceneCommitIntent::Redo(_) => "project.v4_selective_redo",
                SceneCommitIntent::CaptionGenerate(_) => "subtitle.v4_generated",
                SceneCommitIntent::CaptionImport(_) => "subtitle.v4_imported",
                _ => "project.v4_edited",
            }
        };
        let event_payload = if merge.is_some() {
            original_receipt.clone()
        } else if intent.target_branch() != "main" {
            encode_json(
                &json!({"branch_name":intent.target_branch(),"transaction":result}),
                "branch edit event",
            )?
        } else {
            receipt.clone()
        };
        tx.execute("INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json) VALUES (?1,?2,?3,?4,?5)", params![prepared.event_id(),intent.project_id(),result.revision,event,event_payload]).map_err(storage_error)?;
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
    intent: SceneCommitIntent<'_>,
    kind: &str,
    receipt: &str,
) -> Result<(), EditorError> {
    connection.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json) VALUES (?1,?2,?3,?4,?5,?6)", params![intent.family(),intent.scope(),intent.key(),intent.identity()?.current_digest(),kind,receipt]).map_err(idempotency_insert_error)?;
    Ok(())
}

fn require_prepared_request(
    intent: SceneCommitIntent<'_>,
    prepared: &str,
) -> Result<(), EditorError> {
    if prepared != intent.request_json()? {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Prepared V4 payload belongs to another request",
        ));
    }
    Ok(())
}

impl EditorApplication {
    pub fn create_scene_project(
        &mut self,
        input: &CreateProjectRequest,
    ) -> Result<EditorSnapshot, EditorError> {
        scene::create(self, input)
    }
    pub fn promote_project(
        &mut self,
        input: &PromoteProject,
    ) -> Result<TransactionResult, EditorError> {
        scene::promote(self, input)
    }
    pub fn inspect_scene_project(
        &self,
        input: &InspectProject,
    ) -> Result<EditorSnapshot, EditorError> {
        scene::inspect(self, input)
    }
    pub fn apply_scene_project(
        &mut self,
        input: &ApplySceneProject,
    ) -> Result<TransactionResult, EditorError> {
        scene::apply(self, input)
    }
    pub fn restore_project(
        &mut self,
        input: &RestoreProject,
    ) -> Result<TransactionResult, EditorError> {
        scene::restore(self, input)
    }
    pub fn copy_scene_project(
        &mut self,
        input: &CopySceneProject,
    ) -> Result<EditorSnapshot, EditorError> {
        scene::copy(self, input)
    }
}

fn decode_edit_receipt(
    intent: SceneCommitIntent<'_>,
    raw: &str,
) -> Result<TransactionResult, EditorError> {
    if matches!(
        intent,
        SceneCommitIntent::CaptionGenerate(_) | SceneCommitIntent::CaptionImport(_)
    ) {
        Ok(
            decode_json::<opencut_api_contracts::subtitles::CaptionReceipt>(
                raw,
                "Original caption receipt",
            )?
            .transaction,
        )
    } else {
        decode_json(raw, "V4 edit receipt")
    }
}

impl opencut_editor_application::project_catalog::ProjectCreationStore for EditorApplication {
    fn replay_project_creation(
        &self,
        input: &opencut_contracts::project_catalog::CreateProjectRequest,
    ) -> Result<Option<opencut_contracts::project_catalog::ProjectCreated>, EditorError> {
        use opencut_contracts::project_catalog::ProjectCreated;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        authorize(&tx, &self.connection.1, SceneCommitIntent::Create(input))?;
        let mut found = None;
        let mut any = false;
        {
            let mut statement=tx.prepare("SELECT operation_family,request_hash,result_kind,result_json FROM idempotency WHERE scope='workspace' AND idempotency_key=?1 AND operation_family IN ('create_project','project_v3_create','project_v4_create')").map_err(storage_error)?;
            let rows = statement
                .query_map([&input.idempotency_key], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })
                .map_err(storage_error)?;
            for row in rows {
                let (family, stored_hash, kind, raw) = row.map_err(storage_error)?;
                any = true;
                let (identity, expected_kind) = match family.as_str() {
                    "create_project" => (
                        request_hash("create_project", &input.project.id, &input.project)?,
                        "snapshot",
                    ),
                    "project_v3_create" => (
                        request_hash("project_v3_create", "workspace", input)?,
                        "project_v3",
                    ),
                    "project_v4_create" => {
                        (SceneCommitIntent::Create(input).identity()?, "project_v4")
                    }
                    _ => unreachable!(),
                };
                // Legacy creators had independent key spaces. Match their exact
                // original request,never a suffix priority or a current head.
                if !identity.matches(&stored_hash, None) {
                    continue;
                }
                if kind != expected_kind {
                    return Err(EditorError::new(
                        "STORAGE_ERROR",
                        "Creation receipt kind differs from its original operation",
                    ));
                }
                let value: serde_json::Value = decode_json(&raw, "original creation receipt")?;
                let revision = value
                    .get("revision")
                    .and_then(|v| v.as_str())
                    .filter(|v| !v.trim().is_empty())
                    .ok_or_else(|| {
                        EditorError::new("STORAGE_ERROR", "Creation receipt revision is missing")
                    })?;
                let project = value.get("project").ok_or_else(|| {
                    EditorError::new("STORAGE_ERROR", "Creation receipt document is missing")
                })?;
                let document =
                    decode_stored_project(&encode_json(project, "original creation document")?)?;
                let id = match (&document, family.as_str()) {
                    (ProjectStorageDocument::Legacy(p), "create_project") => &p.id,
                    (ProjectStorageDocument::HistoricalVisual(p), "project_v3_create") => {
                        &p.base.id
                    }
                    (ProjectStorageDocument::Editable(p), "project_v4_create") => &p.id,
                    _ => {
                        return Err(EditorError::new(
                            "STORAGE_ERROR",
                            "Creation receipt document differs from its original format",
                        ));
                    }
                };
                if id != &input.project.id {
                    return Err(EditorError::new(
                        "STORAGE_ERROR",
                        "Creation receipt project scope changed",
                    ));
                }
                let original: Option<String> = tx
                    .query_row(
                        "SELECT document_json FROM revisions WHERE project_id=?1 AND revision=?2",
                        params![id, revision],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(storage_error)?;
                let original: serde_json::Value = decode_json(
                    &original.ok_or_else(|| {
                        EditorError::new(
                            "STORAGE_ERROR",
                            "Creation receipt revision is missing from storage",
                        )
                    })?,
                    "original creation revision",
                )?;
                if &original != project || found.is_some() {
                    return Err(EditorError::new(
                        "STORAGE_ERROR",
                        "Creation receipt is inconsistent with immutable storage",
                    ));
                }
                found = Some(ProjectCreated {
                    project_id: id.clone(),
                    revision: revision.into(),
                });
            }
        }
        if any && found.is_none() {
            return Err(EditorError::new(
                "IDEMPOTENCY_KEY_REUSED",
                "Creation key was already used for a different request",
            ));
        }
        tx.commit().map_err(storage_error)?;
        Ok(found)
    }
}
impl EditorApplication {
    pub fn create_project_request(
        &mut self,
        input: &opencut_contracts::project_catalog::CreateProjectRequest,
    ) -> Result<opencut_contracts::project_catalog::ProjectCreated, EditorError> {
        opencut_editor_application::project_catalog::create(self, input)
    }
}
