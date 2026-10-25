use super::*;
use opencut_contracts::{CreateProject, FrameRate, Time, scene_ops::CreateProjectRequest};
use opencut_editor_application::{
    access::{Caller, PrincipalPolicy, ProjectGrant},
    audio_preview::{AudioPreviewCreate, AudioPreviewMachine},
};
fn seed(app: &mut EditorApplication) -> StageAudioPreviewRecovery {
    let revision = app
        .create_scene_project(&CreateProjectRequest {
            project: CreateProject {
                id: "p".into(),
                name: "Preview".into(),
                sequence_id: "s".into(),
                width: 64,
                height: 48,
                fps: FrameRate {
                    numerator: 25,
                    denominator: 1,
                },
            },
            idempotency_key: "create-project".into(),
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
                idempotency_key: "original-preview".into(),
            },
        },
    }
}
fn counts(app: &EditorApplication) -> Vec<u64> {
    [
        "revisions",
        "editor_transactions",
        "idempotency",
        "outbox",
        "audit_records",
    ]
    .iter()
    .map(|table| {
        app.connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    })
    .collect()
}
#[test]
fn original_request_survives_database_reopen_without_content_or_authority_leak() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("workspace.sqlite");
    let mut app = EditorApplication::open(&path).unwrap();
    let input = seed(&mut app);
    let before = counts(&app);
    let staged = app
        .stage_audio_preview_recovery(&input, "first-host")
        .unwrap();
    app.begin_audio_preview_recovery("p", &staged.recovery_id)
        .unwrap();
    assert_eq!(before, counts(&app));
    drop(app);
    let app = EditorApplication::open(&path).unwrap();
    let stored = app.read_audio_preview_recovery("p", None).unwrap().unwrap();
    assert_eq!(stored.original, input);
    assert_eq!(stored.host_instance, "first-host");
    assert!(stored.dispatched);
    assert_eq!(stored.view, staged);
    assert_eq!(before, counts(&app));
    let public = serde_json::to_value(&stored.view).unwrap();
    for private in [
        "original",
        "receipt",
        "lease_id",
        "owner_id",
        "request",
        "host_instance",
        "dispatched",
    ] {
        assert!(public.get(private).is_none());
    }
}
#[test]
fn staged_outcome_cannot_ack_and_retirement_fences_dispatch_before_payload_cleanup() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let input = seed(&mut app);
    let staged = app.stage_audio_preview_recovery(&input, "host").unwrap();
    assert_eq!(
        app.ack_audio_preview_recovery("p", &staged.recovery_id)
            .unwrap_err()
            .code,
        "RECOVERY_UNCONFIRMED"
    );
    assert_eq!(
        app.stage_audio_preview_recovery(&input, "host").unwrap(),
        staged
    );
    let mut different = input.clone();
    if let AudioPreviewRecoveryRequest::Create { request } = &mut different.request {
        request.idempotency_key = "replacement".into();
    }
    assert_eq!(
        app.stage_audio_preview_recovery(&different, "host")
            .unwrap_err()
            .code,
        "RECOVERY_CONFLICT"
    );
    app.retire_audio_preview_recovery("p", &staged.recovery_id)
        .unwrap();
    assert_eq!(
        app.begin_audio_preview_recovery("p", &staged.recovery_id)
            .unwrap_err()
            .code,
        "PREVIEW_RECOVERY_RETIRED"
    );
    app.ack_audio_preview_recovery("p", &staged.recovery_id)
        .unwrap();
    assert!(
        app.read_audio_preview_recovery("p", None)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        app.connection
            .query_row("SELECT COUNT(*) FROM audio_preview_recovery_v1", [], |r| {
                r.get::<_, u64>(0)
            })
            .unwrap(),
        0
    );
    assert_ne!(
        app.stage_audio_preview_recovery(&different, "host")
            .unwrap()
            .recovery_id,
        staged.recovery_id
    );
}
#[test]
fn a_received_native_receipt_can_ack_without_a_project_edit() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let input = seed(&mut app);
    let before = counts(&app);
    let staged = app.stage_audio_preview_recovery(&input, "host").unwrap();
    let native = AudioPreviewMachine::new(
        recovery::create(&input.request),
        "session".into(),
        "owner".into(),
        "lease".into(),
        48000,
        0,
    )
    .unwrap()
    .snapshot(0);
    app.begin_audio_preview_recovery("p", &staged.recovery_id)
        .unwrap();
    app.complete_audio_preview_recovery("p", &staged.recovery_id, &native)
        .unwrap();
    let stored = app.read_audio_preview_recovery("p", None).unwrap().unwrap();
    assert_eq!(stored.receipt, Some(native));
    assert_eq!(stored.view.status, AudioPreviewRecoveryStatus::Completed);
    app.ack_audio_preview_recovery("p", &staged.recovery_id)
        .unwrap();
    assert_eq!(before, counts(&app));
}
#[test]
fn historical_submission_receipt_is_read_without_inventing_output_or_rewriting_bytes() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let input = seed(&mut app);
    let native = AudioPreviewMachine::new(
        recovery::create(&input.request),
        "session".into(),
        "owner".into(),
        "lease".into(),
        48000,
        0,
    )
    .unwrap()
    .snapshot(0);
    let mut stored = serde_json::to_value(native).unwrap();
    let object = stored.as_object_mut().unwrap();
    let submitted = object.remove("submitted_time").unwrap();
    object.remove("presentation_time");
    object.remove("output");
    object.insert("authoritative_time".into(), submitted);
    object.insert("presentation_fresh".into(), serde_json::json!(true));
    let original = serde_json::to_string(&stored).unwrap();
    let restored = decode_receipt(&original).unwrap();
    assert_eq!(restored.presentation_time.ticks, "0");
    assert!(restored.output.is_none());
    assert!(!restored.presentation_fresh);
    assert_eq!(serde_json::to_string(&stored).unwrap(), original);
    stored
        .as_object_mut()
        .unwrap()
        .insert("output".into(), serde_json::Value::Null);
    assert!(decode_receipt(&serde_json::to_string(&stored).unwrap()).is_err());
}
#[test]
fn preview_slot_is_bound_to_exact_credential_and_requires_run_jobs_without_edit() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let input = seed(&mut app);
    app.set_principal_policy(PrincipalPolicy {
        principal_id: "viewer".into(),
        expected_revision: "0".into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: "p".into(),
            actions: vec![ProjectAction::Read, ProjectAction::RunJobs],
        }],
    })
    .unwrap();
    let first = app.issue_credential("viewer").unwrap();
    let second = app.issue_credential("viewer").unwrap();
    let caller = app.authenticate(&first.secret).unwrap();
    app.set_caller(caller.clone());
    let staged = app.stage_audio_preview_recovery(&input, "host").unwrap();
    app.set_caller(app.authenticate(&second.secret).unwrap());
    assert!(
        app.read_audio_preview_recovery("p", Some(&staged.recovery_id))
            .unwrap()
            .is_none()
    );
    app.set_caller(Caller::LocalOwner);
    assert!(
        app.read_audio_preview_recovery("p", None)
            .unwrap()
            .is_none()
    );
    app.revoke_credential(&first.credential_id).unwrap();
    app.set_caller(caller);
    assert_eq!(
        app.read_audio_preview_recovery("p", None)
            .err()
            .unwrap()
            .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.connection
            .query_row("SELECT COUNT(*) FROM audio_preview_recovery_v1", [], |r| {
                r.get::<_, u64>(0)
            })
            .unwrap(),
        1
    );
}
#[test]
fn project_soft_delete_clears_private_preview_slots_and_fences_late_dispatch() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let input = seed(&mut app);
    let staged = app.stage_audio_preview_recovery(&input, "host").unwrap();
    app.delete_projects(&opencut_api_contracts::project_library::DeleteProjects {
        projects: vec![opencut_api_contracts::project_library::DeleteProject {
            project_id: "p".into(),
            expected_revision: recovery::create(&input.request).revision.clone(),
        }],
        idempotency_key: "delete-preview-project".into(),
    })
    .unwrap();
    assert_eq!(
        app.connection
            .query_row(
                "SELECT COUNT(*) FROM audio_preview_recovery_v1 WHERE recovery_id=?1",
                [&staged.recovery_id],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        app.read_audio_preview_recovery("p", None)
            .err()
            .unwrap()
            .code,
        "NOT_FOUND"
    );
    assert_eq!(
        app.stage_audio_preview_recovery(&input, "host")
            .unwrap_err()
            .code,
        "NOT_FOUND"
    );
}
