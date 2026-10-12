use super::*;

#[test]
fn request_binding_head_races_and_cross_family_keys_are_checked() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workspace.sqlite");
    let mut app = EditorApplication::open(&path).unwrap();
    let initial = app.create_v4_project(&create_request("p")).unwrap();
    let request = edit(&initial, "prepared-a", vec![bookmark("a")]);
    let payload = prepared(&mut app, &request);
    let other_request = edit(&initial, "prepared-b", vec![bookmark("b")]);
    let before = counts(&app);
    assert_eq!(
        app.commit_v4_atomic(V4Intent::Apply(&other_request), payload)
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
    assert_eq!(counts(&app), before);
    let payload = prepared(&mut app, &request);
    let mut other = EditorApplication::open(&path).unwrap();
    let advance = edit(
        &initial,
        "advance",
        vec![ProjectOperationV4::Rename {
            name: "New head".into(),
        }],
    );
    other.apply_v4_project(&advance).unwrap();
    let before = counts(&app);
    assert_eq!(
        app.commit_v4_atomic(V4Intent::Apply(&request), payload)
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
    assert_eq!(counts(&app), before);
    let current = inspect(&app, "p");
    let request = edit(&current, "shared-key", vec![bookmark("committed")]);
    let result = app.apply_v4_project(&request).unwrap();
    assert_eq!(app.apply_v4_project(&request).unwrap(), result);
    let restore = RestoreV4Revision {
        project_id: "p".into(),
        expected_revision: result.revision.clone(),
        target_revision: initial.revision,
        idempotency_key: "shared-key".into(),
        asset_versions: BTreeMap::new(),
    };
    assert_eq!(
        app.restore_v4_revision(&restore).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    let legacy = TransactionRequest {
        expected_revision: result.revision,
        idempotency_key: "shared-key".into(),
        label: "legacy-family".into(),
        operations: vec![opencut_contracts::Operation::UpdateSequenceSettings {
            sequence_id: "main".into(),
            name: Some("No projection".into()),
            width: None,
            height: None,
            fps: None,
        }],
    };
    assert_eq!(
        app.apply("p", &legacy).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
}

#[test]
fn policy_revocation_and_owner_hold_between_prepare_and_commit_publish_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workspace.sqlite");
    let mut app = EditorApplication::open(&path).unwrap();
    let initial = app.create_v4_project(&create_request("p")).unwrap();
    let (caller, credential_id) = credential(&mut app, "p");
    app.set_caller(caller.clone());
    let request = edit(&initial, "held", vec![bookmark("marker")]);
    let payload = prepared(&mut app, &request);
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
        app.commit_v4_atomic(V4Intent::Apply(&request), payload)
            .unwrap_err()
            .code,
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
    let payload = prepared(&mut app, &request);
    owner.revoke_credential(&credential_id).unwrap();
    let before = counts(&app);
    assert_eq!(
        app.commit_v4_atomic(V4Intent::Apply(&request), payload)
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(counts(&app), before);
    app.set_caller(Caller::LocalOwner);
    assert_eq!(inspect(&app, "p"), initial);
    // A matching receipt does not revive a credential after revocation.
    let result = app.apply_v4_project(&request).unwrap();
    app.set_caller(caller);
    assert_eq!(
        app.apply_v4_project(&request).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    app.set_caller(Caller::LocalOwner);
    assert_eq!(inspect(&app, "p").revision, result.revision);
}

#[test]
fn restricted_resources_cursor_and_deleted_project_cannot_cross_scope() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(":memory:").unwrap();
    let a = app.create_v4_project(&create_request("a")).unwrap();
    let b = app.create_v4_project(&create_request("b")).unwrap();
    let resource = capture_asset(
        &app,
        directory.path(),
        "private-b",
        b"project b private CAS",
    );
    app.apply_v4_project(&edit(
        &b,
        "bind-b",
        vec![ProjectOperationV4::AssetPut {
            asset: resource.clone(),
        }],
    ))
    .unwrap();
    let owner_cursor = app
        .list_v4_projects(&ListV4Projects {
            cursor: None,
            limit: 1,
        })
        .unwrap()
        .next_cursor
        .unwrap();
    let (caller, _) = credential(&mut app, "a");
    app.set_caller(caller);
    let before = counts(&app);
    assert_eq!(
        app.apply_v4_project(&edit(
            &a,
            "cross-resource",
            vec![ProjectOperationV4::AssetPut { asset: resource }]
        ))
        .unwrap_err()
        .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(counts(&app), before);
    assert_eq!(
        app.list_v4_projects(&ListV4Projects {
            cursor: Some(owner_cursor),
            limit: 1
        })
        .unwrap_err()
        .code,
        "INVALID_CURSOR"
    );
    let page = app
        .list_v4_projects(&ListV4Projects {
            cursor: None,
            limit: 10,
        })
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].project_id, "a");
    app.set_caller(Caller::LocalOwner);
    // Tombstone is the same canonical delete state used by all project tools.
    app.connection.execute("INSERT INTO deleted_projects(project_id,revision,request_id) VALUES (?1,?2,'fixture-delete')", params![a.project.id,a.revision]).unwrap();
    assert_eq!(
        app.inspect_v4_project(&InspectV4Project {
            project_id: "a".into(),
            revision: None
        })
        .unwrap_err()
        .code,
        "NOT_FOUND"
    );
    assert_eq!(
        app.apply_v4_project(&edit(&a, "deleted-edit", vec![bookmark("blocked")]))
            .unwrap_err()
            .code,
        "NOT_FOUND"
    );
    assert_eq!(
        app.create_v4_project(&create_request("a"))
            .unwrap_err()
            .code,
        "NOT_FOUND"
    );
    assert_eq!(
        app.list_v4_projects(&ListV4Projects {
            cursor: None,
            limit: 10
        })
        .unwrap()
        .items
        .len(),
        1
    );
}
