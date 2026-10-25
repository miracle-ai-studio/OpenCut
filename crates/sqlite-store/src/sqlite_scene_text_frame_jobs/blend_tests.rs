use super::*;

use opencut_contracts::scene::BlendMode;
fn fixture() -> Fixture {
    let mut f = graphic_tests::graphic_fixture();
    let snapshot = f
        .app
        .inspect_scene_project(&InspectProject {
            project_id: "frame-project".into(),
            revision: None,
        })
        .unwrap();
    let track = snapshot.project.sequences[0]
        .tracks
        .iter()
        .find(|t| t.id == "graphics")
        .unwrap();
    let mut element = track.elements[0].clone();
    element.visual.as_mut().unwrap().blend_mode = BlendMode::Hue;
    f.revision = f
        .app
        .apply_scene_project(&ApplySceneProject {
            project_id: "frame-project".into(),
            expected_revision: f.revision.clone(),
            idempotency_key: "blend-edit".into(),
            label: "Hue".into(),
            operations: vec![ProjectOperation::ElementPut {
                sequence_id: "scene".into(),
                track_id: "graphics".into(),
                element: Box::new(element),
            }],
        })
        .unwrap()
        .revision;
    f
}
#[test]
fn blend_frame_pins_original_request_across_reopen_and_rejects_normal_profile() {
    let mut f = fixture();
    let request = f.request("blend-pinned");
    let job = start_blend_frame_png(&mut f.app, request.clone()).unwrap();
    assert_eq!(job.kind, "render.project_blend_frame.v4");
    let old = f.request("old-normal");
    assert!(start_graphic_frame_png(&mut f.app, old).is_err());
    f.app
        .apply_scene_project(&ApplySceneProject {
            project_id: "frame-project".into(),
            expected_revision: f.revision.clone(),
            idempotency_key: "later".into(),
            label: "Later".into(),
            operations: vec![ProjectOperation::Rename {
                name: "Later".into(),
            }],
        })
        .unwrap();
    let mut reopened =
        EditorApplication::open(f.directory.path().join("workspace.sqlite")).unwrap();
    assert_eq!(start_blend_frame_png(&mut reopened, request).unwrap(), job);
    assert_eq!(
        reopened
            .job_view(&job.job_id)
            .unwrap()
            .target
            .unwrap()
            .revision,
        f.revision
    );
}
#[test]
fn blend_frame_retains_original_credential_and_cancel_publication_fences() {
    for revoked in [false, true] {
        let mut f = fixture();
        let (caller, credential) = f.credential();
        f.app.set_caller(caller);
        let request = f.request("blend-fenced");
        let job = start_blend_frame_png(&mut f.app, request).unwrap();
        let running = f.claim(&job);
        let input: PreparedTextFramePng = serde_json::from_value(running.input.clone()).unwrap();
        f.app
            .revalidate_scene_blend_frame_job(
                &job.job_id,
                "frame-worker",
                running.attempt,
                &input.authority,
            )
            .unwrap();
        assert!(
            f.app
                .revalidate_scene_graphic_frame_job(
                    &job.job_id,
                    "frame-worker",
                    running.attempt,
                    &input.authority
                )
                .is_err()
        );
        if revoked {
            f.app.revoke_credential(&credential).unwrap();
            let before = counts(&f.app);
            assert_eq!(
                f.app
                    .complete_job(
                        &job.job_id,
                        "frame-worker",
                        running.attempt,
                        now(),
                        Value::Null
                    )
                    .unwrap_err()
                    .code,
                "NOT_AUTHORIZED"
            );
            assert_eq!(counts(&f.app), before);
            assert_eq!(f.app.get_job(&job.job_id).unwrap(), running);
        } else {
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
            assert!(
                done.result.is_none() && f.app.job_view(&job.job_id).unwrap().artifacts.is_empty()
            );
        }
    }
}
