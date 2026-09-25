//! Owner-local schema 3 project packages and schema 4 grant snapshots, built on the v1 media verifier.
use crate::project_package as common;
use opencut_api_contracts::{
    project_package::{PackageManifest, PackageMode},
    project_package_v3::{
        ExportPackageV3, ExportReceipt, ImportPackageV3, PackageManifestV3, PackageToolLock,
        PreparedImportV3,
    },
};
use opencut_asset_store::AssetStore;
use opencut_contracts::EditorError;
use opencut_editor_application::project_package_v3::{self, PackageExportSourceV3};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

fn common_manifest(manifest: &PackageManifestV3) -> PackageManifest {
    PackageManifest {
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
    }
}

pub fn export_package_v3(
    input: &ExportPackageV3,
    source: &PackageExportSourceV3,
    asset_store_root: &Path,
) -> Result<ExportReceipt, EditorError> {
    if source.snapshot.project.base.id != input.project_id
        || source.snapshot.revision != input.revision
    {
        return Err(common::invalid(
            "Export source is not the requested fixed revision",
        ));
    }
    let requested = Path::new(&input.destination);
    if !requested.is_absolute() || requested.exists() {
        return Err(common::invalid(
            "Package destination must be a new absolute directory",
        ));
    }
    let parent = fs::canonicalize(
        requested
            .parent()
            .ok_or_else(|| common::invalid("Invalid package destination"))?,
    )
    .map_err(common::io)?;
    let destination = parent.join(
        requested
            .file_name()
            .ok_or_else(|| common::invalid("Invalid package destination name"))?,
    );
    if destination.exists() {
        return Err(common::invalid("Package destination already exists"));
    }
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| common::invalid("System clock is invalid"))?
        .as_nanos();
    let staging = parent.join(format!(
        ".opencut-package-v3-{}-{suffix}",
        std::process::id()
    ));
    fs::create_dir(&staging).map_err(common::io)?;
    let result = (|| {
        let store = AssetStore::open_managed(asset_store_root)?;
        let mut document = source.snapshot.project.clone();
        project_package_v3::validate_export_document(&document)?;
        let inventory = common::export_media(&mut document.base, &input.mode, &staging, &store)?;
        project_package_v3::validate_export_document(&document)?;
        let document_bytes = serde_json::to_vec(&document)
            .map_err(|_| common::invalid("Could not serialize schema 3 project document"))?;
        if document_bytes.len() as u64 > common::MAX_DOCUMENT_BYTES {
            return Err(common::invalid(
                "Project document exceeds package size limit",
            ));
        }
        common::write_file(&staging.join("project.json"), &document_bytes)?;
        let mut dependencies = project_package_v3::dependencies(source);
        let tool_locks = opencut_media::FfmpegBackend::discover()
            .and_then(|backend| backend.tool_fingerprints())
            .map_err(|error| {
                EditorError::new(
                    "PACKAGE_TOOL_UNAVAILABLE",
                    format!("Could not fingerprint package render tools: {error}"),
                )
            })?
            .into_iter()
            .map(|tool| PackageToolLock {
                name: tool.name,
                version: tool.version,
                binary_sha256: tool.binary_sha256,
            })
            .collect::<Vec<_>>();
        dependencies.tools_not_locked.clear();
        let manifest = PackageManifestV3 {
            schema_version: 4,
            project_schema_version: 3,
            mode: input.mode.clone(),
            source_project_id: input.project_id.clone(),
            source_revision: input.revision.clone(),
            project_path: "project.json".into(),
            project_sha256: common::digest(&document_bytes),
            assets: inventory,
            dependencies: dependencies.clone(),
            tool_locks: tool_locks.clone(),
            plugin_grants: source.plugin_grants.clone(),
            self_contained_media: input.mode == PackageMode::SelfContained,
            fully_reproducible: input.mode == PackageMode::SelfContained
                && dependencies.fonts_not_bundled.is_empty()
                && dependencies.plugins_not_bundled.is_empty()
                && dependencies.tools_not_locked.is_empty()
                && dependencies.unsupported.is_empty(),
        };
        project_package_v3::validate_manifest(&manifest)?;
        let manifest_bytes = serde_json::to_vec(&manifest)
            .map_err(|_| common::invalid("Could not serialize schema 3 manifest"))?;
        if manifest_bytes.len() as u64 > common::MAX_MANIFEST_BYTES {
            return Err(common::invalid("Package manifest exceeds size limit"));
        }
        common::write_file(&staging.join("manifest.json"), &manifest_bytes)?;
        if input.mode == PackageMode::SelfContained {
            common::sync_dir(&staging.join("media"))?;
        }
        common::sync_dir(&staging)?;
        if destination.exists() {
            return Err(common::invalid(
                "Package destination appeared during export",
            ));
        }
        fs::rename(&staging, &destination).map_err(common::io)?;
        common::sync_dir(&parent)?;
        Ok(ExportReceipt {
            package_path: destination.to_string_lossy().into_owned(),
            manifest_sha256: common::digest(&manifest_bytes),
            source_revision: input.revision.clone(),
            asset_count: document.base.assets.len(),
            dependencies,
            tool_locks,
            plugin_grants: source.plugin_grants.clone(),
            self_contained_media: manifest.self_contained_media,
            fully_reproducible: manifest.fully_reproducible,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

pub fn read_package_v3(
    input: &ImportPackageV3,
    asset_store_root: &Path,
) -> Result<PreparedImportV3, EditorError> {
    let root = Path::new(&input.package_path);
    if !root.is_absolute()
        || fs::canonicalize(root).map_err(common::io)? != root
        || !fs::symlink_metadata(root)
            .map_err(common::io)?
            .file_type()
            .is_dir()
    {
        return Err(common::invalid(
            "Package root must be a canonical directory without symlinks",
        ));
    }
    let manifest_bytes = common::bounded_bytes(
        &common::relative(root, "manifest.json")?,
        common::MAX_MANIFEST_BYTES,
    )?;
    let manifest_sha256 = common::digest(&manifest_bytes);
    if !common::valid_hash(&input.expected_manifest_sha256)
        || manifest_sha256 != input.expected_manifest_sha256
    {
        return Err(common::invalid("Package manifest SHA-256 mismatch"));
    }
    let manifest: PackageManifestV3 = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| common::invalid("Schema 3 package manifest is invalid"))?;
    project_package_v3::validate_manifest(&manifest)?;
    let document_bytes = common::bounded_bytes(
        &common::relative(root, &manifest.project_path)?,
        common::MAX_DOCUMENT_BYTES,
    )?;
    if common::digest(&document_bytes) != manifest.project_sha256 {
        return Err(common::invalid("Project document SHA-256 mismatch"));
    }
    let document = serde_json::from_slice(&document_bytes)
        .map_err(|_| common::invalid("Schema 3 project document is invalid"))?;
    project_package_v3::validate_export_document(&document)?;
    if document.base.id != manifest.source_project_id {
        return Err(common::invalid("Project identity differs from manifest"));
    }
    let fonts_missing =
        opencut_media::missing_system_font_families(&manifest.dependencies.fonts_not_bundled);
    let font_environment_checked = true;
    let tool_lock_mismatches = match opencut_media::FfmpegBackend::discover()
        .and_then(|backend| backend.tool_fingerprints())
    {
        Ok(_) if manifest.schema_version == 2 => manifest.dependencies.tools_not_locked.clone(),
        Ok(actual) => manifest
            .tool_locks
            .iter()
            .filter(|expected| {
                !actual.iter().any(|found| {
                    found.name == expected.name
                        && found.version == expected.version
                        && found.binary_sha256 == expected.binary_sha256
                })
            })
            .map(|expected| expected.name.clone())
            .collect::<Vec<_>>(),
        Err(_) => manifest
            .tool_locks
            .iter()
            .map(|expected| expected.name.clone())
            .chain(manifest.dependencies.tools_not_locked.iter().cloned())
            .collect(),
    };
    let tool_environment_matches =
        matches!(manifest.schema_version, 3 | 4) && tool_lock_mismatches.is_empty();
    common::capture_media(
        root,
        &common_manifest(&manifest),
        &document.base,
        asset_store_root,
    )?;
    Ok(PreparedImportV3 {
        manifest_sha256,
        manifest,
        document,
        font_environment_checked,
        fonts_missing,
        tool_environment_matches,
        tool_lock_mismatches,
    })
}
