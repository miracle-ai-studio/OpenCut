//! Public dispatch/queue tests only. Native codec and pixel evidence is separate.
use super::*;

fn shape(host: &mut Automation, scene_effect: bool) -> Value {
    let before = create(host);
    let mut request = json!({"project_id":"p","expected_revision":before["revision"],"sequence_id":"main","idempotency_key":"shape",
        "command":{"kind":"defaults","geometry":"rectangle","start":{"kind":"exact","time":{"ticks":"0","timebase":1}}}});
    let defaults = data(host.call("opencut_graphic_v1_plan", request.clone()));
    let initial = &defaults["initial"];
    request["command"] = json!({"kind":"create","element_id":"shape","track":{"kind":"new","track_id":"graphics"},"values":{
        "name":"Shape","graphic":initial["graphic"],"transform":initial["transform"],"opacity":initial["opacity"],
        "start":{"kind":"exact","time":{"ticks":"0","timebase":1}},"duration":{"kind":"exact","time":{"ticks":"1","timebase":5}}}});
    let planned = data(host.call("opencut_graphic_v1_plan", request));
    let receipt = data(host.call("opencut_edit_v4_apply", planned["transaction"].clone()));
    if !scene_effect {
        return receipt;
    }
    let current = inspect(host);
    let mut element = current["project"]["sequences"][0]["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "graphics")
        .unwrap()["elements"][0]
        .clone();
    element["visual"]["effects"] =
        json!([{"id":"blur","enabled":true,"effect":{"kind":"classic_blur","intensity":12}}]);
    apply(
        host,
        "effect",
        vec![
            json!({"type":"element_put","sequence_id":"main","track_id":"graphics","element":element}),
        ],
    )
}

#[test]
fn configurable_export_public_and_domain_dispatch_keep_fixed_options_and_authority() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = Automation::open(directory.path().join("workspace.sqlite")).unwrap();
    let revision = shape(&mut host, false)["revision"].clone();
    let request = json!({"project_id":"p","revision":revision,"sequence_id":"main","idempotency_key":"video-v2",
        "options":{"format":"webm","quality":"very_high","include_audio":false}});
    let job = data(host.call(
        "opencut_task",
        json!({"action":"export.video.v2","input":request}),
    ));
    assert_eq!(job["kind"], "render.export_video.v2");
    assert_eq!(job["target"]["revision"], revision);
    assert_eq!(
        data(host.call("opencut_job_export_video_v2", request.clone())),
        job
    );
    no_locator(&job, directory.path());
    let private = host.app.get_job(job["job_id"].as_str().unwrap()).unwrap();
    assert_eq!(private.input["options"], request["options"]);
    let mut changed = request.clone();
    changed["options"]["quality"] = json!("low");
    assert!(
        host.call("opencut_job_export_video_v2", changed)
            .error
            .is_some()
    );
    let capability = data(host.call(
        "opencut_task",
        json!({"action":"export.video.v2.capability","input":{}}),
    ));
    assert_eq!(capability["schema_version"], 2);
    assert_eq!(capability["submit_tool"], "opencut_job_export_video_v2");
    policy(&mut host, "0", vec![ProjectAction::Read]);
    let credential = host.app.issue_credential("agent").unwrap();
    host.caller = host.app.authenticate(&credential.secret).unwrap();
    error(
        host.call("opencut_job_export_video_v2", request),
        "NOT_AUTHORIZED",
    );
}

#[test]
fn scene_png_video_and_configurable_export_select_the_complete_fixed_scene() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = Automation::open(directory.path().join("workspace.sqlite")).unwrap();
    let revision = shape(&mut host, true)["revision"].clone();
    let frame = json!({"project_id":"p","revision":revision,"sequence_id":"main","idempotency_key":"scene-frame","time":{"ticks":"0","timebase":1}});
    let video = json!({"project_id":"p","revision":revision,"sequence_id":"main","idempotency_key":"scene-video"});
    for (tool, request, kind) in [
        (
            "opencut_job_v4_scene_frame_png",
            frame.clone(),
            "render.project_scene_frame.v4",
        ),
        (
            "opencut_job_v4_scene_video_mp4",
            video.clone(),
            "render.project_scene_video.v4",
        ),
    ] {
        let job = data(host.call(tool, request.clone()));
        assert_eq!(job["kind"], kind);
        assert_eq!(job["target"]["revision"], revision);
        assert_eq!(data(host.call(tool, request)), job);
        no_locator(&job, directory.path());
    }
    assert!(
        host.call("opencut_job_v4_mask_frame_png", frame.clone())
            .error
            .is_some()
    );
    assert!(
        host.call("opencut_job_v4_mask_video_mp4", video.clone())
            .error
            .is_some()
    );
    let request = json!({"project_id":"p","revision":revision,"sequence_id":"main","idempotency_key":"scene-export",
        "options":{"format":"mp4","quality":"medium","include_audio":false}});
    let job = data(host.call("opencut_job_export_video_v2", request));
    let private = host.app.get_job(job["job_id"].as_str().unwrap()).unwrap();
    assert_eq!(private.input["visual_profile"], "scene_v1");
    policy(&mut host, "0", vec![ProjectAction::Read]);
    let credential = host.app.issue_credential("agent").unwrap();
    host.caller = host.app.authenticate(&credential.secret).unwrap();
    error(
        host.call("opencut_job_v4_scene_frame_png", frame),
        "NOT_AUTHORIZED",
    );
    error(
        host.call("opencut_job_v4_scene_video_mp4", video),
        "NOT_AUTHORIZED",
    );
}
