//! Graphic jobs retain the original fixed-revision durable video queue fences.
use super::*;
use crate::sqlite_project_v4_graphic_video_jobs::start_graphic_video_mp4;
use opencut_contracts::graphic_edit_v1::*;
use opencut_editor_application::project_v4_graphic_video_jobs as graphic;
pub(super) fn fixture() -> Fixture {
    let mut f = Fixture::new();
    let mut request = GraphicPlanRequestV1 {
        project_id: "audio-project".into(),
        expected_revision: f.revision.clone(),
        sequence_id: "scene".into(),
        idempotency_key: "graphic-create".into(),
        command: GraphicPlanCommandV1::Defaults {
            geometry: GraphicKindV1::Rectangle,
            start: GraphicTimeInputV1::Exact { time: time(0, 1) },
        },
    };
    let initial = opencut_editor_application::graphic_edit_v1::plan(&f.app, &request)
        .unwrap()
        .initial
        .unwrap();
    request.command = GraphicPlanCommandV1::Create {
        element_id: "shape".into(),
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
    let transaction = opencut_editor_application::graphic_edit_v1::plan(&f.app, &request)
        .unwrap()
        .transaction
        .unwrap();
    f.revision = f
        .app
        .apply_public_v4_project(&transaction)
        .unwrap()
        .revision;
    f
}
#[test]
fn graphic_video_keeps_fixed_request_and_real_cas_capture_across_head_change_and_reopen() {
    let mut f = fixture();
    let request = f.request("graphic-fixed");
    let job = start_graphic_video_mp4(&mut f.app, request.clone()).unwrap();
    assert_eq!(job.kind, graphic::V4_GRAPHIC_VIDEO_MP4_KIND);
    let prepared: graphic::PreparedV4GraphicVideoMp4V1 =
        serde_json::from_value(job.input.clone()).unwrap();
    let plan = graphic::validate_prepared(&prepared).unwrap();
    assert_eq!(plan.identity.frame_count, "150");
    assert_eq!(prepared.asset_lock.assets.len(), 1);
    let legacy = f.request("graphic-as-legacy");
    assert!(start_video_mp4(&mut f.app, legacy).is_err());
    f.app
        .apply_v4_project(&ApplyV4Project {
            project_id: "audio-project".into(),
            expected_revision: f.revision.clone(),
            idempotency_key: "later-head".into(),
            label: "Later head".into(),
            operations: vec![ProjectOperationV4::Rename {
                name: "Later".into(),
            }],
        })
        .unwrap();
    assert_eq!(
        start_graphic_video_mp4(&mut f.app, request.clone()).unwrap(),
        job
    );
    let mut reopened =
        EditorApplication::open(f.directory.path().join("workspace.sqlite")).unwrap();
    assert_eq!(
        start_graphic_video_mp4(&mut reopened, request).unwrap(),
        job
    );
    let view = reopened.job_view(&job.job_id).unwrap();
    assert_eq!(view.target.unwrap().revision, prepared.snapshot.revision);
    assert!(view.artifacts.is_empty());
}
#[test]
fn graphic_video_retains_original_credential_kind_and_cancellation_publication_fences() {
    for revoked in [false, true] {
        let mut f = fixture();
        let (caller, credential) = f.credential();
        f.app.set_caller(caller);
        let request = f.request("graphic-fenced");
        let job = start_graphic_video_mp4(&mut f.app, request).unwrap();
        let running = f.claim(&job);
        let input: graphic::PreparedV4GraphicVideoMp4V1 =
            serde_json::from_value(running.input.clone()).unwrap();
        f.app
            .revalidate_v4_graphic_video_job(
                &job.job_id,
                "frame-worker",
                running.attempt,
                &input.authority,
            )
            .unwrap();
        assert!(
            f.app
                .revalidate_v4_video_job(
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
