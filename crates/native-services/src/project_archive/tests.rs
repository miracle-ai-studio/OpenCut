//! Real files/CAS exercise identity and capture. Synthetic font bytes are not
//! font-decoder or renderer evidence.
use super::*;
use app::{ArchivedBranch, ArchivedJournal, ArchivedRevision, ArchivedTransaction};
use opencut_contracts::{CreateProject, FrameRate, TransactionResult, scene::*, scene_ops::*};
fn pin(root: &Path, name: &str, bytes: &[u8]) -> AssetVersion {
    fs::create_dir_all(root.parent().unwrap()).unwrap();
    let path = root.parent().unwrap().join(name);
    fs::write(&path, bytes).unwrap();
    AssetStore::open(root)
        .unwrap()
        .pin_asset(&Asset {
            id: name.into(),
            name: name.into(),
            kind: AssetKind::Image,
            uri: path.to_string_lossy().into_owned(),
            duration: None,
            version: None,
            metadata: Default::default(),
        })
        .unwrap()
        .version
        .unwrap()
}
fn source(root: &Path) -> ArchiveSource {
    let first = pin(root, "first", b"retained historical source and unused font");
    let second = pin(root, "second", b"new media version");
    let mut before = opencut_editor_application::scene::new_document(&CreateProject {
        id: "source".into(),
        name: "History".into(),
        sequence_id: "scene".into(),
        width: 64,
        height: 48,
        fps: FrameRate {
            numerator: 30,
            denominator: 1,
        },
    })
    .unwrap();
    before.assets.push(ProjectAsset {
        id: "asset".into(),
        name: "Original media".into(),
        kind: AssetKind::Image,
        version: first.clone(),
        duration: None,
        origin: ResourceOrigin::LocalImport {
            original_name: "old.png".into(),
        },
        metadata: ProjectMediaMetadata {
            mime_type: Some("image/png".into()),
            intrinsic_size: None,
            frame_rate: None,
            rotation_degrees: None,
            audio: AudioPresence::Absent {},
        },
        license: None,
    });
    before.fonts.push(FontResource {
        id: "unused-font".into(),
        family: "Synthetic fixture".into(),
        weight: 400,
        style: FontStyle::Normal,
        face_index: 0,
        content: first,
        origin: ResourceOrigin::LocalImport {
            original_name: "old.ttf".into(),
        },
        license: None,
    });
    let mut after = before.clone();
    after.assets[0].version = second;
    after.fonts.clear();
    let result = TransactionResult {
        transaction_id: "t1".into(),
        base_revision: "r0".into(),
        revision: "r1".into(),
        created_ids: vec![],
        changed_ids: vec!["source".into(), "asset".into()],
        deleted_ids: vec!["unused-font".into()],
    };
    ArchiveSource {
        archive: ProjectArchive {
            schema_version: 1,
            source_project_id: "source".into(),
            main_head_revision: "r1".into(),
            revisions: vec![
                ArchivedRevision {
                    revision: "r0".into(),
                    parent_revision: None,
                    transaction_id: None,
                    document: before.into(),
                    created_at: "2026-01-01".into(),
                },
                ArchivedRevision {
                    revision: "r1".into(),
                    parent_revision: Some("r0".into()),
                    transaction_id: Some("t1".into()),
                    document: after.clone().into(),
                    created_at: "2026-01-02".into(),
                },
            ],
            transactions: vec![ArchivedTransaction {
                transaction_id: "t1".into(),
                base_revision: "r0".into(),
                revision: "r1".into(),
                journal: ArchivedJournal::Apply(ApplySceneProject {
                    project_id: "source".into(),
                    expected_revision: "r0".into(),
                    idempotency_key: "edit".into(),
                    label: "Replace and delete".into(),
                    operations: vec![
                        ProjectOperation::AssetPut {
                            asset: after.assets[0].clone(),
                        },
                        ProjectOperation::FontDelete {
                            font_id: "unused-font".into(),
                        },
                    ],
                }),
                result,
                created_at: "2026-01-02".into(),
                source_order: None,
            }],
            branches: vec![ArchivedBranch {
                name: "original".into(),
                head_revision: "r0".into(),
                created_from_revision: "r0".into(),
                created_at: "2026-01-01".into(),
            }],
            merges: vec![],
        },
        derived_fonts: vec![],
    }
}
fn export(
    root: &Path,
    source: &ArchiveSource,
    destination: &Path,
) -> HistoricalArchiveExportReceipt {
    export_historical_archive(
        &HistoricalArchiveExport {
            project_id: "source".into(),
            expected_revision: "r1".into(),
            destination: destination.to_string_lossy().into_owned(),
        },
        source,
        root,
    )
    .unwrap()
}
fn input(receipt: &HistoricalArchiveExportReceipt) -> HistoricalArchiveImport {
    HistoricalArchiveImport {
        package_path: receipt.package_path.clone(),
        target_project_id: "restored".into(),
        expected_manifest_sha256: receipt.package.manifest_sha256.clone(),
        idempotency_key: "import".into(),
    }
}
#[test]
fn complete_history_retains_deleted_versions_and_deduplicates_cross_kind_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source/assets");
    let source = source(&root);
    let receipt = export(&root, &source, &temp.path().join("package"));
    assert_eq!(receipt.package.revision_count, 2);
    assert_eq!(receipt.package.asset_version_count, 2);
    assert_eq!(receipt.package.font_version_count, 1);
    assert_eq!(
        fs::read_dir(Path::new(&receipt.package_path).join("blobs"))
            .unwrap()
            .count(),
        2
    );
    let target = temp.path().join("target/assets");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    let inspect = inspect_historical_archive(&HistoricalArchiveInspect {
        package_path: receipt.package_path.clone(),
    })
    .unwrap();
    assert!(!target.exists());
    assert!(inspect.history_included);
    assert!(!inspect.fully_reproducible);
    let prepared = read_historical_archive(&input(&receipt), &target).unwrap();
    assert_eq!(
        app::canonical_bytes(&prepared.archive, app::MAX_HISTORY_BYTES).unwrap(),
        app::canonical_bytes(&source.archive, app::MAX_HISTORY_BYTES).unwrap()
    );
    let store = AssetStore::read_only(&target).unwrap();
    for blob in &prepared.manifest.blobs {
        store.open_version_verified(&version(blob)).unwrap();
    }
    assert!(
        export_historical_archive(
            &HistoricalArchiveExport {
                project_id: "source".into(),
                expected_revision: "r1".into(),
                destination: receipt.package_path.clone()
            },
            &source,
            &root
        )
        .is_err()
    );
}
#[test]
fn extra_entries_tampered_history_and_blobs_fail_before_any_capture() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source/assets");
    let source = source(&root);
    let receipt = export(&root, &source, &temp.path().join("package"));
    let destination = temp.path().join("target/assets");
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    let request = input(&receipt);
    let package = Path::new(&receipt.package_path);
    fs::write(package.join("extra"), b"not declared").unwrap();
    assert!(read_historical_archive(&request, &destination).is_err());
    assert!(!destination.exists());
    fs::remove_file(package.join("extra")).unwrap();
    let history = fs::read(package.join("history.json")).unwrap();
    let mut changed = history.clone();
    changed.push(b' ');
    fs::write(package.join("history.json"), changed).unwrap();
    assert!(read_historical_archive(&request, &destination).is_err());
    assert!(!destination.exists());
    fs::write(package.join("history.json"), history).unwrap();
    let blob = fs::read_dir(package.join("blobs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(&blob, b"tampered").unwrap();
    assert!(read_historical_archive(&request, &destination).is_err());
    assert!(!destination.exists());
}
#[cfg(unix)]
#[test]
fn canonical_parent_alias_is_accepted_but_package_root_and_blob_symlinks_are_rejected() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source/assets");
    let source = source(&root);
    let actual = temp.path().join("actual");
    fs::create_dir(&actual).unwrap();
    let alias = temp.path().join("alias");
    symlink(&actual, &alias).unwrap();
    let receipt = export(&root, &source, &alias.join("package"));
    let requested = alias.join("package").to_string_lossy().into_owned();
    assert!(
        inspect_historical_archive(&HistoricalArchiveInspect {
            package_path: requested.clone()
        })
        .is_ok()
    );
    let target = temp.path().join("target/assets");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    let mut request = input(&receipt);
    request.package_path = requested;
    read_historical_archive(&request, &target).unwrap();
    let root_link = temp.path().join("root-link");
    symlink(&receipt.package_path, &root_link).unwrap();
    assert!(
        inspect_historical_archive(&HistoricalArchiveInspect {
            package_path: root_link.to_string_lossy().into_owned()
        })
        .is_err()
    );
    let blob = fs::read_dir(Path::new(&receipt.package_path).join("blobs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let outside = temp.path().join("outside");
    fs::rename(&blob, &outside).unwrap();
    symlink(&outside, &blob).unwrap();
    assert!(
        inspect_historical_archive(&HistoricalArchiveInspect {
            package_path: receipt.package_path
        })
        .is_err()
    );
}
#[cfg(unix)]
#[test]
fn opened_directory_cannot_be_retargeted_by_replacing_the_package_path() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source/assets");
    let source = source(&root);
    let receipt = export(&root, &source, &temp.path().join("package"));
    let directory = PackageDirectory::open(Path::new(&receipt.package_path)).unwrap();
    let original = bounded(
        directory.child("manifest.json", false).unwrap(),
        app::MAX_MANIFEST_BYTES,
    )
    .unwrap();
    let moved = temp.path().join("original-package");
    fs::rename(&receipt.package_path, &moved).unwrap();
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("manifest.json"), b"untrusted replacement").unwrap();
    symlink(&outside, &receipt.package_path).unwrap();
    assert_eq!(
        bounded(
            directory.child("manifest.json", false).unwrap(),
            app::MAX_MANIFEST_BYTES
        )
        .unwrap(),
        original
    );
    let manifest: HistoricalArchiveManifest = serde_json::from_slice(&original).unwrap();
    tree(&directory, &manifest).unwrap();
    for blob in &manifest.blobs {
        verify_blob(directory.resource(&blob.path).unwrap(), blob).unwrap();
    }
    assert!(
        inspect_historical_archive(&HistoricalArchiveInspect {
            package_path: receipt.package_path
        })
        .is_err()
    );
}
