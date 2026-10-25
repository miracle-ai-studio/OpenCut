//! Graphic jobs retain the original fixed-revision durable video queue fences.
use super::*;
use crate::sqlite_scene_graphic_video_jobs::start_graphic_video_mp4;
use opencut_contracts::graphic_edit::*;
use opencut_editor_application::scene_graphic_video_jobs as graphic;
pub(super) fn fixture() -> Fixture {
    let mut f = Fixture::new();
    let mut request = GraphicPlanRequest {
        project_id: "audio-project".into(),
        expected_revision: f.revision.clone(),
        sequence_id: "scene".into(),
        idempotency_key: "graphic-create".into(),
        command: GraphicPlanCommand::Defaults {
            geometry: GraphicKind::Rectangle,
            start: GraphicTimeInput::Exact { time: time(0, 1) },
        },
    };
    let initial = opencut_editor_application::graphic_edit::plan(&f.app, &request)
        .unwrap()
        .initial
        .unwrap();
    request.command = GraphicPlanCommand::Create {
        element_id: "shape".into(),
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
    let transaction = opencut_editor_application::graphic_edit::plan(&f.app, &request)
        .unwrap()
        .transaction
        .unwrap();
    f.revision = f.app.apply_editor_edit(&transaction).unwrap().revision;
    f
}
#[test]
fn graphic_video_keeps_fixed_request_and_real_cas_capture_across_head_change_and_reopen() {
    let mut f = fixture();
    let request = f.request("graphic-fixed");
    let job = start_graphic_video_mp4(&mut f.app, request.clone()).unwrap();
    assert_eq!(job.kind, graphic::GRAPHIC_VIDEO_MP4_KIND);
    let prepared: graphic::PreparedGraphicVideoMp4 =
        serde_json::from_value(job.input.clone()).unwrap();
    let plan = graphic::validate_prepared(&prepared).unwrap();
    assert_eq!(plan.identity.frame_count, "150");
    assert_eq!(prepared.asset_lock.assets.len(), 1);
    let legacy = f.request("graphic-as-legacy");
    assert!(start_video_mp4(&mut f.app, legacy).is_err());
    f.app
        .apply_scene_project(&ApplySceneProject {
            project_id: "audio-project".into(),
            expected_revision: f.revision.clone(),
            idempotency_key: "later-head".into(),
            label: "Later head".into(),
            operations: vec![ProjectOperation::Rename {
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
        let input: graphic::PreparedGraphicVideoMp4 =
            serde_json::from_value(running.input.clone()).unwrap();
        f.app
            .revalidate_scene_graphic_video_job(
                &job.job_id,
                "frame-worker",
                running.attempt,
                &input.authority,
            )
            .unwrap();
        assert!(
            f.app
                .revalidate_scene_video_job(
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
