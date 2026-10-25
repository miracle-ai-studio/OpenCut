//! Owner-local schema 6 packages preserve the V4 aggregate and original CAS bytes.
//! No renderer, network provider, font parser or derivation is invoked here.
use crate::project_package as common;
use opencut_api_contracts::historical_snapshot_package::{
    HistoricalSnapshotExport, HistoricalSnapshotExportReceipt, HistoricalSnapshotImport,
    HistoricalSnapshotInspect, HistoricalSnapshotInspection, HistoricalSnapshotManifest,
    HistoricalSnapshotResource,
};
use opencut_asset_store::AssetStore;
use opencut_contracts::{Asset, AssetKind, AssetVersion, EditorError, scene::EditorDocument};
use opencut_editor_application::historical_snapshot_package::{
    self as application, HistoricalSnapshotSource, PreparedHistoricalSnapshot,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

fn canonical_bytes<T: serde::Serialize>(value: &T, max: u64) -> Result<Vec<u8>, EditorError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|_| common::invalid("Cannot serialize V4 package content"))?;
    if bytes.len() as u64 > max {
        return Err(common::invalid("V4 package content exceeds size limit"));
    }
    Ok(bytes)
}

fn inventory<'a>(
    items: impl Iterator<Item = (&'a str, &'a AssetVersion)>,
    prefix: &str,
) -> Vec<HistoricalSnapshotResource> {
    let mut values: Vec<_> = items
        .map(|(id, version)| HistoricalSnapshotResource {
            resource_id: id.into(),
            content_hash: version.content_hash.clone(),
            size_bytes: version.size,
            path: format!("{prefix}/{}", version.content_hash),
        })
        .collect();
    values.sort_by(|a, b| a.resource_id.cmp(&b.resource_id));
    values
}

// Logical aliases may share bytes, including across media/fonts. Size claims for
// one hash must agree. Count physical content once, plus the two actual JSON files.
fn package_bytes(
    manifest: &HistoricalSnapshotManifest,
    json_bytes: u64,
) -> Result<u64, EditorError> {
    let mut sizes = BTreeMap::new();
    let mut total = json_bytes;
    for item in manifest.assets.iter().chain(&manifest.fonts) {
        if item.size_bytes > common::MAX_ASSET_BYTES {
            return Err(common::invalid("Package resource exceeds size limit"));
        }
        if let Some(previous) = sizes.insert(&item.content_hash, item.size_bytes) {
            if previous != item.size_bytes {
                return Err(common::invalid("One content hash has conflicting sizes"));
            }
        } else {
            total = total
                .checked_add(item.size_bytes)
                .ok_or_else(|| common::invalid("Package size overflow"))?;
        }
        if total > common::MAX_TOTAL_BYTES {
            return Err(common::invalid("Package exceeds total byte limit"));
        }
    }
    if total > common::MAX_TOTAL_BYTES {
        return Err(common::invalid("Package exceeds total byte limit"));
    }
    Ok(total)
}

fn resource_version(item: &HistoricalSnapshotResource) -> AssetVersion {
    AssetVersion {
        id: format!("av_{}", item.content_hash),
        content_hash: item.content_hash.clone(),
        size: item.size_bytes,
    }
}

fn open_regular(path: &Path) -> Result<File, EditorError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(common::io)?;
    if !file.metadata().map_err(common::io)?.is_file() {
        return Err(common::invalid("Package entry is not a regular file"));
    }
    Ok(file)
}

#[cfg(test)]
fn publish_new_directory(staging: &Path, destination: &Path) -> Result<(), EditorError> {
    #[cfg(any(target_vendor = "apple", target_os = "linux", target_os = "android"))]
    {
        // An existence check followed by std::fs::rename can replace a directory
        // created by another writer. The kernel must enforce no replacement.
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            staging,
            rustix::fs::CWD,
            destination,
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .map_err(|error| common::io(error.into()))
    }
    #[cfg(not(any(target_vendor = "apple", target_os = "linux", target_os = "android")))]
    {
        let _ = (staging, destination);
        Err(common::invalid(
            "Atomic non-replacing package publication is unavailable on this host",
        ))
    }
}

#[cfg(test)]
pub fn export_historical_snapshot(
    input: &HistoricalSnapshotExport,
    source: &HistoricalSnapshotSource,
    asset_store_root: &Path,
) -> Result<HistoricalSnapshotExportReceipt, EditorError> {
    application::validate_export_source(source)?;
    if source.snapshot.project.id != input.project_id || source.snapshot.revision != input.revision
    {
        return Err(common::invalid(
            "Export source is not the requested fixed V4 revision",
        ));
    }
    let requested = Path::new(&input.destination);
    if !requested.is_absolute() || fs::symlink_metadata(requested).is_ok() {
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
    let document = &source.snapshot.project;
    let document_bytes = canonical_bytes(document, common::MAX_DOCUMENT_BYTES)?;
    let manifest = HistoricalSnapshotManifest {
        schema_version: 6,
        project_schema_version: 4,
        source_project_id: input.project_id.clone(),
        source_revision: input.revision.clone(),
        project_path: "project.json".into(),
        project_sha256: common::digest(&document_bytes),
        assets: inventory(
            document
                .assets
                .iter()
                .map(|asset| (asset.id.as_str(), &asset.version)),
            "media",
        ),
        fonts: inventory(
            document
                .fonts
                .iter()
                .map(|font| (font.id.as_str(), &font.content)),
            "fonts",
        ),
        derived_fonts: source.derived_fonts.clone(),
        self_contained_resources: true,
        history_included: false,
        fully_reproducible: false,
    };
    application::validate_contents(&manifest, document)?;
    let manifest_bytes = canonical_bytes(&manifest, common::MAX_MANIFEST_BYTES)?;
    package_bytes(
        &manifest,
        (document_bytes.len() + manifest_bytes.len()) as u64,
    )?;
    let store = AssetStore::read_only(asset_store_root)?;
    // TempDir owns only this fresh, private staging directory and cleans failures.
    let staging = tempfile::Builder::new()
        .prefix(".opencut-package-v4-")
        .tempdir_in(&parent)
        .map_err(common::io)?;
    for (name, resources) in [("media", &manifest.assets), ("fonts", &manifest.fonts)] {
        if !resources.is_empty() {
            fs::create_dir(staging.path().join(name)).map_err(common::io)?;
        }
    }
    let mut physical = BTreeMap::<String, PathBuf>::new();
    let mut paths = BTreeSet::new();
    for item in manifest.assets.iter().chain(&manifest.fonts) {
        if !paths.insert(&item.path) {
            continue;
        }
        let target = staging.path().join(&item.path);
        if let Some(previous) = physical.get(&item.content_hash) {
            fs::hard_link(previous, &target).map_err(common::io)?;
        } else {
            let (mut file, size) = store.open_version_verified(&resource_version(item))?;
            if size != item.size_bytes {
                return Err(common::invalid("CAS size differs from package inventory"));
            }
            common::copy_verified(&mut file, &target, &item.content_hash, size)?;
            physical.insert(item.content_hash.clone(), target);
        }
    }
    common::write_file(&staging.path().join("project.json"), &document_bytes)?;
    common::write_file(&staging.path().join("manifest.json"), &manifest_bytes)?;
    for (name, resources) in [("media", &manifest.assets), ("fonts", &manifest.fonts)] {
        if !resources.is_empty() {
            common::sync_dir(&staging.path().join(name))?;
        }
    }
    common::sync_dir(staging.path())?;
    publish_new_directory(staging.path(), &destination)?;
    common::sync_dir(&parent)?;
    Ok(HistoricalSnapshotExportReceipt {
        package_path: destination.to_string_lossy().into_owned(),
        manifest_sha256: common::digest(&manifest_bytes),
        source_revision: input.revision.clone(),
        asset_count: manifest.assets.len(),
        font_count: manifest.fonts.len(),
        derived_font_count: manifest.derived_fonts.len(),
        self_contained_resources: true,
        history_included: false,
        fully_reproducible: false,
    })
}

fn validate_tree(root: &Path, manifest: &HistoricalSnapshotManifest) -> Result<(), EditorError> {
    let mut expected = BTreeSet::from(["manifest.json".to_string(), "project.json".to_string()]);
    for item in manifest.assets.iter().chain(&manifest.fonts) {
        expected.insert(item.path.clone());
    }
    let mut found = BTreeSet::new();
    for entry in fs::read_dir(root).map_err(common::io)? {
        let entry = entry.map_err(common::io)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| common::invalid("Package names must be UTF-8"))?;
        let kind = entry.file_type().map_err(common::io)?;
        if name == "media" && !manifest.assets.is_empty()
            || name == "fonts" && !manifest.fonts.is_empty()
        {
            if !kind.is_dir() {
                return Err(common::invalid(
                    "Package resource directory is not a directory",
                ));
            }
            for child in fs::read_dir(entry.path()).map_err(common::io)? {
                let child = child.map_err(common::io)?;
                let leaf = child
                    .file_name()
                    .into_string()
                    .map_err(|_| common::invalid("Package names must be UTF-8"))?;
                let member = format!("{name}/{leaf}");
                if !child.file_type().map_err(common::io)?.is_file()
                    || !expected.contains(&member)
                    || !found.insert(member)
                {
                    return Err(common::invalid(
                        "Unexpected, duplicate or non-regular package resource",
                    ));
                }
            }
        } else if !kind.is_file() || !expected.contains(&name) || !found.insert(name) {
            return Err(common::invalid(
                "Unexpected, duplicate or non-regular package entry",
            ));
        }
    }
    if found != expected {
        return Err(common::invalid("Package resource file is missing"));
    }
    Ok(())
}

pub(crate) fn verified_package(
    package_path: &str,
    expected_hash: Option<&str>,
) -> Result<(PathBuf, PreparedHistoricalSnapshot, u64), EditorError> {
    let requested = Path::new(package_path);
    if !requested.is_absolute()
        || requested
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(common::invalid(
            "Package root must be an absolute directory without parent traversal",
        ));
    }
    // Export canonicalizes the parent and returns a canonical receipt, while a
    // UI may retain its original /var or /tmp parent alias. Accept that alias,
    // but inspect the final component before canonicalization so a package-root
    // symlink cannot silently become an accepted directory. Normalize `/.` too.
    let requested: PathBuf = requested.components().collect();
    if !fs::symlink_metadata(&requested)
        .map_err(common::io)?
        .file_type()
        .is_dir()
    {
        return Err(common::invalid(
            "Package root must be a directory, not a symbolic link",
        ));
    }
    let canonical_root = fs::canonicalize(&requested).map_err(common::io)?;
    let root = canonical_root.as_path();
    let manifest_bytes = common::bounded_bytes(
        &common::relative(root, "manifest.json")?,
        common::MAX_MANIFEST_BYTES,
    )?;
    let manifest_sha256 = common::digest(&manifest_bytes);
    if expected_hash
        .is_some_and(|expected| !common::valid_hash(expected) || expected != manifest_sha256)
    {
        return Err(common::invalid("Package manifest SHA-256 mismatch"));
    }
    let manifest: HistoricalSnapshotManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| common::invalid("Invalid V4 package manifest"))?;
    application::validate_manifest(&manifest)?;
    if canonical_bytes(&manifest, common::MAX_MANIFEST_BYTES)? != manifest_bytes {
        return Err(common::invalid(
            "Package manifest must use canonical JSON bytes",
        ));
    }
    let document_bytes = common::bounded_bytes(
        &common::relative(root, &manifest.project_path)?,
        common::MAX_DOCUMENT_BYTES,
    )?;
    if common::digest(&document_bytes) != manifest.project_sha256 {
        return Err(common::invalid("Project document SHA-256 mismatch"));
    }
    let document: EditorDocument = serde_json::from_slice(&document_bytes)
        .map_err(|_| common::invalid("Invalid V4 project document"))?;
    if canonical_bytes(&document, common::MAX_DOCUMENT_BYTES)? != document_bytes {
        return Err(common::invalid(
            "Project document must use canonical JSON bytes",
        ));
    }
    application::validate_contents(&manifest, &document)?;
    let total = package_bytes(
        &manifest,
        (document_bytes.len() + manifest_bytes.len()) as u64,
    )?;
    validate_tree(root, &manifest)?;
    let mut verified = BTreeSet::new();
    for item in manifest.assets.iter().chain(&manifest.fonts) {
        if verified.insert(&item.path) {
            common::verify_file(
                &common::relative(root, &item.path)?,
                &item.content_hash,
                item.size_bytes,
            )?;
        }
    }
    Ok((
        root.into(),
        PreparedHistoricalSnapshot {
            manifest_sha256,
            manifest,
            document,
        },
        total,
    ))
}

#[cfg(test)]
pub fn inspect_historical_snapshot(
    input: &HistoricalSnapshotInspect,
) -> Result<HistoricalSnapshotInspection, EditorError> {
    let (_, prepared, _) = verified_package(&input.package_path, None)?;
    let manifest = &prepared.manifest;
    Ok(HistoricalSnapshotInspection {
        manifest_sha256: prepared.manifest_sha256,
        source_project_id: manifest.source_project_id.clone(),
        source_revision: manifest.source_revision.clone(),
        project_name: prepared.document.name,
        asset_count: manifest.assets.len(),
        font_count: manifest.fonts.len(),
        derived_font_count: manifest.derived_fonts.len(),
        self_contained_resources: true,
        history_included: false,
        fully_reproducible: false,
    })
}

pub fn read_historical_snapshot(
    input: &HistoricalSnapshotImport,
    asset_store_root: &Path,
) -> Result<PreparedHistoricalSnapshot, EditorError> {
    let (root, prepared, bytes) =
        verified_package(&input.package_path, Some(&input.expected_manifest_sha256))?;
    // Validate the target too before performing any writable native operation.
    application::prepare_import(&prepared, &input.target_project_id)?;
    let workspace = asset_store_root
        .parent()
        .ok_or_else(|| common::invalid("Invalid asset-store root"))?;
    let _reservation =
        opencut_storage_budget::Budget::new(workspace).reserve("private_asset_copy", bytes)?;
    let staging = tempfile::Builder::new()
        .prefix(".opencut-package-import-v4-")
        .tempdir_in(workspace)
        .map_err(common::io)?;
    let mut unique = BTreeMap::new();
    for item in prepared
        .manifest
        .assets
        .iter()
        .chain(&prepared.manifest.fonts)
    {
        unique.entry(&item.content_hash).or_insert(item);
    }
    // Copy and rehash *all* inputs into owned files before the first CAS write.
    // A later package mutation cannot retarget CAS capture to unchecked bytes.
    for (hash, item) in &unique {
        let mut source = open_regular(&common::relative(&root, &item.path)?)?;
        common::copy_verified(
            &mut source,
            &staging.path().join(hash),
            hash,
            item.size_bytes,
        )?;
    }
    let store = AssetStore::open_managed(asset_store_root)?;
    for (hash, item) in unique {
        // AssetStore's byte capture adapter uses this legacy locator DTO only;
        // the V4 project is never projected, rewritten or reconstructed from it.
        let capture = Asset {
            id: hash.clone(),
            name: hash.clone(),
            kind: AssetKind::Image,
            uri: staging.path().join(hash).to_string_lossy().into_owned(),
            duration: None,
            version: None,
            metadata: BTreeMap::new(),
        };
        let captured = store.pin_asset(&capture)?;
        if captured.version.as_ref() != Some(&resource_version(item)) {
            return Err(common::invalid(
                "CAS capture differs from V4 package inventory",
            ));
        }
    }
    Ok(prepared)
}

#[cfg(test)]
mod tests;
