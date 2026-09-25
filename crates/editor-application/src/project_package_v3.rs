//! Schema 3 package validation keeps every visual dependency attached to its base.
use crate::project_package::{self, PackageExportSource};
use opencut_api_contracts::{
    project_package::{PackageDependencies, PackageManifest, PreparedImport},
    project_package_v3::{PackageManifestV3, PreparedImportV3},
};
use opencut_contracts::{
    EditorError, ProjectSnapshot,
    project_v3::{ProjectDocumentV3, ProjectSnapshotV3},
};

#[derive(Clone, Debug)]
pub struct PackageExportSourceV3 {
    pub snapshot: ProjectSnapshotV3,
    pub plugin_grants: Vec<String>,
}

pub fn dependencies(source: &PackageExportSourceV3) -> PackageDependencies {
    project_package::dependencies(&PackageExportSource {
        snapshot: ProjectSnapshot {
            project: source.snapshot.project.base.clone(),
            revision: source.snapshot.revision.clone(),
        },
        plugin_grants: source.plugin_grants.clone(),
    })
}

pub fn validate_export_document(document: &ProjectDocumentV3) -> Result<(), EditorError> {
    opencut_editor_core::project_v3::validate_project_v3(document)
}

pub fn prepare_import(
    prepared: &PreparedImportV3,
    target_project_id: &str,
) -> Result<ProjectDocumentV3, EditorError> {
    let manifest = &prepared.manifest;
    if !matches!(manifest.schema_version, 2 | 3)
        || manifest.project_schema_version != 3
        || prepared.document.schema_version != 3
    {
        return Err(EditorError::new(
            "PACKAGE_INVALID",
            "Unsupported schema 3 package version",
        ));
    }
    validate_export_document(&prepared.document)?;
    let declared = dependencies(&PackageExportSourceV3 {
        snapshot: ProjectSnapshotV3 {
            project: prepared.document.clone(),
            revision: manifest.source_revision.clone(),
        },
        plugin_grants: manifest.dependencies.plugins_not_bundled.clone(),
    });
    if declared.fonts_not_bundled != manifest.dependencies.fonts_not_bundled
        || (manifest.schema_version == 2
            && declared.tools_not_locked != manifest.dependencies.tools_not_locked)
        || (manifest.schema_version == 3 && !manifest.dependencies.tools_not_locked.is_empty())
        || !manifest.dependencies.unsupported.is_empty()
    {
        return Err(EditorError::new(
            "PACKAGE_INVALID",
            "Package dependency report does not match project content",
        ));
    }
    let base_manifest = PackageManifest {
        schema_version: 1,
        mode: manifest.mode.clone(),
        source_project_id: manifest.source_project_id.clone(),
        source_revision: manifest.source_revision.clone(),
        project_path: manifest.project_path.clone(),
        project_sha256: manifest.project_sha256.clone(),
        assets: manifest.assets.clone(),
        dependencies: manifest.dependencies.clone(),
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
        3 => {
            tool_lock_names == expected_tool_names
                && manifest
                    .tool_locks
                    .iter()
                    .all(|lock| !lock.version.trim().is_empty() && is_sha256(&lock.binary_sha256))
                && manifest.dependencies.tools_not_locked.is_empty()
        }
        _ => false,
    };
    if !tool_locks_valid
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
        project_package_v3::{PackageManifestV3, PackageToolLock},
    };

    fn manifest(schema_version: u32) -> PackageManifestV3 {
        let tool_locks = if schema_version == 3 {
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
                plugins_not_bundled: Vec::new(),
                tools_not_locked: if schema_version == 2 {
                    vec!["ffmpeg".into(), "ffprobe".into()]
                } else {
                    Vec::new()
                },
                unsupported: Vec::new(),
            },
            tool_locks,
            self_contained_media: true,
            fully_reproducible: schema_version == 3,
        }
    }

    #[test]
    fn accepts_legacy_unlocked_and_current_exact_tool_lock_manifests() {
        assert!(validate_manifest(&manifest(2)).is_ok());
        assert!(validate_manifest(&manifest(3)).is_ok());
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
    }
}
