//! New profile, same durable original-caller and publication fences.
use super::*;

#[test]
fn scene_fixed_frame_keeps_original_request_on_reopen_and_checks_lease_kind() {
    let mut f = graphic_tests::graphic_fixture();
    let request = f.request("scene-fixed");
    let job = start_scene_frame_png(&mut f.app, request.clone()).unwrap();
    assert_eq!(job.kind, "render.project_scene_frame.v4");
    let prepared: PreparedTextFramePng = serde_json::from_value(job.input.clone()).unwrap();
    scene::validate_prepared(&prepared).unwrap();
    let mut changed = request.clone();
    changed.time.ticks = "2".into();
    let before = counts(&f.app);
    assert!(start_scene_frame_png(&mut f.app, changed).is_err());
    assert_eq!(counts(&f.app), before);
    let mut reopened =
        EditorApplication::open(f.directory.path().join("workspace.sqlite")).unwrap();
    assert_eq!(start_scene_frame_png(&mut reopened, request).unwrap(), job);
    let running = f.claim(&job);
    f.app
        .revalidate_scene_scene_frame_job(
            &job.job_id,
            "frame-worker",
            running.attempt,
            &prepared.authority,
        )
        .unwrap();
    assert!(
        f.app
            .revalidate_scene_scene_frame_job(
                &job.job_id,
                "other-worker",
                running.attempt,
                &prepared.authority
            )
            .is_err()
    );
    assert!(
        f.app
            .revalidate_scene_mask_frame_job(
                &job.job_id,
                "frame-worker",
                running.attempt,
                &prepared.authority
            )
            .is_err()
    );
    f.app.cancel_job(&job.job_id).unwrap();
    let done = f
        .app
        .complete_job(
            &job.job_id,
            "frame-worker",
            running.attempt,
            now(),
            Value::Null,
        )
        .unwrap();
    assert_eq!(done.state, JobState::Cancelled);
    assert!(done.result.is_none());
    assert!(f.app.job_view(&job.job_id).unwrap().artifacts.is_empty());
}
