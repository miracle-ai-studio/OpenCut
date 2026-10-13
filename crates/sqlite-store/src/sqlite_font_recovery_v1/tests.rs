use super::*;
use opencut_contracts::{
    font_import_v1::ImportFontV1,
    project_v4_ops::CreateV4Project,
    project_v4_public::{ApplyPublicV4Project, PublicProjectOperationV4},
};
use opencut_editor_application::{
    access::{Caller, PrincipalPolicy, ProjectGrant},
    edit_recovery_v1 as v1, edit_recovery_v2 as v2,
};

fn create(app: &mut EditorApplication, id: &str) -> String {
    app.create_v4_project(&CreateV4Project {
        project: CreateProject {
            id: id.into(),
            name: id.into(),
            sequence_id: "main".into(),
            width: 64,
            height: 48,
            fps: opencut_contracts::FrameRate {
                numerator: 25,
                denominator: 1,
            },
        },
        idempotency_key: format!("create-{id}"),
    })
    .unwrap()
    .revision
}
fn stage(project: &str, base: &str, key: &str) -> StageFontRecoveryV1 {
    StageFontRecoveryV1 {
        context: FontRecoveryContextV1 {
            request: ImportFontV1 {
                project_id: project.into(),
                expected_revision: base.into(),
                idempotency_key: key.into(),
                catalog_id: "google-lato-regular-v1".into(),
                font_id: "font".into(),
                required_text: "AV To".into(),
            },
            sequence_id: Some("main".into()),
        },
    }
}
fn read(project: &str) -> ReadFontRecoveryV1 {
    ReadFontRecoveryV1 {
        project_id: project.into(),
        recovery_id: None,
    }
}
fn ack(record: &FontRecoveryRecordV1) -> AckFontRecoveryV1 {
    AckFontRecoveryV1 {
        project_id: record.project_id.clone(),
        recovery_id: record.recovery_id.clone(),
    }
}
fn apply(project: &str, base: &str, key: &str) -> ApplyPublicV4Project {
    ApplyPublicV4Project {
        project_id: project.into(),
        expected_revision: base.into(),
        idempotency_key: key.into(),
        label: "Rename".into(),
        operations: vec![PublicProjectOperationV4::Rename { name: key.into() }],
    }
}
fn stage_v2(project: &str, base: &str, key: &str) -> v2::StageEditRecoveryV2 {
    v2::StageEditRecoveryV2 {
        context: v2::EditRecoveryContextV2 {
            intent: v2::EditRecoveryIntentV2::PublicApply {
                request: Box::new(apply(project, base, key)),
            },
            sequence_id: Some("main".into()),
        },
    }
}
fn stage_v1(
    app: &EditorApplication,
    project: &str,
    base: &str,
    key: &str,
) -> v1::StageEditRecoveryV1 {
    let request = opencut_contracts::timeline_v4::TimelineEditRequestV4 {
        project_id: project.into(),
        expected_revision: base.into(),
        sequence_id: "main".into(),
        idempotency_key: key.into(),
        label: "Add track".into(),
        command: opencut_contracts::timeline_v4::TimelineCommandV4::AddTrack {
            id: format!("track-{key}"),
            name: "Audio".into(),
            track_kind: opencut_contracts::project_v4::TrackKindV4::Audio,
        },
    };
    let plan = app.plan_v4_timeline(&request).unwrap();
    v1::StageEditRecoveryV1 {
        context: v1::EditRecoveryContextV1::TimelineV4 { request, plan },
    }
}
fn rows(app: &EditorApplication) -> Vec<i64> {
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
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    })
    .collect()
}

#[test]
fn font_recovery_survives_reopen_and_never_executes_the_original_import() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workspace.sqlite");
    let mut app = EditorApplication::open(&path).unwrap();
    let base = create(&mut app, "p");
    let request = stage("p", &base, "font-key");
    let before = rows(&app);
    let staged = app.stage_font_recovery_v1(&request).unwrap();
    assert_eq!(rows(&app), before);
    assert!(staged.receipt.is_none());
    assert_eq!(
        app.ack_font_recovery_v1(&ack(&staged)).unwrap_err().code,
        "RECOVERY_UNRESOLVED"
    );
    drop(app);
    let mut app = EditorApplication::open(&path).unwrap();
    assert_eq!(
        app.inspect_font_recovery_v1(&read("p")).unwrap().record,
        Some(staged.clone())
    );
    assert_eq!(app.stage_font_recovery_v1(&request).unwrap(), staged);
    let mut changed = request.clone();
    changed.context.request.required_text = "New".into();
    assert_eq!(
        app.stage_font_recovery_v1(&changed).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    changed.context.request.idempotency_key = "other".into();
    assert_eq!(
        app.stage_font_recovery_v1(&changed).unwrap_err().code,
        "RECOVERY_CONFLICT"
    );
    assert_eq!(rows(&app), before);
    app.apply_public_v4_project(&apply("p", &base, "advance"))
        .unwrap();
    assert_eq!(
        app.stage_font_recovery_v1(&request).unwrap(),
        staged,
        "exact old stage must precede fixed-head checks"
    );
    let resolved = app.ack_font_recovery_v1(&ack(&staged)).unwrap();
    assert_eq!(resolved.resolution, FontRecoveryResolutionV1::NotSaved);
    assert!(resolved.receipt.is_none());
    assert!(
        app.inspect_font_recovery_v1(&read("p"))
            .unwrap()
            .record
            .is_none()
    );
    assert_eq!(
        app.stage_font_recovery_v1(&request).unwrap_err().code,
        "REVISION_CONFLICT"
    );
}

#[test]
fn font_recovery_and_frozen_versions_exclude_each_other_without_rewriting_payloads() {
    for original in 0..3 {
        let mut app = EditorApplication::open(":memory:").unwrap();
        let base = create(&mut app, "p");
        let font = stage("p", &base, "original");
        let legacy = stage_v1(&app, "p", &base, "original");
        let current = stage_v2("p", &base, "original");
        match original {
            0 => {
                app.stage_font_recovery_v1(&font).unwrap();
            }
            1 => {
                app.stage_edit_recovery_v1(&legacy).unwrap();
            }
            _ => {
                app.stage_edit_recovery_v2(&current).unwrap();
            }
        }
        let stored:Vec<(String,String)>=app.connection.prepare("SELECT request_digest,context_json FROM edit_recovery_v1 UNION ALL SELECT request_digest,context_json FROM edit_recovery_v2").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?))).unwrap().map(Result::unwrap).collect();
        if original != 0 {
            assert_eq!(
                app.stage_font_recovery_v1(&font).unwrap_err().code,
                "IDEMPOTENCY_KEY_REUSED"
            );
            let mut other = font.clone();
            other.context.request.idempotency_key = "other".into();
            assert_eq!(
                app.stage_font_recovery_v1(&other).unwrap_err().code,
                "RECOVERY_CONFLICT"
            );
        }
        if original != 1 {
            assert_eq!(
                app.stage_edit_recovery_v1(&legacy).unwrap_err().code,
                "IDEMPOTENCY_KEY_REUSED"
            );
            let other = stage_v1(&app, "p", &base, "other");
            assert_eq!(
                app.stage_edit_recovery_v1(&other).unwrap_err().code,
                "RECOVERY_CONFLICT"
            );
        }
        if original != 2 {
            assert_eq!(
                app.stage_edit_recovery_v2(&current).unwrap_err().code,
                "IDEMPOTENCY_KEY_REUSED"
            );
            assert_eq!(
                app.stage_edit_recovery_v2(&stage_v2("p", &base, "other"))
                    .unwrap_err()
                    .code,
                "RECOVERY_CONFLICT"
            );
        }
        let discovery = v2::ReadEditRecoveryV2 {
            project_id: "p".into(),
            recovery_id: None,
        };
        if original == 0 {
            let staged = app
                .inspect_font_recovery_v1(&read("p"))
                .unwrap()
                .record
                .unwrap();
            assert_eq!(
                app.inspect_edit_recovery_v2(&discovery).unwrap_err().code,
                "RECOVERY_PROTOCOL_REQUIRED"
            );
            assert_eq!(
                app.ack_edit_recovery_v2(&v2::AckEditRecoveryV2 {
                    project_id: "p".into(),
                    recovery_id: staged.recovery_id
                })
                .unwrap_err()
                .code,
                "RECOVERY_PROTOCOL_REQUIRED"
            );
        } else {
            assert!(
                app.inspect_font_recovery_v1(&read("p"))
                    .unwrap()
                    .record
                    .is_none()
            );
            assert!(
                app.inspect_edit_recovery_v2(&discovery)
                    .unwrap()
                    .record
                    .is_some()
            );
        }
        let after:Vec<(String,String)>=app.connection.prepare("SELECT request_digest,context_json FROM edit_recovery_v1 UNION ALL SELECT request_digest,context_json FROM edit_recovery_v2").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?))).unwrap().map(Result::unwrap).collect();
        assert_eq!(stored, after);
    }
}

#[test]
fn font_recovery_shares_quota_with_v1_and_v2() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    for index in 0..v1::MAX_UNRESOLVED_RECORDS {
        let project = format!("p-{index}");
        let base = create(&mut app, &project);
        match index % 3 {
            0 => {
                app.stage_font_recovery_v1(&stage(&project, &base, "font"))
                    .unwrap();
            }
            1 => {
                let input = stage_v1(&app, &project, &base, "legacy");
                app.stage_edit_recovery_v1(&input).unwrap();
            }
            _ => {
                app.stage_edit_recovery_v2(&stage_v2(&project, &base, "current"))
                    .unwrap();
            }
        }
    }
    let base = create(&mut app, "overflow");
    let before = rows(&app);
    assert_eq!(
        app.stage_font_recovery_v1(&stage("overflow", &base, "font"))
            .unwrap_err()
            .code,
        "RECOVERY_QUOTA_EXCEEDED"
    );
    assert_eq!(
        app.stage_edit_recovery_v2(&stage_v2("overflow", &base, "v2"))
            .unwrap_err()
            .code,
        "RECOVERY_QUOTA_EXCEEDED"
    );
    let legacy = stage_v1(&app, "overflow", &base, "v1");
    assert_eq!(
        app.stage_edit_recovery_v1(&legacy).unwrap_err().code,
        "RECOVERY_QUOTA_EXCEEDED"
    );
    assert_eq!(rows(&app), before);
    let payload: (String, u64) = app
        .connection
        .query_row(
            "SELECT context_json,stored_bytes FROM edit_recovery_v2 WHERE project_id='p-0'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(payload.1 > payload.0.len() as u64);
}

#[test]
fn font_recovery_keeps_credential_scope_and_original_write_authority() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let base = create(&mut app, "p");
    let policy = |revision: &str, actions: Vec<ProjectAction>| PrincipalPolicy {
        principal_id: "font-editor".into(),
        expected_revision: revision.into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: "p".into(),
            actions,
        }],
    };
    app.set_principal_policy(policy("0", vec![ProjectAction::Read, ProjectAction::Edit]))
        .unwrap();
    let first = app.issue_credential("font-editor").unwrap();
    let second = app.issue_credential("font-editor").unwrap();
    let caller = app.authenticate(&first.secret).unwrap();
    app.set_caller(caller.clone());
    let request = stage("p", &base, "font");
    let staged = app.stage_font_recovery_v1(&request).unwrap();
    app.set_caller(app.authenticate(&second.secret).unwrap());
    assert!(
        app.inspect_font_recovery_v1(&read("p"))
            .unwrap()
            .record
            .is_none()
    );
    assert_eq!(
        app.ack_font_recovery_v1(&ack(&staged)).unwrap_err().code,
        "NOT_FOUND"
    );
    app.set_caller(Caller::LocalOwner);
    assert!(
        app.inspect_font_recovery_v1(&read("p"))
            .unwrap()
            .record
            .is_none()
    );
    app.set_principal_policy(policy("1", vec![ProjectAction::Read]))
        .unwrap();
    app.set_caller(caller.clone());
    assert_eq!(
        app.inspect_font_recovery_v1(&read("p")).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.stage_font_recovery_v1(&request).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.ack_font_recovery_v1(&ack(&staged)).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.inspect_edit_recovery_v2(&v2::ReadEditRecoveryV2 {
            project_id: "p".into(),
            recovery_id: None
        })
        .unwrap_err()
        .code,
        "NOT_AUTHORIZED"
    );
    app.set_caller(Caller::LocalOwner);
    app.set_principal_policy(policy("2", vec![ProjectAction::Read, ProjectAction::Edit]))
        .unwrap();
    app.revoke_credential(&first.credential_id).unwrap();
    app.set_caller(caller);
    assert_eq!(
        app.inspect_font_recovery_v1(&read("p")).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.connection
            .query_row::<u32, _, _>("SELECT count(*) FROM edit_recovery_v2", [], |r| r.get(0))
            .unwrap(),
        1
    );
}

#[test]
fn font_recovery_rejects_corrupt_domains_and_ack_rolls_back_delete_failure() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let base = create(&mut app, "p");
    let staged = app
        .stage_font_recovery_v1(&stage("p", &base, "font"))
        .unwrap();
    let original: String = app
        .connection
        .query_row("SELECT context_json FROM edit_recovery_v2", [], |r| {
            r.get(0)
        })
        .unwrap();
    for corrupted in [v2::exact_payload_digest(&original), "sha256:bad".into()] {
        app.connection
            .execute("UPDATE edit_recovery_v2 SET request_digest=?1", [corrupted])
            .unwrap();
        assert_eq!(
            app.inspect_font_recovery_v1(&read("p")).unwrap_err().code,
            "STORAGE_ERROR"
        );
        assert_eq!(
            app.ack_font_recovery_v1(&ack(&staged)).unwrap_err().code,
            "STORAGE_ERROR"
        );
        assert_eq!(
            app.inspect_edit_recovery_v2(&v2::ReadEditRecoveryV2 {
                project_id: "p".into(),
                recovery_id: None
            })
            .unwrap_err()
            .code,
            "STORAGE_ERROR"
        );
    }
    app.connection
        .execute(
            "UPDATE edit_recovery_v2 SET request_digest=?1",
            [&staged.request_digest],
        )
        .unwrap();
    app.apply_public_v4_project(&apply("p", &base, "advance"))
        .unwrap();
    app.connection.execute_batch("CREATE TRIGGER reject_font_ack BEFORE DELETE ON edit_recovery_v2 BEGIN SELECT RAISE(ABORT,'ack failure'); END;").unwrap();
    assert!(app.ack_font_recovery_v1(&ack(&staged)).is_err());
    assert_eq!(
        app.inspect_font_recovery_v1(&read("p")).unwrap().record,
        Some(staged)
    );
}

#[test]
fn font_recovery_wrong_family_receipt_cannot_be_reported_as_saved() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let base = create(&mut app, "p");
    let staged = app
        .stage_font_recovery_v1(&stage("p", &base, "same-key"))
        .unwrap();
    app.apply_public_v4_project(&apply("p", &base, "same-key"))
        .unwrap();
    assert_eq!(
        app.inspect_font_recovery_v1(&read("p")).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    assert_eq!(
        app.ack_font_recovery_v1(&ack(&staged)).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    assert_eq!(
        app.connection
            .query_row::<u32, _, _>("SELECT count(*) FROM edit_recovery_v2", [], |r| r.get(0))
            .unwrap(),
        1
    );
}
