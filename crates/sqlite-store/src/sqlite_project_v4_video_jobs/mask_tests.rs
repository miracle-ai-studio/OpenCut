use super::*;
use crate::sqlite_project_v4_mask_video_jobs::start_mask_video_mp4;
use opencut_contracts::project_v4::BlendModeV4;
fn fixture() -> Fixture {
    let mut f = graphic_tests::fixture();
    let snapshot = f
        .app
        .inspect_v4_project(&InspectV4Project {
            project_id: "audio-project".into(),
            revision: None,
        })
        .unwrap();
    let track = snapshot.project.sequences[0]
        .tracks
        .iter()
        .find(|t| t.id == "graphics")
        .unwrap();
    let mut element = track.elements[0].clone();
    element.visual.as_mut().unwrap().blend_mode = BlendModeV4::Hue;
    element.visual.as_mut().unwrap().masks.push(serde_json::from_value(serde_json::json!({
        "id":"mask", "enabled":true, "inverted":false,
        "shape":{"kind":"ellipse","geometry":{"center_x":0.1,"center_y":-0.1,"width":0.6,"height":0.4,"rotation_degrees":23.0,"scale":3.0}},
        "feather":{"unit":"classic_pixels","value":8.0},
        "stroke":{"color":{"encoding":"srgb","red":1.0,"green":1.0,"blue":1.0,"alpha":1.0},"width_px":0.0,"align":"center"}
    })).unwrap());
    f.revision = f
        .app
        .apply_v4_project(&ApplyV4Project {
            project_id: "audio-project".into(),
            expected_revision: f.revision.clone(),
            idempotency_key: "mask-edit".into(),
            label: "Hue".into(),
            operations: vec![ProjectOperationV4::ElementPut {
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
fn mask_video_pins_original_request_across_reopen_and_rejects_old_blend_profile() {
    let mut f = fixture();
    let request = f.request("mask-pinned");
    let job = start_mask_video_mp4(&mut f.app, request.clone()).unwrap();
    assert_eq!(job.kind, "render.project_mask_video.v4");
    let fixed: opencut_editor_application::project_v4_mask_video_jobs::PreparedV4MaskVideoMp4V1 =
        serde_json::from_value(job.input.clone()).unwrap();
    opencut_editor_application::project_v4_mask_video_jobs::validate_prepared(&fixed).unwrap();
    let old = f.request("old-normal");
    assert!(
        crate::sqlite_project_v4_blend_video_jobs::start_blend_video_mp4(&mut f.app, old).is_err()
    );
    f.app
        .apply_v4_project(&ApplyV4Project {
            project_id: "audio-project".into(),
            expected_revision: f.revision.clone(),
            idempotency_key: "later".into(),
            label: "Later".into(),
            operations: vec![ProjectOperationV4::Rename {
                name: "Later".into(),
            }],
        })
        .unwrap();
    let mut reopened =
        EditorApplication::open(f.directory.path().join("workspace.sqlite")).unwrap();
    assert_eq!(start_mask_video_mp4(&mut reopened, request).unwrap(), job);
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
fn mask_video_retains_original_credential_and_cancel_publication_fences() {
    for revoked in [false, true] {
        let mut f = fixture();
        let (caller, credential) = f.credential();
        f.app.set_caller(caller);
        let request = f.request("mask-fenced");
        let job = start_mask_video_mp4(&mut f.app, request).unwrap();
        let running = f.claim(&job);
        let input: opencut_editor_application::project_v4_mask_video_jobs::PreparedV4MaskVideoMp4V1 = serde_json::from_value(running.input.clone()).unwrap();
        f.app
            .revalidate_v4_mask_video_job(
                &job.job_id,
                "frame-worker",
                running.attempt,
                &input.authority,
            )
            .unwrap();
        assert!(
            f.app
                .revalidate_v4_blend_video_job(
                    &job.job_id,
                    "frame-worker",
                    running.attempt,
                    &input.authority
                )
                .is_err()
        );
        for (worker, attempt, authority) in [
            ("different-worker", running.attempt, input.authority.clone()),
            ("frame-worker", running.attempt + 1, input.authority.clone()),
            (
                "frame-worker",
                running.attempt,
                TrustedJobAuthorityV1::LocalOwner,
            ),
        ] {
            assert!(
                f.app
                    .revalidate_v4_mask_video_job(&job.job_id, worker, attempt, &authority)
                    .is_err()
            );
        }
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
