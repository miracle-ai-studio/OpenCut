//! Schema 3 branch operations over the shared project branch and revision graph.
use super::*;
use opencut_api_contracts::{
    branching::{
        BranchCompareRequest, BranchCreateRequest, BranchListRequest, BranchMergeRequest,
        BranchView,
    },
    historical_visual_collaboration::{
        HistoricalVisualBranchComparePlan, HistoricalVisualBranchEditRequest,
        HistoricalVisualBranchMergeCommit,
    },
};
use opencut_contracts::{Operation, historical_visual_ops::VisualOperation};
use opencut_editor_application::projects::AssetPinning;

fn view(connection: &Connection, project: &str, name: &str) -> Result<BranchView, EditorError> {
    sqlite_branching::branch_name(name, true)?;
    if name == "main" {
        let head =
            sqlite_historical_visual::load_historical_visual_snapshot(connection, project, None)?;
        return Ok(BranchView {
            project_id: project.into(),
            branch_name: name.into(),
            head_revision: head.revision,
            created_from_revision: sqlite_branching::root_revision(connection, project)?,
            is_main: true,
        });
    }
    let row:Option<(String,String)>=connection.query_row(
        "SELECT head_revision,created_from_revision FROM project_branches WHERE project_id=?1 AND branch_name=?2",
        params![project,name],|row|Ok((row.get(0)?,row.get(1)?))).optional().map_err(storage_error)?;
    let (head_revision, created_from_revision) =
        row.ok_or_else(|| EditorError::new("NOT_FOUND", "Branch does not exist"))?;
    sqlite_historical_visual::load_historical_visual_snapshot(
        connection,
        project,
        Some(&head_revision),
    )?;
    Ok(BranchView {
        project_id: project.into(),
        branch_name: name.into(),
        head_revision,
        created_from_revision,
        is_main: false,
    })
}

fn compare_on(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    input: &BranchCompareRequest,
) -> Result<HistoricalVisualBranchComparePlan, EditorError> {
    sqlite_historical_visual::require_edit(connection, context, &input.project_id)?;
    let source = view(connection, &input.project_id, &input.source_branch)?;
    let target = view(connection, &input.project_id, &input.target_branch)?;
    if source.head_revision != input.expected_source_revision {
        return Err(revision_conflict(
            &input.expected_source_revision,
            &source.head_revision,
        ));
    }
    if target.head_revision != input.expected_target_revision {
        return Err(revision_conflict(
            &input.expected_target_revision,
            &target.head_revision,
        ));
    }
    let base = sqlite_branching::common_ancestor(
        connection,
        &input.project_id,
        &source.head_revision,
        &target.head_revision,
    )?;
    // A branch rooted before promotion cannot merge across schema boundaries.
    let base_document = sqlite_historical_visual::load_historical_visual_snapshot(
        connection,
        &input.project_id,
        Some(&base),
    )?
    .project;
    let source_document = sqlite_historical_visual::load_historical_visual_snapshot(
        connection,
        &input.project_id,
        Some(&source.head_revision),
    )?
    .project;
    let target_document = sqlite_historical_visual::load_historical_visual_snapshot(
        connection,
        &input.project_id,
        Some(&target.head_revision),
    )?
    .project;
    let plan =
        opencut_editor_application::historical_visual_collaboration::compare_historical_branches(
            input,
            &base,
            &base_document,
            &source_document,
            &target_document,
        );
    if let Some(proposed) = &plan.proposed_project {
        sqlite_collaboration::restricted_asset_bindings(
            connection,
            context,
            &input.project_id,
            &target_document.base,
            &proposed.base,
        )?;
    }
    Ok(plan)
}

impl EditorApplication {
    pub fn list_historical_visual_branches(
        &self,
        input: &BranchListRequest,
    ) -> Result<Vec<BranchView>, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_access::require(
            &tx,
            &self.connection.1,
            &input.project_id,
            ProjectAction::Read,
        )?;
        let mut branches = vec![view(&tx, &input.project_id, "main")?];
        let mut statement = tx
            .prepare(
                "SELECT branch_name FROM project_branches WHERE project_id=?1 ORDER BY branch_name",
            )
            .map_err(storage_error)?;
        let rows = statement
            .query_map([&input.project_id], |row| row.get::<_, String>(0))
            .map_err(storage_error)?;
        for row in rows {
            branches.push(view(&tx, &input.project_id, &row.map_err(storage_error)?)?);
        }
        drop(statement);
        tx.commit().map_err(storage_error)?;
        Ok(branches)
    }

    pub fn create_historical_visual_branch(
        &mut self,
        input: &BranchCreateRequest,
    ) -> Result<BranchView, EditorError> {
        sqlite_branching::branch_name(&input.branch_name, false)?;
        require_idempotency_key(&input.idempotency_key)?;
        let identity = request_hash("project_v3_branch_create", &input.project_id, input)?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_historical_visual::require_edit(&tx, &context, &input.project_id)?;
        sqlite_edit_control::require_writable(&tx, &context, &input.project_id)?;
        if let Some(raw) = load_idempotent(
            &tx,
            "project_v3_branch_create",
            &input.project_id,
            &input.idempotency_key,
            &identity,
            "branch",
        )? {
            return decode_json(&raw, "schema 3 branch receipt");
        }
        let main = view(&tx, &input.project_id, "main")?;
        if main.head_revision != input.expected_revision {
            return Err(revision_conflict(
                &input.expected_revision,
                &main.head_revision,
            ));
        }
        sqlite_historical_visual::load_historical_visual_snapshot(
            &tx,
            &input.project_id,
            Some(&input.from_revision),
        )?;
        if tx
            .query_row(
                "SELECT 1 FROM project_branches WHERE project_id=?1 AND branch_name=?2",
                params![input.project_id, input.branch_name],
                |_| Ok(()),
            )
            .optional()
            .map_err(storage_error)?
            .is_some()
        {
            return Err(EditorError::new("ALREADY_EXISTS", "Branch already exists"));
        }
        let branch = BranchView {
            project_id: input.project_id.clone(),
            branch_name: input.branch_name.clone(),
            head_revision: input.from_revision.clone(),
            created_from_revision: input.from_revision.clone(),
            is_main: false,
        };
        tx.execute("INSERT INTO project_branches(project_id,branch_name,head_revision,created_from_revision)
            VALUES (?1,?2,?3,?4)",params![input.project_id,input.branch_name,input.from_revision,input.from_revision])
            .map_err(constraint_or_storage)?;
        tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json)
            VALUES ('project_v3_branch_create',?1,?2,?3,'branch',?4)",params![input.project_id,input.idempotency_key,
            identity.current_digest(),encode_json(&branch,"schema 3 branch receipt")?]).map_err(idempotency_insert_error)?;
        tx.execute(
            "INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json)
            VALUES (?1,?2,?3,'branch.created',?4)",
            params![
                format!("evt_{}", Uuid::new_v4().simple()),
                input.project_id,
                input.from_revision,
                encode_json(&branch, "schema 3 branch event")?
            ],
        )
        .map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            "project.v3_branch_created",
            sqlite_audit::Links {
                project_id: Some(&input.project_id),
                revision: Some(&input.from_revision),
                ..Default::default()
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(branch)
    }

    pub fn compare_historical_visual_branches(
        &self,
        input: &BranchCompareRequest,
    ) -> Result<HistoricalVisualBranchComparePlan, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let result = compare_on(&tx, &self.connection.1, input)?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }

    pub fn apply_historical_visual_branch_edit(
        &mut self,
        input: &HistoricalVisualBranchEditRequest,
    ) -> Result<TransactionResult, EditorError> {
        sqlite_branching::branch_name(&input.branch_name, false)?;
        if input.project_id != input.request.project_id {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Project scopes differ",
            ));
        }
        require_idempotency_key(&input.request.idempotency_key)?;
        let identity = request_hash("project_v3_branch_apply", &input.project_id, input)?;
        let context = self.connection.1.clone();
        sqlite_historical_visual::require_edit(&self.connection, &context, &input.project_id)?;
        sqlite_edit_control::require_writable(&self.connection, &context, &input.project_id)?;
        sqlite_historical_visual::restricted_base(&self.connection, &context, &input.request)?;
        if let Some(raw) = load_idempotent(
            &self.connection,
            "project_v3_branch_apply",
            &input.project_id,
            &input.request.idempotency_key,
            &identity,
            "transaction",
        )? {
            return decode_json(&raw, "schema 3 branch edit receipt");
        }
        let mut prepared = input.request.clone();
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
        sqlite_historical_visual::require_edit(&tx, &context, &input.project_id)?;
        sqlite_edit_control::require_writable(&tx, &context, &input.project_id)?;
        sqlite_historical_visual::restricted_base(&tx, &context, &input.request)?;
        if let Some(raw) = load_idempotent(
            &tx,
            "project_v3_branch_apply",
            &input.project_id,
            &input.request.idempotency_key,
            &identity,
            "transaction",
        )? {
            return decode_json(&raw, "schema 3 branch edit receipt");
        }
        let current = view(&tx, &input.project_id, &input.branch_name)?;
        if current.head_revision != input.request.expected_revision {
            return Err(revision_conflict(
                &input.request.expected_revision,
                &current.head_revision,
            ));
        }
        let snapshot = sqlite_historical_visual::load_historical_visual_snapshot(
            &tx,
            &input.project_id,
            Some(&current.head_revision),
        )?;
        let plan = opencut_editor_application::historical_visual::plan_apply(
            &snapshot.project,
            &prepared,
        )?;
        let transaction_id = format!("txn_{}", Uuid::new_v4().simple());
        let revision = sqlite_historical_visual::revision_id(
            &plan.proposed_project,
            Some(&current.head_revision),
            Some(&transaction_id),
        )?;
        let result = TransactionResult {
            transaction_id: transaction_id.clone(),
            base_revision: current.head_revision.clone(),
            revision: revision.clone(),
            created_ids: plan.created_ids,
            changed_ids: plan.changed_ids,
            deleted_ids: plan.deleted_ids,
        };
        tx.execute("INSERT INTO revisions(revision,project_id,parent_revision,transaction_id,document_json)
            VALUES (?1,?2,?3,?4,?5)",params![revision,input.project_id,current.head_revision,transaction_id,
            encode_json(&plan.proposed_project,"schema 3 branch document")?]).map_err(storage_error)?;
        let updated=tx.execute("UPDATE project_branches SET head_revision=?1 WHERE project_id=?2 AND branch_name=?3 AND head_revision=?4",
            params![revision,input.project_id,input.branch_name,current.head_revision]).map_err(storage_error)?;
        if updated != 1 {
            return Err(EditorError::new(
                "REVISION_CONFLICT",
                "Branch head moved during schema 3 edit",
            ));
        }
        let receipt = encode_json(&result, "schema 3 branch edit receipt")?;
        tx.execute("INSERT INTO editor_transactions(transaction_id,project_id,base_revision,revision,request_json,result_json)
            VALUES (?1,?2,?3,?4,?5,?6)",params![transaction_id,input.project_id,current.head_revision,revision,
            encode_json(&json!({"kind":"project_v3.branch_apply","branch_name":input.branch_name,"request":prepared}),
                "schema 3 branch journal")?,receipt]).map_err(storage_error)?;
        tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json)
            VALUES ('project_v3_branch_apply',?1,?2,?3,'transaction',?4)",params![input.project_id,
            input.request.idempotency_key,identity.current_digest(),receipt]).map_err(idempotency_insert_error)?;
        tx.execute(
            "INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json)
            VALUES (?1,?2,?3,'branch.transaction.committed',?4)",
            params![
                format!("evt_{}", Uuid::new_v4().simple()),
                input.project_id,
                revision,
                encode_json(
                    &json!({"branch_name":input.branch_name,"transaction":result}),
                    "schema 3 branch event"
                )?
            ],
        )
        .map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            "project.v3_branch_applied",
            sqlite_audit::Links {
                project_id: Some(&input.project_id),
                revision: Some(&revision),
                transaction_id: Some(&transaction_id),
                ..Default::default()
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }

    pub fn merge_historical_visual_branches(
        &mut self,
        input: &BranchMergeRequest,
    ) -> Result<HistoricalVisualBranchMergeCommit, EditorError> {
        require_idempotency_key(&input.idempotency_key)?;
        let identity = request_hash("project_v3_branch_merge", &input.project_id, input)?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_historical_visual::require_edit(&tx, &context, &input.project_id)?;
        sqlite_edit_control::require_writable(&tx, &context, &input.project_id)?;
        if let Some(raw) = load_idempotent(
            &tx,
            "project_v3_branch_merge",
            &input.project_id,
            &input.idempotency_key,
            &identity,
            "branch_merge",
        )? {
            return decode_json(&raw, "schema 3 merge receipt");
        }
        let compare = BranchCompareRequest {
            base_asset_versions: Default::default(),
            project_id: input.project_id.clone(),
            source_branch: input.source_branch.clone(),
            target_branch: input.target_branch.clone(),
            expected_source_revision: input.expected_source_revision.clone(),
            expected_target_revision: input.expected_target_revision.clone(),
        };
        let plan = compare_on(&tx, &context, &compare)?;
        if !plan.conflicts.is_empty() {
            return Err(EditorError {
                code: "BRANCH_MERGE_CONFLICT".into(),
                message: "Branches have conflicting changes".into(),
                details: json!({"conflicts":plan.conflicts}),
            });
        }
        if plan.changes.is_empty() {
            return Err(EditorError::new(
                "NO_EFFECT",
                "Source branch has no unmerged changes",
            ));
        }
        let proposed = plan
            .proposed_project
            .ok_or_else(|| EditorError::new("BRANCH_MERGE_CONFLICT", "No valid merge result"))?;
        let transaction_id = format!("txn_{}", Uuid::new_v4().simple());
        let revision = sqlite_historical_visual::revision_id(
            &proposed,
            Some(&plan.target_revision),
            Some(&transaction_id),
        )?;
        let (created_ids, changed_ids, deleted_ids) =
            sqlite_collaboration::result_ids(&input.project_id, &plan.changes);
        let transaction = TransactionResult {
            transaction_id: transaction_id.clone(),
            base_revision: plan.target_revision.clone(),
            revision: revision.clone(),
            created_ids,
            changed_ids,
            deleted_ids,
        };
        let receipt = HistoricalVisualBranchMergeCommit {
            source_branch: input.source_branch.clone(),
            target_branch: input.target_branch.clone(),
            base_revision: plan.base_revision.clone(),
            source_revision: plan.source_revision.clone(),
            transaction: transaction.clone(),
        };
        tx.execute("INSERT INTO revisions(revision,project_id,parent_revision,transaction_id,document_json)
            VALUES (?1,?2,?3,?4,?5)",params![revision,input.project_id,plan.target_revision,transaction_id,
            encode_json(&proposed,"schema 3 merged document")?]).map_err(storage_error)?;
        let updated = if input.target_branch == "main" {
            tx.execute("UPDATE projects SET head_revision=?1,name=?2 WHERE project_id=?3 AND head_revision=?4",
                params![revision,proposed.base.name,input.project_id,plan.target_revision]).map_err(storage_error)?
        } else {
            tx.execute("UPDATE project_branches SET head_revision=?1 WHERE project_id=?2 AND branch_name=?3 AND head_revision=?4",
                params![revision,input.project_id,input.target_branch,plan.target_revision]).map_err(storage_error)?
        };
        if updated != 1 {
            return Err(EditorError::new(
                "REVISION_CONFLICT",
                "Target branch moved during schema 3 merge",
            ));
        }
        tx.execute("INSERT INTO project_merge_parents(revision,project_id,source_revision) VALUES (?1,?2,?3)",
            params![revision,input.project_id,plan.source_revision]).map_err(storage_error)?;
        tx.execute("INSERT INTO editor_transactions(transaction_id,project_id,base_revision,revision,request_json,result_json)
            VALUES (?1,?2,?3,?4,?5,?6)",params![transaction_id,input.project_id,plan.target_revision,revision,
            encode_json(&json!({"kind":"project_v3.branch_merge","source_branch":input.source_branch,
                "target_branch":input.target_branch,"base_revision":plan.base_revision,
                "source_revision":plan.source_revision,"changes":plan.changes}),"schema 3 merge journal")?,
            encode_json(&transaction,"schema 3 merge transaction")?]).map_err(storage_error)?;
        tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json)
            VALUES ('project_v3_branch_merge',?1,?2,?3,'branch_merge',?4)",params![input.project_id,
            input.idempotency_key,identity.current_digest(),encode_json(&receipt,"schema 3 merge receipt")?])
            .map_err(idempotency_insert_error)?;
        let event_type = if input.target_branch == "main" {
            "transaction.committed"
        } else {
            "branch.merged"
        };
        let payload = if input.target_branch == "main" {
            encode_json(&transaction, "schema 3 main merge event")?
        } else {
            encode_json(&receipt, "schema 3 branch merge event")?
        };
        tx.execute("INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json) VALUES (?1,?2,?3,?4,?5)",
            params![format!("evt_{}",Uuid::new_v4().simple()),input.project_id,revision,event_type,payload]).map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            "project.v3_branch_merged",
            sqlite_audit::Links {
                project_id: Some(&input.project_id),
                revision: Some(&revision),
                transaction_id: Some(&transaction_id),
                ..Default::default()
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite_historical_visual::HistoricalVisualPromotion;
    use opencut_contracts::{
        CreateProject, FrameRate,
        historical_visual_ops::{ApplyHistoricalVisualProject, CreateProjectRequest},
    };

    fn edit(revision: &str, key: &str, operations: Value) -> ApplyHistoricalVisualProject {
        serde_json::from_value(json!({"project_id":"p","expected_revision":revision,
            "idempotency_key":key,"label":key,"operations":operations}))
        .unwrap()
    }
    fn shape() -> Value {
        json!({"type":"shape.put","sequence_id":"main","shape":{
        "id":"card","track_id":"graphics","timeline_range":{"start":{"ticks":"0","timebase":1000},
        "duration":{"ticks":"1000","timebase":1000}},"primitive":{"kind":"rectangle",
        "width_px":200.0,"height_px":100.0},"fill":{"red":255,"green":0,"blue":0,"alpha":255},
        "position_x":0.0,"position_y":0.0,"rotation_degrees":0.0,"opacity":1.0}})
    }
    #[test]
    fn v3_undo_redo_branch_merge_share_revision_graph() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = EditorApplication::open(dir.path().join("workspace.sqlite")).unwrap();
        let created = app
            .create_historical_visual_project(&CreateProjectRequest {
                project: CreateProject {
                    id: "p".into(),
                    name: "P".into(),
                    sequence_id: "main".into(),
                    width: 640,
                    height: 360,
                    fps: FrameRate {
                        numerator: 25,
                        denominator: 1,
                    },
                },
                idempotency_key: "create".into(),
            })
            .unwrap();
        let track = app
            .apply_historical_visual_project(&edit(
                &created.revision,
                "track",
                json!([{"type":"base.apply",
            "operation":{"type":"track.create","sequence_id":"main","track":{"id":"graphics",
            "name":"Graphics","kind":"graphic","enabled":true,"muted":false,
            "locked":false,"clips":[],"params":{}}}}]),
            ))
            .unwrap();
        let shaped = app
            .apply_historical_visual_project(&edit(&track.revision, "shape", json!([shape()])))
            .unwrap();
        let rename = |revision: &str, key: &str, name: &str| {
            edit(
                revision,
                key,
                json!([{"type":"base.apply",
            "operation":{"type":"sequence.update_settings","sequence_id":"main","name":name,
            "width":null,"height":null,"fps":null}}]),
            )
        };
        let later = app
            .apply_historical_visual_project(&rename(&shaped.revision, "human-name", "Human"))
            .unwrap();
        let undo_request = opencut_api_contracts::collaboration::UndoRequest {
            project_id: "p".into(),
            expected_revision: later.revision.clone(),
            target_transaction_id: shaped.transaction_id.clone(),
            idempotency_key: "undo-shape".into(),
        };
        let undo = app.apply_historical_visual_undo(&undo_request).unwrap();
        assert_eq!(
            app.apply_historical_visual_undo(&undo_request).unwrap(),
            undo
        );
        let snapshot = app
            .inspect_historical_visual_project(
                &opencut_contracts::historical_visual_ops::InspectProject {
                    project_id: "p".into(),
                    revision: None,
                },
            )
            .unwrap();
        assert!(snapshot.project.visual.iter().all(|v| v.shapes.is_empty()));
        assert_eq!(snapshot.project.base.sequences[0].name, "Human");
        let redo = app
            .apply_historical_visual_redo(&opencut_api_contracts::collaboration::RedoRequest {
                project_id: "p".into(),
                expected_revision: undo.transaction.revision.clone(),
                undo_transaction_id: undo.transaction.transaction_id.clone(),
                idempotency_key: "redo-shape".into(),
            })
            .unwrap();
        let branch = app
            .create_historical_visual_branch(&BranchCreateRequest {
                project_id: "p".into(),
                branch_name: "agent".into(),
                from_revision: redo.transaction.revision.clone(),
                expected_revision: redo.transaction.revision.clone(),
                idempotency_key: "branch-create".into(),
            })
            .unwrap();
        let effect = json!({"type":"effect.put","sequence_id":"main","effect":{"id":"grade",
            "target_id":"card","effect":{"kind":"color_adjust","exposure_ev":0.2,
            "contrast":1.0,"saturation":1.0}}});
        let branch_edit = app
            .apply_historical_visual_branch_edit(&HistoricalVisualBranchEditRequest {
                project_id: "p".into(),
                branch_name: "agent".into(),
                request: edit(&branch.head_revision, "branch-grade", json!([effect])),
            })
            .unwrap();
        let main = app
            .apply_historical_visual_project(&rename(
                &redo.transaction.revision,
                "main-name",
                "Human later",
            ))
            .unwrap();
        let merge_request = BranchMergeRequest {
            base_asset_versions: Default::default(),
            project_id: "p".into(),
            source_branch: "agent".into(),
            target_branch: "main".into(),
            expected_source_revision: branch_edit.revision.clone(),
            expected_target_revision: main.revision.clone(),
            idempotency_key: "merge".into(),
        };
        let merge = app
            .merge_historical_visual_branches(&merge_request)
            .unwrap();
        assert_eq!(
            app.merge_historical_visual_branches(&merge_request)
                .unwrap(),
            merge
        );
        let merged = app
            .inspect_historical_visual_project(
                &opencut_contracts::historical_visual_ops::InspectProject {
                    project_id: "p".into(),
                    revision: None,
                },
            )
            .unwrap();
        assert_eq!(merged.project.base.sequences[0].name, "Human later");
        assert_eq!(merged.project.visual[0].effects[0].id, "grade");
        let parents =
            sqlite_branching::parents(&app.connection, "p", &merge.transaction.revision).unwrap();
        assert_eq!(parents, vec![main.revision, branch_edit.revision]);
        let undo_branch = app
            .apply_historical_visual_undo(&opencut_api_contracts::collaboration::UndoRequest {
                project_id: "p".into(),
                expected_revision: merge.transaction.revision.clone(),
                target_transaction_id: branch_edit.transaction_id,
                idempotency_key: "undo-merged-grade".into(),
            })
            .unwrap();
        let final_snapshot = app
            .inspect_historical_visual_project(
                &opencut_contracts::historical_visual_ops::InspectProject {
                    project_id: "p".into(),
                    revision: Some(undo_branch.transaction.revision),
                },
            )
            .unwrap();
        assert_eq!(final_snapshot.project.base.sequences[0].name, "Human later");
        assert!(final_snapshot.project.visual[0].effects.is_empty());
        let mapped = crate::sqlite_project_exchange::assert_current_archive_roundtrip(
            &mut app,
            "p",
            &final_snapshot.revision,
            "restored-visual",
        );
        let undone = mapped
            .transactions
            .iter()
            .find(|t| {
                serde_json::from_str::<Value>(&t.journal.storage_json().unwrap())
                    .unwrap()
                    .get("idempotency_key")
                    == Some(&json!("undo-merged-grade"))
            })
            .unwrap();
        let plan = app
            .plan_historical_visual_redo(&opencut_api_contracts::collaboration::RedoRequest {
                project_id: "restored-visual".into(),
                expected_revision: mapped.main_head_revision,
                undo_transaction_id: undone.transaction_id.clone(),
                idempotency_key: "redo-restored".into(),
            })
            .unwrap();
        assert!(plan.conflicts.is_empty());
        assert_eq!(
            plan.proposed_project.unwrap().visual[0].effects[0].id,
            "grade"
        );
    }

    #[test]
    fn schema_2_history_can_be_undone_and_redone_after_promotion() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = EditorApplication::open(dir.path().join("workspace.sqlite")).unwrap();
        let base = app
            .create_project(
                CreateProject {
                    id: "p".into(),
                    name: "P".into(),
                    sequence_id: "main".into(),
                    width: 640,
                    height: 360,
                    fps: FrameRate {
                        numerator: 25,
                        denominator: 1,
                    },
                },
                "legacy-create",
            )
            .unwrap();
        let track = app
            .apply(
                "p",
                &serde_json::from_value(json!({
                    "expected_revision":base.revision,"idempotency_key":"legacy-track",
                    "label":"Create graphics track","operations":[{"type":"track.create",
                    "sequence_id":"main","track":{"id":"graphics","name":"Graphics",
                    "kind":"graphic","enabled":true,"muted":false,"locked":false,
                    "clips":[],"params":{}}}]
                }))
                .unwrap(),
            )
            .unwrap();
        let renamed = app
            .apply(
                "p",
                &serde_json::from_value(json!({
                    "expected_revision":track.revision,"idempotency_key":"legacy-rename",
                    "label":"Rename sequence","operations":[{"type":"sequence.update_settings",
                    "sequence_id":"main","name":"Edited","width":null,"height":null,"fps":null}]
                }))
                .unwrap(),
            )
            .unwrap();
        let promoted = app
            .promote_historical_visual(&HistoricalVisualPromotion {
                project_id: "p".into(),
                expected_revision: renamed.revision,
                idempotency_key: "promote-v3".into(),
            })
            .unwrap();
        let visual_edit = app
            .apply_historical_visual_project(&edit(
                &promoted.revision,
                "post-promotion-shape",
                json!([shape()]),
            ))
            .unwrap();

        let undo_request = opencut_api_contracts::collaboration::UndoRequest {
            project_id: "p".into(),
            expected_revision: visual_edit.revision.clone(),
            target_transaction_id: renamed.transaction_id.clone(),
            idempotency_key: "undo-before-promotion".into(),
        };
        let undo_plan = app.plan_historical_visual_undo(&undo_request).unwrap();
        assert!(undo_plan.conflicts.is_empty(), "{:?}", undo_plan.conflicts);
        assert_eq!(
            undo_plan.proposed_project.as_ref().unwrap().base.sequences[0].name,
            "Main"
        );
        assert_eq!(
            undo_plan.proposed_project.as_ref().unwrap().visual[0].shapes[0].id,
            "card"
        );
        let undo = app.apply_historical_visual_undo(&undo_request).unwrap();
        assert_eq!(
            app.apply_historical_visual_undo(&undo_request).unwrap(),
            undo
        );

        let redo_request = opencut_api_contracts::collaboration::RedoRequest {
            project_id: "p".into(),
            expected_revision: undo.transaction.revision.clone(),
            undo_transaction_id: undo.transaction.transaction_id.clone(),
            idempotency_key: "redo-before-promotion".into(),
        };
        let redo_plan = app.plan_historical_visual_redo(&redo_request).unwrap();
        assert!(redo_plan.conflicts.is_empty(), "{:?}", redo_plan.conflicts);
        assert_eq!(
            redo_plan.proposed_project.as_ref().unwrap().base.sequences[0].name,
            "Edited"
        );
        assert_eq!(
            redo_plan.proposed_project.as_ref().unwrap().visual[0].shapes[0].id,
            "card"
        );
        app.apply_historical_visual_redo(&redo_request).unwrap();
        let head = app
            .inspect_historical_visual_project(
                &opencut_contracts::historical_visual_ops::InspectProject {
                    project_id: "p".into(),
                    revision: None,
                },
            )
            .unwrap();
        assert_eq!(head.project.base.sequences[0].name, "Edited");
        assert_eq!(head.project.visual[0].shapes[0].id, "card");
        assert_eq!(
            sqlite_branching::parents(&app.connection, "p", &undo.transaction.revision).unwrap(),
            [visual_edit.revision]
        );
    }
}
