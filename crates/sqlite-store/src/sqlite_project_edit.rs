//! Scene/project structure uses the same revision, receipt and publication transaction as visual edits.
use super::*;
use opencut_contracts::{project_edit::ProjectEditRequest, project_v3_ops::VisualPlan};
use opencut_editor_application::project_edit;
use sqlite_project_v3::{EditCommit, commit_edit, load_v3_snapshot, require_edit};

impl EditorApplication {
    pub fn plan_project_edit(&self, input: &ProjectEditRequest) -> Result<VisualPlan, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        require_edit(&tx, &self.connection.1, &input.project_id)?;
        let snapshot = load_v3_snapshot(&tx, &input.project_id, None)?;
        let plan = project_edit::plan(&snapshot, input)?;
        tx.commit().map_err(storage_error)?;
        Ok(plan)
    }
    pub fn apply_project_edit(
        &mut self,
        input: &ProjectEditRequest,
    ) -> Result<opencut_contracts::TransactionResult, EditorError> {
        require_idempotency_key(&input.idempotency_key)?;
        let identity = request_hash("project_edit", &input.project_id, input)?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        require_edit(&tx, &context, &input.project_id)?;
        sqlite_edit_control::require_writable(&tx, &context, &input.project_id)?;
        if let Some(receipt) = load_idempotent(
            &tx,
            "project_edit",
            &input.project_id,
            &input.idempotency_key,
            &identity,
            "transaction",
        )? {
            return decode_json(&receipt, "project edit receipt");
        }
        let current = load_v3_snapshot(&tx, &input.project_id, None)?;
        let plan = project_edit::plan(&current, input)?;
        let result = commit_edit(
            &tx,
            &context,
            &current,
            plan,
            EditCommit {
                project_id: &input.project_id,
                idempotency_key: &input.idempotency_key,
                identity: &identity,
                family: "project_edit",
                event: "project.structure_edited",
                request: json!({"kind":"project.structure_edit","request":input}),
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_api_contracts::collaboration::{RedoRequest, UndoRequest};
    use opencut_contracts::{
        project_edit::ProjectEdit,
        project_v3::ProjectSnapshotV3,
        project_v3_ops::{ApplyV3Project, CreateV3Project, InspectV3Project},
    };
    fn setup() -> (EditorApplication, ProjectSnapshotV3) {
        let mut app = EditorApplication::open(":memory:").unwrap();
        let snapshot=app.create_v3_project(&serde_json::from_value::<CreateV3Project>(json!({"project":{"id":"p","name":"Original","sequence_id":"main","width":640,"height":360,"fps":{"numerator":25,"denominator":1}},"idempotency_key":"create"})).unwrap()).unwrap();
        (app, snapshot)
    }
    fn inspect(app: &EditorApplication) -> ProjectSnapshotV3 {
        app.inspect_v3_project(&InspectV3Project {
            project_id: "p".into(),
            revision: None,
        })
        .unwrap()
    }
    fn request(revision: &str, key: &str, operations: Vec<ProjectEdit>) -> ProjectEditRequest {
        ProjectEditRequest {
            project_id: "p".into(),
            expected_revision: revision.into(),
            idempotency_key: key.into(),
            label: key.into(),
            operations,
        }
    }
    fn create() -> ProjectEdit {
        ProjectEdit::CreateScene {
            sequence_id: "second".into(),
            name: "Second".into(),
            video_track_id: "video-second".into(),
        }
    }
    #[test]
    fn project_and_scene_edits_are_atomic_replayable_and_reversible() {
        let (mut app, initial) = setup();
        let req = request(
            &initial.revision,
            "structure",
            vec![
                ProjectEdit::RenameProject {
                    name: "Renamed".into(),
                },
                create(),
                ProjectEdit::RenameScene {
                    sequence_id: "second".into(),
                    name: "B roll".into(),
                },
            ],
        );
        let plan = app.plan_project_edit(&req).unwrap();
        assert_eq!(inspect(&app), initial);
        let committed = app.apply_project_edit(&req).unwrap();
        assert_eq!(app.apply_project_edit(&req).unwrap(), committed);
        assert_eq!(inspect(&app).project, plan.proposed_project);
        assert_eq!(
            app.history("p", None, 1).unwrap().revisions[0]
                .label
                .as_deref(),
            Some("structure")
        );
        let list = app
            .list_v3_projects(&opencut_contracts::project_v3_ops::ListV3Projects {
                cursor: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(list.items[0].name, "Renamed");
        let undo = app
            .apply_v3_undo(&UndoRequest {
                project_id: "p".into(),
                expected_revision: committed.revision.clone(),
                target_transaction_id: committed.transaction_id.clone(),
                idempotency_key: "undo".into(),
            })
            .unwrap();
        assert_eq!(inspect(&app).project, initial.project);
        let redone = app
            .apply_v3_redo(&RedoRequest {
                project_id: "p".into(),
                expected_revision: undo.transaction.revision,
                undo_transaction_id: undo.transaction.transaction_id,
                idempotency_key: "redo".into(),
            })
            .unwrap();
        assert_eq!(inspect(&app).project, plan.proposed_project);
        assert_ne!(redone.transaction.revision, committed.revision);
        let mut stale = req.clone();
        stale.idempotency_key = "stale".into();
        assert_eq!(
            app.apply_project_edit(&stale).unwrap_err().code,
            "REVISION_CONFLICT"
        );
        let mut reused = req;
        reused.label = "different".into();
        assert_eq!(
            app.apply_project_edit(&reused).unwrap_err().code,
            "IDEMPOTENCY_KEY_REUSED"
        );
    }
    #[test]
    fn main_scene_protection_and_failed_audit_leave_no_partial_commit() {
        let (mut app, initial) = setup();
        let bad = request(
            &initial.revision,
            "bad",
            vec![
                create(),
                ProjectEdit::DeleteScene {
                    sequence_id: "main".into(),
                },
            ],
        );
        assert_eq!(
            app.apply_project_edit(&bad).unwrap_err().code,
            "CONSTRAINT_VIOLATION"
        );
        assert_eq!(inspect(&app), initial);
        let req = request(&initial.revision, "retry", vec![create()]);
        app.connection.execute_batch("CREATE TRIGGER deny_structure BEFORE INSERT ON audit_records WHEN NEW.action='project.structure_edited' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        assert!(app.apply_project_edit(&req).is_err());
        assert_eq!(inspect(&app), initial);
        app.connection
            .execute_batch("DROP TRIGGER deny_structure")
            .unwrap();
        app.apply_project_edit(&req).unwrap();
        assert_eq!(inspect(&app).project.base.sequences.len(), 2);
    }
    #[test]
    fn deleting_a_scene_removes_visuals_and_undo_restores_exact_content() {
        let (mut app, initial) = setup();
        let created = app
            .apply_project_edit(&request(&initial.revision, "create-scene", vec![create()]))
            .unwrap();
        let visual:ApplyV3Project=serde_json::from_value(json!({"project_id":"p","expected_revision":created.revision,"idempotency_key":"shape","label":"shape","operations":[{"type":"base.apply","operation":{"type":"track.create","sequence_id":"second","track":{"id":"graphics","name":"Shapes","kind":"graphic","enabled":true,"muted":false,"locked":false,"clips":[],"params":{}}}},{"type":"shape.put","sequence_id":"second","shape":{"id":"shape","track_id":"graphics","timeline_range":{"start":{"ticks":"0","timebase":1},"duration":{"ticks":"2","timebase":1}},"primitive":{"kind":"rectangle","width_px":50.0,"height_px":50.0},"fill":{"red":255,"green":0,"blue":0,"alpha":255},"position_x":0.0,"position_y":0.0,"rotation_degrees":0.0,"opacity":1.0}}]})).unwrap();
        app.apply_v3_project(&visual).unwrap();
        let before = inspect(&app);
        let removed = app
            .apply_project_edit(&request(
                &before.revision,
                "delete",
                vec![ProjectEdit::DeleteScene {
                    sequence_id: "second".into(),
                }],
            ))
            .unwrap();
        assert!(removed.deleted_ids.contains(&"shape".into()));
        assert!(inspect(&app).project.visual.is_empty());
        app.apply_v3_undo(&UndoRequest {
            project_id: "p".into(),
            expected_revision: removed.revision,
            target_transaction_id: removed.transaction_id,
            idempotency_key: "undo-delete".into(),
        })
        .unwrap();
        assert_eq!(inspect(&app).project, before.project);
    }
    #[test]
    fn revoked_editor_cannot_replay_or_plan_scene_mutations() {
        use opencut_editor_application::access::*;
        let (mut app, initial) = setup();
        app.set_principal_policy(PrincipalPolicy {
            principal_id: "editor".into(),
            expected_revision: "0".into(),
            enabled: true,
            grants: vec![ProjectGrant {
                project_id: "p".into(),
                actions: vec![ProjectAction::Read, ProjectAction::Edit],
            }],
        })
        .unwrap();
        let issued = app.issue_credential("editor").unwrap();
        let caller = app.authenticate(&issued.secret).unwrap();
        app.set_caller(caller.clone());
        let req = request(&initial.revision, "create", vec![create()]);
        app.apply_project_edit(&req).unwrap();
        app.set_caller(Caller::LocalOwner);
        app.set_edit_control(&opencut_api_contracts::edit_control::EditControlSet {
            project_id: "p".into(),
            expected_generation: "0".into(),
            held: true,
            idempotency_key: "hold".into(),
        })
        .unwrap();
        app.set_caller(caller.clone());
        assert_eq!(
            app.apply_project_edit(&req).unwrap_err().code,
            "EDIT_HELD_BY_OWNER"
        );
        app.set_caller(Caller::LocalOwner);
        app.revoke_credential(&issued.credential_id).unwrap();
        app.set_caller(caller);
        assert_eq!(
            app.apply_project_edit(&req).unwrap_err().code,
            "NOT_AUTHORIZED"
        );
        assert_eq!(
            app.plan_project_edit(&req).unwrap_err().code,
            "NOT_AUTHORIZED"
        );
    }
}
