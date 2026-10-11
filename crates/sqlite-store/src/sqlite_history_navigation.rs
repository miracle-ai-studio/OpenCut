//! Read the first-parent editor history in one snapshot; merge revisions are single editor actions.
use super::*;
use opencut_api_contracts::history_navigation::{
    HistoryNavigation, HistoryNavigationInput, UndoTarget,
};
use opencut_editor_application::history_navigation::{HistoryStep, Navigation};
use std::collections::HashSet;

impl EditorApplication {
    pub fn history_navigation(
        &self,
        input: &HistoryNavigationInput,
    ) -> Result<HistoryNavigation, EditorError> {
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
        let head: String = tx
            .query_row(
                "SELECT head_revision FROM projects WHERE project_id=?1",
                [&input.project_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage_error)?
            .ok_or_else(|| EditorError::new("NOT_FOUND", "Project does not exist"))?;
        let revision = input.revision.clone().unwrap_or(head);
        let mut current = Some(revision.clone());
        let mut visited = HashSet::new();
        let mut steps = Vec::new();
        {
            let mut statement = tx.prepare("SELECT r.parent_revision,r.transaction_id,t.request_json FROM revisions r LEFT JOIN editor_transactions t ON t.project_id=r.project_id AND t.transaction_id=r.transaction_id WHERE r.project_id=?1 AND r.revision=?2").map_err(storage_error)?;
            while let Some(id) = current {
                if visited.len() >= 100_000 || !visited.insert(id.clone()) {
                    return Err(EditorError::new(
                        "STORAGE_ERROR",
                        "Editor history exceeds traversal limit or contains a cycle",
                    ));
                }
                let row: Option<(Option<String>, Option<String>, Option<String>)> = statement
                    .query_row(params![input.project_id, id], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                    })
                    .optional()
                    .map_err(storage_error)?;
                let (parent, transaction_id, raw) = row.ok_or_else(|| {
                    EditorError::new("NOT_FOUND", "Revision does not belong to this project")
                })?;
                current = parent;
                let Some(transaction_id) = transaction_id else {
                    continue;
                };
                let value: Value = decode_json(
                    &raw.ok_or_else(|| {
                        EditorError::new("STORAGE_ERROR", "History transaction is missing")
                    })?,
                    "history navigation journal",
                )?;
                let required = |key: &str| -> Result<String, EditorError> {
                    value
                        .get(key)
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .ok_or_else(|| {
                            EditorError::new(
                                "STORAGE_ERROR",
                                "History journal is missing its causal target",
                            )
                        })
                };
                let step = match value.get("kind").and_then(Value::as_str) {
                    Some("selective_undo") => HistoryStep::Undo {
                        transaction_id,
                        target: required("target_transaction_id")?,
                    },
                    Some("selective_redo") => HistoryStep::Redo {
                        undo_transaction_id: required("undo_transaction_id")?,
                        target: required("target_transaction_id")?,
                    },
                    Some("project_v3.create" | "project_v3.promote") => continue,
                    Some(kind)
                        if kind.starts_with("selective_")
                            || kind.starts_with("project_v3.selective_") =>
                    {
                        return Err(EditorError::new(
                            "STORAGE_ERROR",
                            "Unrecognized history navigation journal",
                        ));
                    }
                    _ => HistoryStep::Edit(UndoTarget {
                        transaction_id,
                        label: value
                            .get("label")
                            .or_else(|| value.get("request").and_then(|r| r.get("label")))
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    }),
                };
                steps.push(step);
            }
        }
        let mut navigation = Navigation::default();
        for step in steps.into_iter().rev() {
            navigation.push(step);
        }
        let (undo, redo) = navigation.targets();
        tx.commit().map_err(storage_error)?;
        Ok(HistoryNavigation {
            project_id: input.project_id.clone(),
            revision,
            undo,
            redo,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_api_contracts::{
        branching::{BranchCreateRequest, BranchMergeRequest},
        collaboration::{RedoRequest, UndoRequest},
        project_v3_collaboration::V3BranchEditRequest,
    };
    use opencut_contracts::{
        project_edit::{ProjectEdit, ProjectEditRequest},
        project_v3_ops::{CreateV3Project, InspectV3Project},
    };
    fn create(app: &mut EditorApplication, id: &str) -> String {
        app.create_v3_project(&serde_json::from_value::<CreateV3Project>(json!({"project":{"id":id,"name":"Initial","sequence_id":"main","width":640,"height":360,"fps":{"numerator":25,"denominator":1}},"idempotency_key":id})).unwrap()).unwrap().revision
    }
    fn nav(app: &EditorApplication) -> HistoryNavigation {
        app.history_navigation(&HistoryNavigationInput {
            project_id: "p".into(),
            revision: None,
        })
        .unwrap()
    }
    fn rename(app: &mut EditorApplication, name: &str) -> opencut_contracts::TransactionResult {
        app.apply_project_edit(&ProjectEditRequest {
            project_id: "p".into(),
            expected_revision: nav(app).revision,
            idempotency_key: name.into(),
            label: name.into(),
            operations: vec![ProjectEdit::RenameProject { name: name.into() }],
        })
        .unwrap()
    }
    fn step(app: &mut EditorApplication, undo: bool, key: &str) {
        let state = nav(app);
        if undo {
            app.apply_v3_undo(&UndoRequest {
                project_id: "p".into(),
                expected_revision: state.revision,
                target_transaction_id: state.undo.unwrap().transaction_id,
                idempotency_key: key.into(),
            })
            .unwrap();
        } else {
            app.apply_v3_redo(&RedoRequest {
                project_id: "p".into(),
                expected_revision: state.revision,
                undo_transaction_id: state.redo.unwrap().undo_transaction_id,
                idempotency_key: key.into(),
            })
            .unwrap();
        }
    }
    fn name(app: &EditorApplication) -> String {
        app.inspect_v3_project(&InspectV3Project {
            project_id: "p".into(),
            revision: None,
        })
        .unwrap()
        .project
        .base
        .name
    }
    #[test]
    fn durable_multi_step_history_reopens_and_new_edit_invalidates_redo() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("workspace.sqlite");
        let mut app = EditorApplication::open(&database).unwrap();
        create(&mut app, "p");
        assert!(nav(&app).undo.is_none());
        let a = rename(&mut app, "A");
        rename(&mut app, "B");
        rename(&mut app, "C");
        step(&mut app, true, "u-c");
        assert_eq!(name(&app), "B");
        step(&mut app, true, "u-b");
        assert_eq!(name(&app), "A");
        drop(app);
        let mut app = EditorApplication::open(&database).unwrap();
        assert_eq!(nav(&app).undo.unwrap().transaction_id, a.transaction_id);
        assert_eq!(nav(&app).redo.unwrap().label.as_deref(), Some("B"));
        step(&mut app, true, "u-a");
        assert_eq!(name(&app), "Initial");
        assert!(nav(&app).undo.is_none());
        for (key, expected) in [("r-a", "A"), ("r-b", "B"), ("r-c", "C")] {
            step(&mut app, false, key);
            assert_eq!(name(&app), expected);
        }
        assert!(nav(&app).redo.is_none());
        step(&mut app, true, "u-c-again");
        assert_eq!(name(&app), "B");
        rename(&mut app, "D");
        assert!(nav(&app).redo.is_none());
        step(&mut app, true, "u-d");
        assert_eq!(name(&app), "B");
        step(&mut app, true, "u-b-again");
        assert_eq!(name(&app), "A");
        let historic = app
            .history_navigation(&HistoryNavigationInput {
                project_id: "p".into(),
                revision: Some(a.revision),
            })
            .unwrap();
        assert_eq!(historic.undo.unwrap().label.as_deref(), Some("A"));
        assert!(historic.redo.is_none());
    }
    #[test]
    fn undo_and_redo_continue_past_the_browser_history_page() {
        let mut app = EditorApplication::open(":memory:").unwrap();
        create(&mut app, "p");
        for index in 0..55 {
            rename(&mut app, &format!("Edit {index}"));
        }
        assert!(app.history("p", None, 50).unwrap().next_cursor.is_some());
        for index in 0..55 {
            step(&mut app, true, &format!("undo {index}"));
        }
        assert!(nav(&app).undo.is_none());
        assert_eq!(name(&app), "Initial");
        for index in 0..55 {
            step(&mut app, false, &format!("redo {index}"));
        }
        assert!(nav(&app).redo.is_none());
        assert_eq!(name(&app), "Edit 54");
    }

    #[test]
    fn navigation_excludes_unmerged_branch_and_treats_merge_as_one_action() {
        let mut app = EditorApplication::open(":memory:").unwrap();
        let initial = create(&mut app, "p");
        app.create_v3_branch(&BranchCreateRequest {
            project_id: "p".into(),
            branch_name: "alternate".into(),
            from_revision: initial.clone(),
            expected_revision: initial.clone(),
            idempotency_key: "branch".into(),
        })
        .unwrap();
        let branch:V3BranchEditRequest=serde_json::from_value(json!({"project_id":"p","branch_name":"alternate","request":{"project_id":"p","expected_revision":initial,"idempotency_key":"branch-edit","label":"Branch track","operations":[{"type":"base.apply","operation":{"type":"track.create","sequence_id":"main","track":{"id":"branch-track","name":"Branch","kind":"video","enabled":true,"muted":false,"locked":false,"clips":[],"params":{}}}}]}})).unwrap();
        let edited = app.apply_v3_branch_edit(&branch).unwrap();
        assert!(nav(&app).undo.is_none());
        let merged = app
            .merge_v3_branches(&BranchMergeRequest {
                project_id: "p".into(),
                source_branch: "alternate".into(),
                target_branch: "main".into(),
                expected_source_revision: edited.revision,
                expected_target_revision: initial,
                idempotency_key: "merge".into(),
            })
            .unwrap();
        assert_eq!(
            nav(&app).undo.unwrap().transaction_id,
            merged.transaction.transaction_id
        );
        step(&mut app, true, "undo-merge");
        assert!(nav(&app).undo.is_none());
        step(&mut app, false, "redo-merge");
        assert_eq!(
            nav(&app).undo.unwrap().transaction_id,
            merged.transaction.transaction_id
        );
    }
    #[test]
    fn legacy_edits_navigate_across_visual_promotion_without_undoing_the_format() {
        let mut app = EditorApplication::open(":memory:").unwrap();
        let initial = app.create_project(serde_json::from_value(json!({"id":"p","name":"Legacy","sequence_id":"main","width":640,"height":360,"fps":{"numerator":25,"denominator":1}})).unwrap(), "legacy").unwrap();
        let edit = app.apply("p", &serde_json::from_value(json!({"expected_revision":initial.revision,"idempotency_key":"legacy-track","label":"Track","operations":[{"type":"track.create","sequence_id":"main","track":{"id":"legacy-track","name":"Legacy","kind":"video","enabled":true,"muted":false,"locked":false,"clips":[],"params":{}}}]})).unwrap()).unwrap();
        assert_eq!(nav(&app).undo.unwrap().transaction_id, edit.transaction_id);
        let undone = app
            .apply_undo(&UndoRequest {
                project_id: "p".into(),
                expected_revision: edit.revision,
                target_transaction_id: edit.transaction_id.clone(),
                idempotency_key: "legacy-undo".into(),
            })
            .unwrap();
        assert!(nav(&app).undo.is_none());
        let redone = app
            .apply_redo(&RedoRequest {
                project_id: "p".into(),
                expected_revision: undone.transaction.revision,
                undo_transaction_id: nav(&app).redo.unwrap().undo_transaction_id,
                idempotency_key: "legacy-redo".into(),
            })
            .unwrap();
        app.promote_v3_project(&opencut_contracts::project_v3_ops::PromoteV3Project {
            project_id: "p".into(),
            expected_revision: redone.transaction.revision,
            idempotency_key: "promote".into(),
        })
        .unwrap();
        assert_eq!(nav(&app).undo.unwrap().transaction_id, edit.transaction_id);
        step(&mut app, true, "visual-undo");
        assert!(nav(&app).undo.is_none());
        step(&mut app, false, "visual-redo");
        assert_eq!(nav(&app).undo.unwrap().transaction_id, edit.transaction_id);
    }

    #[test]
    fn fixed_navigation_does_not_bypass_revision_or_authorization_checks() {
        use opencut_editor_application::access::*;
        let mut app = EditorApplication::open(":memory:").unwrap();
        create(&mut app, "p");
        let other = create(&mut app, "other");
        rename(&mut app, "A");
        let stale = nav(&app);
        rename(&mut app, "B");
        assert_eq!(
            app.apply_v3_undo(&UndoRequest {
                project_id: "p".into(),
                expected_revision: stale.revision,
                target_transaction_id: stale.undo.unwrap().transaction_id,
                idempotency_key: "stale".into()
            })
            .unwrap_err()
            .code,
            "REVISION_CONFLICT"
        );
        assert_eq!(
            app.history_navigation(&HistoryNavigationInput {
                project_id: "p".into(),
                revision: Some(other)
            })
            .unwrap_err()
            .code,
            "NOT_FOUND"
        );
        app.set_principal_policy(PrincipalPolicy {
            principal_id: "reader".into(),
            expected_revision: "0".into(),
            enabled: true,
            grants: vec![ProjectGrant {
                project_id: "p".into(),
                actions: vec![ProjectAction::Read],
            }],
        })
        .unwrap();
        let issued = app.issue_credential("reader").unwrap();
        let caller = app.authenticate(&issued.secret).unwrap();
        app.set_caller(caller.clone());
        assert!(nav(&app).undo.is_some());
        app.set_caller(Caller::LocalOwner);
        app.revoke_credential(&issued.credential_id).unwrap();
        app.set_caller(caller);
        assert_eq!(
            app.history_navigation(&HistoryNavigationInput {
                project_id: "p".into(),
                revision: None
            })
            .unwrap_err()
            .code,
            "NOT_AUTHORIZED"
        );
    }
}
