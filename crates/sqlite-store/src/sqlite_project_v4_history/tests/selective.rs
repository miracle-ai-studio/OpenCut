use super::*;
use opencut_contracts::Operation;
#[test]
fn selective_undo_redo_preserves_later_edits_and_causality_across_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workspace.sqlite");
    let mut app = EditorApplication::open(&path).unwrap();
    create(&mut app);
    let original = edit(
        &mut app,
        "rename",
        vec![ProjectOperationV4::Rename {
            name: "Target".into(),
        }],
    );
    let original_bytes = raw(&app, &original.revision);
    let later = edit(&mut app, "later", vec![marker("later")]);
    let request = undo(&app, &original.transaction_id, "undo");
    let plan = app.plan_v4_undo(&request).unwrap();
    assert!(plan.conflicts.is_empty());
    assert_eq!(plan.changes.len(), 1);
    let undone = app.apply_v4_undo(&request).unwrap();
    let snapshot = inspect(&app);
    assert_eq!(snapshot.project.name, "Original");
    assert_eq!(snapshot.project.sequences[0].bookmarks.len(), 1);
    assert_eq!(
        navigate(&app).undo.unwrap().transaction_id,
        later.transaction_id
    );
    assert_eq!(
        navigate(&app).redo.unwrap().undo_transaction_id,
        undone.transaction.transaction_id
    );
    let journal: String = app
        .connection
        .query_row(
            "SELECT request_json FROM editor_transactions WHERE transaction_id=?1",
            [&undone.transaction.transaction_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(matches!(
        serde_json::from_str::<CausalJournalV4>(&journal).unwrap(),
        CausalJournalV4::Undo { .. }
    ));
    drop(app);
    let mut app = EditorApplication::open(&path).unwrap();
    assert_eq!(inspect(&app), snapshot);
    let redo_request = redo(&app, &undone.transaction.transaction_id, "redo");
    let redone = app.apply_v4_redo(&redo_request).unwrap();
    assert_eq!(inspect(&app).project.name, "Target");
    assert_eq!(inspect(&app).project.sequences[0].bookmarks.len(), 1);
    assert_eq!(
        navigate(&app).undo.unwrap().transaction_id,
        original.transaction_id
    );
    assert!(navigate(&app).redo.is_none());
    let before = counts(&app);
    assert_eq!(app.apply_v4_undo(&request).unwrap(), undone);
    assert_eq!(app.apply_v4_redo(&redo_request).unwrap(), redone);
    assert_eq!(counts(&app), before);
    assert_eq!(raw(&app, &original.revision), original_bytes);
    assert_eq!(
        app.apply_v4_redo(&redo(
            &app,
            &undone.transaction.transaction_id,
            "repeat-redo"
        ))
        .unwrap_err()
        .code,
        "REDO_UNAVAILABLE"
    );
}
#[test]
fn overlaps_and_locked_content_refuse_without_partial_records() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    create(&mut app);
    let target = edit(
        &mut app,
        "target",
        vec![ProjectOperationV4::Rename {
            name: "Target".into(),
        }],
    );
    edit(
        &mut app,
        "overlap",
        vec![ProjectOperationV4::Rename {
            name: "Later".into(),
        }],
    );
    let request = undo(&app, &target.transaction_id, "conflict");
    let before = counts(&app);
    let head = inspect(&app);
    let plan = app.plan_v4_undo(&request).unwrap();
    assert_eq!(
        plan.conflicts[0].reason,
        HistoryConflictReasonV4::DependentChange
    );
    assert_eq!(
        app.apply_v4_undo(&request).unwrap_err().code,
        "SELECTIVE_CONFLICT"
    );
    assert_eq!(counts(&app), before);
    assert_eq!(inspect(&app), head);
    let mut track = head.project.sequences[0].tracks[0].clone();
    track.name = "Target track".into();
    let track_id = track.id.clone();
    let target = edit(
        &mut app,
        "track",
        vec![ProjectOperationV4::TrackPut {
            sequence_id: "main".into(),
            track,
        }],
    );
    let lock = edit(
        &mut app,
        "lock",
        vec![ProjectOperationV4::TrackLockSet {
            sequence_id: "main".into(),
            track_id,
            locked: true,
        }],
    );
    let request = undo(&app, &target.transaction_id, "locked");
    let before = counts(&app);
    assert_eq!(
        app.plan_v4_undo(&request).unwrap().conflicts[0].reason,
        HistoryConflictReasonV4::LockedTrack
    );
    assert_eq!(
        app.apply_v4_undo(&request).unwrap_err().code,
        "SELECTIVE_CONFLICT"
    );
    assert_eq!(counts(&app), before);
    let unlocked = app
        .apply_v4_undo(&undo(&app, &lock.transaction_id, "unlock"))
        .unwrap();
    assert!(!inspect(&app).project.sequences[0].tracks[0].locked);
    app.apply_v4_redo(&redo(&app, &unlocked.transaction.transaction_id, "relock"))
        .unwrap();
    assert!(inspect(&app).project.sequences[0].tracks[0].locked);
}
#[test]
fn legacy_edit_is_lifted_without_rewriting_and_promotion_is_not_undoable() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let initial = app
        .create_project(
            CreateProject {
                id: "p".into(),
                name: "Original".into(),
                sequence_id: "main".into(),
                width: 64,
                height: 48,
                fps: FrameRate {
                    numerator: 25,
                    denominator: 1,
                },
            },
            "create",
        )
        .unwrap();
    let original_name = initial.project.sequences[0].name.clone();
    let target = app
        .apply(
            "p",
            &TransactionRequest {
                expected_revision: initial.revision,
                idempotency_key: "old-edit".into(),
                label: "Old name edit".into(),
                operations: vec![Operation::UpdateSequenceSettings {
                    sequence_id: "main".into(),
                    name: Some("Old target name".into()),
                    width: None,
                    height: None,
                    fps: None,
                }],
            },
        )
        .unwrap();
    let old_bytes = raw(&app, &target.revision);
    let promotion = app
        .promote_v4_project(&PromoteV4Project {
            project_id: "p".into(),
            expected_revision: target.revision.clone(),
            idempotency_key: "promote".into(),
            asset_versions: BTreeMap::new(),
        })
        .unwrap();
    assert_eq!(
        app.plan_v4_undo(&undo(&app, &promotion.transaction_id, "bad-target"))
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
    edit(&mut app, "later", vec![marker("later")]);
    app.apply_v4_undo(&undo(&app, &target.transaction_id, "undo-old"))
        .unwrap();
    assert_eq!(inspect(&app).project.sequences[0].name, original_name);
    assert_eq!(inspect(&app).project.sequences[0].bookmarks.len(), 1);
    assert_eq!(raw(&app, &target.revision), old_bytes);
}
#[test]
fn historical_resource_redo_requires_bytes_while_undoing_offline_binding_does_not() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(":memory:").unwrap();
    create(&mut app);
    let path = directory.path().join("source.bin");
    std::fs::write(&path, b"tiny resource").unwrap();
    let pinned = opencut_asset_store::AssetStore::open(&app.asset_store_root)
        .unwrap()
        .pin_asset(&Asset {
            id: "asset".into(),
            name: "Asset".into(),
            kind: AssetKind::Image,
            uri: path.to_string_lossy().into(),
            duration: None,
            version: None,
            metadata: BTreeMap::new(),
        })
        .unwrap();
    let version = pinned.version.unwrap();
    let asset = AssetV4 {
        id: "asset".into(),
        name: "Asset".into(),
        kind: AssetKind::Image,
        version: version.clone(),
        duration: None,
        origin: ResourceOriginV4::LocalImport {
            original_name: "source.bin".into(),
        },
        license: None,
        metadata: MediaMetadataV4 {
            mime_type: None,
            intrinsic_size: None,
            frame_rate: None,
            rotation_degrees: None,
            audio: AudioPresenceV4::Unknown {},
        },
    };
    let target = edit(
        &mut app,
        "asset",
        vec![ProjectOperationV4::AssetPut { asset }],
    );
    edit(&mut app, "later", vec![marker("later")]);
    std::fs::remove_file(
        app.asset_store_root
            .join(&version.content_hash[..2])
            .join(&version.content_hash),
    )
    .unwrap();
    let undone = app
        .apply_v4_undo(&undo(&app, &target.transaction_id, "undo-offline"))
        .unwrap();
    assert!(inspect(&app).project.assets.is_empty());
    let before = counts(&app);
    assert_eq!(
        app.apply_v4_redo(&redo(
            &app,
            &undone.transaction.transaction_id,
            "redo-offline"
        ))
        .unwrap_err()
        .code,
        "ASSET_IO_ERROR"
    );
    assert_eq!(counts(&app), before);
    assert_eq!(inspect(&app).project.sequences[0].bookmarks.len(), 1);
}
