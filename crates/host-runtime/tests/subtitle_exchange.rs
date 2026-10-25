use opencut_host_runtime::Automation;
use serde_json::{Value, json};
fn call(host: &mut Automation, name: &str, input: Value) -> Value {
    let response = serde_json::to_value(host.call(name, input)).unwrap();
    assert!(response["error"].is_null(), "{name}: {response}");
    response["data"].clone()
}
#[test]
fn subtitles_share_exchange_contract_with_visual_projects_and_idempotent_history() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = Automation::open(directory.path().join("workspace.sqlite")).unwrap();
    let created = call(
        &mut host,
        "opencut_project_create",
        json!({"project":{"id":"visual","name":"Visual","sequence_id":"main","width":640,"height":360,"fps":{"numerator":24,"denominator":1}},"idempotency_key":"create"}),
    );
    let request = json!({"project_id":"visual","expected_revision":created["revision"],"sequence_id":"main","idempotency_key":"subtitles","format":"srt","content":"1\n00:00:00,125 --> 00:00:01,250\nShared caption\n","style":null});
    let imported = call(&mut host, "opencut_subtitle_import", request.clone());
    assert_eq!(imported["cue_count"], 1);
    assert_eq!(
        call(&mut host, "opencut_subtitle_import", request),
        imported
    );
    let exported = call(
        &mut host,
        "opencut_subtitle_export",
        json!({"project_id":"visual","revision":imported["transaction"]["revision"],"sequence_id":"main","format":"srt"}),
    );
    assert!(
        exported["content"]
            .as_str()
            .unwrap()
            .contains("00:00:00,125 --> 00:00:01,250\nShared caption")
    );
    let original = call(
        &mut host,
        "opencut_project_inspect",
        json!({"project_id":"visual","revision":created["revision"]}),
    );
    assert_eq!(
        original["project"]["base"]["sequences"][0]["caption_cues"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let history = call(
        &mut host,
        "opencut_project_history",
        json!({"project_id":"visual","from_revision":null,"limit":50}),
    );
    assert_eq!(history["revisions"].as_array().unwrap().len(), 2);
}
