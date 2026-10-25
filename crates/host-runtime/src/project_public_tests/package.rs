use super::*;

#[test]
fn package_public_roundtrip_and_original_replay_survive_package_removal() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = Automation::open(dir.path().join("workspace.sqlite")).unwrap();
    let original = create(&mut host);
    let destination = dir.path().join("portable");
    let exported = data(host.call(
        "opencut_project_package_export",
        json!({
            "project_id":"p", "revision":original["revision"], "destination":destination,"scope":"saved_revision","mode":"self_contained"
        }),
    ));
    assert_eq!(exported["package"]["mode"], "self_contained");
    assert_eq!(exported["package"]["scope"], "saved_revision");
    let inspected = data(host.call(
        "opencut_project",
        json!({"action":"package.inspect", "input":{"package_path":destination}}),
    ));
    assert_eq!(
        inspected["manifest_sha256"],
        exported["package"]["manifest_sha256"]
    );
    let request = json!({"package_path":destination,"target_project_id":"restored", "expected_manifest_sha256":exported["package"]["manifest_sha256"],"idempotency_key":"package-fixed"});
    let receipt = data(host.call("opencut_project_package_import", request.clone()));
    let restored = data(host.call(
        "opencut_project_inspect",
        json!({"project_id":"restored","revision":null}),
    ));
    assert_eq!(restored["kind"], "editor");
    let restored = restored["snapshot"].clone();
    let mut expected = original["project"].clone();
    expected["id"] = json!("restored");
    assert_eq!(restored["project"], expected);
    assert_eq!(inspect(&mut host), original);
    std::fs::remove_dir_all(&destination).unwrap();
    assert_eq!(
        data(host.call("opencut_project_package_import", request.clone())),
        receipt
    );
    let mut conflict = request.clone();
    conflict["target_project_id"] = json!("other");
    error(
        host.call("opencut_project_package_import", conflict),
        "IDEMPOTENCY_KEY_REUSED",
    );
    policy(&mut host, "0", vec![ProjectAction::Read]);
    let credential = host.app.issue_credential("agent").unwrap();
    host.caller = host.app.authenticate(&credential.secret).unwrap();
    for operation in [
        "opencut_project_package_choose_location",
        "opencut_project_package_export",
        "opencut_project_package_inspect",
        "opencut_project_package_import",
    ] {
        error(host.call(operation, json!({})), "NOT_AUTHORIZED");
    }
}
