use super::*;
use opencut_contracts::graphic_edit_v1::*;

pub(super) fn graphic_fixture() -> Fixture {
    let mut f = Fixture::new();
    let mut request = GraphicPlanRequestV1 {
        project_id: "frame-project".into(),
        expected_revision: f.revision.clone(),
        sequence_id: "scene".into(),
        idempotency_key: "graphic-create".into(),
        command: GraphicPlanCommandV1::Defaults {
            geometry: GraphicKindV1::Polygon,
            start: GraphicTimeInputV1::Exact {
                time: Time {
                    ticks: "0".into(),
                    timebase: 1,
                },
            },
        },
    };
    let initial = opencut_editor_application::graphic_edit_v1::plan(&f.app, &request)
        .unwrap()
        .initial
        .unwrap();
    request.command = GraphicPlanCommandV1::Create {
        element_id: "polygon".into(),
        track: GraphicTrackTargetV1::New {
            track_id: "graphics".into(),
        },
        values: GraphicValuesV1 {
            name: initial.name,
            graphic: initial.graphic,
            transform: initial.transform,
            opacity: initial.opacity,
            start: GraphicTimeInputV1::Exact {
                time: initial.timeline_range.start,
            },
            duration: GraphicTimeInputV1::Exact {
                time: initial.timeline_range.duration,
            },
        },
    };
    let planned = opencut_editor_application::graphic_edit_v1::plan(&f.app, &request)
        .unwrap()
        .transaction
        .unwrap();
    f.revision = f.app.apply_public_v4_project(&planned).unwrap().revision;
    f
}

#[test]
fn graphic_frame_pins_real_geometry_and_original_request_across_reopen() {
    let mut f = graphic_fixture();
    let request = f.request("graphic-fixed");
    let job = start_graphic_frame_png(&mut f.app, request.clone()).unwrap();
    assert_eq!(job.kind, graphic::V4_GRAPHIC_FRAME_PNG_KIND);
    let prepared: PreparedV4TextFramePng = serde_json::from_value(job.input.clone()).unwrap();
    let frame = graphic::validate_prepared(&prepared).unwrap();
    assert_eq!(frame.graphics().len(), 1);
    assert!(prepared.asset_lock.assets.is_empty() && prepared.font_lock.is_empty());
    let caption_request = f.request("graphic-as-caption");
    assert!(start_caption_frame_png(&mut f.app, caption_request).is_err());
    f.app
        .apply_v4_project(&ApplyV4Project {
            project_id: "frame-project".into(),
            expected_revision: f.revision.clone(),
            idempotency_key: "later".into(),
            label: "Later head".into(),
            operations: vec![ProjectOperationV4::Rename {
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
        let input: PreparedV4TextFramePng = serde_json::from_value(running.input.clone()).unwrap();
        f.app
            .revalidate_v4_graphic_frame_job(
                &job.job_id,
                "frame-worker",
                running.attempt,
                &input.authority,
            )
            .unwrap();
        assert!(
            f.app
                .revalidate_v4_caption_frame_job(
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
