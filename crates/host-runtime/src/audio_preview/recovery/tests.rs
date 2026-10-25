use super::*;
use opencut_contracts::{CreateProject, FrameRate, Time, scene_ops::CreateProjectRequest};
fn seed(host: &mut Automation) -> StageAudioPreviewRecovery {
    let revision = host
        .app
        .create_scene_project(&CreateProjectRequest {
            project: CreateProject {
                id: "p".into(),
                name: "Recovery".into(),
                sequence_id: "s".into(),
                width: 64,
                height: 48,
                fps: FrameRate {
                    numerator: 25,
                    denominator: 1,
                },
            },
            idempotency_key: "project".into(),
        })
        .unwrap()
        .revision;
    StageAudioPreviewRecovery {
        request: AudioPreviewRecoveryRequest::Create {
            request: AudioPreviewCreate {
                project_id: "p".into(),
                revision,
                sequence_id: "s".into(),
                sample_rate: 48000,
                start_time: Time {
                    ticks: "0".into(),
                    timebase: 120000,
                },
                idempotency_key: "preview".into(),
            },
        },
    }
}
#[test]
fn a_new_host_can_discover_and_retire_but_cannot_recreate_a_previous_incarnation() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("workspace.sqlite");
    let mut host = Automation::open(&path).unwrap();
    let original = seed(&mut host);
    let record = host
        .app
        .stage_audio_preview_recovery(&original, "dead-process-incarnation")
        .unwrap();
    drop(host);
    let mut host = Automation::open(&path).unwrap();
    let found = inspect(
        &mut host,
        InspectAudioPreviewRecovery {
            project_id: "p".into(),
        },
    )
    .unwrap()
    .record
    .unwrap();
    assert_eq!(record, found);
    assert_eq!(
        dispatch(
            &mut host,
            DispatchAudioPreviewRecovery {
                project_id: "p".into(),
                recovery_id: record.recovery_id.clone()
            }
        )
        .unwrap_err()
        .code,
        "PREVIEW_RECOVERY_RETIRED"
    );
    let exact = stage(&mut host, original).unwrap();
    assert_eq!(exact.recovery_id, record.recovery_id);
    ack(
        &mut host,
        AckAudioPreviewRecovery {
            project_id: "p".into(),
            recovery_id: record.recovery_id,
            disposition: AudioPreviewRecoveryDisposition::Retire,
        },
    )
    .unwrap();
    assert!(
        inspect(
            &mut host,
            InspectAudioPreviewRecovery {
                project_id: "p".into()
            }
        )
        .unwrap()
        .record
        .is_none()
    );
}
#[test]
fn public_recovery_tools_register_without_native_admission_and_do_not_edit_history() {
    let tmp = tempfile::tempdir().unwrap();
    let mut host = Automation::open(tmp.path().join("workspace.sqlite")).unwrap();
    let original = seed(&mut host);
    let history = host
        .call(
            "opencut_project_history_navigation",
            serde_json::json!({"project_id":"p","revision":null}),
        )
        .data;
    let result = host.call(
        "opencut_audio_preview_recovery_stage",
        serde_json::to_value(&original).unwrap(),
    );
    assert!(result.error.is_none());
    let public = result.data.unwrap();
    assert!(public.get("owner_id").is_none());
    assert!(public.get("lease_id").is_none());
    assert!(public.get("original").is_none());
    let id = public["recovery_id"].as_str().unwrap();
    assert_eq!(
        host.call(
            "opencut_audio_preview_recovery_ack",
            serde_json::json!({"project_id":"p","recovery_id":id,"disposition":"observed"})
        )
        .error
        .unwrap()
        .code,
        "RECOVERY_UNCONFIRMED"
    );
    assert!(
        host.call(
            "opencut_audio_preview_recovery_retire",
            serde_json::to_value(&original).unwrap()
        )
        .error
        .is_none()
    );
    // Exact retry after the response/slot disappeared never dispatches Create.
    assert!(
        host.call(
            "opencut_audio_preview_recovery_retire",
            serde_json::to_value(&original).unwrap(),
        )
        .error
        .is_none()
    );
    assert_eq!(
        host.call(
            "opencut_audio_preview_recovery_dispatch",
            serde_json::json!({"project_id":"p","recovery_id":id})
        )
        .error
        .unwrap()
        .code,
        "NOT_FOUND"
    );
    assert_eq!(
        history,
        host.call(
            "opencut_project_history_navigation",
            serde_json::json!({"project_id":"p","revision":null})
        )
        .data
    );
}
