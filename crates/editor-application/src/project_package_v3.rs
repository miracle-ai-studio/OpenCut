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
    if manifest.schema_version != 2
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
        || declared.tools_not_locked != manifest.dependencies.tools_not_locked
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
    if manifest.schema_version != 2
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
