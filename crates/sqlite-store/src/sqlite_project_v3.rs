//! Schema 3 revisions share the existing project head, journal and audit authority.
pub(super) const MIGRATION: &str = "
UPDATE metadata SET value='23' WHERE key='schema_version' AND value='22';
";

use super::*;
use opencut_contracts::{
    Operation, TransactionRequest,
    project_v3::{ProjectDocumentV3, ProjectSnapshotV3},
    project_v3_ops::{
        ApplyV3Project, CreateV3Project, InspectV3Project, ListV3Projects, PromoteV3Project,
        V3ProjectPage, V3ProjectSummary, VisualOperation,
    },
};
use opencut_editor_application::projects::AssetPinning;
use sha2::{Digest, Sha256};

use super::sqlite_project_document::decode_v3_project;

pub(super) fn load_v3_snapshot(
    connection: &Connection,
    project: &str,
    revision: Option<&str>,
) -> Result<ProjectSnapshotV3, EditorError> {
    sqlite_project_library::require_live(connection, project)?;
    let (document, revision): (String, String) = if let Some(revision) = revision {
        connection
            .query_row(
                "SELECT document_json,revision FROM revisions WHERE project_id=?1 AND revision=?2",
                params![project, revision],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or_else(|| EditorError::new("NOT_FOUND", "Project revision does not exist"))?
    } else {
        connection
            .query_row(
                "SELECT r.document_json,p.head_revision FROM projects p JOIN revisions r
            ON r.revision=p.head_revision WHERE p.project_id=?1",
                [project],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or_else(|| EditorError::new("NOT_FOUND", "Project does not exist"))?
    };
    let project_document = decode_v3_project(&document)?;
    if project_document.base.id != project {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Project identity differs from revision scope",
        ));
    }
    Ok(ProjectSnapshotV3 {
        project: project_document,
        revision,
    })
}

pub(super) fn revision_id(
    document: &ProjectDocumentV3,
    parent: Option<&str>,
    transaction: Option<&str>,
) -> Result<String, EditorError> {
    let material = encode_json(
        &json!({"parent":parent,"transaction_id":transaction,"document":document}),
        "schema 3 revision material",
    )?;
    Ok(format!("rev_{:x}", Sha256::digest(material.as_bytes())))
}
fn require_read(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
) -> Result<(), EditorError> {
    sqlite_access::require(connection, context, project, ProjectAction::Read)
}
pub(super) fn require_edit(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
) -> Result<(), EditorError> {
    require_read(connection, context, project)?;
    sqlite_access::require(connection, context, project, ProjectAction::Edit)
}
fn base_request(input: &ApplyV3Project) -> Option<TransactionRequest> {
    let operations: Vec<_> = input
        .operations
        .iter()
        .filter_map(|operation| match operation {
            VisualOperation::Base { operation } => Some(operation.clone()),
            _ => None,
        })
        .collect();
    (!operations.is_empty()).then(|| TransactionRequest {
        expected_revision: input.expected_revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: input.label.clone(),
        operations,
    })
}
pub(super) fn restricted_base(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    input: &ApplyV3Project,
) -> Result<(), EditorError> {
    if context.caller().is_owner() {
        return Ok(());
    }
    if let Some(request) = base_request(input) {
        access::restricted_edit(&request, &|id| {
            Ok(sqlite_uploads::media(connection, &input.project_id, id)?.map(|r| r.asset))
        })?;
    }
    Ok(())
}
fn page_identity(principal: Option<&str>) -> String {
    format!(
        "{:x}",
        Sha256::digest(principal.unwrap_or("local-owner").as_bytes())
    )
}
fn decode_cursor(cursor: Option<&str>, principal: Option<&str>) -> Result<String, EditorError> {
    let Some(cursor) = cursor else {
        return Ok(String::new());
    };
    if cursor.len() > 1200 {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Invalid project cursor",
        ));
    }
    let parts: Vec<_> = cursor.splitn(3, ':').collect();
    if parts.len() != 3
        || parts[0] != "v1"
        || parts[1] != page_identity(principal)
        || parts[2].is_empty()
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Project cursor is not valid for this caller",
        ));
    }
    Ok(parts[2].into())
}

impl EditorApplication {
    pub fn create_v3_project(
        &mut self,
        input: &CreateV3Project,
    ) -> Result<ProjectSnapshotV3, EditorError> {
        require_idempotency_key(&input.idempotency_key)?;
        let identity = request_hash("project_v3_create", "workspace", input)?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_access::owner(&context)?;
        sqlite_project_library::require_live(&tx, &input.project.id)?;
        if let Some(value) = load_idempotent(
            &tx,
            "project_v3_create",
            "workspace",
            &input.idempotency_key,
            &identity,
            "project_v3",
        )? {
            return decode_json(&value, "schema 3 creation receipt");
        }
        if tx
            .query_row(
                "SELECT 1 FROM projects WHERE project_id=?1",
                [&input.project.id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(storage_error)?
            .is_some()
        {
            return Err(EditorError::new("ALREADY_EXISTS", "Project already exists"));
        }
        let document = opencut_editor_application::project_v3::create(&input.project)?;
        let revision = revision_id(&document, None, None)?;
        let snapshot = ProjectSnapshotV3 {
            project: document,
            revision: revision.clone(),
        };
        tx.execute(
            "INSERT INTO projects(project_id,name,head_revision) VALUES (?1,?2,?3)",
            params![input.project.id, input.project.name, revision],
        )
        .map_err(constraint_or_storage)?;
        tx.execute("INSERT INTO revisions(revision,project_id,parent_revision,transaction_id,document_json)
            VALUES (?1,?2,NULL,NULL,?3)",params![revision,input.project.id,encode_json(&snapshot.project,"schema 3 project")?])
            .map_err(storage_error)?;
        let receipt = encode_json(&snapshot, "schema 3 creation receipt")?;
        tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json)
            VALUES ('project_v3_create','workspace',?1,?2,'project_v3',?3)",
            params![input.idempotency_key,identity.current_digest(),receipt]).map_err(idempotency_insert_error)?;
        tx.execute(
            "INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json)
            VALUES (?1,?2,?3,'project.v3_created',?4)",
            params![
                format!("evt_{}", Uuid::new_v4().simple()),
                input.project.id,
                revision,
                receipt
            ],
        )
        .map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            "project.v3_created",
            sqlite_audit::Links {
                project_id: Some(&input.project.id),
                revision: Some(&revision),
                ..Default::default()
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(snapshot)
    }

    pub fn promote_v3_project(
        &mut self,
        input: &PromoteV3Project,
    ) -> Result<ProjectSnapshotV3, EditorError> {
        require_idempotency_key(&input.idempotency_key)?;
        let identity = request_hash("project_v3_promote", &input.project_id, input)?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_access::owner(&context)?;
        if let Some(value) = load_idempotent(
            &tx,
            "project_v3_promote",
            &input.project_id,
            &input.idempotency_key,
            &identity,
            "project_v3",
        )? {
            return decode_json(&value, "schema 3 promotion receipt");
        }
        let current = load_snapshot(&tx, &input.project_id)?;
        if current.revision != input.expected_revision {
            return Err(revision_conflict(
                &input.expected_revision,
                &current.revision,
            ));
        }
        let document = opencut_editor_application::project_v3::promote(&current.project)?;
        let transaction_id = format!("txn_{}", Uuid::new_v4().simple());
        let revision = revision_id(&document, Some(&current.revision), Some(&transaction_id))?;
        let snapshot = ProjectSnapshotV3 {
            project: document,
            revision: revision.clone(),
        };
        tx.execute("INSERT INTO revisions(revision,project_id,parent_revision,transaction_id,document_json)
            VALUES (?1,?2,?3,?4,?5)",params![revision,input.project_id,current.revision,transaction_id,
                encode_json(&snapshot.project,"promoted schema 3 project")?]).map_err(storage_error)?;
        let updated = tx
            .execute(
                "UPDATE projects SET head_revision=?1 WHERE project_id=?2 AND head_revision=?3",
                params![revision, input.project_id, current.revision],
            )
            .map_err(storage_error)?;
        if updated != 1 {
            return Err(EditorError::new(
                "REVISION_CONFLICT",
                "Project head moved during promotion",
            ));
        }
        let result = opencut_contracts::TransactionResult {
            transaction_id: transaction_id.clone(),
            base_revision: current.revision.clone(),
            revision: revision.clone(),
            created_ids: vec![],
            changed_ids: vec![input.project_id.clone()],
            deleted_ids: vec![],
        };
        tx.execute("INSERT INTO editor_transactions(transaction_id,project_id,base_revision,revision,request_json,result_json)
            VALUES (?1,?2,?3,?4,?5,?6)",params![transaction_id,input.project_id,current.revision,revision,
                encode_json(&json!({"kind":"project_v3.promote","request":input}),"promotion journal")?,
                encode_json(&result,"promotion result")?]).map_err(storage_error)?;
        let receipt = encode_json(&snapshot, "schema 3 promotion receipt")?;
        tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json)
            VALUES ('project_v3_promote',?1,?2,?3,'project_v3',?4)",
            params![input.project_id,input.idempotency_key,identity.current_digest(),receipt]).map_err(idempotency_insert_error)?;
        tx.execute(
            "INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json)
            VALUES (?1,?2,?3,'project.v3_promoted',?4)",
            params![
                format!("evt_{}", Uuid::new_v4().simple()),
                input.project_id,
                revision,
                receipt
            ],
        )
        .map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            "project.v3_promoted",
            sqlite_audit::Links {
                project_id: Some(&input.project_id),
                revision: Some(&revision),
                transaction_id: Some(&transaction_id),
                ..Default::default()
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(snapshot)
    }

    pub fn inspect_v3_project(
        &self,
        input: &InspectV3Project,
    ) -> Result<ProjectSnapshotV3, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        require_read(&tx, &self.connection.1, &input.project_id)?;
        let snapshot = load_v3_snapshot(&tx, &input.project_id, input.revision.as_deref())?;
        tx.commit().map_err(storage_error)?;
        Ok(snapshot)
    }

    pub fn list_v3_projects(&self, input: &ListV3Projects) -> Result<V3ProjectPage, EditorError> {
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
        let after = decode_cursor(input.cursor.as_deref(), principal.as_deref())?;
        let mut statement = tx
            .prepare(
                "SELECT p.project_id,p.name,p.head_revision FROM projects p
            JOIN revisions r ON r.revision=p.head_revision
            WHERE (?1 IS NULL OR EXISTS(SELECT 1 FROM access_grants g WHERE g.principal_id=?1
              AND g.project_id=p.project_id AND g.action_mask & 1 != 0))
              AND NOT EXISTS(SELECT 1 FROM deleted_projects d WHERE d.project_id=p.project_id)
              AND p.project_id>?2 AND json_extract(r.document_json,'$.schema_version')=3
            ORDER BY p.project_id LIMIT ?3",
            )
            .map_err(storage_error)?;
        let rows = statement
            .query_map(params![principal, after, input.limit + 1], |row| {
                Ok(V3ProjectSummary {
                    project_id: row.get(0)?,
                    name: row.get(1)?,
                    revision: row.get(2)?,
                    schema_version: 3,
                })
            })
            .map_err(storage_error)?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(storage_error)?);
        }
        let next_cursor = if items.len() > input.limit {
            items.truncate(input.limit);
            Some(format!(
                "v1:{}:{}",
                page_identity(principal.as_deref()),
                items.last().expect("positive limit").project_id
            ))
        } else {
            None
        };
        drop(statement);
        tx.commit().map_err(storage_error)?;
        Ok(V3ProjectPage { items, next_cursor })
    }

    pub fn execution_snapshot_v3(
        &self,
        project: &str,
        revision: &str,
    ) -> Result<opencut_editor_application::project_v3::AuthorizedSnapshotV3, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_access::require(&tx, &self.connection.1, project, ProjectAction::RunJobs)?;
        let snapshot = load_v3_snapshot(&tx, project, Some(revision))?;
        let sources = if self.connection.1.caller().is_owner() {
            opencut_editor_application::access::SourceAccess::OwnerPaths
        } else {
            opencut_editor_application::access::SourceAccess::ProjectContent
        };
        tx.commit().map_err(storage_error)?;
        Ok(opencut_editor_application::project_v3::AuthorizedSnapshotV3 { snapshot, sources })
    }

    pub fn plan_v3_project(
        &self,
        input: &ApplyV3Project,
    ) -> Result<opencut_contracts::project_v3_ops::VisualPlan, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        require_edit(&tx, &self.connection.1, &input.project_id)?;
        restricted_base(&tx, &self.connection.1, input)?;
        let snapshot = load_v3_snapshot(&tx, &input.project_id, None)?;
        if snapshot.revision != input.expected_revision {
            return Err(revision_conflict(
                &input.expected_revision,
                &snapshot.revision,
            ));
        }
        let plan = opencut_editor_application::project_v3::plan_apply(&snapshot.project, input)?;
        tx.commit().map_err(storage_error)?;
        Ok(plan)
    }

    pub fn apply_v3_project(
        &mut self,
        input: &ApplyV3Project,
    ) -> Result<opencut_contracts::TransactionResult, EditorError> {
        require_idempotency_key(&input.idempotency_key)?;
        let identity = request_hash("project_v3_apply", &input.project_id, input)?;
        let context = self.connection.1.clone();
        require_edit(&self.connection, &context, &input.project_id)?;
        sqlite_edit_control::require_writable(&self.connection, &context, &input.project_id)?;
        restricted_base(&self.connection, &context, input)?;
        if let Some(value) = load_idempotent(
            &self.connection,
            "project_v3_apply",
            &input.project_id,
            &input.idempotency_key,
            &identity,
            "transaction",
        )? {
            return decode_json(&value, "schema 3 edit receipt");
        }
        let mut prepared = input.clone();
        let mut pinning = sqlite_projects::NativeAssetPinning::new(&self.asset_store_root);
        for operation in &mut prepared.operations {
            if let VisualOperation::Base {
                operation: Operation::BindAsset { asset } | Operation::ReplaceAsset { asset },
            } = operation
            {
                *asset = pinning.pin(asset)?;
            }
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        require_edit(&tx, &context, &input.project_id)?;
        sqlite_edit_control::require_writable(&tx, &context, &input.project_id)?;
        restricted_base(&tx, &context, input)?;
        if let Some(value) = load_idempotent(
            &tx,
            "project_v3_apply",
            &input.project_id,
            &input.idempotency_key,
            &identity,
            "transaction",
        )? {
            return decode_json(&value, "schema 3 edit receipt");
        }
        let current = load_v3_snapshot(&tx, &input.project_id, None)?;
        if current.revision != input.expected_revision {
            return Err(revision_conflict(
                &input.expected_revision,
                &current.revision,
            ));
        }
        let plan = opencut_editor_application::project_v3::plan_apply(&current.project, &prepared)?;
        let result = commit_edit(
            &tx,
            &context,
            &current,
            plan,
            EditCommit {
                project_id: &input.project_id,
                idempotency_key: &input.idempotency_key,
                identity: &identity,
                family: "project_v3_apply",
                event: "project.v3_applied",
                request: json!({"kind":"project_v3.apply","request":prepared}),
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
}

pub(super) struct EditCommit<'a> {
    pub project_id: &'a str,
    pub idempotency_key: &'a str,
    pub identity: &'a RequestIdentity,
    pub family: &'a str,
    pub event: &'a str,
    pub request: Value,
}
pub(super) fn commit_edit(
    tx: &rusqlite::Transaction<'_>,
    context: &opencut_editor_application::audit::ExecutionContext,
    current: &ProjectSnapshotV3,
    plan: opencut_contracts::project_v3_ops::VisualPlan,
    edit: EditCommit<'_>,
) -> Result<opencut_contracts::TransactionResult, EditorError> {
    let transaction_id = format!("txn_{}", Uuid::new_v4().simple());
    let revision = revision_id(
        &plan.proposed_project,
        Some(&current.revision),
        Some(&transaction_id),
    )?;
    let result = opencut_contracts::TransactionResult {
        transaction_id: transaction_id.clone(),
        base_revision: current.revision.clone(),
        revision: revision.clone(),
        created_ids: plan.created_ids,
        changed_ids: plan.changed_ids,
        deleted_ids: plan.deleted_ids,
    };
    tx.execute(
        "INSERT INTO revisions(revision,project_id,parent_revision,transaction_id,document_json)
        VALUES (?1,?2,?3,?4,?5)",
        params![
            revision,
            edit.project_id,
            current.revision,
            transaction_id,
            encode_json(&plan.proposed_project, "schema 3 edit document")?
        ],
    )
    .map_err(storage_error)?;
    let updated = tx
        .execute(
            "UPDATE projects SET head_revision=?1,name=?4 WHERE project_id=?2 AND head_revision=?3",
            params![
                revision,
                edit.project_id,
                current.revision,
                plan.proposed_project.base.name
            ],
        )
        .map_err(storage_error)?;
    if updated != 1 {
        return Err(EditorError::new(
            "REVISION_CONFLICT",
            "Project head moved during schema 3 edit",
        ));
    }
    let receipt = encode_json(&result, "schema 3 edit receipt")?;
    tx.execute("INSERT INTO editor_transactions(transaction_id,project_id,base_revision,revision,request_json,result_json)
        VALUES (?1,?2,?3,?4,?5,?6)",params![transaction_id,edit.project_id,current.revision,revision,
            encode_json(&edit.request,"project edit journal")?,receipt]).map_err(storage_error)?;
    tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json)
        VALUES (?5,?1,?2,?3,'transaction',?4)",
        params![edit.project_id,edit.idempotency_key,edit.identity.current_digest(),receipt,edit.family]).map_err(idempotency_insert_error)?;
    tx.execute(
        "INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json)
        VALUES (?1,?2,?3,?5,?4)",
        params![
            format!("evt_{}", Uuid::new_v4().simple()),
            edit.project_id,
            revision,
            receipt,
            edit.event
        ],
    )
    .map_err(storage_error)?;
    sqlite_audit::insert(
        tx,
        context,
        edit.event,
        sqlite_audit::Links {
            project_id: Some(edit.project_id),
            revision: Some(&revision),
            transaction_id: Some(&transaction_id),
            ..Default::default()
        },
    )?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{CreateProject, FrameRate};

    fn create() -> CreateV3Project {
        CreateV3Project {
            project: CreateProject {
                id: "v3".into(),
                name: "Visual".into(),
                sequence_id: "main".into(),
                width: 640,
                height: 360,
                fps: FrameRate {
                    numerator: 25,
                    denominator: 1,
                },
            },
            idempotency_key: "create-v3".into(),
        }
    }
    fn shape_request(revision: &str, key: &str) -> ApplyV3Project {
        serde_json::from_value(json!({"project_id":"v3","expected_revision":revision,
            "idempotency_key":key,"label":"Add rectangle","operations":[{"type":"shape.put",
            "sequence_id":"main","shape":{"id":"card","track_id":"graphics",
            "timeline_range":{"start":{"ticks":"0","timebase":1000},
            "duration":{"ticks":"1000","timebase":1000}},
            "primitive":{"kind":"rectangle","width_px":200.0,"height_px":100.0},
            "fill":{"red":255,"green":0,"blue":0,"alpha":255},
            "position_x":0.0,"position_y":0.0,"rotation_degrees":0.0,"opacity":1.0}}]}))
        .unwrap()
    }

    fn keyframe_curve_request(revision: &str) -> ApplyV3Project {
        serde_json::from_value(json!({
            "project_id":"v3",
            "expected_revision":revision,
            "idempotency_key":"set-curve",
            "label":"Add eased opacity",
            "operations":[
                {"type":"base.apply","operation":{"type":"track.create","sequence_id":"main",
                    "track":{"id":"video","name":"Video","kind":"video","enabled":true,
                    "muted":false,"locked":false,"clips":[],"params":{}}}},
                {"type":"base.apply","operation":{"type":"clip.insert","sequence_id":"main",
                    "track_id":"video","clip":{"id":"animated","asset_id":null,
                    "timeline_range":{"start":{"ticks":"0","timebase":1000},
                    "duration":{"ticks":"1000","timebase":1000}},"source_range":null,
                    "params":{},"channels":[{"parameter":"opacity","keyframes":[
                    {"id":"fade-in","time":{"ticks":"0","timebase":1000},"value":0.0,"interpolation":"linear"},
                    {"id":"fade-out","time":{"ticks":"1000","timebase":1000},"value":1.0,"interpolation":"linear"}]}]}}},
                {"type":"keyframe.curve_set","sequence_id":"main","curve":{
                    "id":"opacity-ease","schema_version":1,"sequence_id":"main","clip_id":"animated",
                    "parameter":"opacity","keyframe_id":"fade-in",
                    "curve":{"x1":0.0,"y1":0.0,"x2":1.0,"y2":0.0}}}
            ]
        }))
        .unwrap()
    }
    #[test]
    fn visual_history_reads_metadata_without_legacy_document_projection() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = EditorApplication::open(directory.path().join("workspace.sqlite")).unwrap();
        let initial = app.create_v3_project(&create()).unwrap();
        let applied = app
            .apply_v3_project(&keyframe_curve_request(&initial.revision))
            .unwrap();
        let page = app.history("v3", None, 1).unwrap();
        assert_eq!(page.head_revision, applied.revision);
        assert_eq!(
            page.revisions[0].transaction_id.as_deref(),
            Some(applied.transaction_id.as_str())
        );
        assert_eq!(page.next_cursor.as_deref(), Some(initial.revision.as_str()));
        let older = app.history("v3", page.next_cursor.as_deref(), 1).unwrap();
        assert_eq!(older.revisions[0].revision, initial.revision);
        assert!(older.next_cursor.is_none());
        assert_eq!(
            app.snapshot("v3").unwrap_err().code,
            "UNSUPPORTED_PROJECT_VERSION"
        );
        assert_eq!(
            app.history("absent", Some(&initial.revision), 1)
                .unwrap_err()
                .code,
            "NOT_FOUND"
        );
    }

    #[test]
    fn v3_create_apply_fixed_revision_reopen_and_legacy_rejection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.sqlite");
        let mut app = EditorApplication::open(&path).unwrap();
        let initial = app.create_v3_project(&create()).unwrap();
        assert_eq!(app.create_v3_project(&create()).unwrap(), initial);
        assert_eq!(
            app.snapshot("v3").unwrap_err().code,
            "UNSUPPORTED_PROJECT_VERSION"
        );
        let mut request = shape_request(&initial.revision, "track-and-shape");
        request.operations.insert(
            0,
            VisualOperation::Base {
                operation: serde_json::from_value(json!({
            "type":"track.create","sequence_id":"main","track":{"id":"graphics",
            "name":"Graphics","kind":"graphic","enabled":true,"muted":false,
            "locked":false,"clips":[],"params":{}}}))
                .unwrap(),
            },
        );
        let plan = app.plan_v3_project(&request).unwrap();
        assert!(
            plan.created_ids.contains(&"graphics".into())
                && plan.created_ids.contains(&"card".into())
        );
        let applied = app.apply_v3_project(&request).unwrap();
        assert_eq!(app.apply_v3_project(&request).unwrap(), applied);
        assert_eq!(
            app.inspect_v3_project(&InspectV3Project {
                project_id: "v3".into(),
                revision: Some(initial.revision.clone())
            })
            .unwrap()
            .project
            .visual
            .len(),
            0
        );
        assert_eq!(
            app.inspect_v3_project(&InspectV3Project {
                project_id: "v3".into(),
                revision: Some(applied.revision.clone())
            })
            .unwrap()
            .project
            .visual[0]
                .shapes[0]
                .id,
            "card"
        );
        let other = app
            .create_project(
                CreateProject {
                    id: "legacy".into(),
                    name: "Legacy".into(),
                    sequence_id: "main".into(),
                    width: 640,
                    height: 360,
                    fps: FrameRate {
                        numerator: 25,
                        denominator: 1,
                    },
                },
                "create-legacy",
            )
            .unwrap();
        assert_eq!(app.list_projects().unwrap().len(), 1);
        assert_eq!(app.list_projects().unwrap()[0].revision, other.revision);
        assert_eq!(
            app.list_v3_projects(&ListV3Projects {
                cursor: None,
                limit: 10
            })
            .unwrap()
            .items
            .len(),
            1
        );
        drop(app);
        let app = EditorApplication::open(&path).unwrap();
        assert_eq!(
            app.inspect_v3_project(&InspectV3Project {
                project_id: "v3".into(),
                revision: Some(applied.revision)
            })
            .unwrap()
            .project
            .schema_version,
            3
        );
    }

    #[test]
    fn promotion_visual_edit_and_audit_failure_are_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.sqlite");
        let mut app = EditorApplication::open(&path).unwrap();
        let input = create().project;
        let base = app.create_project(input, "legacy-create").unwrap();
        let track: opencut_contracts::TransactionRequest = serde_json::from_value(json!({
            "expected_revision":base.revision,"idempotency_key":"track","label":"Track",
            "operations":[{"type":"track.create","sequence_id":"main","track":{
                "id":"graphics","name":"Graphics","kind":"graphic","enabled":true,
                "muted":false,"locked":false,"clips":[],"params":{}}}]}))
        .unwrap();
        let tracked = app.apply("v3", &track).unwrap();
        let promotion = PromoteV3Project {
            project_id: "v3".into(),
            expected_revision: tracked.revision.clone(),
            idempotency_key: "promote".into(),
        };
        let promoted = app.promote_v3_project(&promotion).unwrap();
        assert_eq!(app.promote_v3_project(&promotion).unwrap(), promoted);
        assert_eq!(
            app.snapshot("v3").unwrap_err().code,
            "UNSUPPORTED_PROJECT_VERSION"
        );
        let request = shape_request(&promoted.revision, "shape");
        let plan = app.plan_v3_project(&request).unwrap();
        assert_eq!(plan.created_ids, vec!["card"]);
        assert_eq!(
            app.inspect_v3_project(&InspectV3Project {
                project_id: "v3".into(),
                revision: None
            })
            .unwrap(),
            promoted
        );
        app.connection
            .execute_batch(
                "CREATE TRIGGER fail_v3_audit BEFORE INSERT ON audit_records
            WHEN NEW.action='project.v3_applied' BEGIN SELECT RAISE(ABORT,'injected'); END;",
            )
            .unwrap();
        assert_eq!(
            app.apply_v3_project(&request).unwrap_err().code,
            "STORAGE_ERROR"
        );
        assert_eq!(
            app.inspect_v3_project(&InspectV3Project {
                project_id: "v3".into(),
                revision: None
            })
            .unwrap(),
            promoted
        );
        app.connection
            .execute_batch("DROP TRIGGER fail_v3_audit")
            .unwrap();
        let applied = app.apply_v3_project(&request).unwrap();
        assert_eq!(app.apply_v3_project(&request).unwrap(), applied);
        assert_eq!(
            app.inspect_v3_project(&InspectV3Project {
                project_id: "v3".into(),
                revision: None
            })
            .unwrap()
            .project
            .visual[0]
                .shapes[0]
                .id,
            "card"
        );
        drop(app);
        let app = EditorApplication::open(&path).unwrap();
        assert_eq!(
            app.inspect_v3_project(&InspectV3Project {
                project_id: "v3".into(),
                revision: Some(promoted.revision)
            })
            .unwrap()
            .project
            .visual
            .len(),
            0
        );
        assert_eq!(
            app.inspect_v3_project(&InspectV3Project {
                project_id: "v3".into(),
                revision: Some(applied.revision)
            })
            .unwrap()
            .project
            .visual[0]
                .shapes
                .len(),
            1
        );
    }

    #[test]
    fn keyframe_curve_edit_is_versioned_persisted_idempotent_and_reopenable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.sqlite");
        let mut app = EditorApplication::open(&path).unwrap();
        let initial = app.create_v3_project(&create()).unwrap();
        let request = keyframe_curve_request(&initial.revision);
        let planned = app.plan_v3_project(&request).unwrap();
        assert_eq!(planned.proposed_project.keyframe_curves.len(), 1);
        let applied = app.apply_v3_project(&request).unwrap();
        assert_eq!(app.apply_v3_project(&request).unwrap(), applied);
        drop(app);

        let app = EditorApplication::open(&path).unwrap();
        let inspected = app
            .inspect_v3_project(&InspectV3Project {
                project_id: "v3".into(),
                revision: Some(applied.revision),
            })
            .unwrap();
        assert_eq!(inspected.project.keyframe_curves.len(), 1);
        assert_eq!(inspected.project.keyframe_curves[0].schema_version, 1);
        assert_eq!(inspected.project.keyframe_curves[0].curve.y2, 0.0);
    }

    #[test]
    fn v3_project_list_is_bounded_and_cursor_advances() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = EditorApplication::open(dir.path().join("workspace.sqlite")).unwrap();
        for id in ["a", "b", "c"] {
            let mut request = create();
            request.project.id = id.into();
            request.idempotency_key = format!("create-{id}");
            app.create_v3_project(&request).unwrap();
        }
        let first = app
            .list_v3_projects(&ListV3Projects {
                cursor: None,
                limit: 2,
            })
            .unwrap();
        assert_eq!(
            first
                .items
                .iter()
                .map(|p| p.project_id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert!(first.items.iter().all(|p| p.schema_version == 3));
        let second = app
            .list_v3_projects(&ListV3Projects {
                cursor: first.next_cursor,
                limit: 2,
            })
            .unwrap();
        assert_eq!(
            second
                .items
                .iter()
                .map(|p| p.project_id.as_str())
                .collect::<Vec<_>>(),
            ["c"]
        );
        assert!(second.next_cursor.is_none());
        assert_eq!(
            app.list_v3_projects(&ListV3Projects {
                cursor: None,
                limit: 0
            })
            .unwrap_err()
            .code,
            "INVALID_ARGUMENT"
        );
    }
}
