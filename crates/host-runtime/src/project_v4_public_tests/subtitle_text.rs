use super::*;
fn seed(host: &mut Automation) -> Value {
    let source = json!({"version":31,"metadata":{"id":"classic","name":"Subtitles"},"settings":{"fps":{"numerator":25,"denominator":1},"canvasSize":{"width":64,"height":48},"background":{"type":"color","color":"#000000"}},"scenes":[{"id":"main","name":"Main","isMain":true,"bookmarks":[],"tracks":{"main":{"id":"video","name":"Video","type":"video","elements":[]},"overlay":[{"id":"subs","name":"Subtitles","type":"text","elements":[{"id":"cue","name":"Cue","type":"text","startTime":0,"duration":120000,"trimStart":0,"trimEnd":0,"params":{"content":"Hello\nworld","fontFamily":"Lato","fontSize":12,"color":"#ffffff","textAlign":"center"}}]}],"audio":[]}}]});
    let input = json!({"content":source.to_string(),"target_project_id":"p","media_mappings":[],"font_mappings":[]});
    let analysis = data(host.call("opencut_classic_analyze_v4", input.clone()));
    assert_eq!(analysis["blockers"], json!([]));
    let losses: Vec<_> = analysis["losses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["code"].clone())
        .collect();
    data(host.call("opencut_classic_import_v4",json!({"source":input,"expected_source_content_id":analysis["source_content_id"],"idempotency_key":"seed-subtitles","accepted_loss_codes":losses})));
    inspect(host)
}
fn plan(snapshot: &Value) -> Value {
    let mut style = snapshot["project"]["sequences"][0]["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "subs")
        .unwrap()["elements"][0]["content"]["text"]
        .clone();
    style["content"] = json!("");
    style["font"] = json!({"kind":"locked","font_id":"missing-font"});
    json!({"project_id":"p","expected_revision":snapshot["revision"],"sequence_id":"main","idempotency_key":"plan-subtitles","track":{"kind":"existing","track_id":"subs"},"source":{"kind":"file","format":"srt","content":"1\n00:00:00,000 --> 00:00:01,000\nHello\n\n"},"style":style,"max_width_px":null,"observations":null})
}
fn unchanged(host: &mut Automation, before: &Value, history: &Value) {
    assert_eq!(inspect(host), *before);
    assert_eq!(
        data(host.call(
            "opencut_project_v4_history_navigation",
            json!({"project_id":"p","revision":null})
        )),
        *history
    );
}
#[test]
fn public_plan_errors_are_read_only_and_domain_dispatch_is_registered() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = Automation::open(dir.path().join("workspace.sqlite")).unwrap();
    let before = seed(&mut host);
    let history = data(host.call(
        "opencut_project_v4_history_navigation",
        json!({"project_id":"p","revision":null}),
    ));
    let input = plan(&before);
    error(
        host.call("opencut_subtitle_text_v1_plan", input.clone()),
        "INVALID_PROJECT",
    );
    let mut wrong = input.clone();
    wrong["track"]["track_id"] = json!("video");
    error(
        host.call(
            "opencut_edit",
            json!({"action":"subtitle.text.v1.plan","input":wrong}),
        ),
        "INVALID_SUBTITLE_TEXT_PLAN",
    );
    let mut stale = input.clone();
    stale["expected_revision"] = json!("stale");
    error(
        host.call("opencut_subtitle_text_v1_plan", stale),
        "REVISION_CONFLICT",
    );
    let mut missing_job = input;
    missing_job["source"] =
        json!({"kind":"transcript","analysis_job_id":"missing-job","words_per_caption":3});
    error(
        host.call("opencut_subtitle_text_v1_plan", missing_job),
        "NOT_FOUND",
    );
    unchanged(&mut host, &before, &history);
}
#[test]
fn readonly_export_is_scoped_and_plan_requires_edit() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = Automation::open(dir.path().join("workspace.sqlite")).unwrap();
    let before = seed(&mut host);
    let request = json!({"project_id":"p","revision":before["revision"],"sequence_id":"main","track_id":"subs","format":"srt"});
    let owner = data(host.call("opencut_subtitle_text_v1_export", request.clone()));
    assert_eq!(owner["cue_count"], 1);
    assert!(owner["content"].as_str().unwrap().contains("Hello\nworld"));
    policy(&mut host, "0", vec![ProjectAction::Read]);
    let credential = host.app.issue_credential("agent").unwrap();
    host.caller = host.app.authenticate(&credential.secret).unwrap();
    assert_eq!(
        data(host.call(
            "opencut_edit",
            json!({"action":"subtitle.text.v1.export","input":request.clone()})
        )),
        owner
    );
    error(
        host.call("opencut_subtitle_text_v1_plan", plan(&before)),
        "NOT_AUTHORIZED",
    );
    let mut wrong = request.clone();
    wrong["track_id"] = json!("video");
    error(
        host.call("opencut_subtitle_text_v1_export", wrong),
        "SUBTITLE_INVALID",
    );
    let mut absent = request;
    absent["revision"] = json!("absent");
    error(
        host.call("opencut_subtitle_text_v1_export", absent),
        "NOT_FOUND",
    );
    host.caller = Caller::LocalOwner;
    policy(&mut host, "1", vec![]);
    host.caller = host.app.authenticate(&credential.secret).unwrap();
    error(host.call("opencut_subtitle_text_v1_export",json!({"project_id":"p","revision":before["revision"],"sequence_id":"main","track_id":"subs","format":"srt"})),"NOT_AUTHORIZED");
}
