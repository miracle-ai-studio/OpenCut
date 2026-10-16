use super::*;
use opencut_contracts::{
    FrameRate,
    project_v4::TrackKindV4,
    project_v4_ops::CreateV4Project,
    project_v4_public::{ApplyPublicV4Project, PublicProjectOperationV4},
    timeline_v4::{TimelineCommandV4, TimelineEditRequestV4},
};
use opencut_editor_application::access::{Caller, PrincipalPolicy, ProjectGrant};

fn create(app: &mut EditorApplication, id: &str) -> String {
    app.create_v4_project(&CreateV4Project {
        project: CreateProject {
            id: id.into(),
            name: id.into(),
            sequence_id: "main".into(),
            width: 64,
            height: 48,
            fps: FrameRate {
                numerator: 25,
                denominator: 1,
            },
        },
        idempotency_key: format!("create-{id}"),
    })
    .unwrap()
    .revision
}
fn stage_input(
    app: &EditorApplication,
    project: &str,
    base: &str,
    key: &str,
) -> StageEditRecoveryV1 {
    let request = TimelineEditRequestV4 {
        project_id: project.into(),
        expected_revision: base.into(),
        sequence_id: "main".into(),
        idempotency_key: key.into(),
        label: "Add audio track".into(),
        command: TimelineCommandV4::AddTrack {
            id: format!("track-{key}"),
            name: "Audio".into(),
            track_kind: TrackKindV4::Audio,
        },
    };
    let plan = app.plan_v4_timeline(&request).unwrap();
    StageEditRecoveryV1 {
        context: EditRecoveryContextV1::TimelineV4 { request, plan },
    }
}
fn read(project: &str, id: Option<&str>) -> ReadEditRecoveryV1 {
    ReadEditRecoveryV1 {
        project_id: project.into(),
        recovery_id: id.map(str::to_owned),
    }
}
fn ack(project: &str, id: &str) -> AckEditRecoveryV1 {
    AckEditRecoveryV1 {
        project_id: project.into(),
        recovery_id: id.into(),
    }
}
fn rename(app: &mut EditorApplication, project: &str, base: &str, key: &str) -> TransactionResult {
    app.apply_public_v4_project(&ApplyPublicV4Project {
        project_id: project.into(),
        expected_revision: base.into(),
        idempotency_key: key.into(),
        label: "Rename".into(),
        operations: vec![PublicProjectOperationV4::Rename { name: key.into() }],
    })
    .unwrap()
}
fn counts(app: &EditorApplication) -> Vec<i64> {
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
fn exact_stage_survives_reopen_and_ack_uses_original_receipt_after_later_edit() {
    let tmp = tempfile::tempdir().unwrap();
    let database = tmp.path().join("workspace.sqlite");
    let mut app = EditorApplication::open(&database).unwrap();
    let base = create(&mut app, "p");
    let input = stage_input(&app, "p", &base, "original");
    let before = counts(&app);
    let staged = app.stage_edit_recovery_v1(&input).unwrap();
    assert_eq!(
        counts(&app),
        before,
        "staging cannot edit or journal the project"
    );
    assert_eq!(app.stage_edit_recovery_v1(&input).unwrap(), staged);
    drop(app);
    let mut app = EditorApplication::open(&database).unwrap();
    let orphan = app
        .read_edit_recovery_v1(&read("p", None))
        .unwrap()
        .unwrap();
    assert_eq!(
        orphan, staged,
        "no browser pointer is required to discover a staged write"
    );
    let result = app
        .apply_public_v4_project(recovery::transaction(&orphan.context).unwrap())
        .unwrap();
    rename(&mut app, "p", &result.revision, "later");
    assert_eq!(
        app.stage_edit_recovery_v1(&input).unwrap().recovery_id,
        staged.recovery_id,
        "re-stage does not require the old head"
    );
    assert_eq!(
        app.read_edit_recovery_v1(&read("p", Some(&staged.recovery_id)))
            .unwrap()
            .unwrap()
            .receipt,
        Some(result.clone())
    );
    let resolved = app
        .ack_edit_recovery_v1(&ack("p", &staged.recovery_id))
        .unwrap();
    assert_eq!(resolved.resolution, EditRecoveryResolutionV1::Saved);
    assert_eq!(resolved.receipt, Some(result));
    assert!(
        app.read_edit_recovery_v1(&read("p", None))
            .unwrap()
            .is_none()
    );
}

#[test]
fn ack_never_uses_head_as_success_and_same_key_cannot_replace_the_review() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let base = create(&mut app, "p");
    let input = stage_input(&app, "p", &base, "original");
    let staged = app.stage_edit_recovery_v1(&input).unwrap();
    assert_eq!(
        app.ack_edit_recovery_v1(&ack("p", &staged.recovery_id))
            .unwrap_err()
            .code,
        "RECOVERY_UNRESOLVED"
    );
    let mut changed = input.clone();
    let EditRecoveryContextV1::TimelineV4 { plan, .. } = &mut changed.context;
    plan.decision.created_ids.push("different-review".into());
    assert_eq!(
        app.stage_edit_recovery_v1(&changed).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    let other = stage_input(&app, "p", &base, "other");
    assert_eq!(
        app.stage_edit_recovery_v1(&other).unwrap_err().code,
        "RECOVERY_CONFLICT"
    );
    let later = rename(&mut app, "p", &base, "unrelated");
    assert_ne!(later.revision, base);
    let observed = app
        .read_edit_recovery_v1(&read("p", None))
        .unwrap()
        .unwrap();
    assert!(
        observed.receipt.is_none(),
        "a changed head is not the original receipt"
    );
    let resolved = app
        .ack_edit_recovery_v1(&ack("p", &staged.recovery_id))
        .unwrap();
    assert_eq!(resolved.resolution, EditRecoveryResolutionV1::NotSaved);
    assert!(resolved.receipt.is_none());
    assert_eq!(
        app.apply_public_v4_project(recovery::transaction(&input.context).unwrap())
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
}

#[test]
fn credential_scope_and_live_revoke_do_not_erase_unknown_records() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let base = create(&mut app, "p");
    app.set_principal_policy(PrincipalPolicy {
        principal_id: "editor".into(),
        expected_revision: "0".into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: "p".into(),
            actions: vec![ProjectAction::Read, ProjectAction::Edit],
        }],
    })
    .unwrap();
    let first = app.issue_credential("editor").unwrap();
    let second = app.issue_credential("editor").unwrap();
    let caller = app.authenticate(&first.secret).unwrap();
    app.set_caller(caller.clone());
    let input = stage_input(&app, "p", &base, "first");
    let staged = app.stage_edit_recovery_v1(&input).unwrap();
    app.set_caller(app.authenticate(&second.secret).unwrap());
    assert!(
        app.read_edit_recovery_v1(&read("p", None))
            .unwrap()
            .is_none()
    );
    assert!(
        app.read_edit_recovery_v1(&read("p", Some(&staged.recovery_id)))
            .unwrap()
            .is_none()
    );
    app.set_caller(Caller::LocalOwner);
    assert!(
        app.read_edit_recovery_v1(&read("p", None))
            .unwrap()
            .is_none(),
        "owner scope is not an override to another credential's recovery context"
    );
    app.set_principal_policy(PrincipalPolicy {
        principal_id: "editor".into(),
        expected_revision: "1".into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: "p".into(),
            actions: vec![ProjectAction::Read],
        }],
    })
    .unwrap();
    app.set_caller(caller.clone());
    assert_eq!(
        app.read_edit_recovery_v1(&read("p", None))
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.stage_edit_recovery_v1(&input).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.ack_edit_recovery_v1(&ack("p", &staged.recovery_id))
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    app.set_caller(Caller::LocalOwner);
    app.set_principal_policy(PrincipalPolicy {
        principal_id: "editor".into(),
        expected_revision: "2".into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: "p".into(),
            actions: vec![ProjectAction::Read, ProjectAction::Edit],
        }],
    })
    .unwrap();
    app.revoke_credential(&first.credential_id).unwrap();
    app.set_caller(caller);
    assert_eq!(
        app.read_edit_recovery_v1(&read("p", None))
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.connection
            .query_row("SELECT count(*) FROM edit_recovery_v1", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        1
    );
}

#[test]
fn unresolved_quota_refuses_new_stage_without_eviction_and_releases_only_after_ack() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let mut first = None;
    for n in 0..32 {
        let id = format!("p{n}");
        let base = create(&mut app, &id);
        let input = stage_input(&app, &id, &base, "track");
        let staged = app.stage_edit_recovery_v1(&input).unwrap();
        if n == 0 {
            first = Some((input, staged));
        }
    }
    let base = create(&mut app, "overflow");
    let pending = stage_input(&app, "overflow", &base, "track");
    assert_eq!(
        app.stage_edit_recovery_v1(&pending).unwrap_err().code,
        "RECOVERY_QUOTA_EXCEEDED"
    );
    let (input, staged) = first.unwrap();
    assert_eq!(
        app.read_edit_recovery_v1(&read("p0", None))
            .unwrap()
            .unwrap()
            .recovery_id,
        staged.recovery_id
    );
    app.apply_public_v4_project(recovery::transaction(&input.context).unwrap())
        .unwrap();
    app.ack_edit_recovery_v1(&ack("p0", &staged.recovery_id))
        .unwrap();
    app.stage_edit_recovery_v1(&pending).unwrap();
    let mut huge = pending.clone();
    let EditRecoveryContextV1::TimelineV4 { request, plan } = &mut huge.context;
    request.command = TimelineCommandV4::AddTrack {
        id: "large".into(),
        name: "x".repeat(MAX_RECORD_BYTES),
        track_kind: TrackKindV4::Audio,
    };
    plan.original_request_digest = opencut_editor_application::identity::request_identity(
        "project_v4_timeline",
        &request.project_id,
        request,
    )
    .unwrap()
    .current_digest()
    .into();
    assert_eq!(
        recovery::prepare(&huge).err().unwrap().code,
        "RECOVERY_QUOTA_EXCEEDED"
    );
}

#[test]
fn schema_twenty_seven_migration_preserves_raw_history_and_receipt_and_rolls_back_on_collision() {
    fn raw_history(app: &EditorApplication) -> Vec<(String, String)> {
        let mut statement = app
            .connection
            .prepare("SELECT revision,document_json FROM revisions ORDER BY revision")
            .unwrap();
        statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }
    fn raw_receipt(connection: &Connection) -> (String, String) {
        connection.query_row("SELECT request_hash,result_json FROM idempotency WHERE operation_family='project_v4_public_apply' AND scope='p' AND idempotency_key='original'",[],|r| Ok((r.get(0)?,r.get(1)?))).unwrap()
    }
    for collision in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let database = tmp.path().join("workspace.sqlite");
        let mut app = EditorApplication::open(&database).unwrap();
        let base = create(&mut app, "p");
        let input = stage_input(&app, "p", &base, "original");
        let receipt = app
            .apply_public_v4_project(recovery::transaction(&input.context).unwrap())
            .unwrap();
        let history = raw_history(&app);
        let original_receipt = raw_receipt(&app.connection);
        app.connection.execute_batch("DROP TABLE audio_preview_recovery_v1; DROP TABLE font_derivations_v1;
DROP TABLE edit_recovery_v2; DROP TABLE edit_recovery_v1; UPDATE metadata SET value='27' WHERE key='schema_version';").unwrap();
        if collision {
            app.connection
                .execute_batch("CREATE TABLE edit_recovery_v1(collision TEXT)")
                .unwrap();
        }
        drop(app);
        if collision {
            assert!(
                EditorApplication::open(&database).is_err(),
                "migration must reject the unexpected existing table"
            );
            let connection = Connection::open(&database).unwrap();
            assert_eq!(
                connection
                    .query_row::<String, _, _>(
                        "SELECT value FROM metadata WHERE key='schema_version'",
                        [],
                        |r| r.get(0)
                    )
                    .unwrap(),
                "27"
            );
            assert_eq!(
                connection
                    .query_row::<String, _, _>(
                        "SELECT name FROM pragma_table_info('edit_recovery_v1')",
                        [],
                        |r| r.get(0)
                    )
                    .unwrap(),
                "collision"
            );
            assert_eq!(raw_receipt(&connection), original_receipt);
            connection
                .execute_batch("DROP TABLE edit_recovery_v1")
                .unwrap();
            drop(connection);
        }
        let mut migrated = EditorApplication::open(&database).unwrap();
        assert_eq!(
            migrated
                .connection
                .query_row::<String, _, _>(
                    "SELECT value FROM metadata WHERE key='schema_version'",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
            crate::DATABASE_SCHEMA_VERSION.to_string()
        );
        assert_eq!(
            raw_history(&migrated),
            history,
            "migration cannot rewrite historical document bytes"
        );
        assert_eq!(
            raw_receipt(&migrated.connection),
            original_receipt,
            "request identity and receipt bytes remain exact"
        );
        assert!(
            migrated
                .read_edit_recovery_v1(&read("p", None))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            migrated
                .apply_public_v4_project(recovery::transaction(&input.context).unwrap())
                .unwrap(),
            receipt
        );
    }
}
