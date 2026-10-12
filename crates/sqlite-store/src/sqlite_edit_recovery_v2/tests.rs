use super::*;
use opencut_contracts::{
    FrameRate,
    project_v4_history::{RedoV4Project, UndoV4Project},
    project_v4_ops::{CreateV4Project, RestoreV4Revision},
    project_v4_public::{ApplyPublicV4Project, PublicProjectOperationV4},
};
use opencut_editor_application::{
    access::{Caller, PrincipalPolicy, ProjectGrant},
    edit_recovery_v1 as legacy,
};

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
fn apply(project: &str, base: &str, key: &str) -> ApplyPublicV4Project {
    ApplyPublicV4Project {
        project_id: project.into(),
        expected_revision: base.into(),
        idempotency_key: key.into(),
        label: "Rename".into(),
        operations: vec![PublicProjectOperationV4::Rename { name: key.into() }],
    }
}
fn input(intent: EditRecoveryIntentV2) -> StageEditRecoveryV2 {
    StageEditRecoveryV2 {
        context: EditRecoveryContextV2 {
            intent,
            sequence_id: Some("main".into()),
        },
    }
}
fn staged_apply(project: &str, base: &str, key: &str) -> StageEditRecoveryV2 {
    input(EditRecoveryIntentV2::PublicApply {
        request: Box::new(apply(project, base, key)),
    })
}
fn read(project: &str) -> ReadEditRecoveryV2 {
    ReadEditRecoveryV2 {
        project_id: project.into(),
        recovery_id: None,
    }
}
fn ack(record: &EditRecoveryRecordV2) -> AckEditRecoveryV2 {
    AckEditRecoveryV2 {
        project_id: record.project_id.clone(),
        recovery_id: record.recovery_id.clone(),
    }
}
fn current(app: &EditorApplication, project: &str) -> EditRecoveryRecordV2 {
    let Some(DiscoveredEditRecoveryV2::CurrentV2 { record }) =
        app.inspect_edit_recovery_v2(&read(project)).unwrap().record
    else {
        panic!("expected V2 record")
    };
    *record
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
fn legacy_input(
    app: &EditorApplication,
    project: &str,
    base: &str,
    key: &str,
) -> legacy::StageEditRecoveryV1 {
    use opencut_contracts::{
        project_v4::TrackKindV4,
        timeline_v4::{TimelineCommandV4, TimelineEditRequestV4},
    };
    let request = TimelineEditRequestV4 {
        project_id: project.into(),
        expected_revision: base.into(),
        sequence_id: "main".into(),
        idempotency_key: key.into(),
        label: "Add track".into(),
        command: TimelineCommandV4::AddTrack {
            id: format!("track-{key}"),
            name: "Audio".into(),
            track_kind: TrackKindV4::Audio,
        },
    };
    let plan = app.plan_v4_timeline(&request).unwrap();
    legacy::StageEditRecoveryV1 {
        context: legacy::EditRecoveryContextV1::TimelineV4 { request, plan },
    }
}

#[test]
fn public_apply_reopen_exact_restage_and_readonly_original_receipt_after_later_head() {
    let tmp = tempfile::tempdir().unwrap();
    let database = tmp.path().join("workspace.sqlite");
    let mut app = EditorApplication::open(&database).unwrap();
    let base = create(&mut app, "p");
    let request = staged_apply("p", &base, "original");
    let before = counts(&app);
    let staged = app.stage_edit_recovery_v2(&request).unwrap();
    assert_eq!(counts(&app), before);
    assert_eq!(
        app.ack_edit_recovery_v2(&ack(&staged)).unwrap_err().code,
        "RECOVERY_UNRESOLVED"
    );
    let mut changed = request.clone();
    changed.context.sequence_id = None;
    assert_eq!(
        app.stage_edit_recovery_v2(&changed).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    drop(app);
    let mut app = EditorApplication::open(&database).unwrap();
    assert_eq!(current(&app, "p"), staged);
    let original = app
        .apply_public_v4_project(&apply("p", &base, "original"))
        .unwrap();
    app.apply_public_v4_project(&apply("p", &original.revision, "later"))
        .unwrap();
    let before = counts(&app);
    let expected = EditRecoveryReceiptV2::PublicApply {
        transaction: original.clone(),
    };
    assert_eq!(
        app.stage_edit_recovery_v2(&request).unwrap().receipt,
        Some(expected.clone())
    );
    assert_eq!(current(&app, "p").receipt, Some(expected.clone()));
    assert_eq!(
        counts(&app),
        before,
        "receipt reads/replays cannot mutate or follow head"
    );
    let resolved = app.ack_edit_recovery_v2(&ack(&staged)).unwrap();
    assert_eq!(resolved.resolution, EditRecoveryResolutionV2::Saved);
    assert_eq!(resolved.receipt, Some(expected));
    assert!(
        app.inspect_edit_recovery_v2(&read("p"))
            .unwrap()
            .record
            .is_none()
    );
}

#[test]
fn history_undo_redo_receipts_preserve_complete_causal_reply_without_reexecution() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let base = create(&mut app, "p");
    let original = app
        .apply_public_v4_project(&apply("p", &base, "edit"))
        .unwrap();
    let undo = UndoV4Project {
        project_id: "p".into(),
        expected_revision: original.revision.clone(),
        target_transaction_id: original.transaction_id.clone(),
        idempotency_key: "undo".into(),
    };
    let staged = app
        .stage_edit_recovery_v2(&input(EditRecoveryIntentV2::Undo {
            request: undo.clone(),
        }))
        .unwrap();
    let committed = app.apply_v4_undo(&undo).unwrap();
    let before = counts(&app);
    let receipt = EditRecoveryReceiptV2::Undo {
        commit: committed.clone(),
    };
    assert_eq!(current(&app, "p").receipt, Some(receipt.clone()));
    assert_eq!(counts(&app), before);
    assert_eq!(
        app.ack_edit_recovery_v2(&ack(&staged)).unwrap().receipt,
        Some(receipt)
    );
    let redo = RedoV4Project {
        project_id: "p".into(),
        expected_revision: committed.transaction.revision.clone(),
        undo_transaction_id: committed.transaction.transaction_id.clone(),
        idempotency_key: "redo".into(),
    };
    let request = input(EditRecoveryIntentV2::Redo {
        request: redo.clone(),
    });
    let staged = app.stage_edit_recovery_v2(&request).unwrap();
    let committed = app.apply_v4_redo(&redo).unwrap();
    app.apply_public_v4_project(&apply("p", &committed.transaction.revision, "later"))
        .unwrap();
    let before = counts(&app);
    let receipt = EditRecoveryReceiptV2::Redo { commit: committed };
    assert_eq!(
        app.stage_edit_recovery_v2(&request).unwrap().receipt,
        Some(receipt.clone())
    );
    assert_eq!(current(&app, "p").receipt, Some(receipt.clone()));
    assert_eq!(counts(&app), before);
    assert_eq!(
        app.ack_edit_recovery_v2(&ack(&staged)).unwrap().receipt,
        Some(receipt)
    );
}

#[test]
fn restore_receipt_and_missing_receipt_changed_head_resolution_are_distinct() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let base = create(&mut app, "p");
    let edited = app
        .apply_public_v4_project(&apply("p", &base, "first"))
        .unwrap();
    let restore = RestoreV4Revision {
        project_id: "p".into(),
        expected_revision: edited.revision.clone(),
        target_revision: base.clone(),
        asset_versions: Default::default(),
        idempotency_key: "restore".into(),
    };
    let staged = app
        .stage_edit_recovery_v2(&input(EditRecoveryIntentV2::Restore {
            request: restore.clone(),
        }))
        .unwrap();
    let committed = app.restore_v4_revision(&restore).unwrap();
    app.apply_public_v4_project(&apply("p", &committed.revision, "later"))
        .unwrap();
    assert_eq!(
        app.ack_edit_recovery_v2(&ack(&staged)).unwrap().receipt,
        Some(EditRecoveryReceiptV2::Restore {
            transaction: committed
        })
    );
    let head = sqlite_project_document::project_head_revision(&app.connection, "p").unwrap();
    let staged = app
        .stage_edit_recovery_v2(&staged_apply("p", &head, "not-submitted"))
        .unwrap();
    app.apply_public_v4_project(&apply("p", &head, "other"))
        .unwrap();
    assert!(current(&app, "p").receipt.is_none());
    let resolved = app.ack_edit_recovery_v2(&ack(&staged)).unwrap();
    assert_eq!(resolved.resolution, EditRecoveryResolutionV2::NotSaved);
    assert!(resolved.receipt.is_none());
}

#[test]
fn legacy_discovery_preserves_bytes_and_v1_v2_share_scope_and_global_quota() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let base = create(&mut app, "p0");
    let legacy = legacy_input(&app, "p0", &base, "original");
    let staged = app.stage_edit_recovery_v1(&legacy).unwrap();
    let raw: (String, String) = app
        .connection
        .query_row(
            "SELECT context_json,request_digest FROM edit_recovery_v1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let Some(DiscoveredEditRecoveryV2::LegacyV1 { record }) =
        app.inspect_edit_recovery_v2(&read("p0")).unwrap().record
    else {
        panic!("legacy discovery")
    };
    assert_eq!(*record, staged);
    assert_eq!(
        app.stage_edit_recovery_v2(&staged_apply("p0", &base, "original"))
            .unwrap_err()
            .code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    assert_eq!(
        app.stage_edit_recovery_v2(&staged_apply("p0", &base, "other"))
            .unwrap_err()
            .code,
        "RECOVERY_CONFLICT"
    );
    assert_eq!(
        app.ack_edit_recovery_v2(&AckEditRecoveryV2 {
            project_id: "p0".into(),
            recovery_id: staged.recovery_id
        })
        .unwrap_err()
        .code,
        "NOT_FOUND"
    );
    for n in 1..32 {
        let project = format!("p{n}");
        let base = create(&mut app, &project);
        if n % 2 == 0 {
            let input = legacy_input(&app, &project, &base, "original");
            app.stage_edit_recovery_v1(&input).unwrap();
        } else {
            app.stage_edit_recovery_v2(&staged_apply(&project, &base, "original"))
                .unwrap();
        }
    }
    let base = create(&mut app, "overflow");
    assert_eq!(
        app.stage_edit_recovery_v2(&staged_apply("overflow", &base, "full"))
            .unwrap_err()
            .code,
        "RECOVERY_QUOTA_EXCEEDED"
    );
    let input = legacy_input(&app, "overflow", &base, "full");
    assert_eq!(
        app.stage_edit_recovery_v1(&input).unwrap_err().code,
        "RECOVERY_QUOTA_EXCEEDED"
    );
    let before = app
        .connection
        .query_row(
            "SELECT context_json,request_digest FROM edit_recovery_v1 WHERE project_id='p0'",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .unwrap();
    assert_eq!(before, raw);
    // Reciprocal pending exclusion still uses exact key identity, not a family fallback.
    let project = "p1";
    let head = sqlite_project_document::project_head_revision(&app.connection, project).unwrap();
    let input = legacy_input(&app, project, &head, "original");
    assert_eq!(
        app.stage_edit_recovery_v1(&input).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
}

#[test]
fn loaded_original_action_and_credential_remain_required_but_empty_discovery_only_requires_read() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let base = create(&mut app, "p");
    let policy = |revision: &str, actions: Vec<ProjectAction>| PrincipalPolicy {
        principal_id: "editor".into(),
        expected_revision: revision.into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: "p".into(),
            actions,
        }],
    };
    app.set_principal_policy(policy(
        "0",
        vec![ProjectAction::Read, ProjectAction::Restore],
    ))
    .unwrap();
    let first = app.issue_credential("editor").unwrap();
    let second = app.issue_credential("editor").unwrap();
    let caller = app.authenticate(&first.secret).unwrap();
    app.set_caller(caller.clone());
    assert!(
        app.inspect_edit_recovery_v2(&read("p"))
            .unwrap()
            .record
            .is_none()
    );
    let restore = RestoreV4Revision {
        project_id: "p".into(),
        expected_revision: base.clone(),
        target_revision: base,
        asset_versions: Default::default(),
        idempotency_key: "restore".into(),
    };
    let request = input(EditRecoveryIntentV2::Restore { request: restore });
    let staged = app.stage_edit_recovery_v2(&request).unwrap();
    app.set_caller(app.authenticate(&second.secret).unwrap());
    assert!(
        app.inspect_edit_recovery_v2(&read("p"))
            .unwrap()
            .record
            .is_none()
    );
    app.set_caller(Caller::LocalOwner);
    assert!(
        app.inspect_edit_recovery_v2(&read("p"))
            .unwrap()
            .record
            .is_none()
    );
    app.set_principal_policy(policy("1", vec![ProjectAction::Read]))
        .unwrap();
    app.set_caller(caller.clone());
    assert_eq!(
        app.inspect_edit_recovery_v2(&read("p")).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.stage_edit_recovery_v2(&request).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.ack_edit_recovery_v2(&ack(&staged)).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    app.set_caller(Caller::LocalOwner);
    app.set_principal_policy(policy(
        "2",
        vec![ProjectAction::Read, ProjectAction::Restore],
    ))
    .unwrap();
    app.revoke_credential(&first.credential_id).unwrap();
    app.set_caller(caller);
    assert_eq!(
        app.inspect_edit_recovery_v2(&read("p")).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.connection
            .query_row("SELECT count(*) FROM edit_recovery_v2", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        1
    );
}

#[test]
fn wrong_family_receipt_is_not_success_and_ack_delete_failure_rolls_back_record() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let base = create(&mut app, "p");
    let request = staged_apply("p", &base, "original");
    let staged = app.stage_edit_recovery_v2(&request).unwrap();
    let committed = app
        .apply_public_v4_project(&apply("p", &base, "original"))
        .unwrap();
    app.connection.execute("UPDATE idempotency SET operation_family='project_v4_restore' WHERE scope='p' AND idempotency_key='original'",[]).unwrap();
    assert_eq!(
        app.inspect_edit_recovery_v2(&read("p")).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    assert_eq!(
        app.ack_edit_recovery_v2(&ack(&staged)).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    app.connection.execute("UPDATE idempotency SET operation_family='project_v4_public_apply' WHERE scope='p' AND idempotency_key='original'",[]).unwrap();
    app.connection.execute_batch("CREATE TRIGGER reject_recovery_ack BEFORE DELETE ON edit_recovery_v2 BEGIN SELECT RAISE(ABORT,'ack failure'); END;").unwrap();
    assert!(app.ack_edit_recovery_v2(&ack(&staged)).is_err());
    assert_eq!(
        current(&app, "p").receipt,
        Some(EditRecoveryReceiptV2::PublicApply {
            transaction: committed
        })
    );
}

#[test]
fn corrupted_full_result_or_same_base_other_journal_cannot_be_saved_or_acknowledged() {
    for other_transaction in [false, true] {
        let mut app = EditorApplication::open(":memory:").unwrap();
        let base = create(&mut app, "p");
        let input = staged_apply("p", &base, "original");
        let staged = app.stage_edit_recovery_v2(&input).unwrap();
        let original = app
            .apply_public_v4_project(&apply("p", &base, "original"))
            .unwrap();
        let corrupted = if other_transaction {
            // Build a second real commit on the same parent for this corruption
            // fixture. Both journals/revision edges are individually valid;
            // its request identity must still not resolve the original edit.
            app.connection
                .execute(
                    "UPDATE projects SET head_revision=?1 WHERE project_id='p'",
                    [&base],
                )
                .unwrap();
            app.apply_public_v4_project(&apply("p", &base, "other"))
                .unwrap()
        } else {
            let mut result = original;
            result.created_ids.push("invented-id".into());
            result
        };
        app.connection.execute("UPDATE idempotency SET result_json=?1 WHERE operation_family='project_v4_public_apply' AND scope='p' AND idempotency_key='original'",
            [serde_json::to_string(&corrupted).unwrap()]).unwrap();
        let before = counts(&app);
        assert_eq!(
            app.inspect_edit_recovery_v2(&read("p")).unwrap_err().code,
            "STORAGE_ERROR"
        );
        assert_eq!(
            app.ack_edit_recovery_v2(&ack(&staged)).unwrap_err().code,
            "STORAGE_ERROR"
        );
        assert_eq!(counts(&app), before);
        assert_eq!(
            app.connection
                .query_row::<u32, _, _>(
                    "SELECT count(*) FROM edit_recovery_v2 WHERE recovery_id=?1",
                    [&staged.recovery_id],
                    |row| row.get(0)
                )
                .unwrap(),
            1
        );
    }
}

#[test]
fn db28_to29_preserves_old_raw_context_digest_history_and_rollback_on_table_collision() {
    for collision in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let database = tmp.path().join("workspace.sqlite");
        let mut app = EditorApplication::open(&database).unwrap();
        let base = create(&mut app, "p");
        let input = legacy_input(&app, "p", &base, "original");
        app.stage_edit_recovery_v1(&input).unwrap();
        let raw = format!("\n{}", serde_json::to_string(&input).unwrap());
        let digest = legacy::exact_payload_digest(&raw);
        app.connection.execute("UPDATE edit_recovery_v1 SET context_json=?1,request_digest=?2,stored_bytes=stored_bytes+1",params![raw,digest]).unwrap();
        let history: String = app
            .connection
            .query_row(
                "SELECT document_json FROM revisions WHERE revision=?1",
                [&base],
                |r| r.get(0),
            )
            .unwrap();
        app.connection.execute_batch("DROP TRIGGER recovery_v1_excludes_v2; DROP TABLE edit_recovery_v2; UPDATE metadata SET value='28' WHERE key='schema_version';").unwrap();
        if collision {
            app.connection
                .execute_batch("CREATE TABLE edit_recovery_v2(collision TEXT)")
                .unwrap();
        }
        drop(app);
        if collision {
            assert!(EditorApplication::open(&database).is_err());
            let db = Connection::open(&database).unwrap();
            assert_eq!(
                db.query_row::<String, _, _>(
                    "SELECT value FROM metadata WHERE key='schema_version'",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
                "28"
            );
            assert_eq!(
                db.query_row::<String, _, _>(
                    "SELECT context_json FROM edit_recovery_v1",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
                raw
            );
            db.execute_batch("DROP TABLE edit_recovery_v2").unwrap();
        }
        let app = EditorApplication::open(&database).unwrap();
        assert_eq!(
            app.connection
                .query_row::<String, _, _>(
                    "SELECT value FROM metadata WHERE key='schema_version'",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
            "29"
        );
        assert_eq!(
            app.connection
                .query_row::<(String, String), _, _>(
                    "SELECT context_json,request_digest FROM edit_recovery_v1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?))
                )
                .unwrap(),
            (raw, digest)
        );
        assert_eq!(
            app.connection
                .query_row::<String, _, _>(
                    "SELECT document_json FROM revisions WHERE revision=?1",
                    [&base],
                    |r| r.get(0)
                )
                .unwrap(),
            history
        );
        assert!(matches!(
            app.inspect_edit_recovery_v2(&read("p")).unwrap().record,
            Some(DiscoveredEditRecoveryV2::LegacyV1 { .. })
        ));
    }
}

#[test]
fn concurrent_v1_v2_staging_has_one_pending_owner_scope_and_no_project_write() {
    for same_key in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let database = tmp.path().join("workspace.sqlite");
        let mut app = EditorApplication::open(&database).unwrap();
        let base = create(&mut app, "p");
        let old = legacy_input(&app, "p", &base, "old");
        let new = staged_apply("p", &base, if same_key { "old" } else { "new" });
        let before = counts(&app);
        drop(app);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let first_database = database.clone();
        let first_barrier = barrier.clone();
        let first = std::thread::spawn(move || {
            let mut app = EditorApplication::open(&first_database).unwrap();
            first_barrier.wait();
            app.stage_edit_recovery_v1(&old).map(|_| ())
        });
        let second = std::thread::spawn(move || {
            let mut app = EditorApplication::open(&database).unwrap();
            barrier.wait();
            app.stage_edit_recovery_v2(&new).map(|_| ())
        });
        let results = [first.join().unwrap(), second.join().unwrap()];
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results.into_iter().find_map(Result::err).unwrap().code,
            if same_key {
                "IDEMPOTENCY_KEY_REUSED"
            } else {
                "RECOVERY_CONFLICT"
            }
        );
        let app = EditorApplication::open(&tmp.path().join("workspace.sqlite")).unwrap();
        assert_eq!(counts(&app), before);
        assert_eq!(app.connection.query_row::<u32, _, _>(
            "SELECT (SELECT count(*) FROM edit_recovery_v1)+(SELECT count(*) FROM edit_recovery_v2)", [], |row| row.get(0)).unwrap(), 1);
    }
}

#[test]
fn shared_record_byte_limit_charges_scope_and_digest_and_counts_both_versions() {
    use opencut_editor_application::edit_recovery_v1::{MAX_RECORD_BYTES, MAX_UNRESOLVED_BYTES};
    let mut app = EditorApplication::open(":memory:").unwrap();
    let base = create(&mut app, "p");
    let source = sqlite_project_v4::load_source(&app.connection, "p", Some(&base)).unwrap();
    let opencut_editor_application::project_v4::StoredProjectDocumentV4::V4(project) =
        source.document
    else {
        panic!("V4 fixture")
    };
    let mut track = project.sequences[0].tracks[0].clone();
    track.name.clear();
    let mut request = input(EditRecoveryIntentV2::PublicApply {
        request: Box::new(ApplyPublicV4Project {
            operations: vec![PublicProjectOperationV4::TrackPut {
                sequence_id: "main".into(),
                track,
            }],
            ..apply("p", &base, "byte-boundary")
        }),
    });
    let identity = scope(&app.connection.1);
    let overhead = "recovery_".len()
        + 32
        + "p".len()
        + identity.principal.len()
        + identity.credential.len()
        + 71
        + 16;
    let empty = recovery::prepare(&request).unwrap().payload().len();
    let EditRecoveryIntentV2::PublicApply {
        request: transaction,
    } = &mut request.context.intent
    else {
        unreachable!()
    };
    let PublicProjectOperationV4::TrackPut { track, .. } = &mut transaction.operations[0] else {
        unreachable!()
    };
    // This is a valid empty main-track replacement, not oversized untyped SQL.
    track.name = "x".repeat(MAX_RECORD_BYTES - overhead - empty + 1);
    let prepared = recovery::prepare(&request).unwrap();
    assert_eq!(prepared.payload().len() + overhead, MAX_RECORD_BYTES + 1);
    assert!(
        prepared.payload().len() < MAX_RECORD_BYTES,
        "typed payload alone fits; its durable scope charge does not"
    );
    let before = counts(&app);
    assert_eq!(
        app.stage_edit_recovery_v2(&request).unwrap_err().code,
        "RECOVERY_QUOTA_EXCEEDED"
    );
    assert_eq!(counts(&app), before);
    assert!(
        app.inspect_edit_recovery_v2(&read("p"))
            .unwrap()
            .record
            .is_none()
    );
    let EditRecoveryIntentV2::PublicApply {
        request: transaction,
    } = &mut request.context.intent
    else {
        unreachable!()
    };
    let PublicProjectOperationV4::TrackPut { track, .. } = &mut transaction.operations[0] else {
        unreachable!()
    };
    track.name.pop();
    app.stage_edit_recovery_v2(&request).unwrap();
    assert_eq!(
        app.connection
            .query_row::<u64, _, _>(
                "SELECT stored_bytes FROM edit_recovery_v2 WHERE project_id='p'",
                [],
                |row| row.get(0)
            )
            .unwrap(),
        MAX_RECORD_BYTES as u64
    );
    let other_base = create(&mut app, "legacy");
    let old = legacy_input(&app, "legacy", &other_base, "track");
    app.stage_edit_recovery_v1(&old).unwrap();
    let other_base = create(&mut app, "current");
    app.stage_edit_recovery_v2(&staged_apply("current", &other_base, "rename"))
        .unwrap();
    let v1: u64 = app
        .connection
        .query_row(
            "SELECT sum(stored_bytes) FROM edit_recovery_v1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let v2: u64 = app
        .connection
        .query_row(
            "SELECT sum(stored_bytes) FROM edit_recovery_v2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let (count,total):(u64,u64) = app.connection.query_row("SELECT count(*),sum(stored_bytes) FROM (SELECT stored_bytes FROM edit_recovery_v1 UNION ALL SELECT stored_bytes FROM edit_recovery_v2)",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
    assert_eq!(count, 3);
    assert_eq!(total, v1 + v2);
    assert!(total > MAX_RECORD_BYTES as u64);
    let before = counts(&app);
    sqlite_edit_recovery_shared::reserve(&app.connection, MAX_RECORD_BYTES).unwrap();
    assert_eq!(
        sqlite_edit_recovery_shared::reserve(&app.connection, MAX_RECORD_BYTES + 1)
            .unwrap_err()
            .code,
        "RECOVERY_QUOTA_EXCEEDED"
    );
    assert_eq!(counts(&app), before);
    // For valid rows 32 * 2 MiB == 64 MiB: the total-byte ceiling is a
    // redundant safety fence, not an independently reachable early rejection.
    assert_eq!(32 * MAX_RECORD_BYTES as u64, MAX_UNRESOLVED_BYTES);
}
