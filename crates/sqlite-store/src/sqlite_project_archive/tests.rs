use super::*;
use opencut_contracts::{FrameRate, project_history::*, scene::*, scene_ops::*};
fn create(app: &mut EditorApplication, id: &str) -> EditorSnapshot {
    app.create_scene_project(&CreateProjectRequest {
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
fn inspect(app: &EditorApplication, id: &str) -> EditorSnapshot {
    app.inspect_scene_project(&InspectProject {
        project_id: id.into(),
        revision: None,
    })
    .unwrap()
}
fn rename(app: &mut EditorApplication, id: &str, name: &str) -> TransactionResult {
    app.apply_scene_project(&ApplySceneProject {
        project_id: id.into(),
        expected_revision: inspect(app, id).revision,
        idempotency_key: format!("rename-{name}"),
        label: format!("Rename {name}"),
        operations: vec![ProjectOperation::Rename { name: name.into() }],
    })
    .unwrap()
}
fn prepare(source: ArchiveSource) -> PreparedHistoricalArchive {
    let manifest = package::make_manifest(&source).unwrap();
    PreparedHistoricalArchive {
        manifest_sha256: package::digest(
            &package::canonical_bytes(&manifest, MAX_MANIFEST_BYTES).unwrap(),
        ),
        manifest,
        archive: source.archive,
    }
}
fn input(p: &PreparedHistoricalArchive, target: &str) -> HistoricalArchiveImport {
    HistoricalArchiveImport {
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
fn history(app: &mut EditorApplication) -> PreparedHistoricalArchive {
    let original = create(app, "source");
    let first = rename(app, "source", "A");
    let undo = app
        .apply_project_undo(&UndoRequest {
            project_id: "source".into(),
            expected_revision: first.revision.clone(),
            target_transaction_id: first.transaction_id,
            idempotency_key: "undo-a".into(),
        })
        .unwrap();
    let redo = app
        .apply_project_redo(&RedoRequest {
            project_id: "source".into(),
            expected_revision: undo.transaction.revision,
            undo_transaction_id: undo.transaction.transaction_id,
            idempotency_key: "redo-a".into(),
        })
        .unwrap();
    let restored = app
        .restore_project(&RestoreProject {
            branch_name: None,
            label: None,
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
        app.historical_archive_source("source", &main.revision)
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
        .historical_archive_fixture_commit(&request, &prepared)
        .unwrap();
    assert_eq!(receipt.package.revision_count, 6);
    assert_eq!(receipt.package.transaction_count, 5);
    assert_eq!(receipt.package.branch_count, 1);
    assert_eq!(receipt.package.merge_count, 1);
    let expected = package::prepare_import(&prepared, "restored").unwrap();
    let read = app
        .historical_archive_source("restored", &receipt.revision)
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
        app.historical_archive_fixture_commit(&request, &forged)
            .unwrap()
            .revision,
        receipt.revision
    );
    assert_eq!(counts(&app), before);
    drop(app);
    let mut app = EditorApplication::open(&db).unwrap();
    assert_eq!(
        app.historical_archive_replay(&request)
            .unwrap()
            .unwrap()
            .revision,
        receipt.revision
    );
    let nav = app
        .editor_history_navigation(&HistoryNavigationInput {
            project_id: "restored".into(),
            revision: None,
        })
        .unwrap();
    let undo = app
        .apply_project_undo(&UndoRequest {
            project_id: "restored".into(),
            expected_revision: nav.revision,
            target_transaction_id: nav.undo.unwrap().transaction_id,
            idempotency_key: "restored-undo".into(),
        })
        .unwrap();
    assert_eq!(inspect(&app, "restored").project.name, "Original");
    app.apply_project_redo(&RedoRequest {
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
        app.historical_archive_replay(&conflict).unwrap_err().code,
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
        app.historical_archive_fixture_commit(&request, &prepared)
            .is_err()
    );
    assert_eq!(counts(&app), before);
    assert!(app.historical_archive_replay(&request).unwrap().is_none());
    app.connection
        .execute_batch("DROP TRIGGER reject_history_branch;")
        .unwrap();
    app.historical_archive_fixture_commit(&request, &prepared)
        .unwrap();
}
#[test]
fn stale_head_unknown_journal_mixed_legacy_and_prepared_tampering_fail_explicitly() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(temp.path().join("workspace.sqlite")).unwrap();
    let prepared = history(&mut app);
    assert_eq!(
        app.historical_archive_source("source", "stale")
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
    let before = counts(&app);
    let mut bad = prepared.clone();
    let ArchivedDocument::Editable(document) = &mut bad.archive.revisions[0].document else {
        panic!("editable fixture");
    };
    document.name = "forged".into();
    assert!(
        app.historical_archive_fixture_commit(&input(&prepared, "bad"), &bad)
            .is_err()
    );
    assert_eq!(counts(&app), before);
    app.connection.execute("INSERT INTO project_plugin_grants(project_id,plugin_id,version,revision,grant_json) VALUES('source','plugin','1',1,'{}')",[]).unwrap();
    assert_eq!(
        app.historical_archive_source("source", &prepared.archive.main_head_revision)
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
        app.historical_archive_source("source", &prepared.archive.main_head_revision)
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
    // Extend the existing compound storage case with actual original writers,
    // rather than manufacture journals matching the archive reader itself.
    use opencut_contracts::{Operation, TransactionRequest};
    let edit = app
        .apply(
            "legacy",
            &TransactionRequest {
                expected_revision: legacy.revision.clone(),
                idempotency_key: "legacy-name".into(),
                label: "Original edit".into(),
                operations: vec![Operation::UpdateSequenceSettings {
                    sequence_id: "scene".into(),
                    name: Some("Edited scene".into()),
                    width: None,
                    height: None,
                    fps: None,
                }],
            },
        )
        .unwrap();
    let later = app
        .apply(
            "legacy",
            &TransactionRequest {
                expected_revision: edit.revision.clone(),
                idempotency_key: "legacy-width".into(),
                label: "Later width".into(),
                operations: vec![Operation::UpdateSequenceSettings {
                    sequence_id: "scene".into(),
                    name: None,
                    width: Some(96),
                    height: None,
                    fps: None,
                }],
            },
        )
        .unwrap();
    let undo = app
        .apply_undo(&UndoRequest {
            project_id: "legacy".into(),
            expected_revision: later.revision,
            target_transaction_id: edit.transaction_id.clone(),
            idempotency_key: "legacy-undo".into(),
        })
        .unwrap();
    let redo = app
        .apply_redo(&RedoRequest {
            project_id: "legacy".into(),
            expected_revision: undo.transaction.revision,
            undo_transaction_id: undo.transaction.transaction_id,
            idempotency_key: "legacy-redo".into(),
        })
        .unwrap();
    let restored = app
        .restore_revision(
            "legacy",
            &opencut_api_contracts::RestoreRevisionRequest {
                expected_revision: redo.transaction.revision,
                target_revision: edit.revision.clone(),
                idempotency_key: "legacy-restore".into(),
                label: "Original legacy/source label".into(),
            },
        )
        .unwrap();
    let visual = app
        .promote_historical_visual(&sqlite_historical_visual::HistoricalVisualPromotion {
            project_id: "legacy".into(),
            expected_revision: restored.revision,
            idempotency_key: "visual-promotion".into(),
        })
        .unwrap();
    let visual_edit = app
        .apply_historical_visual_project(
            &opencut_contracts::historical_visual_ops::ApplyHistoricalVisualProject {
                project_id: "legacy".into(),
                expected_revision: visual.revision,
                idempotency_key: "visual-later-width".into(),
                label: "Later visual edit".into(),
                operations: vec![
                    opencut_contracts::historical_visual_ops::VisualOperation::Base {
                        operation: Operation::UpdateSequenceSettings {
                            sequence_id: "scene".into(),
                            name: None,
                            width: Some(128),
                            height: None,
                            fps: None,
                        },
                    },
                ],
            },
        )
        .unwrap();
    let visual_undo = app
        .apply_historical_visual_undo(&UndoRequest {
            project_id: "legacy".into(),
            expected_revision: visual_edit.revision,
            target_transaction_id: edit.transaction_id,
            idempotency_key: "visual-undo-legacy".into(),
        })
        .unwrap();
    let visual_redo = app
        .apply_historical_visual_redo(&RedoRequest {
            project_id: "legacy".into(),
            expected_revision: visual_undo.transaction.revision,
            undo_transaction_id: visual_undo.transaction.transaction_id,
            idempotency_key: "visual-redo-legacy".into(),
        })
        .unwrap();
    let promoted = app
        .promote_project(&PromoteProject {
            branch_name: None,
            project_id: "legacy".into(),
            expected_revision: visual_redo.transaction.revision,
            idempotency_key: "promote".into(),
            asset_versions: Default::default(),
        })
        .unwrap();
    assert_eq!(
        app.historical_archive_source("legacy", &promoted.revision)
            .unwrap_err()
            .code,
        "PACKAGE_HISTORY_UNSUPPORTED"
    );
    let final_head = app
        .restore_project(&RestoreProject {
            project_id: "legacy".into(),
            branch_name: None,
            expected_revision: promoted.revision,
            target_revision: edit.revision,
            idempotency_key: "current-restore-original".into(),
            label: None,
            asset_versions: Default::default(),
        })
        .unwrap();
    use opencut_api_contracts::project_exchange::{ExportPackage, PackageMode, PackageScope};
    use opencut_editor_application::project_exchange as exchange;
    let captured = app
        .project_package_source(&ExportPackage {
            project_id: "legacy".into(),
            revision: final_head.revision,
            destination: temp
                .path()
                .join("not-written")
                .to_string_lossy()
                .into_owned(),
            mode: PackageMode::SelfContained,
            scope: PackageScope::History,
        })
        .unwrap();
    assert_eq!(captured.content.archive.revisions.len(), 12);
    assert_eq!(captured.content.archive.transactions.len(), 11);
    let bytes = exchange::archive_bytes(&captured.content.archive).unwrap();
    let decoded = exchange::decode_archive(&bytes).unwrap();
    let manifest = exchange::make_manifest(
        &captured,
        PackageScope::History,
        PackageMode::SelfContained,
        vec![],
        vec![],
        vec![],
    )
    .unwrap();
    let manifest_sha256 =
        exchange::digest(&exchange::canonical_bytes(&manifest, MAX_MANIFEST_BYTES).unwrap());
    let mapped = exchange::prepare_import(
        &exchange::PreparedPackage {
            manifest_sha256,
            manifest,
            archive: decoded,
            tool_lock_mismatches: vec![],
            fonts_missing: vec![],
            install_plan: vec![],
        },
        "mapped",
    )
    .unwrap();
    assert!(
        mapped
            .revisions
            .iter()
            .all(|r| r.document.project_id() == "mapped")
    );
    let causal: Vec<_> = mapped
        .transactions
        .iter()
        .filter(|t| matches!(&t.journal, ArchivedJournal::HistoricalCausal(_)))
        .collect();
    assert_eq!(causal.len(), 4);
    let visual_causal: Vec<Value> = causal
        .iter()
        .map(|t| serde_json::from_str::<Value>(&t.journal.storage_json().unwrap()).unwrap())
        .filter(|j| j.get("schema_version") == Some(&json!(3)))
        .collect();
    assert_eq!(visual_causal.len(), 2);
    assert!(
        visual_causal
            .iter()
            .all(|j| j.get("project_id").is_none() && j.get("expected_revision").is_none())
    );
    let restored = mapped
        .transactions
        .iter()
        .find_map(|t| match &t.journal {
            ArchivedJournal::LegacyRestore(r) => Some(r),
            _ => None,
        })
        .unwrap();
    assert_eq!(restored.label, "Original legacy/source label");
    assert!(
        mapped
            .revisions
            .iter()
            .any(|r| r.revision == restored.target_revision)
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
    let asset = ProjectAsset {
        id: "asset".into(),
        name: "Unused history".into(),
        kind: AssetKind::Image,
        version: version.clone(),
        duration: None,
        origin: ResourceOrigin::LocalImport {
            original_name: "source.bin".into(),
        },
        metadata: ProjectMediaMetadata {
            mime_type: Some("image/png".into()),
            intrinsic_size: None,
            frame_rate: None,
            rotation_degrees: None,
            audio: AudioPresence::Absent {},
        },
        license: None,
    };
    // Package source stands in for an already verified external archive; CAS
    // authority is checked independently by import even when main has no asset.
    let mut source = app
        .historical_archive_source("source", &initial.revision)
        .unwrap();
    let ArchivedDocument::Editable(document) = &mut source.archive.revisions[0].document else {
        panic!("editable fixture");
    };
    document.assets.push(asset);
    source.archive.main_head_revision = "after-delete".into();
    source.archive.revisions.push(ArchivedRevision {
        revision: "after-delete".into(),
        parent_revision: Some(initial.revision.clone()),
        transaction_id: Some("delete-asset".into()),
        document: initial.project.clone().into(),
        created_at: "2026-01-02".into(),
    });
    source.archive.transactions.push(ArchivedTransaction {
        transaction_id: "delete-asset".into(),
        base_revision: initial.revision.clone(),
        revision: "after-delete".into(),
        journal: ArchivedJournal::Apply(ApplySceneProject {
            project_id: "source".into(),
            expected_revision: initial.revision.clone(),
            idempotency_key: "delete-asset".into(),
            label: "Delete".into(),
            operations: vec![ProjectOperation::AssetDelete {
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
        source_order: None,
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
            .historical_archive_fixture_commit(&input(&p, "copy"), &p)
            .is_err()
    );
    assert_eq!(counts(&target), before);
    // Existing source CAS can restore the same complete bytes without an upload
    // grant; import does not restore historical grants or original idempotency.
    app.historical_archive_fixture_commit(&input(&p, "copy"), &p)
        .unwrap();
}
#[test]
fn deleted_historical_derived_font_proof_and_original_bytes_survive_both_publications() {
    use opencut_asset_store::AssetStore;
    use opencut_contracts::{Asset, AssetKind};
    use opencut_editor_application::font_derived as derived;
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
        .find(|p| p.source.face.weight == 400 && p.source.face.style == FontStyle::Normal)
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
    let font = FontResource {
        id: "font".into(),
        family: proof.face.family.clone(),
        weight: proof.face.weight,
        style: proof.face.style,
        face_index: 0,
        content: version,
        origin: ResourceOrigin::Generated {
            provider: derived::OUTLINE_FONT_PROVIDER.into(),
            generation_id: derivation,
        },
        license: Some(ResourceLicense {
            identifier: Some(proof.license_id.clone()),
            source_url: Some(proof.license_url.clone()),
            notice: Some(proof.license_notice.clone()),
            attribution: None,
        }),
    };
    let mut source = app
        .historical_archive_source("source", &initial.revision)
        .unwrap();
    let ArchivedDocument::Editable(document) = &mut source.archive.revisions[0].document else {
        panic!("editable fixture");
    };
    document.fonts.push(font);
    source.derived_fonts = vec![proof.clone()];
    source.archive.main_head_revision = "font-deleted".into();
    source.archive.revisions.push(ArchivedRevision {
        revision: "font-deleted".into(),
        parent_revision: Some(initial.revision.clone()),
        transaction_id: Some("delete-font".into()),
        document: initial.project.into(),
        created_at: "2026-01-02".into(),
    });
    source.archive.transactions.push(ArchivedTransaction {
        transaction_id: "delete-font".into(),
        base_revision: initial.revision.clone(),
        revision: "font-deleted".into(),
        journal: ArchivedJournal::Apply(ApplySceneProject {
            project_id: "source".into(),
            expected_revision: initial.revision.clone(),
            idempotency_key: "delete-font".into(),
            label: "Delete font".into(),
            operations: vec![ProjectOperation::FontDelete {
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
        source_order: None,
    });
    source
        .archive
        .revisions
        .sort_by(|a, b| a.revision.cmp(&b.revision));
    let prepared = prepare(source);
    let first = app
        .historical_archive_fixture_commit(&input(&prepared, "copy-one"), &prepared)
        .unwrap();
    assert!(inspect(&app, "copy-one").project.fonts.is_empty());
    let exported = app
        .historical_archive_source("copy-one", &first.revision)
        .unwrap();
    assert_eq!(
        serde_json::to_value(&exported.derived_fonts).unwrap(),
        serde_json::to_value(vec![proof]).unwrap()
    );
    let second = prepare(exported);
    app.historical_archive_fixture_commit(&input(&second, "copy-two"), &second)
        .unwrap();
    assert_eq!(
        app.connection
            .query_row("SELECT count(*) FROM font_derivations", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
