//! Selective history planning and atomic causal undo/redo persistence.
use super::*;
use opencut_api_contracts::collaboration::{
    CollaborationCommit, CollaborationPlan, Direction, RedoRequest, UndoRequest,
};
use opencut_contracts::{Operation, ProjectDocument, ProjectSnapshot, TransactionRequest};
use opencut_editor_application::access;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Clone)]
pub(super) struct TransactionEntry {
    pub base_revision: String,
    pub revision: String,
    pub request: Value,
}

fn authorize(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
) -> Result<(), EditorError> {
    sqlite_access::require(connection, context, project, ProjectAction::Read)?;
    sqlite_access::require(connection, context, project, ProjectAction::Edit)
}

pub(super) fn entry(
    connection: &Connection,
    project: &str,
    transaction_id: &str,
) -> Result<TransactionEntry, EditorError> {
    let row: Option<(String, String, String)> = connection.query_row(
        "SELECT base_revision,revision,request_json FROM editor_transactions WHERE project_id=?1 AND transaction_id=?2",
        params![project, transaction_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional().map_err(storage_error)?;
    let (base_revision, revision, request_json) = row.ok_or_else(|| {
        EditorError::new("NOT_FOUND", "Transaction does not belong to this project")
    })?;
    Ok(TransactionEntry {
        base_revision,
        revision,
        request: decode_json(&request_json, "transaction request")?,
    })
}

fn revision(
    connection: &Connection,
    project: &str,
    id: &str,
) -> Result<ProjectSnapshot, EditorError> {
    let document: Option<String> = connection
        .query_row(
            "SELECT document_json FROM revisions WHERE project_id=?1 AND revision=?2",
            params![project, id],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    Ok(ProjectSnapshot {
        revision: id.into(),
        project: sqlite_project_v3::decode_legacy_project(&document.ok_or_else(|| {
            EditorError::new("NOT_FOUND", "Revision does not belong to this project")
        })?)?,
    })
}

pub(super) fn ancestor(
    connection: &Connection,
    project: &str,
    descendant: &str,
    desired: &str,
) -> Result<bool, EditorError> {
    let mut pending = std::collections::VecDeque::from([descendant.to_owned()]);
    let mut visited = std::collections::HashSet::new();
    while let Some(id) = pending.pop_front() {
        if id == desired {
            return Ok(true);
        }
        if !visited.insert(id.clone()) {
            continue;
        }
        if visited.len() > 100_000 {
            return Err(EditorError::new(
                "STORAGE_ERROR",
                "Revision ancestry exceeds traversal limit",
            ));
        }
        pending.extend(sqlite_branching::parents(connection, project, &id)?);
    }
    Ok(false)
}

pub(super) fn active_undo(
    connection: &Connection,
    project: &str,
    original: &str,
    head: &str,
) -> Result<Option<String>, EditorError> {
    let mut statement = connection.prepare(
        "SELECT transaction_id,revision,request_json FROM editor_transactions WHERE project_id=?1 ORDER BY rowid"
    ).map_err(storage_error)?;
    let rows = statement
        .query_map([project], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(storage_error)?;
    let mut active: Option<String> = None;
    for row in rows {
        let (id, revision, raw) = row.map_err(storage_error)?;
        let value: Value = decode_json(&raw, "transaction request")?;
        if value.get("target_transaction_id").and_then(Value::as_str) != Some(original) {
            continue;
        }
        if !ancestor(connection, project, head, &revision)? {
            continue;
        }
        match value.get("kind").and_then(Value::as_str) {
            Some("selective_undo") => active = Some(id),
            Some("selective_redo")
                if value.get("undo_transaction_id").and_then(Value::as_str)
                    == active.as_deref() =>
            {
                active = None
            }
            _ => {}
        }
    }
    Ok(active)
}

fn planned(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
    expected_revision: &str,
    direction: Direction,
    target_id: &str,
    undo_id: Option<&str>,
) -> Result<CollaborationPlan, EditorError> {
    let head = load_snapshot(connection, project)?;
    if head.revision != expected_revision {
        return Err(revision_conflict(expected_revision, &head.revision));
    }
    let target = entry(connection, project, target_id)?;
    if target
        .request
        .get("kind")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind.starts_with("selective_"))
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "A selective history transaction is not an original edit target",
        ));
    }
    if !ancestor(connection, project, &head.revision, &target.revision)? {
        return Err(EditorError::new(
            "CAUSAL_CONFLICT",
            "Target edit is not in the current revision ancestry",
        ));
    }
    let active = active_undo(connection, project, target_id, &head.revision)?;
    match direction {
        Direction::Undo if active.is_some() => {
            return Err(EditorError::new(
                "UNDO_UNAVAILABLE",
                "Target edit is already selectively undone",
            ));
        }
        Direction::Redo if active.as_deref() != undo_id => {
            return Err(EditorError::new(
                "REDO_UNAVAILABLE",
                "Undo is not the active causal predecessor",
            ));
        }
        _ => {}
    }
    let before = revision(connection, project, &target.base_revision)?.project;
    let after = revision(connection, project, &target.revision)?.project;
    let (from, to) = match direction {
        Direction::Undo => (&after, &before),
        Direction::Redo => (&before, &after),
    };
    let result = opencut_editor_application::collaboration::plan(
        direction,
        expected_revision,
        target_id,
        undo_id,
        from,
        to,
        &head.project,
    );
    if let Some(proposed) = &result.proposed_project {
        restricted_asset_bindings(connection, context, project, &head.project, proposed)?;
    }
    Ok(result)
}

pub(super) fn restricted_asset_bindings(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project_id: &str,
    before: &ProjectDocument,
    after: &ProjectDocument,
) -> Result<(), EditorError> {
    if context.caller().is_owner() {
        return Ok(());
    }
    let operations = after
        .assets
        .iter()
        .filter_map(
            |asset| match before.assets.iter().find(|old| old.id == asset.id) {
                Some(old) if old == asset => None,
                Some(_) => Some(Operation::ReplaceAsset {
                    asset: asset.clone(),
                }),
                None => Some(Operation::BindAsset {
                    asset: asset.clone(),
                }),
            },
        )
        .collect();
    let request = TransactionRequest {
        expected_revision: String::new(),
        idempotency_key: String::new(),
        label: String::new(),
        operations,
    };
    access::restricted_edit(&request, &|id| {
        Ok(sqlite_uploads::media(connection, project_id, id)?.map(|resource| resource.asset))
    })
}

pub(super) fn redo_target(
    connection: &Connection,
    project: &str,
    undo_id: &str,
    head_revision: &str,
) -> Result<String, EditorError> {
    let undo = entry(connection, project, undo_id)?;
    if undo.request.get("kind").and_then(Value::as_str) != Some("selective_undo") {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Redo requires a selective undo transaction",
        ));
    }
    if !ancestor(connection, project, head_revision, &undo.revision)? {
        return Err(EditorError::new(
            "CAUSAL_CONFLICT",
            "Undo is not in the current revision ancestry",
        ));
    }
    undo.request
        .get("target_transaction_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| EditorError::new("STORAGE_ERROR", "Undo journal has no target"))
}

fn plan_undo_on(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    request: &UndoRequest,
) -> Result<CollaborationPlan, EditorError> {
    authorize(connection, context, &request.project_id)?;
    planned(
        connection,
        context,
        &request.project_id,
        &request.expected_revision,
        Direction::Undo,
        &request.target_transaction_id,
        None,
    )
}

fn plan_redo_on(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    request: &RedoRequest,
) -> Result<CollaborationPlan, EditorError> {
    authorize(connection, context, &request.project_id)?;
    let head = load_snapshot(connection, &request.project_id)?;
    if head.revision != request.expected_revision {
        return Err(revision_conflict(
            &request.expected_revision,
            &head.revision,
        ));
    }
    let target = redo_target(
        connection,
        &request.project_id,
        &request.undo_transaction_id,
        &head.revision,
    )?;
    planned(
        connection,
        context,
        &request.project_id,
        &request.expected_revision,
        Direction::Redo,
        &target,
        Some(&request.undo_transaction_id),
    )
}

pub(super) fn result_ids(
    project_id: &str,
    changes: &[opencut_api_contracts::collaboration::PropertyChange],
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut created = BTreeSet::new();
    let mut changed = BTreeSet::new();
    let mut deleted = BTreeSet::new();
    for change in changes {
        let parts: Vec<_> = change
            .path
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        let mut entity = project_id;
        let mut direct_entity = false;
        for window in parts.windows(2) {
            if matches!(
                window[0],
                "assets"
                    | "sequences"
                    | "tracks"
                    | "clips"
                    | "caption_cues"
                    | "channels"
                    | "keyframes"
                    | "shapes"
                    | "effects"
                    | "masks"
                    | "transitions"
            ) {
                entity = window[1];
                direct_entity = parts.last() == Some(&entity);
            }
        }
        if direct_entity && change.before.is_none() {
            created.insert(entity.to_owned());
        } else if direct_entity && change.after.is_none() {
            deleted.insert(entity.to_owned());
        } else {
            changed.insert(entity.to_owned());
        }
    }
    (
        created.into_iter().collect(),
        changed.into_iter().collect(),
        deleted.into_iter().collect(),
    )
}

fn commit(
    self_app: &mut EditorApplication,
    direction: Direction,
    project_id: &str,
    expected_revision: &str,
    key: &str,
    request_value: Value,
    plan: impl FnOnce(
        &Connection,
        &opencut_editor_application::audit::ExecutionContext,
    ) -> Result<CollaborationPlan, EditorError>,
) -> Result<CollaborationCommit, EditorError> {
    require_idempotency_key(key)?;
    let family = match direction {
        Direction::Undo => "selective_undo",
        Direction::Redo => "selective_redo",
    };
    let identity = request_hash(family, project_id, &request_value)?;
    let context = self_app.connection.1.clone();
    let tx = self_app
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    authorize(&tx, &context, project_id)?;
    sqlite_edit_control::require_writable(&tx, &context, project_id)?;
    if let Some(value) = load_idempotent(&tx, family, project_id, key, &identity, "collaboration")?
    {
        return decode_json(&value, "collaboration receipt");
    }
    let planned = plan(&tx, &context)?;
    if !planned.conflicts.is_empty() {
        return Err(EditorError {
            code: "SELECTIVE_CONFLICT".into(),
            message: "Target edit overlaps later changes or violates project constraints".into(),
            details: json!({"conflicts":planned.conflicts}),
        });
    }
    let project = planned
        .proposed_project
        .ok_or_else(|| EditorError::new("SELECTIVE_CONFLICT", "No valid selective edit result"))?;
    let transaction_id = format!("txn_{}", Uuid::new_v4().simple());
    let material = encode_json(
        &json!({"parent":expected_revision,"transaction_id":transaction_id,"document":project}),
        "revision material",
    )?;
    let digest = Sha256::digest(material.as_bytes());
    let revision = format!(
        "rev_{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let (created_ids, changed_ids, deleted_ids) = result_ids(project_id, &planned.changes);
    let result = TransactionResult {
        transaction_id: transaction_id.clone(),
        base_revision: expected_revision.into(),
        revision: revision.clone(),
        created_ids,
        changed_ids,
        deleted_ids,
    };
    let public = CollaborationCommit {
        direction,
        target_transaction_id: planned.target_transaction_id.clone(),
        undo_transaction_id: planned.undo_transaction_id.clone(),
        transaction: result.clone(),
    };
    let request_json = encode_json(
        &json!({
            "kind": family, "project_id": project_id, "expected_revision": expected_revision,
            "target_transaction_id": planned.target_transaction_id,
            "undo_transaction_id": planned.undo_transaction_id,
            "idempotency_key": key, "changes": planned.changes,
        }),
        "collaboration journal",
    )?;
    tx.execute("INSERT INTO revisions(revision,project_id,parent_revision,transaction_id,document_json) VALUES (?1,?2,?3,?4,?5)",
        params![revision,project_id,expected_revision,transaction_id,encode_json(&project,"selectively edited project")?]).map_err(storage_error)?;
    let updated = tx
        .execute(
            "UPDATE projects SET head_revision=?1,name=?2 WHERE project_id=?3 AND head_revision=?4",
            params![revision, project.name, project_id, expected_revision],
        )
        .map_err(storage_error)?;
    if updated != 1 {
        return Err(revision_conflict(
            expected_revision,
            &load_snapshot(&tx, project_id)?.revision,
        ));
    }
    tx.execute("INSERT INTO editor_transactions(transaction_id,project_id,base_revision,revision,request_json,result_json) VALUES (?1,?2,?3,?4,?5,?6)",
        params![transaction_id,project_id,expected_revision,revision,request_json,encode_json(&result,"collaboration transaction")?]).map_err(storage_error)?;
    tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json) VALUES (?1,?2,?3,?4,'collaboration',?5)",
        params![family,project_id,key,identity.current_digest(),encode_json(&public,"collaboration receipt")?]).map_err(idempotency_insert_error)?;
    tx.execute("INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json) VALUES (?1,?2,?3,'transaction.committed',?4)",
        params![format!("evt_{}",Uuid::new_v4().simple()),project_id,revision,encode_json(&result,"collaboration event")?]).map_err(storage_error)?;
    sqlite_audit::insert(
        &tx,
        &context,
        match direction {
            Direction::Undo => "project.selective_undo",
            Direction::Redo => "project.selective_redo",
        },
        sqlite_audit::Links {
            project_id: Some(project_id),
            revision: Some(&revision),
            transaction_id: Some(&transaction_id),
            ..Default::default()
        },
    )?;
    tx.commit().map_err(storage_error)?;
    Ok(public)
}

impl EditorApplication {
    pub fn plan_undo(&self, request: &UndoRequest) -> Result<CollaborationPlan, EditorError> {
        plan_undo_on(&self.connection, &self.connection.1, request)
    }
    pub fn apply_undo(
        &mut self,
        request: &UndoRequest,
    ) -> Result<CollaborationCommit, EditorError> {
        let request_value = serde_json::to_value(request)
            .map_err(|error| EditorError::new("SERIALIZATION_ERROR", error.to_string()))?;
        commit(
            self,
            Direction::Undo,
            &request.project_id,
            &request.expected_revision,
            &request.idempotency_key,
            request_value,
            |tx, context| plan_undo_on(tx, context, request),
        )
    }
    pub fn plan_redo(&self, request: &RedoRequest) -> Result<CollaborationPlan, EditorError> {
        plan_redo_on(&self.connection, &self.connection.1, request)
    }
    pub fn apply_redo(
        &mut self,
        request: &RedoRequest,
    ) -> Result<CollaborationCommit, EditorError> {
        let request_value = serde_json::to_value(request)
            .map_err(|error| EditorError::new("SERIALIZATION_ERROR", error.to_string()))?;
        commit(
            self,
            Direction::Redo,
            &request.project_id,
            &request.expected_revision,
            &request.idempotency_key,
            request_value,
            |tx, context| plan_redo_on(tx, context, request),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{CreateProject, FrameRate, TransactionRequest};
    use opencut_editor_application::access::{
        Caller, PrincipalPolicy, ProjectAction, ProjectGrant,
    };

    fn edit(revision: &str, key: &str, operations: Value) -> TransactionRequest {
        serde_json::from_value(json!({
            "expected_revision": revision,
            "idempotency_key": key,
            "label": key,
            "operations": operations,
        }))
        .unwrap()
    }

    fn setup(app: &mut EditorApplication) -> (String, String) {
        let created = app
            .create_project(
                CreateProject {
                    id: "project-1".into(),
                    name: "Project".into(),
                    sequence_id: "main".into(),
                    width: 640,
                    height: 360,
                    fps: FrameRate {
                        numerator: 25,
                        denominator: 1,
                    },
                },
                "create",
            )
            .unwrap();
        let added = app
            .apply(
                "project-1",
                &edit(
                    &created.revision,
                    "add-track",
                    json!([
                        {"type":"track.create","sequence_id":"main","track":{
                            "id":"voice","name":"Voice","kind":"audio","enabled":true,
                            "muted":false,"locked":false,"clips":[],"params":{}
                        }}
                    ]),
                ),
            )
            .unwrap();
        (added.revision, added.transaction_id)
    }

    #[test]
    fn selective_undo_and_redo_preserve_later_caption_and_durable_causality() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("workspace.sqlite");
        let mut app = EditorApplication::open(&database).unwrap();
        let (base, _) = setup(&mut app);
        let agent = app.apply("project-1", &edit(&base,"agent-volume",json!([
            {"type":"parameter.set_track","sequence_id":"main","track_id":"voice","params":{"volume":0.4}}
        ]))).unwrap();
        let human = app.apply("project-1", &edit(&agent.revision,"human-caption",json!([
            {"type":"caption.insert","sequence_id":"main","cue":{
                "id":"human","timeline_range":{"start":{"ticks":"0","timebase":25},"duration":{"ticks":"25","timebase":25}},
                "text":"Keep me","source_anchor":null,"style":{
                    "font_family":"Arial","font_size":24,"color":"#ffffff","background_color":null,
                    "max_width":600,"line_height":28,"text_align":"center"
                }
            }}
        ]))).unwrap();
        let undo = UndoRequest {
            project_id: "project-1".into(),
            expected_revision: human.revision.clone(),
            target_transaction_id: agent.transaction_id.clone(),
            idempotency_key: "undo-agent".into(),
        };
        let plan = app.plan_undo(&undo).unwrap();
        assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
        assert!(
            plan.changes
                .iter()
                .any(|change| change.path == "/sequences/main/tracks/voice/params/volume")
        );
        let applied = app.apply_undo(&undo).unwrap();
        assert_eq!(app.apply_undo(&undo).unwrap(), applied);
        let snapshot = app.snapshot("project-1").unwrap();
        assert_eq!(
            snapshot.project.sequences[0].caption_cues[0].text,
            "Keep me"
        );
        assert!(
            !snapshot.project.sequences[0].tracks[0]
                .params
                .contains_key("volume")
        );
        let redo = RedoRequest {
            project_id: "project-1".into(),
            expected_revision: applied.transaction.revision.clone(),
            undo_transaction_id: applied.transaction.transaction_id.clone(),
            idempotency_key: "redo-agent".into(),
        };
        assert!(app.plan_redo(&redo).unwrap().conflicts.is_empty());
        let redone = app.apply_redo(&redo).unwrap();
        assert_eq!(app.apply_redo(&redo).unwrap(), redone);
        drop(app);
        let app = EditorApplication::open(&database).unwrap();
        let snapshot = app.snapshot("project-1").unwrap();
        assert_eq!(
            snapshot.project.sequences[0].caption_cues[0].text,
            "Keep me"
        );
        assert_eq!(
            snapshot.project.sequences[0].tracks[0].params["volume"],
            json!(0.4)
        );
        let repeated = app.plan_redo(&RedoRequest {
            expected_revision: redone.transaction.revision,
            ..redo
        });
        assert_eq!(repeated.unwrap_err().code, "REDO_UNAVAILABLE");
        let audits: i64 = app.connection.query_row(
            "SELECT COUNT(*) FROM audit_records WHERE action IN ('project.selective_undo','project.selective_redo')",[],|row|row.get(0)
        ).unwrap();
        assert_eq!(audits, 2);
    }

    #[test]
    fn conflicting_property_and_stale_head_do_not_commit() {
        let mut app = EditorApplication::open(":memory:").unwrap();
        let (base, _) = setup(&mut app);
        let agent = app.apply("project-1", &edit(&base,"agent-volume",json!([
            {"type":"parameter.set_track","sequence_id":"main","track_id":"voice","params":{"volume":0.4}}
        ]))).unwrap();
        let human = app.apply("project-1", &edit(&agent.revision,"human-volume",json!([
            {"type":"parameter.set_track","sequence_id":"main","track_id":"voice","params":{"volume":0.8}}
        ]))).unwrap();
        let request = UndoRequest {
            project_id: "project-1".into(),
            expected_revision: human.revision.clone(),
            target_transaction_id: agent.transaction_id,
            idempotency_key: "undo-conflict".into(),
        };
        let plan = app.plan_undo(&request).unwrap();
        assert_eq!(
            plan.conflicts[0].path,
            "/sequences/main/tracks/voice/params/volume"
        );
        assert!(plan.proposed_project.is_none());
        assert_eq!(
            app.apply_undo(&request).unwrap_err().code,
            "SELECTIVE_CONFLICT"
        );
        assert_eq!(app.snapshot("project-1").unwrap().revision, human.revision);
        let stale = UndoRequest {
            expected_revision: agent.revision,
            ..request
        };
        assert_eq!(app.plan_undo(&stale).unwrap_err().code, "REVISION_CONFLICT");
    }

    #[test]
    fn scoped_permissions_and_audit_failure_are_atomic() {
        let mut app = EditorApplication::open(":memory:").unwrap();
        let (base, _) = setup(&mut app);
        let agent = app.apply("project-1", &edit(&base,"agent-volume",json!([
            {"type":"parameter.set_track","sequence_id":"main","track_id":"voice","params":{"volume":0.4}}
        ]))).unwrap();
        let request = UndoRequest {
            project_id: "project-1".into(),
            expected_revision: agent.revision.clone(),
            target_transaction_id: agent.transaction_id,
            idempotency_key: "undo-scoped".into(),
        };
        app.set_principal_policy(PrincipalPolicy {
            principal_id: "reader".into(),
            expected_revision: "0".into(),
            enabled: true,
            grants: vec![ProjectGrant {
                project_id: "project-1".into(),
                actions: vec![ProjectAction::Read],
            }],
        })
        .unwrap();
        let reader_secret = app.issue_credential("reader").unwrap();
        let reader = app.authenticate(&reader_secret.secret).unwrap();
        app.set_caller(reader);
        assert_eq!(app.plan_undo(&request).unwrap_err().code, "NOT_AUTHORIZED");
        assert_eq!(app.apply_undo(&request).unwrap_err().code, "NOT_AUTHORIZED");
        app.set_caller(Caller::LocalOwner);
        app.set_principal_policy(PrincipalPolicy {
            principal_id: "editor".into(),
            expected_revision: "0".into(),
            enabled: true,
            grants: vec![ProjectGrant {
                project_id: "project-1".into(),
                actions: vec![ProjectAction::Read, ProjectAction::Edit],
            }],
        })
        .unwrap();
        let editor_secret = app.issue_credential("editor").unwrap();
        let editor = app.authenticate(&editor_secret.secret).unwrap();
        app.set_caller(editor);
        assert!(app.plan_undo(&request).unwrap().conflicts.is_empty());
        app.set_caller(Caller::LocalOwner);
        app.connection.execute_batch("CREATE TRIGGER reject_selective_audit BEFORE INSERT ON audit_records WHEN NEW.action='project.selective_undo' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        let before = app.snapshot("project-1").unwrap().revision;
        assert_eq!(app.apply_undo(&request).unwrap_err().code, "STORAGE_ERROR");
        assert_eq!(app.snapshot("project-1").unwrap().revision, before);
        let count: i64 = app.connection.query_row("SELECT COUNT(*) FROM editor_transactions WHERE request_json LIKE '%selective_undo%'",[],|row|row.get(0)).unwrap();
        assert_eq!(count, 0);
        app.connection
            .execute_batch("DROP TRIGGER reject_selective_audit")
            .unwrap();
        assert_eq!(app.apply_undo(&request).unwrap().direction, Direction::Undo);
    }

    #[test]
    fn scoped_undo_cannot_restore_an_owner_local_asset_path() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("owner.wav");
        std::fs::write(&source, b"owner-only source").unwrap();
        let mut app = EditorApplication::open(directory.path().join("workspace.sqlite")).unwrap();
        let created = app
            .create_project(
                CreateProject {
                    id: "project-1".into(),
                    name: "Project".into(),
                    sequence_id: "main".into(),
                    width: 640,
                    height: 360,
                    fps: FrameRate {
                        numerator: 25,
                        denominator: 1,
                    },
                },
                "create",
            )
            .unwrap();
        let bound = app.apply("project-1",&edit(&created.revision,"bind-owner",json!([
            {"type":"asset.bind","asset":{"id":"owner-asset","name":"Owner file","kind":"audio",
                "uri":source,"duration":null,"metadata":{}}}
        ]))).unwrap();
        let removed = app
            .apply(
                "project-1",
                &edit(
                    &bound.revision,
                    "remove-owner",
                    json!([
                        {"type":"asset.unbind","asset_id":"owner-asset"}
                    ]),
                ),
            )
            .unwrap();
        app.set_principal_policy(PrincipalPolicy {
            principal_id: "editor".into(),
            expected_revision: "0".into(),
            enabled: true,
            grants: vec![ProjectGrant {
                project_id: "project-1".into(),
                actions: vec![ProjectAction::Read, ProjectAction::Edit],
            }],
        })
        .unwrap();
        let secret = app.issue_credential("editor").unwrap();
        app.set_caller(app.authenticate(&secret.secret).unwrap());
        let undo = UndoRequest {
            project_id: "project-1".into(),
            expected_revision: removed.revision.clone(),
            target_transaction_id: removed.transaction_id,
            idempotency_key: "undo-owner-removal".into(),
        };
        assert_eq!(app.plan_undo(&undo).unwrap_err().code, "NOT_AUTHORIZED");
        assert_eq!(app.apply_undo(&undo).unwrap_err().code, "NOT_AUTHORIZED");
        assert_eq!(
            app.snapshot("project-1").unwrap().revision,
            removed.revision
        );
    }
}
