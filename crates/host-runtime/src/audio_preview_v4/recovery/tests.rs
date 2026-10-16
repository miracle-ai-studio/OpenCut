use super::*;
use opencut_contracts::{CreateProject, FrameRate, Time, project_v4_ops::CreateV4Project};
fn seed(host: &mut Automation) -> StageAudioPreviewRecoveryV1 {
    let revision = host
        .app
        .create_v4_project(&CreateV4Project {
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
    StageAudioPreviewRecoveryV1 {
        request: AudioPreviewRecoveryRequestV1::Create {
            request: AudioPreviewCreateV4 {
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
        .stage_audio_preview_recovery_v1(&original, "dead-process-incarnation")
        .unwrap();
    drop(host);
    let mut host = Automation::open(&path).unwrap();
    let found = inspect(
        &mut host,
        InspectAudioPreviewRecoveryV1 {
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
            DispatchAudioPreviewRecoveryV1 {
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
        AckAudioPreviewRecoveryV1 {
            project_id: "p".into(),
            recovery_id: record.recovery_id,
            disposition: AudioPreviewRecoveryDispositionV1::Retire,
        },
    )
    .unwrap();
    assert!(
        inspect(
            &mut host,
            InspectAudioPreviewRecoveryV1 {
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
            "opencut_project_v4_history_navigation",
            serde_json::json!({"project_id":"p","revision":null}),
        )
        .data;
    let result = host.call(
        "opencut_audio_preview_recovery_v1_stage",
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
            "opencut_audio_preview_recovery_v1_ack",
            serde_json::json!({"project_id":"p","recovery_id":id,"disposition":"observed"})
        )
        .error
        .unwrap()
        .code,
        "RECOVERY_UNCONFIRMED"
    );
    assert!(
        host.call(
            "opencut_audio_preview_recovery_v1_ack",
            serde_json::json!({"project_id":"p","recovery_id":id,"disposition":"retire"})
        )
        .error
        .is_none()
    );
    assert_eq!(
        host.call(
            "opencut_audio_preview_recovery_v1_dispatch",
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
            "opencut_project_v4_history_navigation",
            serde_json::json!({"project_id":"p","revision":null})
        )
        .data
    );
}
