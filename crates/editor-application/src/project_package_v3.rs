//! Schema 3 project packages validate their visual graph and portable schema 4 grant snapshots.
use crate::project_package::{self, PackageExportSource};
use opencut_api_contracts::{
    project_package::{PackageDependencies, PackageManifest, PreparedImport},
    project_package_v3::{
        PackageManifestV3, PackagePluginGrant, PluginGrantRestoreAction, PreparedImportV3,
    },
};
use opencut_contracts::{
    EditorError, ProjectSnapshot,
    project_v3::{ProjectDocumentV3, ProjectSnapshotV3},
};

#[derive(Clone, Debug)]
pub struct PackageExportSourceV3 {
    pub snapshot: ProjectSnapshotV3,
    pub plugin_grants: Vec<PackagePluginGrant>,
}

pub fn dependencies(source: &PackageExportSourceV3) -> PackageDependencies {
    project_package::dependencies(&PackageExportSource {
        snapshot: ProjectSnapshot {
            project: source.snapshot.project.base.clone(),
            revision: source.snapshot.revision.clone(),
        },
        plugin_grants: plugin_dependencies(&source.plugin_grants),
    })
}

fn plugin_dependencies(grants: &[PackagePluginGrant]) -> Vec<String> {
    let mut dependencies = grants
        .iter()
        .map(plugin_dependency_identity)
        .collect::<Vec<_>>();
    dependencies.sort();
    dependencies.dedup();
    dependencies
}

fn plugin_dependency_identity(grant: &PackagePluginGrant) -> String {
    match grant {
        PackagePluginGrant::PlannerV1 {
            id,
            version,
            package_sha256,
            ..
        } => format!("v1:{id}@{version}#{package_sha256}"),
        PackagePluginGrant::PresetV2 {
            id,
            version,
            package_sha256,
            ..
        } => format!("v2:{id}@{version}#{package_sha256}"),
    }
}

fn validate_plugin_grants(grants: &[PackagePluginGrant]) -> Result<(), EditorError> {
    if grants.len() > 128 {
        return Err(EditorError::new(
            "PACKAGE_INVALID",
            "Package contains too many project plugin grants",
        ));
    }
    let mut previous = None;
    for grant in grants {
        let (kind, id, version) = match grant {
            PackagePluginGrant::PlannerV1 {
                id,
                version,
                package_sha256,
                operations,
                enabled,
            } => {
                let input = opencut_api_contracts::project_plugins::SetGrant {
                    target: opencut_api_contracts::project_plugins::PluginTarget {
                        project_id: "package-validation".into(),
                        id: id.clone(),
                        version: version.clone(),
                    },
                    package_sha256: package_sha256.clone(),
                    operations: operations.clone(),
                    enabled: *enabled,
                    expected_revision: "0".into(),
                    idempotency_key: "package-validation".into(),
                };
                crate::project_plugins::validate_grant(&input)?;
                ("v1", id, version)
            }
            PackagePluginGrant::PresetV2 {
                id,
                version,
                package_sha256,
                preset_names,
                enabled,
            } => {
                let input = opencut_api_contracts::plugin_v2::SetGrant {
                    target: opencut_api_contracts::plugin_v2::GrantTarget {
                        project_id: "package-validation".into(),
                        id: id.clone(),
                        version: version.clone(),
                    },
                    package_sha256: package_sha256.clone(),
                    preset_names: preset_names.clone(),
                    enabled: *enabled,
                    expected_revision: "0".into(),
                    idempotency_key: "package-validation".into(),
                };
                crate::plugin_v2::validate_grant(&input)?;
                ("v2", id, version)
            }
        };
        let identity = format!("{kind}:{id}@{version}");
        if previous
            .as_ref()
            .is_some_and(|previous| previous >= &identity)
        {
            return Err(EditorError::new(
                "PACKAGE_INVALID",
                "Package plugin grants are duplicated or not canonically ordered",
            ));
        }
        previous = Some(identity);
    }
    Ok(())
}

pub fn plugin_grant_restore_plan(
    grants: &[PackagePluginGrant],
    target_project_id: &str,
    manifest_sha256: &str,
) -> Result<Vec<PluginGrantRestoreAction>, EditorError> {
    if target_project_id.trim().is_empty()
        || target_project_id.len() > 1024
        || target_project_id.chars().any(char::is_control)
        || !is_sha256(manifest_sha256)
    {
        return Err(EditorError::new(
            "PACKAGE_INVALID",
            "Invalid plugin grant restore plan identity",
        ));
    }
    validate_plugin_grants(grants)?;
    Ok(grants
        .iter()
        .enumerate()
        .map(|(index, grant)| {
            let idempotency_key = format!("package-restore-{manifest_sha256}-{index}");
            match grant {
                PackagePluginGrant::PlannerV1 {
                    id,
                    version,
                    package_sha256,
                    operations,
                    enabled,
                } => PluginGrantRestoreAction::PlannerV1(
                    opencut_api_contracts::project_plugins::SetGrant {
                        target: opencut_api_contracts::project_plugins::PluginTarget {
                            project_id: target_project_id.into(),
                            id: id.clone(),
                            version: version.clone(),
                        },
                        package_sha256: package_sha256.clone(),
                        operations: operations.clone(),
                        enabled: *enabled,
                        expected_revision: "0".into(),
                        idempotency_key,
                    },
                ),
                PackagePluginGrant::PresetV2 {
                    id,
                    version,
                    package_sha256,
                    preset_names,
                    enabled,
                } => {
                    PluginGrantRestoreAction::PresetV2(opencut_api_contracts::plugin_v2::SetGrant {
                        target: opencut_api_contracts::plugin_v2::GrantTarget {
                            project_id: target_project_id.into(),
                            id: id.clone(),
                            version: version.clone(),
                        },
                        package_sha256: package_sha256.clone(),
                        preset_names: preset_names.clone(),
                        enabled: *enabled,
                        expected_revision: "0".into(),
                        idempotency_key,
                    })
                }
            }
        })
        .collect())
}

pub fn validate_export_document(document: &ProjectDocumentV3) -> Result<(), EditorError> {
    opencut_editor_core::project_v3::validate_project_v3(document)
}

pub fn prepare_import(
    prepared: &PreparedImportV3,
    target_project_id: &str,
) -> Result<ProjectDocumentV3, EditorError> {
    let manifest = &prepared.manifest;
    if !matches!(manifest.schema_version, 2..=4)
        || manifest.project_schema_version != 3
        || prepared.document.schema_version != 3
    {
        return Err(EditorError::new(
            "PACKAGE_INVALID",
            "Unsupported schema 3 package version",
        ));
    }
    validate_export_document(&prepared.document)?;
    if manifest.schema_version == 4 {
        validate_plugin_grants(&manifest.plugin_grants)?;
    }
    let source = PackageExportSourceV3 {
        snapshot: ProjectSnapshotV3 {
            project: prepared.document.clone(),
            revision: manifest.source_revision.clone(),
        },
        plugin_grants: manifest.plugin_grants.clone(),
    };
    let declared = dependencies(&source);
    let plugin_dependencies_valid = match manifest.schema_version {
        2 | 3 => manifest.plugin_grants.is_empty(),
        4 => declared.plugins_not_bundled == manifest.dependencies.plugins_not_bundled,
        _ => false,
    };
    if declared.fonts_not_bundled != manifest.dependencies.fonts_not_bundled
        || !plugin_dependencies_valid
        || (manifest.schema_version == 2
            && declared.tools_not_locked != manifest.dependencies.tools_not_locked)
        || (matches!(manifest.schema_version, 3 | 4)
            && !manifest.dependencies.tools_not_locked.is_empty())
        || !manifest.dependencies.unsupported.is_empty()
    {
        return Err(EditorError::new(
            "PACKAGE_INVALID",
            "Package dependency report does not match project content",
        ));
    }
    let mut base_dependencies = manifest.dependencies.clone();
    // Grant state travels in the reviewed restore plan and is never enabled by package import.
    if manifest.schema_version == 4 {
        base_dependencies.plugins_not_bundled.clear();
    }
    let base_manifest = PackageManifest {
        schema_version: 1,
        mode: manifest.mode.clone(),
        source_project_id: manifest.source_project_id.clone(),
        source_revision: manifest.source_revision.clone(),
        project_path: manifest.project_path.clone(),
        project_sha256: manifest.project_sha256.clone(),
        assets: manifest.assets.clone(),
        dependencies: base_dependencies,
        self_contained_media: manifest.self_contained_media,
        fully_reproducible: manifest.fully_reproducible,
    };
    let base = project_package::prepare_import(
        &PreparedImport {
            manifest_sha256: prepared.manifest_sha256.clone(),
            manifest: base_manifest,
            document: prepared.document.base.clone(),
        },
        target_project_id,
    )?
    .document;
    let mut document = prepared.document.clone();
    document.base = base;
    // Covers shape track references, effect/mask targets and transition source closure.
    validate_export_document(&document)?;
    Ok(document)
}

pub fn validate_manifest(manifest: &PackageManifestV3) -> Result<(), EditorError> {
    let tool_lock_names: Vec<_> = manifest
        .tool_locks
        .iter()
        .map(|lock| lock.name.as_str())
        .collect();
    let expected_tool_names = ["ffmpeg", "ffprobe"];
    let tool_locks_valid = match manifest.schema_version {
        2 => {
            manifest.tool_locks.is_empty()
                && manifest.dependencies.tools_not_locked == expected_tool_names.map(str::to_owned)
        }
        3 | 4 => {
            tool_lock_names == expected_tool_names
                && manifest
                    .tool_locks
                    .iter()
                    .all(|lock| !lock.version.trim().is_empty() && is_sha256(&lock.binary_sha256))
                && manifest.dependencies.tools_not_locked.is_empty()
        }
        _ => false,
    };
    let grants_valid = match manifest.schema_version {
        2 | 3 => manifest.plugin_grants.is_empty(),
        4 => {
            validate_plugin_grants(&manifest.plugin_grants).is_ok()
                && plugin_dependencies(&manifest.plugin_grants)
                    == manifest.dependencies.plugins_not_bundled
        }
        _ => false,
    };
    if !tool_locks_valid
        || !grants_valid
        || manifest.project_schema_version != 3
        || manifest.project_path != "project.json"
        || manifest.self_contained_media
            != (manifest.mode == opencut_api_contracts::project_package::PackageMode::SelfContained)
        || manifest.fully_reproducible
            != (manifest.self_contained_media
                && manifest.dependencies.fonts_not_bundled.is_empty()
                && manifest.dependencies.plugins_not_bundled.is_empty()
                && manifest.dependencies.tools_not_locked.is_empty()
                && manifest.dependencies.unsupported.is_empty())
    {
        return Err(EditorError::new(
            "PACKAGE_INVALID",
            "Package schema or mode is unsupported",
        ));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_api_contracts::{
        project_package::{PackageDependencies, PackageMode},
        project_package_v3::{PackageManifestV3, PackagePluginGrant, PackageToolLock},
    };

    fn manifest(schema_version: u32) -> PackageManifestV3 {
        let plugin_grants = if schema_version == 4 {
            vec![
                PackagePluginGrant::PlannerV1 {
                    id: "org.example.planner".into(),
                    version: "1.0.0".into(),
                    package_sha256: "c".repeat(64),
                    operations: ["org.example.planner.plan".to_string()].into(),
                    enabled: true,
                },
                PackagePluginGrant::PresetV2 {
                    id: "org.example.preset".into(),
                    version: "2.0.0".into(),
                    package_sha256: "d".repeat(64),
                    preset_names: ["captions".to_string()].into(),
                    enabled: true,
                },
            ]
        } else {
            Vec::new()
        };
        let tool_locks = if matches!(schema_version, 3 | 4) {
            ["ffmpeg", "ffprobe"]
                .into_iter()
                .map(|name| PackageToolLock {
                    name: name.into(),
                    version: format!("{name} version fixture-1"),
                    binary_sha256: "a".repeat(64),
                })
                .collect()
        } else {
            Vec::new()
        };
        PackageManifestV3 {
            schema_version,
            project_schema_version: 3,
            mode: PackageMode::SelfContained,
            source_project_id: "source".into(),
            source_revision: "rev_1".into(),
            project_path: "project.json".into(),
            project_sha256: "b".repeat(64),
            assets: Vec::new(),
            dependencies: PackageDependencies {
                fonts_not_bundled: Vec::new(),
                plugins_not_bundled: plugin_dependencies(&plugin_grants),
                tools_not_locked: if schema_version == 2 {
                    vec!["ffmpeg".into(), "ffprobe".into()]
                } else {
                    Vec::new()
                },
                unsupported: Vec::new(),
            },
            tool_locks,
            plugin_grants,
            self_contained_media: true,
            fully_reproducible: schema_version == 3,
        }
    }

    #[test]
    fn accepts_legacy_unlocked_tool_locks_and_schema4_plugin_snapshots() {
        assert!(validate_manifest(&manifest(2)).is_ok());
        assert!(validate_manifest(&manifest(3)).is_ok());
        assert!(validate_manifest(&manifest(4)).is_ok());
    }

    #[test]
    fn restore_plan_is_typed_target_bound_and_idempotent() {
        let package = manifest(4);
        let plan =
            plugin_grant_restore_plan(&package.plugin_grants, "restored-project", &"e".repeat(64))
                .unwrap();
        assert_eq!(plan.len(), 2);
        let v1 = serde_json::to_value(&plan[0]).unwrap();
        assert_eq!(v1["tool"], "opencut_project_plugin_grant");
        assert_eq!(v1["input"]["target"]["project_id"], "restored-project");
        assert_eq!(v1["input"]["expected_revision"], "0");
        assert_eq!(v1["input"]["enabled"], true);
        assert_eq!(
            v1["input"]["idempotency_key"],
            format!("package-restore-{}-0", "e".repeat(64))
        );
        let v2 = serde_json::to_value(&plan[1]).unwrap();
        assert_eq!(v2["tool"], "opencut_plugin_v2_grant");
        assert_eq!(v2["input"]["preset_names"], serde_json::json!(["captions"]));
        assert_eq!(
            serde_json::to_value(plan).unwrap(),
            serde_json::to_value(
                plugin_grant_restore_plan(
                    &package.plugin_grants,
                    "restored-project",
                    &"e".repeat(64)
                )
                .unwrap()
            )
            .unwrap()
        );
    }

    #[test]
    fn rejects_incomplete_or_misreported_tool_locks() {
        let mut incomplete = manifest(3);
        incomplete.tool_locks.pop();
        assert_eq!(
            validate_manifest(&incomplete).unwrap_err().code,
            "PACKAGE_INVALID"
        );

        let mut unlocked = manifest(3);
        unlocked.dependencies.tools_not_locked.push("ffmpeg".into());
        assert_eq!(
            validate_manifest(&unlocked).unwrap_err().code,
            "PACKAGE_INVALID"
        );

        let mut malformed_hash = manifest(3);
        malformed_hash.tool_locks[0].binary_sha256 = "not-a-sha".into();
        assert_eq!(
            validate_manifest(&malformed_hash).unwrap_err().code,
            "PACKAGE_INVALID"
        );

        let mut missing_plugin_dependency = manifest(4);
        missing_plugin_dependency
            .dependencies
            .plugins_not_bundled
            .clear();
        assert_eq!(
            validate_manifest(&missing_plugin_dependency)
                .unwrap_err()
                .code,
            "PACKAGE_INVALID"
        );

        let mut invalid_snapshot = manifest(4);
        if let PackagePluginGrant::PlannerV1 { operations, .. } =
            &mut invalid_snapshot.plugin_grants[0]
        {
            operations.clear();
        }
        assert_eq!(
            validate_manifest(&invalid_snapshot).unwrap_err().code,
            "PACKAGE_INVALID"
        );
    }
}
