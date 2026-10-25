use super::*;

#[test]
fn existing_missing_media_remain_editable_new_bindings_and_execution_are_strict() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(":memory:").unwrap();
    let initial = app.create_scene_project(&create_request("p")).unwrap();
    let resource = capture_asset(&app, directory.path(), "image", b"tiny fixed resource");
    app.apply_scene_project(&edit(
        &initial,
        "bind",
        vec![ProjectOperation::AssetPut {
            asset: resource.clone(),
        }],
    ))
    .unwrap();
    let object = app
        .asset_store_root
        .join(&resource.version.content_hash[..2])
        .join(&resource.version.content_hash);
    std::fs::remove_file(&object).unwrap();
    let current = inspect(&app, "p");
    app.apply_scene_project(&edit(
        &current,
        "offline-bookmark",
        vec![bookmark("offline-marker")],
    ))
    .unwrap();
    // Renaming/re-identifying the same immutable binding also does not rehash it.
    let mut renamed = resource.clone();
    renamed.id = "alias".into();
    renamed.name = "Renamed offline media".into();
    let current = inspect(&app, "p");
    app.apply_scene_project(&edit(
        &current,
        "offline-alias",
        vec![ProjectOperation::AssetPut { asset: renamed }],
    ))
    .unwrap();
    let execution_asset = Asset {
        id: resource.id.clone(),
        name: resource.name.clone(),
        kind: resource.kind.clone(),
        uri: directory.path().join("image.bin").to_string_lossy().into(),
        duration: None,
        version: Some(resource.version.clone()),
        metadata: BTreeMap::new(),
    };
    // Strict resource execution never falls back to the still-existing provenance.
    assert_eq!(
        AssetStore::read_only(&app.asset_store_root)
            .unwrap()
            .open_verified(&execution_asset)
            .unwrap_err()
            .code,
        "ASSET_IO_ERROR"
    );
    let mut new = resource;
    new.id = "unavailable-new".into();
    new.version = AssetVersion {
        id: format!("av_{}", "b".repeat(64)),
        content_hash: "b".repeat(64),
        size: 17,
    };
    new.origin = ResourceOrigin::LegacyLocator {
        uri: directory.path().join("image.bin").to_string_lossy().into(),
    };
    let current = inspect(&app, "p");
    let before = counts(&app);
    assert_eq!(
        app.apply_scene_project(&edit(
            &current,
            "new-missing",
            vec![ProjectOperation::AssetPut { asset: new }]
        ))
        .unwrap_err()
        .code,
        "ASSET_IO_ERROR"
    );
    assert_eq!(counts(&app), before);
    // Copy publishes a new project authority and must verify all source bytes.
    let copy = CopySceneProject {
        source_project_id: "p".into(),
        source_revision: current.revision,
        project_id: "copy".into(),
        name: "Copy".into(),
        idempotency_key: "copy-missing".into(),
    };
    assert_eq!(
        app.copy_scene_project(&copy).unwrap_err().code,
        "ASSET_IO_ERROR"
    );
    assert_eq!(counts(&app), before);
}

#[test]
fn promotion_verifies_actual_cas_before_changing_schema_or_history() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(":memory:").unwrap();
    let initial = app
        .create_project(create_request("p").project, "legacy-create")
        .unwrap();
    let resource = capture_asset(&app, directory.path(), "media", b"original pinned bytes");
    let legacy_asset = Asset {
        id: "media".into(),
        name: "media".into(),
        kind: AssetKind::Image,
        uri: directory.path().join("media.bin").to_string_lossy().into(),
        duration: None,
        version: Some(resource.version.clone()),
        metadata: BTreeMap::new(),
    };
    app.apply(
        "p",
        &TransactionRequest {
            expected_revision: initial.revision,
            idempotency_key: "legacy-bind".into(),
            label: "Bind known CAS".into(),
            operations: vec![opencut_contracts::Operation::BindAsset {
                asset: legacy_asset,
            }],
        },
    )
    .unwrap();
    let current = app.snapshot("p").unwrap();
    let old_bytes = raw(&app, &current.revision);
    let object = app
        .asset_store_root
        .join(&resource.version.content_hash[..2])
        .join(&resource.version.content_hash);
    // Simulate an external privileged mutation; production CAS objects remain
    // read-only. Restore their original protection after corrupting this fixture.
    let original_permissions = std::fs::metadata(&object).unwrap().permissions();
    let mut writable_permissions = original_permissions.clone();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        writable_permissions.set_mode(original_permissions.mode() | 0o200);
    }
    #[cfg(not(unix))]
    writable_permissions.set_readonly(false);
    std::fs::set_permissions(&object, writable_permissions).unwrap();
    std::fs::write(&object, b"tampered pinned bytes").unwrap();
    std::fs::set_permissions(&object, original_permissions).unwrap();
    let before = counts(&app);
    let promote = PromoteProject {
        branch_name: None,
        project_id: "p".into(),
        expected_revision: current.revision.clone(),
        idempotency_key: "promote-tampered".into(),
        asset_versions: BTreeMap::new(),
    };
    assert_eq!(
        app.promote_project(&promote).unwrap_err().code,
        "ASSET_INTEGRITY_ERROR"
    );
    assert_eq!(counts(&app), before);
    assert_eq!(raw(&app, &current.revision), old_bytes);
    assert_eq!(app.snapshot("p").unwrap(), current);
}

#[test]
fn resource_reads_select_fixed_asset_or_font_with_current_authority() {
    use opencut_contracts::project_resources::*;
    let directory = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(":memory:").unwrap();
    let initial = app.create_scene_project(&create_request("p")).unwrap();
    let resource = capture_asset(
        &app,
        directory.path(),
        "resource",
        b"exact authorized bytes",
    );
    let version = resource.version.clone();
    app.apply_scene_project(&edit(
        &initial,
        "bind-two-kinds",
        vec![
            ProjectOperation::AssetPut { asset: resource },
            ProjectOperation::FontPut {
                font: FontResource {
                    id: "font".into(),
                    family: "Fixture".into(),
                    weight: 400,
                    style: FontStyle::Normal,
                    face_index: 0,
                    content: version.clone(),
                    origin: ResourceOrigin::Generated {
                        provider: "fixture".into(),
                        generation_id: "fixture".into(),
                    },
                    license: None,
                },
            },
        ],
    ))
    .unwrap();
    let bound = inspect(&app, "p");
    let asset_request = InspectResource {
        project_id: "p".into(),
        revision: bound.revision.clone(),
        kind: ResourceKind::Asset,
        resource_id: "resource".into(),
    };
    let font_request = InspectResource {
        kind: ResourceKind::Font,
        resource_id: "font".into(),
        ..asset_request.clone()
    };
    assert_eq!(
        app.inspect_project_resource(&asset_request)
            .unwrap()
            .version,
        version
    );
    assert_eq!(
        app.inspect_project_resource(&font_request).unwrap().version,
        version
    );
    assert_eq!(
        app.inspect_project_resource(&InspectResource {
            kind: ResourceKind::Font,
            ..asset_request.clone()
        })
        .unwrap_err()
        .code,
        "NOT_FOUND"
    );
    assert_eq!(
        app.inspect_project_resource(&InspectResource {
            revision: initial.revision,
            ..asset_request.clone()
        })
        .unwrap_err()
        .code,
        "NOT_FOUND"
    );
    // Head changes do not retarget an explicitly pinned historical read.
    app.apply_scene_project(&edit(
        &bound,
        "remove-head-resource",
        vec![ProjectOperation::AssetDelete {
            asset_id: "resource".into(),
        }],
    ))
    .unwrap();
    assert_eq!(
        app.inspect_project_resource(&asset_request)
            .unwrap()
            .version,
        version
    );
    let (caller, credential_id) = credential(&mut app, "p");
    app.set_caller(caller.clone());
    assert!(app.inspect_project_resource(&font_request).is_ok());
    app.set_caller(Caller::LocalOwner);
    app.create_scene_project(&create_request("other")).unwrap();
    app.set_caller(caller.clone());
    assert_eq!(
        app.inspect_project_resource(&InspectResource {
            project_id: "other".into(),
            ..asset_request.clone()
        })
        .unwrap_err()
        .code,
        "NOT_AUTHORIZED"
    );
    app.set_caller(Caller::LocalOwner);
    app.revoke_credential(&credential_id).unwrap();
    app.set_caller(caller);
    assert_eq!(
        app.inspect_project_resource(&font_request)
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    app.set_caller(Caller::LocalOwner);
    // Descriptors remain editable metadata observations even when bytes vanish.
    let object = app
        .asset_store_root
        .join(&version.content_hash[..2])
        .join(&version.content_hash);
    std::fs::remove_file(object).unwrap();
    assert!(app.inspect_project_resource(&font_request).is_ok());
    assert!(
        AssetStore::read_only(&app.asset_store_root)
            .unwrap()
            .open_version_verified(&version)
            .is_err()
    );
}
