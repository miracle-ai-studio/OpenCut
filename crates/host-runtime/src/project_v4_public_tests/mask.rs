use super::*;

#[test]
fn mask_geometry_and_jobs_dispatch_with_fixed_revision_and_original_authority() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = Automation::open(dir.path().join("workspace.sqlite")).unwrap();
    let before = create(&mut host);
    let base = json!({"project_id":"p","expected_revision":before["revision"],"sequence_id":"main","idempotency_key":"shape",
        "command":{"kind":"defaults","geometry":"rectangle","start":{"kind":"exact","time":{"ticks":"0","timebase":1}}}});
    let defaults = data(host.call("opencut_graphic_v1_plan", base.clone()));
    let initial = &defaults["initial"];
    let mut request = base;
    request["command"] = json!({"kind":"create","element_id":"rect","track":{"kind":"new","track_id":"graphics"},"values":{
        "name":"Rectangle","graphic":initial["graphic"],"transform":initial["transform"],"opacity":initial["opacity"],
        "start":{"kind":"exact","time":initial["timeline_range"]["start"]},"duration":{"kind":"exact","time":initial["timeline_range"]["duration"]}}});
    let planned = data(host.call("opencut_graphic_v1_plan", request));
    data(host.call("opencut_edit_v4_apply", planned["transaction"].clone()));
    let current = inspect(&mut host);
    let geometry = json!({"project_id":"p","expected_revision":current["revision"],"sequence_id":"main","element_id":"rect","time":{"ticks":"0","timebase":1}});
    let logical = data(host.call("opencut_mask_v1_geometry", geometry.clone()));
    assert_eq!(logical["revision"], current["revision"]);
    assert_eq!(inspect(&mut host), current, "geometry is read only");
    no_locator(&logical, dir.path());
    let mut element = current["project"]["sequences"][0]["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "graphics")
        .unwrap()["elements"][0]
        .clone();
    element["visual"]["masks"] = json!([{"id":"mask","enabled":true,"shape":{"kind":"rectangle","geometry":logical["default_geometry"]},
        "inverted":false,"feather":{"unit":"classic_pixels","value":12},
        "stroke":{"width_px":0,"align":"inside","color":{"encoding":"srgb","red":0,"green":0,"blue":0,"alpha":1}}}]);
    let receipt = apply(
        &mut host,
        "mask",
        vec![
            json!({"type":"element_put","sequence_id":"main","track_id":"graphics","element":element}),
        ],
    );
    error(
        host.call("opencut_mask_v1_geometry", geometry.clone()),
        "REVISION_CONFLICT",
    );
    let frame = json!({"project_id":"p","revision":receipt["revision"],"sequence_id":"main","time":{"ticks":"0","timebase":1},"idempotency_key":"mask-png"});
    let video = json!({"project_id":"p","revision":receipt["revision"],"sequence_id":"main","idempotency_key":"mask-video"});
    for (tool, input, kind) in [
        (
            "opencut_job_v4_mask_frame_png",
            frame.clone(),
            "render.project_mask_frame.v4",
        ),
        (
            "opencut_job_v4_mask_video_mp4",
            video.clone(),
            "render.project_mask_video.v4",
        ),
    ] {
        let job = data(host.call(tool, input.clone()));
        assert_eq!(job["kind"], kind);
        assert_eq!(job["target"]["revision"], receipt["revision"]);
        assert_eq!(data(host.call(tool, input)), job);
        no_locator(&job, dir.path());
    }
    policy(&mut host, "0", vec![ProjectAction::Read]);
    let credential = host.app.issue_credential("agent").unwrap();
    host.caller = host.app.authenticate(&credential.secret).unwrap();
    let mut geometry = geometry;
    geometry["expected_revision"] = receipt["revision"].clone();
    error(
        host.call("opencut_mask_v1_geometry", geometry),
        "NOT_AUTHORIZED",
    );
    error(
        host.call("opencut_job_v4_mask_frame_png", frame),
        "NOT_AUTHORIZED",
    );
    error(
        host.call("opencut_job_v4_mask_video_mp4", video),
        "NOT_AUTHORIZED",
    );
}
