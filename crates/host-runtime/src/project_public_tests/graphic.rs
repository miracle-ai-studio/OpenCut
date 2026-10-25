use super::*;

#[test]
fn graphic_public_plan_apply_and_fixed_frame_dispatch_share_original_authority() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = Automation::open(dir.path().join("workspace.sqlite")).unwrap();
    let before = create(&mut host);
    let base = json!({"project_id":"p","expected_revision":before["revision"],"sequence_id":"main","idempotency_key":"shape",
        "command":{"kind":"defaults","geometry":"star","start":{"kind":"exact","time":{"ticks":"0","timebase":1}}}});
    let defaults = data(host.call(
        "opencut_edit",
        json!({"action":"graphic.plan","input":base}),
    ));
    assert_eq!(defaults["initial"]["graphic"]["geometry"]["kind"], "star");
    assert!(defaults["transaction"].is_null());
    assert_eq!(inspect(&mut host), before);
    let initial = &defaults["initial"];
    let mut request = base.clone();
    request["command"] = json!({"kind":"create","element_id":"star","track":{"kind":"new","track_id":"graphics"},"values":{
        "name":"Star","graphic":initial["graphic"],"transform":initial["transform"],"opacity":initial["opacity"],
        "start":{"kind":"exact","time":initial["timeline_range"]["start"]},"duration":{"kind":"exact","time":initial["timeline_range"]["duration"]}}});
    let planned = data(host.call("opencut_graphic_plan", request.clone()));
    no_locator(&planned, dir.path());
    let receipt = data(host.call("opencut_edit_apply", planned["transaction"].clone()));
    assert_eq!(
        data(host.call("opencut_edit_apply", planned["transaction"].clone())),
        receipt
    );
    let frame = json!({"project_id":"p","revision":receipt["revision"],"sequence_id":"main","time":{"ticks":"0","timebase":1},"idempotency_key":"graphic-png"});
    let job = data(host.call("opencut_job_graphic_frame_png", frame.clone()));
    assert_eq!(job["kind"], "render.project_graphic_frame.v4");
    assert_eq!(job["target"]["revision"], receipt["revision"]);
    assert_eq!(
        data(host.call("opencut_job_graphic_frame_png", frame.clone())),
        job
    );
    no_locator(&job, dir.path());
    let video = json!({"project_id":"p","revision":receipt["revision"],"sequence_id":"main","idempotency_key":"graphic-video"});
    let video_job = data(host.call("opencut_job_graphic_video_mp4", video.clone()));
    assert_eq!(video_job["kind"], "render.project_graphic_video.v4");
    assert_eq!(video_job["target"]["revision"], receipt["revision"]);
    assert_eq!(
        data(host.call("opencut_job_graphic_video_mp4", video.clone())),
        video_job
    );
    no_locator(&video_job, dir.path());
    policy(&mut host, "0", vec![ProjectAction::Read]);
    let credential = host.app.issue_credential("agent").unwrap();
    host.caller = host.app.authenticate(&credential.secret).unwrap();
    request["expected_revision"] = receipt["revision"].clone();
    error(host.call("opencut_graphic_plan", request), "NOT_AUTHORIZED");
    error(
        host.call("opencut_job_graphic_frame_png", frame),
        "NOT_AUTHORIZED",
    );
    error(
        host.call("opencut_job_graphic_video_mp4", video),
        "NOT_AUTHORIZED",
    );
}
