use super::*;
use opencut_contracts::{FrameRate, project_v4::*};
use serde_json::json;

fn pin(root: &Path, name: &str, bytes: &[u8]) -> AssetVersion {
    fs::create_dir_all(root.parent().unwrap()).unwrap();
    let input = root.parent().unwrap().join(name);
    fs::write(&input, bytes).unwrap();
    AssetStore::open(root)
        .unwrap()
        .pin_asset(&Asset {
            id: name.into(),
            name: name.into(),
            kind: AssetKind::Image,
            uri: input.to_string_lossy().into_owned(),
            duration: None,
            version: None,
            metadata: BTreeMap::new(),
        })
        .unwrap()
        .version
        .unwrap()
}
fn source(root: &Path) -> PackageExportSourceV4 {
    let media = pin(root, "sample.bin", b"independent original media bytes");
    let font = pin(
        root,
        "original.ttf",
        b"independent original static font bytes",
    );
    let asset = |id: &str, version: AssetVersion| AssetV4 {
        id: id.into(),
        name: id.into(),
        kind: AssetKind::Image,
        version,
        duration: None,
        origin: ResourceOriginV4::LocalImport {
            original_name: "source.png".into(),
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
    let font_resource = |id: &str| FontResourceV4 {
        id: id.into(),
        family: "Fixture family".into(),
        weight: 400,
        style: FontStyleV4::Normal,
        face_index: 0,
        content: font.clone(),
        origin: ResourceOriginV4::LocalImport {
            original_name: "original.ttf".into(),
        },
        license: Some(ResourceLicenseV4 {
            identifier: Some("OFL-1.1".into()),
            source_url: None,
            notice: Some("Original license notice retained verbatim".into()),
            attribution: Some("Fixture author".into()),
        }),
    };
    let track = |id: &str, kind: TrackKindV4| TrackV4 {
        id: id.into(),
        name: id.into(),
        kind,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1.0,
        gain: GainV4::Decibels { value: 0.0 },
        elements: vec![],
        channels: vec![],
    };
    let mut graphic = track("graphics", TrackKindV4::Graphic);
    graphic.elements.push(serde_json::from_value(json!({
        "id":"rectangle", "name":"Preserved parameter graphic", "enabled":false,
        "timeline_range":{"start":{"ticks":"1","timebase":7},"duration":{"ticks":"2","timebase":3}},
        "source_span":{"profile":"classic_trimmed","trim_start":{"ticks":"0","timebase":1},"trim_end":{"ticks":"0","timebase":1},"source_duration":null},
        "content":{"kind":"graphic","graphic":{"source_size":{"width":60,"height":50},"geometry":{"kind":"rectangle","corner_radius_percent":25},"fill":{"encoding":"srgb","red":0.8,"green":0.2,"blue":0.3,"alpha":0.7},"stroke":{"color":{"encoding":"srgb","red":1,"green":1,"blue":1,"alpha":1},"width_px":0,"align":"inside"}}},
        "visual":{"profile":"classic_canvas_v31","transform":{"position_x_px":-8,"position_y_px":4,"rotation_degrees":14,"scale_x":0.32,"scale_y":0.27},"opacity":0.9,"blend_mode":"normal","effects":[],"masks":[]},"channels":[]
    })).unwrap());
    PackageExportSourceV4 {
        snapshot: ProjectSnapshotV4 {
            revision: "fixed-revision".into(),
            project: ProjectDocumentV4 {
                schema_version: 4,
                id: "source-project".into(),
                name: "V4 exact source".into(),
                default_sequence_id: "scene".into(),
                settings: SettingsProfileV4::ClassicGlobal {
                    settings: ProjectSettingsV4 {
                        fps: FrameRate {
                            numerator: 30000,
                            denominator: 1001,
                        },
                        canvas_size: CanvasSizeV4 {
                            width: 320,
                            height: 180,
                        },
                        canvas_size_mode: CanvasSizeModeV4::Custom,
                        last_custom_canvas_size: Some(CanvasSizeV4 {
                            width: 320,
                            height: 180,
                        }),
                        original_canvas_size: None,
                        background: BackgroundV4::MainTrackBlur { intensity: 10.0 },
                    },
                },
                // Deliberately non-sorted logical arrays; the document must remain exact.
                assets: vec![
                    asset("media-z", media.clone()),
                    asset("media-a", media),
                    asset("font-as-unused-media", font.clone()),
                ],
                fonts: vec![font_resource("font-z"), font_resource("font-a")],
                sequences: vec![SequenceV4 {
                    id: "scene".into(),
                    name: "Scene".into(),
                    main_track_id: "main".into(),
                    tracks: vec![track("main", TrackKindV4::Video), graphic],
                    captions: vec![],
                    transitions: vec![],
                    bookmarks: vec![],
                }],
            },
        },
        derived_fonts: vec![],
    }
}
fn export(directory: &Path) -> (PackageExportSourceV4, ExportReceiptV4, PathBuf) {
    let cas = directory.join("origin/assets");
    let source = source(&cas);
    let receipt = export_package_v4(
        &ExportPackageV4 {
            project_id: source.snapshot.project.id.clone(),
            revision: source.snapshot.revision.clone(),
            destination: directory.join("package").to_string_lossy().into_owned(),
        },
        &source,
        &cas,
    )
    .unwrap();
    (source, receipt, cas)
}
fn import_request(receipt: &ExportReceiptV4) -> ImportPackageV4 {
    ImportPackageV4 {
        package_path: receipt.package_path.clone(),
        target_project_id: "restored-project".into(),
        expected_manifest_sha256: receipt.manifest_sha256.clone(),
        idempotency_key: "restore-once".into(),
    }
}

#[test]
fn exact_v4_roundtrip_keeps_unused_aliases_fonts_and_graphics_after_source_removal() {
    let directory = tempfile::tempdir().unwrap();
    let (source, receipt, cas) = export(directory.path());
    let package = Path::new(&receipt.package_path);
    assert_eq!((receipt.asset_count, receipt.font_count), (3, 2));
    assert!(receipt.self_contained_resources);
    assert!(!receipt.history_included && !receipt.fully_reproducible);
    assert_eq!(fs::read_dir(package.join("media")).unwrap().count(), 2);
    assert_eq!(fs::read_dir(package.join("fonts")).unwrap().count(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let hash = &source.snapshot.project.fonts[0].content.content_hash;
        assert_eq!(
            fs::metadata(package.join("fonts").join(hash))
                .unwrap()
                .ino(),
            fs::metadata(package.join("media").join(hash))
                .unwrap()
                .ino()
        );
    }
    fs::remove_dir_all(cas.parent().unwrap()).unwrap();
    let before = fs::read(package.join("manifest.json")).unwrap();
    let inspected = inspect_package_v4(&InspectPackageV4 {
        package_path: receipt.package_path.clone(),
    })
    .unwrap();
    assert_eq!(inspected.manifest_sha256, receipt.manifest_sha256);
    assert_eq!(inspected.project_name, source.snapshot.project.name);
    assert_eq!(fs::read(package.join("manifest.json")).unwrap(), before);
    assert!(!directory.path().join("target").exists());
    fs::create_dir(directory.path().join("target")).unwrap();
    let target = directory.path().join("target/assets");
    let prepared = read_package_v4(&import_request(&receipt), &target).unwrap();
    assert_eq!(prepared.document, source.snapshot.project);
    assert_eq!(
        prepared
            .manifest
            .assets
            .iter()
            .map(|a| a.resource_id.as_str())
            .collect::<Vec<_>>(),
        vec!["font-as-unused-media", "media-a", "media-z"]
    );
    let store = AssetStore::read_only(&target).unwrap();
    for resource in prepared
        .manifest
        .assets
        .iter()
        .chain(&prepared.manifest.fonts)
    {
        store
            .open_version_verified(&resource_version(resource))
            .unwrap();
    }
}

#[test]
fn malformed_tail_or_unlisted_file_fails_before_any_cas_capture() {
    let directory = tempfile::tempdir().unwrap();
    let (source, receipt, _) = export(directory.path());
    let package = Path::new(&receipt.package_path);
    fs::create_dir(directory.path().join("target")).unwrap();
    let target = directory.path().join("target/assets");
    let request = import_request(&receipt);
    let fail = || {
        assert!(
            inspect_package_v4(&InspectPackageV4 {
                package_path: receipt.package_path.clone()
            })
            .is_err()
        );
        assert!(read_package_v4(&request, &target).is_err());
        assert!(!target.exists(), "Rejected package must not create CAS");
        assert_eq!(
            fs::read_dir(directory.path().join("target"))
                .unwrap()
                .count(),
            0
        );
    };
    fs::write(package.join("unlisted"), b"unexpected").unwrap();
    fail();
    fs::remove_file(package.join("unlisted")).unwrap();
    fs::write(package.join("media/unlisted"), b"unexpected").unwrap();
    fail();
    fs::remove_file(package.join("media/unlisted")).unwrap();
    let font = package
        .join("fonts")
        .join(&source.snapshot.project.fonts[0].content.content_hash);
    let bytes = fs::read(&font).unwrap();
    // Hard links intentionally share this font with a media entry. Corruption
    // invalidates both; neither may be published to the target CAS.
    fs::write(&font, b"corrupt").unwrap();
    fail();
    fs::write(&font, &bytes).unwrap();
    fs::remove_file(&font).unwrap();
    fail();
    fs::write(&font, &bytes).unwrap();
    #[cfg(unix)]
    {
        fs::remove_file(&font).unwrap();
        let outside = directory.path().join("same-font.ttf");
        fs::write(&outside, &bytes).unwrap();
        std::os::unix::fs::symlink(&outside, &font).unwrap();
        fail();
        fs::remove_file(&font).unwrap();
        fs::write(&font, &bytes).unwrap();
        let outside_dir = directory.path().join("outside-fonts");
        fs::rename(package.join("fonts"), &outside_dir).unwrap();
        std::os::unix::fs::symlink(&outside_dir, package.join("fonts")).unwrap();
        fail();
        fs::remove_file(package.join("fonts")).unwrap();
        fs::rename(&outside_dir, package.join("fonts")).unwrap();
    }
    let mut wrong = request.clone();
    wrong.expected_manifest_sha256 = "0".repeat(64);
    assert!(read_package_v4(&wrong, &target).is_err());
    assert!(!target.exists());
    // Inspection does not authorize a later changed package.
    inspect_package_v4(&InspectPackageV4 {
        package_path: receipt.package_path.clone(),
    })
    .unwrap();
    fs::write(&font, b"changed after inspection").unwrap();
    assert!(read_package_v4(&request, &target).is_err());
    assert!(!target.exists());
}

#[test]
fn canonical_json_and_exact_inventory_are_required_even_with_updated_hashes() {
    let directory = tempfile::tempdir().unwrap();
    let (_, receipt, _) = export(directory.path());
    let root = Path::new(&receipt.package_path);
    let original = fs::read(root.join("manifest.json")).unwrap();
    let manifest: PackageManifestV4 = serde_json::from_slice(&original).unwrap();
    fs::create_dir(directory.path().join("target")).unwrap();
    let target = directory.path().join("target/assets");
    let verify_bad = |bytes: &[u8]| {
        fs::write(root.join("manifest.json"), bytes).unwrap();
        let mut request = import_request(&receipt);
        request.expected_manifest_sha256 = common::digest(bytes);
        assert!(read_package_v4(&request, &target).is_err());
        assert!(!target.exists());
    };
    let mut spaced = original.clone();
    spaced.push(b'\n');
    verify_bad(&spaced);
    let mut duplicate = manifest.clone();
    duplicate.assets.push(duplicate.assets[0].clone());
    verify_bad(&serde_json::to_vec(&duplicate).unwrap());
    let mut traversal = manifest.clone();
    traversal.assets[0].path = "../outside".into();
    verify_bad(&serde_json::to_vec(&traversal).unwrap());
    let mut missing = manifest.clone();
    missing.fonts.pop();
    verify_bad(&serde_json::to_vec(&missing).unwrap());
    let project = fs::read(root.join("project.json")).unwrap();
    let mut noncanonical = project.clone();
    noncanonical.push(b'\n');
    fs::write(root.join("project.json"), &noncanonical).unwrap();
    let mut rehashed = manifest;
    rehashed.project_sha256 = common::digest(&noncanonical);
    verify_bad(&serde_json::to_vec(&rehashed).unwrap());
}

#[test]
fn export_never_replaces_destination_and_cleans_only_its_failed_stage() {
    let directory = tempfile::tempdir().unwrap();
    let (mut source, receipt, cas) = export(directory.path());
    let manifest_before = fs::read(Path::new(&receipt.package_path).join("manifest.json")).unwrap();
    let mut input = ExportPackageV4 {
        project_id: source.snapshot.project.id.clone(),
        revision: source.snapshot.revision.clone(),
        destination: receipt.package_path.clone(),
    };
    assert!(export_package_v4(&input, &source, &cas).is_err());
    assert_eq!(
        fs::read(Path::new(&receipt.package_path).join("manifest.json")).unwrap(),
        manifest_before
    );
    let destination = directory.path().join("occupied");
    fs::create_dir(&destination).unwrap();
    let stage = directory.path().join("owned-stage");
    fs::create_dir(&stage).unwrap();
    fs::write(stage.join("sentinel"), b"owned").unwrap();
    assert!(publish_new_directory(&stage, &destination).is_err());
    assert!(stage.join("sentinel").is_file());
    assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
    let missing = "0".repeat(64);
    source.snapshot.project.assets[0].version.content_hash = missing.clone();
    source.snapshot.project.assets[0].version.id = format!("av_{missing}");
    input.destination = directory
        .path()
        .join("failed-export")
        .to_string_lossy()
        .into_owned();
    assert!(export_package_v4(&input, &source, &cas).is_err());
    assert!(!Path::new(&input.destination).exists());
    assert!(fs::read_dir(directory.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".opencut-package-v4-")
    }));
    assert!(stage.join("sentinel").is_file());
}

#[test]
fn total_budget_counts_deduplicated_bytes_and_json_but_rejects_conflicting_sizes() {
    let directory = tempfile::tempdir().unwrap();
    let (_, receipt, _) = export(directory.path());
    let mut manifest: PackageManifestV4 = serde_json::from_slice(
        &fs::read(Path::new(&receipt.package_path).join("manifest.json")).unwrap(),
    )
    .unwrap();
    let unique: BTreeMap<_, _> = manifest
        .assets
        .iter()
        .chain(&manifest.fonts)
        .map(|r| (&r.content_hash, r.size_bytes))
        .collect();
    assert_eq!(
        package_bytes(&manifest, 123).unwrap(),
        123 + unique.values().sum::<u64>()
    );
    manifest.assets[0].size_bytes += 1;
    assert!(package_bytes(&manifest, 123).is_err());
    let mut single = manifest.clone();
    single.fonts.clear();
    single.assets = (0..5)
        .map(|i| PackageResourceV4 {
            resource_id: format!("a{i}"),
            content_hash: format!("{i:064x}"),
            size_bytes: common::MAX_ASSET_BYTES,
            path: format!("media/{i:064x}"),
        })
        .collect();
    assert!(package_bytes(&single, 0).is_err());
    single.assets.truncate(4);
    assert!(package_bytes(&single, 0).is_ok());
    assert!(package_bytes(&single, 1).is_err());
}

#[cfg(unix)]
#[test]
fn parent_alias_export_inspect_and_read_agree_but_package_root_symlinks_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let cas = directory.path().join("origin/assets");
    let source = source(&cas);
    let real_parent = directory.path().join("real-parent");
    fs::create_dir(&real_parent).unwrap();
    let alias_parent = directory.path().join("parent-alias");
    std::os::unix::fs::symlink(&real_parent, &alias_parent).unwrap();
    let attempted_path = alias_parent.join("package").to_string_lossy().into_owned();
    let receipt = export_package_v4(
        &ExportPackageV4 {
            project_id: source.snapshot.project.id.clone(),
            revision: source.snapshot.revision.clone(),
            destination: attempted_path.clone(),
        },
        &source,
        &cas,
    )
    .unwrap();
    assert_eq!(
        Path::new(&receipt.package_path),
        fs::canonicalize(real_parent.join("package")).unwrap()
    );
    assert_ne!(receipt.package_path, attempted_path);
    let inspection = inspect_package_v4(&InspectPackageV4 {
        package_path: attempted_path.clone(),
    })
    .unwrap();
    assert_eq!(inspection.manifest_sha256, receipt.manifest_sha256);
    fs::create_dir(directory.path().join("target")).unwrap();
    let mut request = import_request(&receipt);
    request.package_path = attempted_path;
    let prepared = read_package_v4(&request, &directory.path().join("target/assets")).unwrap();
    assert_eq!(prepared.document, source.snapshot.project);
    let root_link = directory.path().join("package-root-alias");
    std::os::unix::fs::symlink(&receipt.package_path, &root_link).unwrap();
    for path in [root_link.clone(), root_link.join(".")] {
        request.package_path = path.to_string_lossy().into_owned();
        assert!(
            inspect_package_v4(&InspectPackageV4 {
                package_path: request.package_path.clone()
            })
            .is_err()
        );
        assert!(read_package_v4(&request, &directory.path().join("unwritten-assets")).is_err());
        assert!(!directory.path().join("unwritten-assets").exists());
    }
}
