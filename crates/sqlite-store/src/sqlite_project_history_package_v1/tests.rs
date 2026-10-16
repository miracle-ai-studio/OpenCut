use super::*;
use opencut_contracts::{FrameRate, project_v4::*, project_v4_history::*, project_v4_ops::*};
fn create(app: &mut EditorApplication, id: &str) -> ProjectSnapshotV4 {
    app.create_v4_project(&CreateV4Project {
        project: CreateProject {
            id: id.into(),
            name: "Original".into(),
            sequence_id: "scene".into(),
            width: 64,
            height: 48,
            fps: FrameRate {
                numerator: 30,
                denominator: 1,
            },
        },
        idempotency_key: format!("create-{id}"),
    })
    .unwrap()
}
fn inspect(app: &EditorApplication, id: &str) -> ProjectSnapshotV4 {
    app.inspect_v4_project(&InspectV4Project {
        project_id: id.into(),
        revision: None,
    })
    .unwrap()
}
fn rename(app: &mut EditorApplication, id: &str, name: &str) -> TransactionResult {
    app.apply_v4_project(&ApplyV4Project {
        project_id: id.into(),
        expected_revision: inspect(app, id).revision,
        idempotency_key: format!("rename-{name}"),
        label: format!("Rename {name}"),
        operations: vec![ProjectOperationV4::Rename { name: name.into() }],
    })
    .unwrap()
}
fn prepare(source: HistoryPackageSourceV1) -> PreparedHistoryImportV1 {
    let manifest = package::make_manifest(&source).unwrap();
    PreparedHistoryImportV1 {
        manifest_sha256: package::digest(
            &package::canonical_bytes(&manifest, MAX_MANIFEST_BYTES).unwrap(),
        ),
        manifest,
        archive: source.archive,
    }
}
fn input(p: &PreparedHistoryImportV1, target: &str) -> ImportHistoryPackageV1 {
    ImportHistoryPackageV1 {
        package_path: "/removed/original/package".into(),
        target_project_id: target.into(),
        expected_manifest_sha256: p.manifest_sha256.clone(),
        idempotency_key: format!("import-{target}"),
    }
}
fn counts(app: &EditorApplication) -> Vec<i64> {
    [
        "projects",
        "revisions",
        "editor_transactions",
        "project_branches",
        "project_merge_parents",
        "idempotency",
        "outbox",
        "audit_records",
    ]
    .into_iter()
    .map(|t| {
        app.connection
            .query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
            .unwrap()
    })
    .collect()
}
fn history(app: &mut EditorApplication) -> PreparedHistoryImportV1 {
    let original = create(app, "source");
    let first = rename(app, "source", "A");
    let undo = app
        .apply_v4_undo(&UndoV4Project {
            project_id: "source".into(),
            expected_revision: first.revision.clone(),
            target_transaction_id: first.transaction_id,
            idempotency_key: "undo-a".into(),
        })
        .unwrap();
    let redo = app
        .apply_v4_redo(&RedoV4Project {
            project_id: "source".into(),
            expected_revision: undo.transaction.revision,
            undo_transaction_id: undo.transaction.transaction_id,
            idempotency_key: "redo-a".into(),
        })
        .unwrap();
    let restored = app
        .restore_v4_revision(&RestoreV4Revision {
            project_id: "source".into(),
            expected_revision: redo.transaction.revision,
            target_revision: original.revision.clone(),
            idempotency_key: "restore-original".into(),
            asset_versions: Default::default(),
        })
        .unwrap();
    // Existing storage supports these graph rows; the old branch-edit API does
    // not expose V4 edits. Construct the branch split directly for this adapter
    // closure test, without claiming a public V4 merge authoring workflow.
    app.connection.execute("INSERT INTO project_branches(project_id,branch_name,head_revision,created_from_revision) VALUES('source','saved',?1,?2)",params![restored.revision,original.revision]).unwrap();
    app.connection
        .execute(
            "UPDATE projects SET head_revision=?1,name='Original' WHERE project_id='source'",
            [&original.revision],
        )
        .unwrap();
    let main = rename(app, "source", "B");
    app.connection.execute("INSERT INTO project_merge_parents(revision,project_id,source_revision) VALUES(?1,'source',?2)",params![main.revision,restored.revision]).unwrap();
    prepare(
        app.history_package_v1_export_source("source", &main.revision)
            .unwrap(),
    )
}
#[test]
fn full_graph_original_results_causal_history_reopen_and_import_replay_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("workspace.sqlite");
    let mut app = EditorApplication::open(&db).unwrap();
    let prepared = history(&mut app);
    let request = input(&prepared, "restored");
    let receipt = app
        .history_package_v1_import_commit(&request, &prepared)
        .unwrap();
    assert_eq!(receipt.package.revision_count, 6);
    assert_eq!(receipt.package.transaction_count, 5);
    assert_eq!(receipt.package.branch_count, 1);
    assert_eq!(receipt.package.merge_count, 1);
    let expected = package::prepare_import(&prepared, "restored").unwrap();
    let read = app
        .history_package_v1_export_source("restored", &receipt.revision)
        .unwrap();
    assert_eq!(
        package::canonical_bytes(&read.archive, MAX_HISTORY_BYTES).unwrap(),
        package::canonical_bytes(&expected, MAX_HISTORY_BYTES).unwrap()
    );
    assert_eq!(
        app.connection
            .query_row(
                "SELECT count(*) FROM idempotency WHERE scope='restored'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        app.connection
            .query_row(
                "SELECT count(*) FROM job_projects WHERE project_id='restored'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let before = counts(&app);
    let mut forged = prepared.clone();
    forged.archive.revisions.clear();
    assert_eq!(
        app.history_package_v1_import_commit(&request, &forged)
            .unwrap()
            .revision,
        receipt.revision
    );
    assert_eq!(counts(&app), before);
    drop(app);
    let mut app = EditorApplication::open(&db).unwrap();
    assert_eq!(
        app.history_package_v1_import_replay(&request)
            .unwrap()
            .unwrap()
            .revision,
        receipt.revision
    );
    let nav = app
        .v4_history_navigation(&HistoryNavigationInputV4 {
            project_id: "restored".into(),
            revision: None,
        })
        .unwrap();
    let undo = app
        .apply_v4_undo(&UndoV4Project {
            project_id: "restored".into(),
            expected_revision: nav.revision,
            target_transaction_id: nav.undo.unwrap().transaction_id,
            idempotency_key: "restored-undo".into(),
        })
        .unwrap();
    assert_eq!(inspect(&app, "restored").project.name, "Original");
    app.apply_v4_redo(&RedoV4Project {
        project_id: "restored".into(),
        expected_revision: undo.transaction.revision,
        undo_transaction_id: undo.transaction.transaction_id,
        idempotency_key: "restored-redo".into(),
    })
    .unwrap();
    assert_eq!(inspect(&app, "restored").project.name, "B");
    let mut conflict = request;
    conflict.target_project_id = "other".into();
    assert_eq!(
        app.history_package_v1_import_replay(&conflict)
            .unwrap_err()
            .code,
        "IDEMPOTENCY_KEY_REUSED"
    );
}
#[test]
fn late_branch_failure_rolls_back_all_publication_and_same_request_can_retry() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(temp.path().join("workspace.sqlite")).unwrap();
    let prepared = history(&mut app);
    let request = input(&prepared, "broken");
    let before = counts(&app);
    app.connection.execute_batch("CREATE TRIGGER reject_history_branch BEFORE INSERT ON project_branches WHEN NEW.project_id='broken' BEGIN SELECT RAISE(ABORT,'synthetic publication failure'); END;").unwrap();
    assert!(
        app.history_package_v1_import_commit(&request, &prepared)
            .is_err()
    );
    assert_eq!(counts(&app), before);
    assert!(
        app.history_package_v1_import_replay(&request)
            .unwrap()
            .is_none()
    );
    app.connection
        .execute_batch("DROP TRIGGER reject_history_branch;")
        .unwrap();
    app.history_package_v1_import_commit(&request, &prepared)
        .unwrap();
}
#[test]
fn stale_head_unknown_journal_mixed_legacy_and_prepared_tampering_fail_explicitly() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(temp.path().join("workspace.sqlite")).unwrap();
    let prepared = history(&mut app);
    assert_eq!(
        app.history_package_v1_export_source("source", "stale")
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
    let before = counts(&app);
    let mut bad = prepared.clone();
    bad.archive.revisions[0].document.name = "forged".into();
    assert!(
        app.history_package_v1_import_commit(&input(&prepared, "bad"), &bad)
            .is_err()
    );
    assert_eq!(counts(&app), before);
    app.connection.execute("INSERT INTO project_plugin_grants(project_id,plugin_id,version,revision,grant_json) VALUES('source','plugin','1',1,'{}')",[]).unwrap();
    assert_eq!(
        app.history_package_v1_export_source("source", &prepared.archive.main_head_revision)
            .unwrap_err()
            .code,
        "PACKAGE_DEPENDENCY_UNSUPPORTED"
    );
    app.connection
        .execute(
            "DELETE FROM project_plugin_grants WHERE project_id='source'",
            [],
        )
        .unwrap();
    app.connection
        .execute(
            "UPDATE editor_transactions SET request_json='{}' WHERE transaction_id=?1",
            [&prepared.archive.transactions[0].transaction_id],
        )
        .unwrap();
    assert_eq!(
        app.history_package_v1_export_source("source", &prepared.archive.main_head_revision)
            .unwrap_err()
            .code,
        "PACKAGE_HISTORY_UNSUPPORTED"
    );
    let legacy = app
        .create_project(
            CreateProject {
                id: "legacy".into(),
                name: "Legacy".into(),
                sequence_id: "scene".into(),
                width: 64,
                height: 48,
                fps: FrameRate {
                    numerator: 30,
                    denominator: 1,
                },
            },
            "legacy",
        )
        .unwrap();
    let promoted = app
        .promote_v4_project(&PromoteV4Project {
            project_id: "legacy".into(),
            expected_revision: legacy.revision,
            idempotency_key: "promote".into(),
            asset_versions: Default::default(),
        })
        .unwrap();
    assert_eq!(
        app.history_package_v1_export_source("legacy", &promoted.revision)
            .unwrap_err()
            .code,
        "PACKAGE_HISTORY_UNSUPPORTED"
    );
}
#[test]
fn historical_only_cas_bytes_are_rechecked_before_commit() {
    use opencut_asset_store::AssetStore;
    use opencut_contracts::{Asset, AssetKind};
    let temp = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(temp.path().join("workspace.sqlite")).unwrap();
    let initial = create(&mut app, "source");
    let file = temp.path().join("historical.bin");
    std::fs::write(&file, b"historical resource").unwrap();
    let version = AssetStore::open(app.asset_store_root())
        .unwrap()
        .pin_asset(&Asset {
            id: "asset".into(),
            name: "old".into(),
            kind: AssetKind::Image,
            uri: file.to_string_lossy().into_owned(),
            duration: None,
            version: None,
            metadata: Default::default(),
        })
        .unwrap()
        .version
        .unwrap();
    let asset = AssetV4 {
        id: "asset".into(),
        name: "Unused history".into(),
        kind: AssetKind::Image,
        version: version.clone(),
        duration: None,
        origin: ResourceOriginV4::LocalImport {
            original_name: "source.bin".into(),
        },
        metadata: MediaMetadataV4 {
            mime_type: Some("image/png".into()),
            intrinsic_size: None,
            frame_rate: None,
            rotation_degrees: None,
            audio: AudioPresenceV4::Absent {},
        },
        license: None,
    };
    // Package source stands in for an already verified external archive; CAS
    // authority is checked independently by import even when main has no asset.
    let mut source = app
        .history_package_v1_export_source("source", &initial.revision)
        .unwrap();
    source.archive.revisions[0].document.assets.push(asset);
    source.archive.main_head_revision = "after-delete".into();
    source.archive.revisions.push(HistoryRevisionV1 {
        revision: "after-delete".into(),
        parent_revision: Some(initial.revision.clone()),
        transaction_id: Some("delete-asset".into()),
        document: initial.project.clone(),
        created_at: "2026-01-02".into(),
    });
    source.archive.transactions.push(HistoryTransactionV1 {
        transaction_id: "delete-asset".into(),
        base_revision: initial.revision.clone(),
        revision: "after-delete".into(),
        journal: HistoryPackageJournalV1::Apply(ApplyV4Project {
            project_id: "source".into(),
            expected_revision: initial.revision.clone(),
            idempotency_key: "delete-asset".into(),
            label: "Delete".into(),
            operations: vec![ProjectOperationV4::AssetDelete {
                asset_id: "asset".into(),
            }],
        }),
        result: TransactionResult {
            transaction_id: "delete-asset".into(),
            base_revision: initial.revision,
            revision: "after-delete".into(),
            created_ids: vec![],
            changed_ids: vec!["source".into()],
            deleted_ids: vec!["asset".into()],
        },
        created_at: "2026-01-02".into(),
    });
    source
        .archive
        .revisions
        .sort_by(|a, b| a.revision.cmp(&b.revision));
    let p = prepare(source);
    let absent = temp.path().join("other/workspace.sqlite");
    std::fs::create_dir_all(absent.parent().unwrap()).unwrap();
    let mut target = EditorApplication::open(&absent).unwrap();
    let before = counts(&target);
    assert!(
        target
            .history_package_v1_import_commit(&input(&p, "copy"), &p)
            .is_err()
    );
    assert_eq!(counts(&target), before);
    // Existing source CAS can restore the same complete bytes without an upload
    // grant; import does not restore historical grants or original idempotency.
    app.history_package_v1_import_commit(&input(&p, "copy"), &p)
        .unwrap();
}
#[test]
fn deleted_historical_derived_font_proof_and_original_bytes_survive_both_publications() {
    use opencut_asset_store::AssetStore;
    use opencut_contracts::{Asset, AssetKind};
    use opencut_editor_application::font_derived_v1 as derived;
    fn canonical(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut keys = map.keys().collect::<Vec<_>>();
                keys.sort();
                Value::Object(
                    keys.into_iter()
                        .map(|k| (k.clone(), canonical(&map[k])))
                        .collect(),
                )
            }
            Value::Array(a) => Value::Array(a.iter().map(canonical).collect()),
            _ => value.clone(),
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(temp.path().join("workspace.sqlite")).unwrap();
    let initial = create(&mut app, "source");
    let path = temp.path().join("synthetic-font");
    std::fs::write(&path, b"proof-bound synthetic font bytes").unwrap();
    let version = AssetStore::open(app.asset_store_root())
        .unwrap()
        .pin_asset(&Asset {
            id: "font".into(),
            name: "font".into(),
            kind: AssetKind::Image,
            uri: path.to_string_lossy().into_owned(),
            duration: None,
            version: None,
            metadata: Default::default(),
        })
        .unwrap()
        .version
        .unwrap();
    // This is a provenance/storage test, not font parsing or derivation evidence.
    let mut proof = derived::catalog()
        .unwrap()
        .into_iter()
        .find(|p| p.source.face.weight == 400 && p.source.face.style == FontStyleV4::Normal)
        .unwrap()
        .source;
    let mut json: Value = serde_json::from_str(&proof.proof_json).unwrap();
    json.as_object_mut().unwrap().remove("derivation_id");
    json["output_sha256"] = json!(version.content_hash);
    json["output_size"] = json!(version.size);
    let derivation = format!(
        "font-derived_{}",
        package::digest(&serde_json::to_vec(&canonical(&json)).unwrap())
    );
    json["derivation_id"] = json!(derivation);
    proof.face.derivation_id = derivation.clone();
    proof.face.content_hash = version.content_hash.clone();
    proof.face.size_bytes = version.size.to_string();
    proof.proof_json = serde_json::to_string(&canonical(&json)).unwrap();
    let font = FontResourceV4 {
        id: "font".into(),
        family: proof.face.family.clone(),
        weight: proof.face.weight,
        style: proof.face.style,
        face_index: 0,
        content: version,
        origin: ResourceOriginV4::Generated {
            provider: derived::DERIVED_FONT_PROVIDER_V1.into(),
            generation_id: derivation,
        },
        license: Some(ResourceLicenseV4 {
            identifier: Some(proof.license_id.clone()),
            source_url: Some(proof.license_url.clone()),
            notice: Some(proof.license_notice.clone()),
            attribution: None,
        }),
    };
    let mut source = app
        .history_package_v1_export_source("source", &initial.revision)
        .unwrap();
    source.archive.revisions[0].document.fonts.push(font);
    source.derived_fonts = vec![proof.clone()];
    source.archive.main_head_revision = "font-deleted".into();
    source.archive.revisions.push(HistoryRevisionV1 {
        revision: "font-deleted".into(),
        parent_revision: Some(initial.revision.clone()),
        transaction_id: Some("delete-font".into()),
        document: initial.project,
        created_at: "2026-01-02".into(),
    });
    source.archive.transactions.push(HistoryTransactionV1 {
        transaction_id: "delete-font".into(),
        base_revision: initial.revision.clone(),
        revision: "font-deleted".into(),
        journal: HistoryPackageJournalV1::Apply(ApplyV4Project {
            project_id: "source".into(),
            expected_revision: initial.revision.clone(),
            idempotency_key: "delete-font".into(),
            label: "Delete font".into(),
            operations: vec![ProjectOperationV4::FontDelete {
                font_id: "font".into(),
            }],
        }),
        result: TransactionResult {
            transaction_id: "delete-font".into(),
            base_revision: initial.revision,
            revision: "font-deleted".into(),
            created_ids: vec![],
            changed_ids: vec!["source".into()],
            deleted_ids: vec!["font".into()],
        },
        created_at: "2026-01-02".into(),
    });
    source
        .archive
        .revisions
        .sort_by(|a, b| a.revision.cmp(&b.revision));
    let prepared = prepare(source);
    let first = app
        .history_package_v1_import_commit(&input(&prepared, "copy-one"), &prepared)
        .unwrap();
    assert!(inspect(&app, "copy-one").project.fonts.is_empty());
    let exported = app
        .history_package_v1_export_source("copy-one", &first.revision)
        .unwrap();
    assert_eq!(
        serde_json::to_value(&exported.derived_fonts).unwrap(),
        serde_json::to_value(vec![proof]).unwrap()
    );
    let second = prepare(exported);
    app.history_package_v1_import_commit(&input(&second, "copy-two"), &second)
        .unwrap();
    assert_eq!(
        app.connection
            .query_row("SELECT count(*) FROM font_derivations_v1", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
