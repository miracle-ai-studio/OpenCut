use super::*;
#[test]
fn restore_permission_and_current_owner_hold_are_enforced_before_replay() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workspace.sqlite");
    let mut app = EditorApplication::open(&path).unwrap();
    create(&mut app);
    let target = edit(&mut app, "target", vec![marker("target")]);
    let request = undo(&app, &target.transaction_id, "undo");
    app.set_principal_policy(PrincipalPolicy {
        principal_id: "agent".into(),
        expected_revision: "0".into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: "p".into(),
            actions: vec![ProjectAction::Read, ProjectAction::Edit],
        }],
    })
    .unwrap();
    let issued = app.issue_credential("agent").unwrap();
    let caller = app.authenticate(&issued.secret).unwrap();
    app.set_caller(caller.clone());
    let before = counts(&app);
    assert_eq!(
        app.apply_v4_undo(&request).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(counts(&app), before);
    app.set_caller(Caller::LocalOwner);
    app.set_principal_policy(PrincipalPolicy {
        principal_id: "agent".into(),
        expected_revision: "1".into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: "p".into(),
            actions: vec![ProjectAction::Read, ProjectAction::Restore],
        }],
    })
    .unwrap();
    app.set_caller(caller.clone());
    let result = app.apply_v4_undo(&request).unwrap();
    let mut owner = EditorApplication::open(&path).unwrap();
    owner
        .set_edit_control(&opencut_api_contracts::edit_control::EditControlSet {
            project_id: "p".into(),
            expected_generation: "0".into(),
            held: true,
            idempotency_key: "hold".into(),
        })
        .unwrap();
    let before = counts(&app);
    assert_eq!(
        app.apply_v4_undo(&request).unwrap_err().code,
        "EDIT_HELD_BY_OWNER"
    );
    assert_eq!(counts(&app), before);
    owner
        .set_edit_control(&opencut_api_contracts::edit_control::EditControlSet {
            project_id: "p".into(),
            expected_generation: "1".into(),
            held: false,
            idempotency_key: "release".into(),
        })
        .unwrap();
    owner.revoke_credential(&issued.credential_id).unwrap();
    assert_eq!(
        app.apply_v4_undo(&request).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    app.set_caller(Caller::LocalOwner);
    assert_eq!(inspect(&app).revision, result.transaction.revision);
}
#[test]
fn causal_target_scope_and_cross_family_keys_do_not_publish_and_audit_failure_rolls_back() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    create(&mut app);
    let original = edit(&mut app, "target", vec![marker("target")]);
    let mut wrong = undo(&app, &original.transaction_id, "wrong");
    wrong.target_transaction_id = "missing".into();
    assert_eq!(
        app.apply_v4_undo(&wrong).unwrap_err().code,
        "CAUSAL_CONFLICT"
    );
    let mut reused = undo(&app, &original.transaction_id, "target");
    assert_eq!(
        app.apply_v4_undo(&reused).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    reused.idempotency_key = "rollback".into();
    let snapshot = inspect(&app);
    let before = counts(&app);
    app.connection.execute_batch("CREATE TRIGGER reject_undo_audit BEFORE INSERT ON audit_records WHEN NEW.action='project.v4_selective_undo' BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
    assert_eq!(
        app.apply_v4_undo(&reused).unwrap_err().code,
        "STORAGE_ERROR"
    );
    assert_eq!(inspect(&app), snapshot);
    assert_eq!(counts(&app), before);
    app.connection
        .execute_batch("DROP TRIGGER reject_undo_audit;")
        .unwrap();
    let result = app.apply_v4_undo(&reused).unwrap();
    assert_eq!(result.transaction.base_revision, snapshot.revision);
    let mut same = redo(&app, &result.transaction.transaction_id, "rollback");
    assert_eq!(
        app.apply_v4_redo(&same).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    same.idempotency_key = "stale-head".into();
    same.expected_revision = snapshot.revision;
    assert_eq!(
        app.apply_v4_redo(&same).unwrap_err().code,
        "REVISION_CONFLICT"
    );
}
