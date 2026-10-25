//! Schema 7 directory adapter. All records/bytes are verified before CAS capture;
//! publication never overwrites another package and never restores authority.
use crate::project_package as common;
use opencut_api_contracts::project_archive::*;
use opencut_asset_store::AssetStore;
use opencut_contracts::{Asset, AssetKind, AssetVersion, EditorError};
use opencut_editor_application::project_archive::{
    self as app, ArchiveSource, PreparedHistoricalArchive, ProjectArchive,
};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};

#[cfg(test)]
mod tests;

use crate::package_io::{PackageDirectory, bounded, publish, root, verify_blob};
fn version(blob: &ArchiveBlob) -> AssetVersion {
    AssetVersion {
        id: format!("av_{}", blob.content_hash),
        content_hash: blob.content_hash.clone(),
        size: blob.size_bytes,
    }
}
fn total(manifest: &HistoricalArchiveManifest, json: usize) -> Result<u64, EditorError> {
    let bytes = manifest
        .blobs
        .iter()
        .try_fold(json as u64, |sum, b| sum.checked_add(b.size_bytes))
        .ok_or_else(|| app::invalid("Package size overflow"))?;
    if bytes > app::MAX_RESOURCE_BYTES {
        return Err(app::invalid("Package exceeds total byte budget"));
    }
    Ok(bytes)
}
fn tree(root: &PackageDirectory, m: &HistoricalArchiveManifest) -> Result<(), EditorError> {
    let mut expected = BTreeSet::from(["manifest.json".to_owned(), "history.json".to_owned()]);
    if !m.blobs.is_empty() {
        expected.insert("blobs".into());
    }
    if root.names()? != expected {
        return Err(app::invalid("Unexpected or missing history package entry"));
    }
    root.child("manifest.json", false)?;
    root.child("history.json", false)?;
    if !m.blobs.is_empty() {
        let blobs = PackageDirectory {
            file: root.child("blobs", true)?,
        };
        let expected = m.blobs.iter().map(|b| b.content_hash.clone()).collect();
        if blobs.names()? != expected {
            return Err(app::invalid("Unexpected or missing historical blob"));
        }
        for blob in &m.blobs {
            blobs.child(&blob.content_hash, false)?;
        }
    }
    Ok(())
}
#[cfg(test)]
pub fn export_historical_archive(
    input: &HistoricalArchiveExport,
    source: &ArchiveSource,
    asset_store_root: &Path,
) -> Result<HistoricalArchiveExportReceipt, EditorError> {
    if source.archive.source_project_id != input.project_id
        || source.archive.main_head_revision != input.expected_revision
    {
        return Err(app::invalid(
            "History export source differs from requested main head",
        ));
    }
    let manifest = app::make_manifest(source)?;
    let history = app::canonical_bytes(&source.archive, app::MAX_HISTORY_BYTES)?;
    let manifest_bytes = app::canonical_bytes(&manifest, app::MAX_MANIFEST_BYTES)?;
    total(&manifest, history.len() + manifest_bytes.len())?;
    let requested = Path::new(&input.destination);
    if !requested.is_absolute()
        || requested
            .components()
            .any(|p| matches!(p, Component::ParentDir))
        || fs::symlink_metadata(requested).is_ok()
    {
        return Err(app::invalid(
            "History package destination must be a new absolute directory",
        ));
    }
    let parent = fs::canonicalize(
        requested
            .parent()
            .ok_or_else(|| app::invalid("Package destination has no parent"))?,
    )
    .map_err(common::io)?;
    let destination = parent.join(
        requested
            .file_name()
            .ok_or_else(|| app::invalid("Invalid package destination"))?,
    );
    let store = AssetStore::read_only(asset_store_root)?;
    let staging = tempfile::Builder::new()
        .prefix(".opencut-history-package-")
        .tempdir_in(&parent)
        .map_err(common::io)?;
    if !manifest.blobs.is_empty() {
        fs::create_dir(staging.path().join("blobs")).map_err(common::io)?;
    }
    for b in &manifest.blobs {
        let (mut file, size) = store.open_version_verified(&version(b))?;
        if size != b.size_bytes {
            return Err(app::invalid("CAS size differs from historical inventory"));
        }
        common::copy_verified(
            &mut file,
            &staging.path().join(&b.path),
            &b.content_hash,
            size,
        )?;
    }
    common::write_file(&staging.path().join("history.json"), &history)?;
    common::write_file(&staging.path().join("manifest.json"), &manifest_bytes)?;
    if !manifest.blobs.is_empty() {
        common::sync_dir(&staging.path().join("blobs"))?;
    }
    common::sync_dir(staging.path())?;
    publish(staging.path(), &destination)?;
    common::sync_dir(&parent)?;
    let prepared = PreparedHistoricalArchive {
        manifest_sha256: app::digest(&manifest_bytes),
        manifest,
        archive: source.archive.clone(),
    };
    Ok(HistoricalArchiveExportReceipt {
        package_path: destination.to_string_lossy().into_owned(),
        package: app::inspection(&prepared),
    })
}
pub(crate) fn verified(
    path: &str,
    expected: Option<&str>,
) -> Result<(PackageDirectory, PreparedHistoricalArchive, u64), EditorError> {
    let root = PackageDirectory::open(&root(path)?)?;
    let bytes = bounded(root.child("manifest.json", false)?, app::MAX_MANIFEST_BYTES)?;
    let hash = app::digest(&bytes);
    if expected.is_some_and(|e| !app::valid_hash(e) || e != hash) {
        return Err(app::invalid("History package manifest SHA-256 mismatch"));
    }
    let manifest: HistoricalArchiveManifest = serde_json::from_slice(&bytes)
        .map_err(|_| app::invalid("Invalid history package manifest"))?;
    app::validate_manifest(&manifest)?;
    if bytes != app::canonical_bytes(&manifest, app::MAX_MANIFEST_BYTES)? {
        return Err(app::invalid("Manifest is not canonical JSON"));
    }
    tree(&root, &manifest)?;
    let history = bounded(
        root.child(&manifest.history_path, false)?,
        app::MAX_HISTORY_BYTES,
    )?;
    if app::digest(&history) != manifest.history_sha256 {
        return Err(app::invalid("Historical archive SHA-256 mismatch"));
    }
    let archive: ProjectArchive =
        serde_json::from_slice(&history).map_err(|_| app::invalid("Invalid V4 history archive"))?;
    if history != app::canonical_bytes(&archive, app::MAX_HISTORY_BYTES)? {
        return Err(app::invalid("History is not canonical JSON"));
    }
    let size = total(&manifest, history.len() + bytes.len())?;
    let prepared = PreparedHistoricalArchive {
        manifest_sha256: hash,
        manifest,
        archive,
    };
    app::validate_prepared(&prepared)?;
    for blob in &prepared.manifest.blobs {
        verify_blob(root.resource(&blob.path)?, blob)?;
    }
    Ok((root, prepared, size))
}
#[cfg(test)]
pub fn inspect_historical_archive(
    input: &HistoricalArchiveInspect,
) -> Result<HistoricalArchiveInspection, EditorError> {
    let (_, prepared, _) = verified(&input.package_path, None)?;
    Ok(app::inspection(&prepared))
}
pub fn read_historical_archive(
    input: &HistoricalArchiveImport,
    asset_store_root: &Path,
) -> Result<PreparedHistoricalArchive, EditorError> {
    let (root, prepared, size) =
        verified(&input.package_path, Some(&input.expected_manifest_sha256))?;
    app::prepare_import(&prepared, &input.target_project_id)?;
    let workspace = asset_store_root
        .parent()
        .ok_or_else(|| app::invalid("Asset store has no workspace"))?;
    let _budget =
        opencut_storage_budget::Budget::new(workspace).reserve("private_asset_copy", size)?;
    let staging = tempfile::Builder::new()
        .prefix(".opencut-history-import-")
        .tempdir_in(workspace)
        .map_err(common::io)?;
    // Every potentially mutable package input is rehashed into owned staging
    // before the first CAS write. CAS then consumes only these owned files.
    for blob in &prepared.manifest.blobs {
        let mut file = root.resource(&blob.path)?;
        common::copy_verified(
            &mut file,
            &staging.path().join(&blob.content_hash),
            &blob.content_hash,
            blob.size_bytes,
        )?;
    }
    let store = AssetStore::open_managed(asset_store_root)?;
    for blob in &prepared.manifest.blobs {
        let pinned = store.pin_asset(&Asset {
            id: blob.content_hash.clone(),
            name: blob.content_hash.clone(),
            kind: AssetKind::Image,
            uri: staging
                .path()
                .join(&blob.content_hash)
                .to_string_lossy()
                .into_owned(),
            duration: None,
            version: None,
            metadata: Default::default(),
        })?;
        if pinned.version.as_ref() != Some(&version(blob)) {
            return Err(app::invalid(
                "Captured CAS version differs from history manifest",
            ));
        }
    }
    Ok(prepared)
}
