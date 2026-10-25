//! Schema5 shares the original revision/receipt/media/audit authority. No sidecar.
use super::*;
use opencut_contracts::{project_readonly::*, scene::EditorSnapshot, scene_ops::*};
use opencut_editor_application::{
    project_readonly::{self as app, *},
    scene::{SceneCommitIntent, StoredProjectDocument},
};
const FAMILY: &str = "project_v5_readonly_v1_create";
const SCOPE: &str = "workspace";
#[cfg(test)]
mod tests;
fn copy_intent(i: &CreateReadOnlyProject) -> CopySceneProject {
    CopySceneProject {
        source_project_id: i.source_project_id.clone(),
        source_revision: i.source_revision.clone(),
        project_id: i.project_id.clone(),
        name: i.name.clone(),
        idempotency_key: i.idempotency_key.clone(),
    }
}
fn authorize(
    c: &Connection,
    ctx: &opencut_editor_application::audit::ExecutionContext,
    i: &CreateReadOnlyProject,
) -> Result<(), EditorError> {
    sqlite_access::owner(ctx)?;
    sqlite_scene::authorize(c, ctx, SceneCommitIntent::Copy(&copy_intent(i)))
}
fn replay(
    c: &Connection,
    i: &CreateReadOnlyProject,
) -> Result<Option<StoredReadOnlySnapshot>, EditorError> {
    load_idempotent(
        c,
        FAMILY,
        SCOPE,
        &i.idempotency_key,
        &request_hash(FAMILY, SCOPE, i)?,
        "project_v5_readonly",
    )?
    .map(|raw| {
        let snapshot: StoredReadOnlySnapshot = decode_json(&raw, "read-only creation receipt")?;
        app::validate_document(&snapshot.document)?;
        let source = sqlite_scene::load_source(c, &i.source_project_id, Some(&i.source_revision))?;
        let StoredProjectDocument::Editable(base) = source.document else {
            return Err(app::readonly_error());
        };
        let expected = app::prepare(
            i,
            EditorSnapshot {
                project: *base,
                revision: source.revision,
            },
        )?;
        if expected.snapshot() != &snapshot {
            return Err(EditorError::new(
                "STORAGE_ERROR",
                "Preservation receipt differs from fixed source and payload",
            ));
        }
        if snapshot.document.id != i.project_id
            || snapshot.revision != app::revision(&snapshot.document)?
        {
            return Err(EditorError::new(
                "STORAGE_ERROR",
                "Read-only creation receipt differs from its document",
            ));
        }
        Ok(snapshot)
    })
    .transpose()
}
fn checkpoint(
    c: &Connection,
    ctx: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
    revision: &str,
) -> Result<(), EditorError> {
    sqlite_access::require(c, ctx, project, ProjectAction::Read)?;
    sqlite_project_library::require_live(c, project)?;
    let exists: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM revisions WHERE project_id=?1 AND revision=?2)",
            params![project, revision],
            |r| r.get(0),
        )
        .map_err(storage_error)?;
    if !exists {
        return Err(EditorError::new(
            "NOT_FOUND",
            "Read-only revision does not exist",
        ));
    }
    Ok(())
}
impl ReadOnlyProjectStore for EditorApplication {
    fn authorize_readonly_create(&self, i: &CreateReadOnlyProject) -> Result<(), EditorError> {
        authorize(&self.connection, &self.connection.1, i)
    }
    fn replay_readonly_create(
        &self,
        i: &CreateReadOnlyProject,
    ) -> Result<Option<StoredReadOnlySnapshot>, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        authorize(&tx, &self.connection.1, i)?;
        let value = replay(&tx, i)?;
        tx.commit().map_err(storage_error)?;
        Ok(value)
    }
    fn commit_readonly_create(
        &mut self,
        i: &CreateReadOnlyProject,
        prepared: PreparedReadOnlyCreate,
    ) -> Result<StoredReadOnlySnapshot, EditorError> {
        self.authorize_readonly_create(i)?;
        if prepared.request_json() != encode_json(i, "read-only request")? {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Prepared preservation belongs to another request",
            ));
        }
        let s = prepared.snapshot();
        let _cas = sqlite_scene::resources::verify_document(
            &self.asset_store_root,
            &s.document.base,
            None,
        )?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        authorize(&tx, &context, i)?;
        if let Some(result) = replay(&tx, i)? {
            return Ok(result);
        }
        let source =
            sqlite_scene::load_source(&tx, &i.source_project_id, Some(&i.source_revision))?;
        let StoredProjectDocument::Editable(base) = source.document else {
            return Err(EditorError::new(
                "UNSUPPORTED_PROJECT_VERSION",
                "Preservation requires an exact V4 source",
            ));
        };
        let expected = app::prepare(
            i,
            EditorSnapshot {
                project: *base,
                revision: source.revision,
            },
        )?;
        if expected.snapshot() != s {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Prepared preservation differs from its fixed source",
            ));
        }
        let copy = copy_intent(i);
        sqlite_scene::resources::authorize_document(
            &tx,
            &context,
            SceneCommitIntent::Copy(&copy),
            &s.document.base,
        )?;
        let doc = encode_json(&s.document, "preserved project document")?;
        let receipt = encode_json(s, "read-only creation receipt")?;
        tx.execute(
            "INSERT INTO projects(project_id,name,head_revision) VALUES (?1,?2,?3)",
            params![s.document.id, s.document.name, s.revision],
        )
        .map_err(constraint_or_storage)?;
        tx.execute("INSERT INTO revisions(revision,project_id,parent_revision,transaction_id,document_json) VALUES (?1,?2,NULL,NULL,?3)",params![s.revision,s.document.id,doc]).map_err(storage_error)?;
        tx.execute("INSERT INTO project_media_bindings(project_id,media_id) SELECT ?1,media_id FROM available_project_media WHERE project_id=?2",params![s.document.id,i.source_project_id]).map_err(storage_error)?;
        tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json) VALUES (?1,?2,?3,?4,'project_v5_readonly',?5)",params![FAMILY,SCOPE,i.idempotency_key,request_hash(FAMILY,SCOPE,i)?.current_digest(),receipt]).map_err(idempotency_insert_error)?;
        let event = "project.readonly_created";
        let summary = json!({"project_id":s.document.id,"revision":s.revision,"schema_version":5,"read_only":true});
        tx.execute("INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json) VALUES (?1,?2,?3,?4,?5)",params![prepared.event_id(),s.document.id,s.revision,event,encode_json(&summary,"preservation event")?]).map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            event,
            sqlite_audit::Links {
                project_id: Some(&s.document.id),
                revision: Some(&s.revision),
                ..Default::default()
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(prepared.snapshot().clone())
    }
    fn read_readonly_project(
        &self,
        i: &InspectProject,
    ) -> Result<StoredReadOnlySnapshot, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_access::require(&tx, &self.connection.1, &i.project_id, ProjectAction::Read)?;
        sqlite_project_library::require_live(&tx, &i.project_id)?;
        let row: Option<(String, String)> = if let Some(revision) = &i.revision {
            tx.query_row(
                "SELECT document_json,revision FROM revisions WHERE project_id=?1 AND revision=?2",
                params![i.project_id, revision],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(storage_error)?
        } else {
            tx.query_row("SELECT r.document_json,p.head_revision FROM projects p JOIN revisions r ON r.project_id=p.project_id AND r.revision=p.head_revision WHERE p.project_id=?1",[&i.project_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage_error)?
        };
        let (raw, revision) = row.ok_or_else(|| {
            EditorError::new("NOT_FOUND", "Read-only project revision does not exist")
        })?;
        let sqlite_project_document::ProjectStorageDocument::ReadOnly(document) =
            sqlite_project_document::decode_stored_project(&raw)?
        else {
            return Err(EditorError::new(
                "UNSUPPORTED_PROJECT_VERSION",
                "Use the project's normal versioned editor",
            ));
        };
        if document.id != i.project_id || revision != app::revision(&document)? {
            return Err(EditorError::new(
                "STORAGE_ERROR",
                "Preserved project identity differs from scope",
            ));
        }
        tx.commit().map_err(storage_error)?;
        Ok(StoredReadOnlySnapshot { document, revision })
    }
    fn checkpoint_readonly_read(&self, p: &str, r: &str) -> Result<(), EditorError> {
        checkpoint(&self.connection, &self.connection.1, p, r)
    }
}
