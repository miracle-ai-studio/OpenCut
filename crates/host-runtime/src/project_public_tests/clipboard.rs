//! Real public dispatch/SQLite publication; synthetic existing CAS bytes test
//! resource identity and edits, not video decode or renderer parity.
use super::*;

fn copy_input(snapshot: &Value, track_id: &str) -> Value {
    json!({"project_id":"p","revision":snapshot["revision"],"sequence_id":"main",
        "targets":[{"track_id":track_id,"element_id":"video"}]})
}
fn paste_input(snapshot: &Value, clipboard: &Value) -> Value {
    json!({"project_id":"p","expected_revision":snapshot["revision"],"sequence_id":"destination",
        "idempotency_key":"clipboard-paste","label":"Paste historical video",
        "clipboard":clipboard,"at":{"ticks":"1","timebase":7},
        "placements":[{"source_track_id":clipboard["items"][0]["source_track_id"],
            "target":{"kind":"new","track_id":"pasted-visual","name":"Pasted video","track_kind":"video","insertion_anchor":{"kind":"highest"}},
            "elements":[{"source_element_id":"video","new_element_id":"pasted-video"}]}]})
}
fn add_destination(automation: &mut Automation) {
    let current = inspect(automation);
    let mut scene = current["project"]["sequences"][0].clone();
    scene["id"] = json!("destination");
    scene["name"] = json!("Paste destination");
    scene["main_track_id"] = json!("destination-main");
    scene["tracks"][0]["id"] = json!("destination-main");
    scene["tracks"][0]["elements"] = json!([]);
    apply(
        automation,
        "clipboard-destination",
        vec![json!({"type":"sequence_put","sequence":scene})],
    );
}
fn receipt_count(db: &rusqlite::Connection) -> i64 {
    db.query_row("SELECT COUNT(*) FROM idempotency WHERE operation_family='project_v4_public_apply' AND scope='p' AND idempotency_key='clipboard-paste'",[],|row|row.get(0)).unwrap()
}

#[test]
fn clipboard_public_fixed_source_restore_is_one_atomic_commit_reopen_and_selective_undo_redo() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("workspace.sqlite");
    let mut automation = Automation::open(&database).unwrap();
    let (locator, track_id) = seed_source_video(&mut automation, directory.path());
    add_destination(&mut automation);
    let copied_head = inspect(&mut automation);
    let copy = copy_input(&copied_head, &track_id);
    let clipboard = data(automation.call("opencut_timeline_copy", copy.clone()));
    no_locator(&clipboard, &locator);
    assert_eq!(inspect(&mut automation), copied_head);
    let db = rusqlite::Connection::open(&database).unwrap();
    let raw_source: String = db
        .query_row(
            "SELECT document_json FROM revisions WHERE revision=?1",
            [copied_head["revision"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    // The current head no longer contains the resource or source element.
    // Its authorized historical revision still supplies both complete values.
    apply(
        &mut automation,
        "remove-current-source",
        vec![
            json!({"type":"element_delete","sequence_id":"main","element_id":"video"}),
            json!({"type":"asset_delete","asset_id":"asset"}),
        ],
    );
    let before = inspect(&mut automation);
    let input = paste_input(&before, &clipboard);
    let planned = data(automation.call("opencut_timeline_paste_plan", input.clone()));
    assert_eq!(
        data(automation.call("opencut_timeline_copy", copy)),
        clipboard
    );
    assert_eq!(
        data(automation.call("opencut_timeline_paste_plan", input.clone())),
        planned
    );
    assert_eq!(inspect(&mut automation), before);
    no_locator(&planned, &locator);
    assert_eq!(planned["decision"]["restored_asset_ids"], json!(["asset"]));
    assert_eq!(
        planned["transaction"]["operations"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        planned["transaction"]["operations"][0]["source"],
        json!({"kind":"project_asset","revision":copied_head["revision"],"asset_id":"asset"})
    );
    db.execute_batch("CREATE TRIGGER fail_clipboard BEFORE INSERT ON audit_records BEGIN SELECT RAISE(ABORT,'injected clipboard audit'); END;").unwrap();
    assert!(
        automation
            .call("opencut_edit_apply", planned["transaction"].clone())
            .error
            .is_some()
    );
    assert_eq!(inspect(&mut automation), before);
    assert_eq!(receipt_count(&db), 0);
    db.execute_batch("DROP TRIGGER fail_clipboard").unwrap();
    let committed = data(automation.call("opencut_edit_apply", planned["transaction"].clone()));
    assert_eq!(
        data(automation.call("opencut_edit_apply", planned["transaction"].clone())),
        committed
    );
    assert_eq!(receipt_count(&db), 1);
    let revision_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM revisions WHERE transaction_id=?1",
            [committed["transaction_id"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(revision_count, 1);
    error(
        automation.call("opencut_timeline_paste_plan", input),
        "REVISION_CONFLICT",
    );
    let after = inspect(&mut automation);
    assert_eq!(after["project"]["settings"], before["project"]["settings"]);
    assert_eq!(
        after["project"]["sequences"][0],
        before["project"]["sequences"][0]
    );
    assert_eq!(after["project"]["assets"], copied_head["project"]["assets"]);
    let cloned = &after["project"]["sequences"][1]["tracks"][1]["elements"][0];
    let original = &copied_head["project"]["sequences"][0]["tracks"][0]["elements"][0];
    let mut expected = original.clone();
    expected["id"] = json!("pasted-video");
    let actual_start: opencut_contracts::Time =
        serde_json::from_value(cloned["timeline_range"]["start"].clone()).unwrap();
    assert_eq!(
        opencut_time::RationalTime::try_from(&actual_start)
            .unwrap()
            .cmp_exact(opencut_time::RationalTime::new(1, 7).unwrap()),
        std::cmp::Ordering::Equal
    );
    expected["timeline_range"]["start"] = cloned["timeline_range"]["start"].clone();
    assert_eq!(cloned, &expected);
    drop(automation);
    let mut automation = Automation::open(&database).unwrap();
    assert_eq!(inspect(&mut automation), after);
    let undo = undo_input(
        &mut automation,
        &committed["transaction_id"],
        "undo-clipboard",
    );
    let undone = data(automation.call("opencut_edit_undo_apply", undo));
    assert_eq!(inspect(&mut automation)["project"], before["project"]);
    let current = inspect(&mut automation);
    let redo = json!({"project_id":"p","expected_revision":current["revision"],"undo_transaction_id":undone["transaction"]["transaction_id"],"idempotency_key":"redo-clipboard"});
    data(automation.call("opencut_edit_redo_apply", redo));
    assert_eq!(inspect(&mut automation)["project"], after["project"]);
    let current_raw: String = db
        .query_row(
            "SELECT document_json FROM revisions WHERE revision=?1",
            [copied_head["revision"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(current_raw, raw_source);
}

#[test]
fn clipboard_read_references_never_grant_edit_or_survive_live_read_and_credential_revocation() {
    let directory = tempfile::tempdir().unwrap();
    let mut automation = Automation::open(directory.path().join("workspace.sqlite")).unwrap();
    let (locator, track_id) = seed_source_video(&mut automation, directory.path());
    add_destination(&mut automation);
    policy(&mut automation, "0", vec![ProjectAction::Read]);
    let issued = automation.app.issue_credential("agent").unwrap();
    let caller = automation.app.authenticate(&issued.secret).unwrap();
    automation.caller = caller.clone();
    let fixed = inspect(&mut automation);
    let copy = copy_input(&fixed, &track_id);
    let clipboard = data(automation.call("opencut_timeline_copy", copy.clone()));
    let input = paste_input(&fixed, &clipboard);
    error(
        automation.call("opencut_timeline_paste_plan", input.clone()),
        "NOT_AUTHORIZED",
    );
    policy(
        &mut automation,
        "1",
        vec![ProjectAction::Read, ProjectAction::Edit],
    );
    automation.caller = caller.clone();
    let planned = data(automation.call("opencut_timeline_paste_plan", input.clone()));
    assert_eq!(inspect(&mut automation), fixed);
    no_locator(&planned, &locator);
    let mut collision = input.clone();
    collision["placements"][0]["elements"][0]["new_element_id"] = json!("asset");
    error(
        automation.call("opencut_timeline_paste_plan", collision),
        "INVALID_ARGUMENT",
    );
    let mut transfer = input.clone();
    transfer["clipboard"]["source_project_id"] = json!("another-project");
    error(
        automation.call("opencut_timeline_paste_plan", transfer),
        "UNSUPPORTED_CLIPBOARD_TRANSFER",
    );
    policy(&mut automation, "2", vec![]);
    automation.caller = caller.clone();
    error(
        automation.call("opencut_timeline_copy", copy.clone()),
        "NOT_AUTHORIZED",
    );
    error(
        automation.call("opencut_timeline_paste_plan", input.clone()),
        "NOT_AUTHORIZED",
    );
    error(
        automation.call("opencut_edit_apply", planned["transaction"].clone()),
        "NOT_AUTHORIZED",
    );
    policy(
        &mut automation,
        "3",
        vec![ProjectAction::Read, ProjectAction::Edit],
    );
    automation
        .app
        .revoke_credential(&issued.credential_id)
        .unwrap();
    automation.caller = caller;
    error(
        automation.call("opencut_timeline_copy", copy),
        "NOT_AUTHORIZED",
    );
    error(
        automation.call("opencut_timeline_paste_plan", input),
        "NOT_AUTHORIZED",
    );
    error(
        automation.call("opencut_edit_apply", planned["transaction"].clone()),
        "NOT_AUTHORIZED",
    );
    automation.caller = Caller::LocalOwner;
    assert_eq!(inspect(&mut automation), fixed);
}

#[test]
fn recovery_inspect_empty_public_envelope_preserves_live_edit_and_read_authority() {
    let directory = tempfile::tempdir().unwrap();
    let mut automation = Automation::open(directory.path().join("workspace.sqlite")).unwrap();
    let fixed = create(&mut automation);
    let request = json!({"project_id":"p","recovery_id":null});
    let expected = json!({"record":null});
    // Explicit absence is distinct from a missing transport envelope.
    assert_eq!(
        data(automation.call("opencut_edit_recovery_inspect", request.clone())),
        expected
    );
    policy(&mut automation, "0", vec![ProjectAction::Read]);
    let issued = automation.app.issue_credential("agent").unwrap();
    let caller = automation.app.authenticate(&issued.secret).unwrap();
    automation.caller = caller.clone();
    assert_eq!(
        data(automation.call("opencut_edit_recovery_inspect", request.clone())),
        expected
    );
    policy(
        &mut automation,
        "1",
        vec![ProjectAction::Read, ProjectAction::Edit],
    );
    automation.caller = caller.clone();
    assert_eq!(
        data(automation.call("opencut_edit_recovery_inspect", request.clone())),
        expected
    );
    policy(&mut automation, "2", vec![]);
    automation.caller = caller;
    error(
        automation.call("opencut_edit_recovery_inspect", request.clone()),
        "NOT_AUTHORIZED",
    );
    automation.caller = Caller::LocalOwner;
    assert_eq!(
        data(automation.call("opencut_edit_recovery_inspect", request)),
        expected
    );
    assert_eq!(inspect(&mut automation), fixed);
}
