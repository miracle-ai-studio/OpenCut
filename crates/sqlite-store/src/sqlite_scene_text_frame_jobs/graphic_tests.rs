use super::*;
use opencut_contracts::graphic_edit::*;

pub(super) fn graphic_fixture() -> Fixture {
    let mut f = Fixture::new();
    let mut request = GraphicPlanRequest {
        project_id: "frame-project".into(),
        expected_revision: f.revision.clone(),
        sequence_id: "scene".into(),
        idempotency_key: "graphic-create".into(),
        command: GraphicPlanCommand::Defaults {
            geometry: GraphicKind::Polygon,
            start: GraphicTimeInput::Exact {
                time: Time {
                    ticks: "0".into(),
                    timebase: 1,
                },
            },
        },
    };
    let initial = opencut_editor_application::graphic_edit::plan(&f.app, &request)
        .unwrap()
        .initial
        .unwrap();
    request.command = GraphicPlanCommand::Create {
        element_id: "polygon".into(),
        track: GraphicTrackTarget::New {
            track_id: "graphics".into(),
        },
        values: GraphicValues {
            name: initial.name,
            graphic: initial.graphic,
            transform: initial.transform,
            opacity: initial.opacity,
            start: GraphicTimeInput::Exact {
                time: initial.timeline_range.start,
            },
            duration: GraphicTimeInput::Exact {
                time: initial.timeline_range.duration,
            },
        },
    };
    let planned = opencut_editor_application::graphic_edit::plan(&f.app, &request)
        .unwrap()
        .transaction
        .unwrap();
    f.revision = f.app.apply_editor_edit(&planned).unwrap().revision;
    f
}

#[test]
fn graphic_frame_pins_real_geometry_and_original_request_across_reopen() {
    let mut f = graphic_fixture();
    let request = f.request("graphic-fixed");
    let job = start_graphic_frame_png(&mut f.app, request.clone()).unwrap();
    assert_eq!(job.kind, graphic::GRAPHIC_FRAME_PNG_KIND);
    let prepared: PreparedTextFramePng = serde_json::from_value(job.input.clone()).unwrap();
    let frame = graphic::validate_prepared(&prepared).unwrap();
    assert_eq!(frame.graphics().len(), 1);
    assert!(prepared.asset_lock.assets.is_empty() && prepared.font_lock.is_empty());
    let caption_request = f.request("graphic-as-caption");
    assert!(start_caption_frame_png(&mut f.app, caption_request).is_err());
    f.app
        .apply_scene_project(&ApplySceneProject {
            project_id: "frame-project".into(),
            expected_revision: f.revision.clone(),
            idempotency_key: "later".into(),
            label: "Later head".into(),
            operations: vec![ProjectOperation::Rename {
                name: "Later".into(),
            }],
        })
        .unwrap();
    assert_eq!(
        start_graphic_frame_png(&mut f.app, request.clone()).unwrap(),
        job
    );
    let mut changed = request.clone();
    changed.time.ticks = "2".into();
    assert!(start_graphic_frame_png(&mut f.app, changed).is_err());
    let mut reopened =
        EditorApplication::open(f.directory.path().join("workspace.sqlite")).unwrap();
    assert_eq!(
        start_graphic_frame_png(&mut reopened, request).unwrap(),
        job
    );
    assert_eq!(
        reopened
            .job_view(&job.job_id)
            .unwrap()
            .target
            .unwrap()
            .revision,
        prepared.snapshot.revision
    );
}

#[test]
fn graphic_frame_original_authority_and_cancel_fence_worker_completion() {
    for revoked in [false, true] {
        let mut f = graphic_fixture();
        let (caller, credential) = f.credential();
        f.app.set_caller(caller);
        let request = f.request("graphic-fenced");
        let job = start_graphic_frame_png(&mut f.app, request).unwrap();
        let running = f.claim(&job);
        let input: PreparedTextFramePng = serde_json::from_value(running.input.clone()).unwrap();
        f.app
            .revalidate_scene_graphic_frame_job(
                &job.job_id,
                "frame-worker",
                running.attempt,
                &input.authority,
            )
            .unwrap();
        assert!(
            f.app
                .revalidate_scene_caption_frame_job(
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
