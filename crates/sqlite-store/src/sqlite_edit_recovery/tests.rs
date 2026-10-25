use super::*;
use opencut_contracts::{
    FrameRate,
    project_history::{RedoRequest, UndoRequest},
    project_public::{ApplyProject, ProjectEditOperation},
    scene_ops::{CreateProjectRequest, RestoreProject},
};
use opencut_editor_application::access::{Caller, PrincipalPolicy, ProjectGrant};

fn create(app: &mut EditorApplication, id: &str) -> String {
    app.create_scene_project(&CreateProjectRequest {
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
fn apply(project: &str, base: &str, key: &str) -> ApplyProject {
    ApplyProject {
        branch_name: None,
        project_id: project.into(),
        expected_revision: base.into(),
        idempotency_key: key.into(),
        label: "Rename".into(),
        operations: vec![ProjectEditOperation::Rename { name: key.into() }],
    }
}
fn input(intent: EditRecoveryIntent) -> StageEditRecovery {
    StageEditRecovery {
        context: EditRecoveryContext {
            intent,
            sequence_id: Some("main".into()),
        },
    }
}
fn staged_apply(project: &str, base: &str, key: &str) -> StageEditRecovery {
    input(EditRecoveryIntent::PublicApply {
        request: Box::new(apply(project, base, key)),
    })
}
fn read(project: &str) -> ReadEditRecovery {
    ReadEditRecovery {
        project_id: project.into(),
        recovery_id: None,
    }
}
fn ack(record: &EditRecoveryRecord) -> AckEditRecovery {
    AckEditRecovery {
        project_id: record.project_id.clone(),
        recovery_id: record.recovery_id.clone(),
    }
}
fn current(app: &EditorApplication, project: &str) -> EditRecoveryRecord {
    app.inspect_edit_recovery(&read(project))
        .unwrap()
        .record
        .expect("original record")
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
fn timeline_input(
    app: &EditorApplication,
    project: &str,
    base: &str,
    key: &str,
) -> StageEditRecovery {
    let (input, _, _) = historical_timeline(app, project, base, key);
    input
}
fn historical_timeline(
    app: &EditorApplication,
    project: &str,
    base: &str,
    key: &str,
) -> (StageEditRecovery, String, String) {
    use opencut_contracts::{
        scene::SceneTrackKind,
        timeline::{TimelineCommand, TimelineEditRequest},
    };
    use sha2::{Digest, Sha256};
    let request = TimelineEditRequest {
        project_id: project.into(),
        expected_revision: base.into(),
        sequence_id: "main".into(),
        idempotency_key: key.into(),
        label: "Add track".into(),
        command: TimelineCommand::AddTrack {
            id: format!("track-{key}"),
            name: "Audio".into(),
            track_kind: SceneTrackKind::Audio,
        },
    };
    let mut plan = app.plan_project_timeline(&request).unwrap();
    plan.original_request_digest = opencut_editor_application::identity::request_identity(
        "project_v4_timeline",
        project,
        &request,
    )
    .unwrap()
    .current_digest()
    .into();
    let input = StageEditRecovery {
        context: EditRecoveryContext {
            intent: EditRecoveryIntent::PublicApply {
                request: Box::new(plan.transaction.clone().unwrap()),
            },
            sequence_id: Some("main".into()),
        },
    };
    let raw = format!(
        "\n{}",
        serde_json::json!({"context":{"kind":"timeline_v4","request":request,"plan":plan}})
    );
    let mut hash = Sha256::new();
    hash.update(b"opencut.edit-recovery.v1\0");
    hash.update(raw.as_bytes());
    (input, raw, format!("sha256:{:x}", hash.finalize()))
}
fn insert_stored(app: &EditorApplication, table: &str, project: &str, raw: &str, digest: &str) {
    let identity = scope(&app.connection.1);
    app.connection.execute(&format!("INSERT INTO {table}(recovery_id,project_id,principal_id,credential_id,owner,request_digest,context_json,stored_bytes) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)"),
        params![format!("original-{project}"),project,identity.principal,identity.credential,identity.owner,digest,raw,raw.len()+128]).unwrap();
}

#[test]
fn public_apply_reopen_exact_restage_and_readonly_original_receipt_after_later_head() {
    let tmp = tempfile::tempdir().unwrap();
    let database = tmp.path().join("workspace.sqlite");
    let mut app = EditorApplication::open(&database).unwrap();
    let base = create(&mut app, "p");
    let request = staged_apply("p", &base, "original");
    let before = counts(&app);
    let staged = app.stage_edit_recovery(&request).unwrap();
    assert_eq!(counts(&app), before);
    assert_eq!(
        app.ack_edit_recovery(&ack(&staged)).unwrap_err().code,
        "RECOVERY_UNRESOLVED"
    );
    let mut changed = request.clone();
    changed.context.sequence_id = None;
    assert_eq!(
        app.stage_edit_recovery(&changed).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    drop(app);
    let mut app = EditorApplication::open(&database).unwrap();
    assert_eq!(current(&app, "p"), staged);
    let original = app
        .apply_editor_edit(&apply("p", &base, "original"))
        .unwrap();
    app.apply_editor_edit(&apply("p", &original.revision, "later"))
        .unwrap();
    let before = counts(&app);
    let expected = EditRecoveryReceipt::PublicApply {
        transaction: original.clone(),
    };
    assert_eq!(
        app.stage_edit_recovery(&request).unwrap().receipt,
        Some(expected.clone())
    );
    assert_eq!(current(&app, "p").receipt, Some(expected.clone()));
    assert_eq!(
        counts(&app),
        before,
        "receipt reads/replays cannot mutate or follow head"
    );
    let resolved = app.ack_edit_recovery(&ack(&staged)).unwrap();
    assert_eq!(resolved.resolution, EditRecoveryResolution::Saved);
    assert_eq!(resolved.receipt, Some(expected));
    assert!(
        app.inspect_edit_recovery(&read("p"))
            .unwrap()
            .record
            .is_none()
    );
}

#[test]
fn history_undo_redo_receipts_preserve_complete_causal_reply_without_reexecution() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let base = create(&mut app, "p");
    let original = app.apply_editor_edit(&apply("p", &base, "edit")).unwrap();
    let undo = UndoRequest {
        project_id: "p".into(),
        expected_revision: original.revision.clone(),
        target_transaction_id: original.transaction_id.clone(),
        idempotency_key: "undo".into(),
    };
    let staged = app
        .stage_edit_recovery(&input(EditRecoveryIntent::Undo {
            request: undo.clone(),
        }))
        .unwrap();
    let committed = app.apply_project_undo(&undo).unwrap();
    let before = counts(&app);
    let receipt = EditRecoveryReceipt::Undo {
        commit: committed.clone(),
    };
    assert_eq!(current(&app, "p").receipt, Some(receipt.clone()));
    assert_eq!(counts(&app), before);
    assert_eq!(
        app.ack_edit_recovery(&ack(&staged)).unwrap().receipt,
        Some(receipt)
    );
    let redo = RedoRequest {
        project_id: "p".into(),
        expected_revision: committed.transaction.revision.clone(),
        undo_transaction_id: committed.transaction.transaction_id.clone(),
        idempotency_key: "redo".into(),
    };
    let request = input(EditRecoveryIntent::Redo {
        request: redo.clone(),
    });
    let staged = app.stage_edit_recovery(&request).unwrap();
    let committed = app.apply_project_redo(&redo).unwrap();
    app.apply_editor_edit(&apply("p", &committed.transaction.revision, "later"))
        .unwrap();
    let before = counts(&app);
    let receipt = EditRecoveryReceipt::Redo { commit: committed };
    assert_eq!(
        app.stage_edit_recovery(&request).unwrap().receipt,
        Some(receipt.clone())
    );
    assert_eq!(current(&app, "p").receipt, Some(receipt.clone()));
    assert_eq!(counts(&app), before);
    assert_eq!(
        app.ack_edit_recovery(&ack(&staged)).unwrap().receipt,
        Some(receipt)
    );
}

#[test]
fn restore_receipt_and_missing_receipt_changed_head_resolution_are_distinct() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let base = create(&mut app, "p");
    let edited = app.apply_editor_edit(&apply("p", &base, "first")).unwrap();
    let restore = RestoreProject {
        branch_name: None,
        label: None,
        project_id: "p".into(),
        expected_revision: edited.revision.clone(),
        target_revision: base.clone(),
        asset_versions: Default::default(),
        idempotency_key: "restore".into(),
    };
    let staged = app
        .stage_edit_recovery(&input(EditRecoveryIntent::Restore {
            request: restore.clone(),
        }))
        .unwrap();
    let committed = app.restore_project(&restore).unwrap();
    app.apply_editor_edit(&apply("p", &committed.revision, "later"))
        .unwrap();
    assert_eq!(
        app.ack_edit_recovery(&ack(&staged)).unwrap().receipt,
        Some(EditRecoveryReceipt::Restore {
            transaction: committed
        })
    );
    let head = sqlite_project_document::project_head_revision(&app.connection, "p").unwrap();
    let staged = app
        .stage_edit_recovery(&staged_apply("p", &head, "not-submitted"))
        .unwrap();
    app.apply_editor_edit(&apply("p", &head, "other")).unwrap();
    assert!(current(&app, "p").receipt.is_none());
    let resolved = app.ack_edit_recovery(&ack(&staged)).unwrap();
    assert_eq!(resolved.resolution, EditRecoveryResolution::NotSaved);
    assert!(resolved.receipt.is_none());
}

#[test]
fn historical_discovery_normalizes_without_rewriting_and_shares_scope_and_quota() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let base = create(&mut app, "p0");
    let (input, raw, digest) = historical_timeline(&app, "p0", &base, "original");
    insert_stored(&app, "edit_recovery", "p0", &raw, &digest);
    let record = current(&app, "p0");
    assert_eq!(record.context, input.context);
    assert_eq!(app.stage_edit_recovery(&input).unwrap(), record);
    assert_eq!(
        app.stage_edit_recovery(&staged_apply("p0", &base, "original"))
            .unwrap_err()
            .code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    assert_eq!(
        app.stage_edit_recovery(&staged_apply("p0", &base, "other"))
            .unwrap_err()
            .code,
        "RECOVERY_CONFLICT"
    );
    let derived_id = opencut_editor_application::font_derived::catalog().unwrap()[0]
        .source
        .face
        .id
        .clone();
    for n in 1..32 {
        let project = format!("p{n}");
        let base = create(&mut app, &project);
        if n % 3 == 0 {
            app.stage_edit_recovery(&staged_apply(&project, &base, "original"))
                .unwrap();
        } else {
            use sha2::{Digest, Sha256};
            let derived = n % 3 == 2;
            let request = opencut_contracts::font_import::ImportFont {
                project_id: project.clone(),
                expected_revision: base,
                idempotency_key: "original".into(),
                catalog_id: if derived {
                    derived_id.clone()
                } else {
                    "google-lato-regular-v1".into()
                },
                font_id: "font".into(),
                required_text: "AV".into(),
            };
            let raw =
                serde_json::json!({"kind":if derived{"derived_font_import_v1"}else{"font_import_v1"},
                "context":{"request":request,"sequence_id":"main"}})
                .to_string();
            let domain: &[u8] = if derived {
                b"opencut.derived-font-recovery.v1\0"
            } else {
                b"opencut.font-recovery.v1\0"
            };
            let mut hash = Sha256::new();
            hash.update(domain);
            hash.update(raw.as_bytes());
            let digest = format!("sha256:{:x}", hash.finalize());
            insert_stored(&app, "edit_recovery", &project, &raw, &digest);
            assert_eq!(
                current(&app, &project).context.intent,
                EditRecoveryIntent::FontImport { request }
            );
            let stored: (String, String) = app
                .connection
                .query_row(
                    "SELECT context_json,request_digest FROM edit_recovery WHERE project_id=?1",
                    [&project],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(stored, (raw, digest));
        }
    }
    let base = create(&mut app, "overflow");
    assert_eq!(
        app.stage_edit_recovery(&staged_apply("overflow", &base, "full"))
            .unwrap_err()
            .code,
        "RECOVERY_QUOTA_EXCEEDED"
    );
    let stored: (String, String) = app
        .connection
        .query_row(
            "SELECT context_json,request_digest FROM edit_recovery WHERE project_id='p0'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(stored, (raw, digest));
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
        app.inspect_edit_recovery(&read("p"))
            .unwrap()
            .record
            .is_none()
    );
    let restore = RestoreProject {
        branch_name: None,
        label: None,
        project_id: "p".into(),
        expected_revision: base.clone(),
        target_revision: base,
        asset_versions: Default::default(),
        idempotency_key: "restore".into(),
    };
    let request = input(EditRecoveryIntent::Restore { request: restore });
    let staged = app.stage_edit_recovery(&request).unwrap();
    app.set_caller(app.authenticate(&second.secret).unwrap());
    assert!(
        app.inspect_edit_recovery(&read("p"))
            .unwrap()
            .record
            .is_none()
    );
    app.set_caller(Caller::LocalOwner);
    assert!(
        app.inspect_edit_recovery(&read("p"))
            .unwrap()
            .record
            .is_none()
    );
    app.set_principal_policy(policy("1", vec![ProjectAction::Read]))
        .unwrap();
    app.set_caller(caller.clone());
    assert_eq!(
        app.inspect_edit_recovery(&read("p")).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.stage_edit_recovery(&request).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.ack_edit_recovery(&ack(&staged)).unwrap_err().code,
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
        app.inspect_edit_recovery(&read("p")).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        app.connection
            .query_row("SELECT count(*) FROM edit_recovery", [], |r| r
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
    let staged = app.stage_edit_recovery(&request).unwrap();
    let committed = app
        .apply_editor_edit(&apply("p", &base, "original"))
        .unwrap();
    app.connection.execute("UPDATE idempotency SET operation_family='project_v4_restore' WHERE scope='p' AND idempotency_key='original'",[]).unwrap();
    assert_eq!(
        app.inspect_edit_recovery(&read("p")).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    assert_eq!(
        app.ack_edit_recovery(&ack(&staged)).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    app.connection.execute("UPDATE idempotency SET operation_family='project_v4_public_apply' WHERE scope='p' AND idempotency_key='original'",[]).unwrap();
    app.connection.execute_batch("CREATE TRIGGER reject_recovery_ack BEFORE DELETE ON edit_recovery BEGIN SELECT RAISE(ABORT,'ack failure'); END;").unwrap();
    assert!(app.ack_edit_recovery(&ack(&staged)).is_err());
    assert_eq!(
        current(&app, "p").receipt,
        Some(EditRecoveryReceipt::PublicApply {
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
        let staged = app.stage_edit_recovery(&input).unwrap();
        let original = app
            .apply_editor_edit(&apply("p", &base, "original"))
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
            app.apply_editor_edit(&apply("p", &base, "other")).unwrap()
        } else {
            let mut result = original;
            result.created_ids.push("invented-id".into());
            result
        };
        app.connection.execute("UPDATE idempotency SET result_json=?1 WHERE operation_family='project_v4_public_apply' AND scope='p' AND idempotency_key='original'",
            [serde_json::to_string(&corrupted).unwrap()]).unwrap();
        let before = counts(&app);
        assert_eq!(
            app.inspect_edit_recovery(&read("p")).unwrap_err().code,
            "STORAGE_ERROR"
        );
        assert_eq!(
            app.ack_edit_recovery(&ack(&staged)).unwrap_err().code,
            "STORAGE_ERROR"
        );
        assert_eq!(counts(&app), before);
        assert_eq!(
            app.connection
                .query_row::<u32, _, _>(
                    "SELECT count(*) FROM edit_recovery WHERE recovery_id=?1",
                    [&staged.recovery_id],
                    |row| row.get(0)
                )
                .unwrap(),
            1
        );
    }
}

#[test]
fn recovery_migration_preserves_raw_context_digest_history_and_rolls_back_collision() {
    for collision in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let database = tmp.path().join("workspace.sqlite");
        let mut app = EditorApplication::open(&database).unwrap();
        let base = create(&mut app, "p");
        let (input, raw, digest) = historical_timeline(&app, "p", &base, "original");
        let history: String = app
            .connection
            .query_row(
                "SELECT document_json FROM revisions WHERE revision=?1",
                [&base],
                |r| r.get(0),
            )
            .unwrap();
        app.connection.execute_batch("DROP TABLE edit_recovery;DROP TRIGGER font_derivations_immutable_update;DROP TRIGGER font_derivations_immutable_delete;ALTER TABLE font_derivations RENAME TO font_derivations_v1;CREATE TRIGGER font_derivations_v1_immutable_update BEFORE UPDATE ON font_derivations_v1 BEGIN SELECT RAISE(ABORT,'immutable font derivation');END;CREATE TRIGGER font_derivations_v1_immutable_delete BEFORE DELETE ON font_derivations_v1 BEGIN SELECT RAISE(ABORT,'retained font derivation');END;UPDATE metadata SET value='32' WHERE key='schema_version';").unwrap();
        app.connection
            .execute_batch(recovery_migrations::CREATE_TIMELINE_STORE)
            .unwrap();
        app.connection
            .execute_batch(recovery_migrations::CREATE_INTENT_STORE)
            .unwrap();
        insert_stored(&app, "edit_recovery_v1", "p", &raw, &digest);
        if collision {
            app.connection
                .execute_batch("CREATE TABLE edit_recovery(collision TEXT)")
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
                "32"
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
            db.execute_batch("DROP TABLE edit_recovery").unwrap();
        }
        let app = EditorApplication::open(&database).unwrap();
        let stored: (String, String) = app
            .connection
            .query_row(
                "SELECT context_json,request_digest FROM edit_recovery",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, (raw, digest));
        assert_eq!(current(&app, "p").context, input.context);
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
        let obsolete:u32=app.connection.query_row("SELECT count(*) FROM sqlite_master WHERE name IN ('edit_recovery_v1','edit_recovery_v2')",[],|r|r.get(0)).unwrap();
        assert_eq!(obsolete, 0);
    }
}

#[test]
fn concurrent_intents_have_one_pending_owner_scope_and_no_project_write() {
    for same_key in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let database = tmp.path().join("workspace.sqlite");
        let mut app = EditorApplication::open(&database).unwrap();
        let base = create(&mut app, "p");
        let old = timeline_input(&app, "p", &base, "old");
        let new = staged_apply("p", &base, if same_key { "old" } else { "new" });
        let before = counts(&app);
        drop(app);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let first_database = database.clone();
        let first_barrier = barrier.clone();
        let first = std::thread::spawn(move || {
            let mut app = EditorApplication::open(&first_database).unwrap();
            first_barrier.wait();
            app.stage_edit_recovery(&old).map(|_| ())
        });
        let second = std::thread::spawn(move || {
            let mut app = EditorApplication::open(&database).unwrap();
            barrier.wait();
            app.stage_edit_recovery(&new).map(|_| ())
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
        assert_eq!(
            app.connection
                .query_row::<u32, _, _>("SELECT count(*) FROM edit_recovery", [], |row| row.get(0))
                .unwrap(),
            1
        );
    }
}

#[test]
fn shared_record_byte_limit_charges_scope_digest_and_all_intents() {
    use opencut_editor_application::edit_recovery::{MAX_RECORD_BYTES, MAX_UNRESOLVED_BYTES};
    let mut app = EditorApplication::open(":memory:").unwrap();
    let base = create(&mut app, "p");
    let source = sqlite_scene::load_source(&app.connection, "p", Some(&base)).unwrap();
    let opencut_editor_application::scene::StoredProjectDocument::Editable(project) =
        source.document
    else {
        panic!("V4 fixture")
    };
    let mut track = project.sequences[0].tracks[0].clone();
    track.name.clear();
    let mut request = input(EditRecoveryIntent::PublicApply {
        request: Box::new(ApplyProject {
            operations: vec![ProjectEditOperation::TrackPut {
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
    let EditRecoveryIntent::PublicApply {
        request: transaction,
    } = &mut request.context.intent
    else {
        unreachable!()
    };
    let ProjectEditOperation::TrackPut { track, .. } = &mut transaction.operations[0] else {
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
        app.stage_edit_recovery(&request).unwrap_err().code,
        "RECOVERY_QUOTA_EXCEEDED"
    );
    assert_eq!(counts(&app), before);
    assert!(
        app.inspect_edit_recovery(&read("p"))
            .unwrap()
            .record
            .is_none()
    );
    let EditRecoveryIntent::PublicApply {
        request: transaction,
    } = &mut request.context.intent
    else {
        unreachable!()
    };
    let ProjectEditOperation::TrackPut { track, .. } = &mut transaction.operations[0] else {
        unreachable!()
    };
    track.name.pop();
    app.stage_edit_recovery(&request).unwrap();
    assert_eq!(
        app.connection
            .query_row::<u64, _, _>(
                "SELECT stored_bytes FROM edit_recovery WHERE project_id='p'",
                [],
                |row| row.get(0)
            )
            .unwrap(),
        MAX_RECORD_BYTES as u64
    );
    let other_base = create(&mut app, "legacy");
    let old = timeline_input(&app, "legacy", &other_base, "track");
    app.stage_edit_recovery(&old).unwrap();
    let other_base = create(&mut app, "current");
    app.stage_edit_recovery(&staged_apply("current", &other_base, "rename"))
        .unwrap();
    let (count, total): (u64, u64) = app
        .connection
        .query_row(
            "SELECT count(*),sum(stored_bytes) FROM edit_recovery",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(count, 3);
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
