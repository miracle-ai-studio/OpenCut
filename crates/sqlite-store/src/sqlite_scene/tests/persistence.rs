use super::*;
use opencut_contracts::project_catalog::LibraryInput;

#[test]
fn frozen_history_navigation_rejects_v4_heads_and_fixed_revisions_without_writes() {
    use opencut_api_contracts::history_navigation::HistoryNavigationInput;
    let mut app = EditorApplication::open(":memory:").unwrap();
    let legacy = app
        .create_project(create_request("p").project, "legacy-create")
        .unwrap();
    let old_bytes = raw(&app, &legacy.revision);
    let promoted = app
        .promote_project(&PromoteProject {
            branch_name: None,
            project_id: "p".into(),
            expected_revision: legacy.revision.clone(),
            idempotency_key: "promote".into(),
            asset_versions: BTreeMap::new(),
        })
        .unwrap();
    let before = counts(&app);
    // Selecting an older document cannot bypass the unsupported current head.
    for revision in [
        None,
        Some(legacy.revision.clone()),
        Some(promoted.revision.clone()),
    ] {
        assert_eq!(
            app.history_navigation(&HistoryNavigationInput {
                project_id: "p".into(),
                revision
            })
            .unwrap_err()
            .code,
            "UNSUPPORTED_PROJECT_VERSION"
        );
    }
    assert_eq!(counts(&app), before);
    assert_eq!(inspect(&app, "p").revision, promoted.revision);
    // Exercise the independent fixed-revision gate in a mixed-schema retained
    // history. This SQL setup is not a supported head downgrade operation.
    app.connection
        .execute(
            "UPDATE projects SET head_revision=?1 WHERE project_id='p'",
            [&legacy.revision],
        )
        .unwrap();
    let before_fixed = counts(&app);
    app.history_navigation(&HistoryNavigationInput {
        project_id: "p".into(),
        revision: None,
    })
    .unwrap();
    assert_eq!(
        app.history_navigation(&HistoryNavigationInput {
            project_id: "p".into(),
            revision: Some(promoted.revision)
        })
        .unwrap_err()
        .code,
        "UNSUPPORTED_PROJECT_VERSION"
    );
    assert_eq!(counts(&app), before_fixed);
    assert_eq!(app.snapshot("p").unwrap().revision, legacy.revision);
    assert_eq!(raw(&app, &legacy.revision), old_bytes);
}

#[test]
fn aggregate_resources_bookmarks_receipts_and_history_survive_reopening() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workspace.sqlite");
    let mut app = EditorApplication::open(&path).unwrap();
    let initial = app.create_scene_project(&create_request("p")).unwrap();
    let asset = capture_asset(
        &app,
        directory.path(),
        "image",
        b"small CAS identity fixture",
    );
    let font = FontResource {
        id: "font".into(),
        family: "Fixture face".into(),
        weight: 400,
        style: FontStyle::Normal,
        face_index: 0,
        content: asset.version.clone(),
        origin: ResourceOrigin::Generated {
            provider: "storage-fixture".into(),
            generation_id: "font-content".into(),
        },
        license: None,
    };
    // The fixture checks bytes/aggregate persistence, not whether arbitrary CAS
    // bytes are a usable image/font. Media execution validates those formats.
    let request = edit(
        &initial,
        "aggregate",
        vec![
            ProjectOperation::AssetPut {
                asset: asset.clone(),
            },
            ProjectOperation::FontPut { font: font.clone() },
            bookmark("bookmark"),
        ],
    );
    let result = app.apply_scene_project(&request).unwrap();
    let snapshot = inspect(&app, "p");
    assert_eq!(snapshot.project.assets, vec![asset]);
    assert_eq!(snapshot.project.fonts, vec![font]);
    assert_eq!(
        snapshot.project.sequences[0].bookmarks[0].time,
        opencut_contracts::Time {
            // 3003/3000 seconds rounds to frame 25 at the project's 25 FPS.
            ticks: "1".into(),
            timebase: 1
        }
    );
    let stored = raw(&app, &result.revision);
    let before_replay = counts(&app);
    assert_eq!(app.apply_scene_project(&request).unwrap(), result);
    assert_eq!(counts(&app), before_replay);
    let legacy_error = app.snapshot("p").unwrap_err();
    assert_eq!(legacy_error.code, "UNSUPPORTED_PROJECT_VERSION");
    let schema: String = app
        .connection
        .query_row(
            "SELECT value FROM metadata WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(schema, DATABASE_SCHEMA_VERSION.to_string());
    drop(app);
    let app = EditorApplication::open(&path).unwrap();
    assert_eq!(inspect(&app, "p"), snapshot);
    assert_eq!(raw(&app, &result.revision), stored);
    assert_eq!(
        app.inspect_scene_project(&InspectProject {
            project_id: "p".into(),
            revision: Some(initial.revision.clone())
        })
        .unwrap(),
        initial
    );
}

#[test]
fn promotion_and_legacy_restore_append_v4_without_rewriting_old_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("legacy26.sqlite");
    let mut app = EditorApplication::open(&path).unwrap();
    let legacy = app
        .create_project(create_request("p").project, "legacy-create")
        .unwrap();
    let legacy_raw = raw(&app, &legacy.revision);
    // Reconstruct the old schema before its marker: V27 changes the barrier,
    // V28 adds recovery records without rewriting revisions or receipts.
    app.connection
        .execute_batch(
            "DROP TABLE audio_preview_recovery_v1; DROP TABLE font_derivations;
DROP TABLE edit_recovery",
        )
        .unwrap();
    app.connection
        .execute(
            "UPDATE metadata SET value='26' WHERE key='schema_version'",
            [],
        )
        .unwrap();
    let before_upgrade = counts(&app);
    drop(app);
    let mut app = EditorApplication::open(&path).unwrap();
    let schema: String = app
        .connection
        .query_row(
            "SELECT value FROM metadata WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(schema, DATABASE_SCHEMA_VERSION.to_string());
    assert_eq!(counts(&app), before_upgrade);
    assert_eq!(raw(&app, &legacy.revision), legacy_raw);
    assert_eq!(app.snapshot("p").unwrap(), legacy);
    let promote = PromoteProject {
        branch_name: None,
        project_id: "p".into(),
        expected_revision: legacy.revision.clone(),
        idempotency_key: "promote".into(),
        asset_versions: BTreeMap::new(),
    };
    let promoted = app.promote_project(&promote).unwrap();
    assert_ne!(promoted.revision, legacy.revision);
    let request = edit(&inspect(&app, "p"), "bookmark", vec![bookmark("marker")]);
    let edited = app.apply_scene_project(&request).unwrap();
    let restore = RestoreProject {
        branch_name: None,
        label: None,
        project_id: "p".into(),
        expected_revision: edited.revision.clone(),
        target_revision: legacy.revision.clone(),
        idempotency_key: "restore-old".into(),
        asset_versions: BTreeMap::new(),
    };
    let restored = app.restore_project(&restore).unwrap();
    let snapshot = inspect(&app, "p");
    assert_eq!(snapshot.project.schema_version, 4);
    assert!(snapshot.project.sequences[0].bookmarks.is_empty());
    assert_eq!(raw(&app, &legacy.revision), legacy_raw);
    assert_eq!(app.restore_project(&restore).unwrap(), restored);
    assert_eq!(app.promote_project(&promote).unwrap(), promoted);
    let wrong_scope = app
        .inspect_scene_project(&InspectProject {
            project_id: "other".into(),
            revision: Some(restored.revision),
        })
        .unwrap_err();
    assert_eq!(wrong_scope.code, "NOT_FOUND");
}

#[test]
fn audit_failure_rolls_back_every_publication_record() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let initial = app.create_scene_project(&create_request("p")).unwrap();
    let before = counts(&app);
    app.connection.execute_batch("CREATE TRIGGER reject_v4_audit BEFORE INSERT ON audit_records WHEN NEW.action='project.v4_edited' BEGIN SELECT RAISE(ABORT,'injected audit failure'); END;").unwrap();
    let request = edit(&initial, "rollback", vec![bookmark("marker")]);
    assert_eq!(
        app.apply_scene_project(&request).unwrap_err().code,
        "STORAGE_ERROR"
    );
    assert_eq!(inspect(&app, "p"), initial);
    assert_eq!(counts(&app), before);
    app.connection
        .execute_batch("DROP TRIGGER reject_v4_audit;")
        .unwrap();
    app.apply_scene_project(&request).unwrap();
    assert_eq!(inspect(&app, "p").project.sequences[0].bookmarks.len(), 1);
}

#[test]
fn copy_is_fixed_complete_and_library_distinguishes_outer_schemas() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let initial = app
        .create_scene_project(&create_request("b-source"))
        .unwrap();
    app.create_project(create_request("a-legacy").project, "legacy-create")
        .unwrap();
    let request = edit(&initial, "bookmark", vec![bookmark("marker")]);
    app.apply_scene_project(&request).unwrap();
    let source = inspect(&app, "b-source");
    let copy = CopySceneProject {
        source_project_id: "b-source".into(),
        source_revision: source.revision.clone(),
        project_id: "c-copy".into(),
        name: "Copy".into(),
        idempotency_key: "copy".into(),
    };
    let copied = app.copy_scene_project(&copy).unwrap();
    let mut expected = source.project.clone();
    expected.id = "c-copy".into();
    expected.name = "Copy".into();
    assert_eq!(copied.project, expected);
    assert_eq!(app.copy_scene_project(&copy).unwrap(), copied);
    for (project, revision, kind) in [
        ("b-source", &initial.revision, "project.v4_created"),
        ("c-copy", &copied.revision, "project.v4_copied"),
    ] {
        let payload: String = app
            .connection
            .query_row(
                "SELECT payload_json FROM outbox WHERE project_id=?1 AND event_type=?2",
                params![project, kind],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&payload).unwrap(),
            serde_json::json!({"project_id":project,"revision":revision,"schema_version":4})
        );
    }
    let rename = edit(
        &source,
        "source-change",
        vec![ProjectOperation::Rename {
            name: "Changed source".into(),
        }],
    );
    app.apply_scene_project(&rename).unwrap();
    assert_eq!(inspect(&app, "c-copy"), copied);
    let first = app
        .project_library(&LibraryInput {
            cursor: None,
            limit: 1,
        })
        .unwrap();
    assert_eq!(first.items[0].project_id, "a-legacy");
    assert_eq!(
        first.items[0].format,
        opencut_contracts::project_catalog::ProjectFormat::Base
    );
    let second = app
        .project_library(&LibraryInput {
            cursor: first.next_cursor,
            limit: 10,
        })
        .unwrap();
    assert_eq!(second.items.len(), 2);
    assert!(
        second
            .items
            .iter()
            .all(|item| item.format == opencut_contracts::project_catalog::ProjectFormat::Editor)
    );
    let legacy_copy = CopySceneProject {
        source_project_id: "a-legacy".into(),
        source_revision: app.snapshot("a-legacy").unwrap().revision,
        project_id: "d-rejected".into(),
        name: "Needs promotion".into(),
        idempotency_key: "legacy-copy".into(),
    };
    assert_eq!(
        app.copy_scene_project(&legacy_copy).unwrap_err().code,
        "UNSUPPORTED_PROJECT_VERSION"
    );
}
