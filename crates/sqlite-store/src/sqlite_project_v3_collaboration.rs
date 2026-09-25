//! Schema 3 selective history over the existing revisions, journal, audit and outbox.
use super::*;
use opencut_api_contracts::{
    collaboration::{CollaborationCommit, Direction, RedoRequest, UndoRequest},
    project_v3_collaboration::V3CollaborationPlan,
};
use opencut_contracts::project_v3::ProjectDocumentV3;

fn authorize(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
) -> Result<(), EditorError> {
    sqlite_project_v3::require_edit(connection, context, project)
}

fn planned(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
    expected_revision: &str,
    direction: Direction,
    target_id: &str,
    undo_id: Option<&str>,
) -> Result<V3CollaborationPlan, EditorError> {
    authorize(connection, context, project)?;
    let head = sqlite_project_v3::load_v3_snapshot(connection, project, None)?;
    if head.revision != expected_revision {
        return Err(revision_conflict(expected_revision, &head.revision));
    }
    let target = sqlite_collaboration::entry(connection, project, target_id)?;
    let kind = target
        .request
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if kind.starts_with("selective_")
        || kind.starts_with("project_v3.selective_")
        || kind == "project_v3.promote"
        || kind == "project_v3.create"
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Transaction is not a schema 3 edit target",
        ));
    }
    if !sqlite_collaboration::ancestor(connection, project, &head.revision, &target.revision)? {
        return Err(EditorError::new(
            "CAUSAL_CONFLICT",
            "Target edit is not in the current revision ancestry",
        ));
    }
    let active = sqlite_collaboration::active_undo(connection, project, target_id, &head.revision)?;
    match direction {
        Direction::Undo if active.is_some() => {
            return Err(EditorError::new(
                "UNDO_UNAVAILABLE",
                "Target is already undone",
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
    // A promotion crosses schema versions and cannot be inverted as a schema 3 edit.
    let before =
        sqlite_project_v3::load_v3_snapshot(connection, project, Some(&target.base_revision))?
            .project;
    let after =
        sqlite_project_v3::load_v3_snapshot(connection, project, Some(&target.revision))?.project;
    let (from, to) = match direction {
        Direction::Undo => (&after, &before),
        Direction::Redo => (&before, &after),
    };
    let plan = opencut_editor_application::project_v3_collaboration::plan_history(
        direction,
        expected_revision,
        target_id,
        undo_id,
        from,
        to,
        &head.project,
    );
    if let Some(proposed) = &plan.proposed_project {
        sqlite_collaboration::restricted_asset_bindings(
            connection,
            context,
            project,
            &head.project.base,
            &proposed.base,
        )?;
    }
    Ok(plan)
}

fn plan_undo_on(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    request: &UndoRequest,
) -> Result<V3CollaborationPlan, EditorError> {
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
) -> Result<V3CollaborationPlan, EditorError> {
    authorize(connection, context, &request.project_id)?;
    let head = sqlite_project_v3::load_v3_snapshot(connection, &request.project_id, None)?;
    if head.revision != request.expected_revision {
        return Err(revision_conflict(
            &request.expected_revision,
            &head.revision,
        ));
    }
    let target = sqlite_collaboration::redo_target(
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

fn commit(
    app: &mut EditorApplication,
    direction: Direction,
    project: &str,
    expected_revision: &str,
    key: &str,
    request: Value,
    plan: impl FnOnce(
        &Connection,
        &opencut_editor_application::audit::ExecutionContext,
    ) -> Result<V3CollaborationPlan, EditorError>,
) -> Result<CollaborationCommit, EditorError> {
    require_idempotency_key(key)?;
    let family = match direction {
        Direction::Undo => "project_v3_selective_undo",
        Direction::Redo => "project_v3_selective_redo",
    };
    let identity = request_hash(family, project, &request)?;
    let context = app.connection.1.clone();
    let tx = app
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    authorize(&tx, &context, project)?;
    sqlite_edit_control::require_writable(&tx, &context, project)?;
    if let Some(raw) = load_idempotent(&tx, family, project, key, &identity, "collaboration")? {
        return decode_json(&raw, "schema 3 collaboration receipt");
    }
    let planned = plan(&tx, &context)?;
    if !planned.conflicts.is_empty() {
        return Err(EditorError {
            code: "SELECTIVE_CONFLICT".into(),
            message: "Target edit overlaps later changes or violates project constraints".into(),
            details: json!({"conflicts":planned.conflicts}),
        });
    }
    let proposed: ProjectDocumentV3 = planned
        .proposed_project
        .ok_or_else(|| EditorError::new("SELECTIVE_CONFLICT", "No valid selective edit result"))?;
    let transaction_id = format!("txn_{}", Uuid::new_v4().simple());
    let revision =
        sqlite_project_v3::revision_id(&proposed, Some(expected_revision), Some(&transaction_id))?;
    let (created_ids, changed_ids, deleted_ids) =
        sqlite_collaboration::result_ids(project, &planned.changes);
    let transaction = TransactionResult {
        transaction_id: transaction_id.clone(),
        base_revision: expected_revision.into(),
        revision: revision.clone(),
        created_ids,
        changed_ids,
        deleted_ids,
    };
    let receipt = CollaborationCommit {
        direction,
        target_transaction_id: planned.target_transaction_id.clone(),
        undo_transaction_id: planned.undo_transaction_id.clone(),
        transaction: transaction.clone(),
    };
    tx.execute(
        "INSERT INTO revisions(revision,project_id,parent_revision,transaction_id,document_json)
        VALUES (?1,?2,?3,?4,?5)",
        params![
            revision,
            project,
            expected_revision,
            transaction_id,
            encode_json(&proposed, "schema 3 selective document")?
        ],
    )
    .map_err(storage_error)?;
    let updated = tx
        .execute(
            "UPDATE projects SET head_revision=?1,name=?2 WHERE project_id=?3 AND head_revision=?4",
            params![revision, proposed.base.name, project, expected_revision],
        )
        .map_err(storage_error)?;
    if updated != 1 {
        return Err(EditorError::new(
            "REVISION_CONFLICT",
            "Project head moved during schema 3 selective edit",
        ));
    }
    tx.execute("INSERT INTO editor_transactions(transaction_id,project_id,base_revision,revision,request_json,result_json)
        VALUES (?1,?2,?3,?4,?5,?6)",params![transaction_id,project,expected_revision,revision,
        encode_json(&json!({"kind":match direction {Direction::Undo=>"selective_undo",Direction::Redo=>"selective_redo"},
            "schema_version":3,"target_transaction_id":planned.target_transaction_id,
            "undo_transaction_id":planned.undo_transaction_id,"idempotency_key":key,
            "changes":planned.changes}),"schema 3 selective journal")?,
        encode_json(&transaction,"schema 3 selective transaction")?]).map_err(storage_error)?;
    tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json)
        VALUES (?1,?2,?3,?4,'collaboration',?5)",params![family,project,key,identity.current_digest(),
        encode_json(&receipt,"schema 3 collaboration receipt")?]).map_err(idempotency_insert_error)?;
    tx.execute(
        "INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json)
        VALUES (?1,?2,?3,'transaction.committed',?4)",
        params![
            format!("evt_{}", Uuid::new_v4().simple()),
            project,
            revision,
            encode_json(&transaction, "schema 3 selective event")?
        ],
    )
    .map_err(storage_error)?;
    sqlite_audit::insert(
        &tx,
        &context,
        match direction {
            Direction::Undo => "project.v3_selective_undo",
            Direction::Redo => "project.v3_selective_redo",
        },
        sqlite_audit::Links {
            project_id: Some(project),
            revision: Some(&revision),
            transaction_id: Some(&transaction_id),
            ..Default::default()
        },
    )?;
    tx.commit().map_err(storage_error)?;
    Ok(receipt)
}

impl EditorApplication {
    pub fn plan_v3_undo(&self, request: &UndoRequest) -> Result<V3CollaborationPlan, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let plan = plan_undo_on(&tx, &self.connection.1, request)?;
        tx.commit().map_err(storage_error)?;
        Ok(plan)
    }
    pub fn apply_v3_undo(
        &mut self,
        request: &UndoRequest,
    ) -> Result<CollaborationCommit, EditorError> {
        commit(
            self,
            Direction::Undo,
            &request.project_id,
            &request.expected_revision,
            &request.idempotency_key,
            serde_json::to_value(request)
                .map_err(|e| EditorError::new("SERIALIZATION_ERROR", e.to_string()))?,
            |tx, context| plan_undo_on(tx, context, request),
        )
    }
    pub fn plan_v3_redo(&self, request: &RedoRequest) -> Result<V3CollaborationPlan, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let plan = plan_redo_on(&tx, &self.connection.1, request)?;
        tx.commit().map_err(storage_error)?;
        Ok(plan)
    }
    pub fn apply_v3_redo(
        &mut self,
        request: &RedoRequest,
    ) -> Result<CollaborationCommit, EditorError> {
        commit(
            self,
            Direction::Redo,
            &request.project_id,
            &request.expected_revision,
            &request.idempotency_key,
            serde_json::to_value(request)
                .map_err(|e| EditorError::new("SERIALIZATION_ERROR", e.to_string()))?,
            |tx, context| plan_redo_on(tx, context, request),
        )
    }
}
