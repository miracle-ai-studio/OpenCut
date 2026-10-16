//! Schema 7 directory adapter. All records/bytes are verified before CAS capture;
//! publication never overwrites another package and never restores authority.
use crate::project_package as common;
use opencut_api_contracts::project_history_package_v1::*;
use opencut_asset_store::AssetStore;
use opencut_contracts::{Asset, AssetKind, AssetVersion, EditorError};
use opencut_editor_application::project_history_package_v1::{
    self as app, HistoryArchiveV1, HistoryPackageSourceV1, PreparedHistoryImportV1,
};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};

#[cfg(test)]
mod tests;

// Anchor all package reads to an opened directory. A concurrent rename or
// symlink replacement cannot redirect later verification/capture outside it.
struct PackageDirectory {
    file: File,
}
impl PackageDirectory {
    #[cfg(unix)]
    fn open(path: &Path) -> Result<Self, EditorError> {
        use rustix::fs::{Mode, OFlags, open};
        let fd = open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| common::io(e.into()))?;
        Ok(Self {
            file: File::from(fd),
        })
    }
    #[cfg(not(unix))]
    fn open(_path: &Path) -> Result<Self, EditorError> {
        Err(EditorError::new(
            "CAPABILITY_UNAVAILABLE",
            "Anchored history package I/O requires Unix directory descriptors",
        ))
    }
    #[cfg(unix)]
    fn child(&self, name: &str, directory: bool) -> Result<File, EditorError> {
        use rustix::fs::{Mode, OFlags, openat};
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.contains('/')
            || name.contains('\\')
        {
            return Err(app::invalid("Invalid package entry name"));
        }
        let mut flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
        if directory {
            flags |= OFlags::DIRECTORY;
        }
        let file = File::from(
            openat(&self.file, name, flags, Mode::empty()).map_err(|e| common::io(e.into()))?,
        );
        let metadata = file.metadata().map_err(common::io)?;
        if if directory {
            !metadata.is_dir()
        } else {
            !metadata.is_file()
        } {
            return Err(app::invalid("Package entry has an invalid type"));
        }
        Ok(file)
    }
    #[cfg(not(unix))]
    fn child(&self, _name: &str, _directory: bool) -> Result<File, EditorError> {
        Err(EditorError::new(
            "CAPABILITY_UNAVAILABLE",
            "Anchored package I/O is unavailable",
        ))
    }
    fn resource(&self, path: &str) -> Result<File, EditorError> {
        let hash = path
            .strip_prefix("blobs/")
            .filter(|hash| app::valid_hash(hash))
            .ok_or_else(|| app::invalid("Invalid blob path"))?;
        Self {
            file: self.child("blobs", true)?,
        }
        .child(hash, false)
    }
    #[cfg(unix)]
    fn names(&self) -> Result<BTreeSet<String>, EditorError> {
        let mut entries =
            rustix::fs::Dir::read_from(&self.file).map_err(|e| common::io(e.into()))?;
        let mut names = BTreeSet::new();
        while let Some(entry) = entries.read() {
            let entry = entry.map_err(|e| common::io(e.into()))?;
            let name = entry
                .file_name()
                .to_str()
                .map_err(|_| app::invalid("Package names must be UTF-8"))?;
            if name == "." || name == ".." {
                continue;
            }
            if names.len() >= app::MAX_HISTORY_ROWS || !names.insert(name.to_owned()) {
                return Err(app::invalid(
                    "Duplicate or oversized package entry inventory",
                ));
            }
        }
        Ok(names)
    }
    #[cfg(not(unix))]
    fn names(&self) -> Result<BTreeSet<String>, EditorError> {
        Err(EditorError::new(
            "CAPABILITY_UNAVAILABLE",
            "Anchored package I/O is unavailable",
        ))
    }
}
fn bounded(file: File, max: usize) -> Result<Vec<u8>, EditorError> {
    if file.metadata().map_err(common::io)?.len() > max as u64 {
        return Err(app::invalid("History file exceeds its byte limit"));
    }
    let mut bytes = Vec::new();
    file.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(common::io)?;
    if bytes.len() > max {
        return Err(app::invalid("History file grew beyond its byte limit"));
    }
    Ok(bytes)
}
fn verify_blob(mut file: File, blob: &HistoryPackageBlobV1) -> Result<(), EditorError> {
    use sha2::{Digest, Sha256};
    if file.metadata().map_err(common::io)?.len() != blob.size_bytes {
        return Err(app::invalid("History blob size differs"));
    }
    let mut hash = Sha256::new();
    let mut seen = 0u64;
    let mut block = [0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut block).map_err(common::io)?;
        if n == 0 {
            break;
        }
        seen = seen
            .checked_add(n as u64)
            .ok_or_else(|| app::invalid("Blob size overflow"))?;
        if seen > blob.size_bytes {
            return Err(app::invalid("History blob grew during verification"));
        }
        hash.update(&block[..n]);
    }
    if seen != blob.size_bytes || format!("{:x}", hash.finalize()) != blob.content_hash {
        return Err(app::invalid("History blob SHA-256 mismatch"));
    }
    Ok(())
}
fn version(blob: &HistoryPackageBlobV1) -> AssetVersion {
    AssetVersion {
        id: format!("av_{}", blob.content_hash),
        content_hash: blob.content_hash.clone(),
        size: blob.size_bytes,
    }
}
fn total(manifest: &HistoryPackageManifestV1, json: usize) -> Result<u64, EditorError> {
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
fn root(path: &str) -> Result<PathBuf, EditorError> {
    let path = Path::new(path);
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(app::invalid(
            "Package root must be absolute without parent traversal",
        ));
    }
    let path: PathBuf = path.components().collect();
    if !fs::symlink_metadata(&path)
        .map_err(common::io)?
        .file_type()
        .is_dir()
    {
        return Err(app::invalid(
            "Package root must be a directory, not a symlink",
        ));
    }
    // A canonical parent alias (/var -> /private/var on macOS) is permitted.
    fs::canonicalize(path).map_err(common::io)
}
fn tree(root: &PackageDirectory, m: &HistoryPackageManifestV1) -> Result<(), EditorError> {
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
fn publish(staging: &Path, destination: &Path) -> Result<(), EditorError> {
    #[cfg(any(target_vendor = "apple", target_os = "linux", target_os = "android"))]
    {
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            staging,
            rustix::fs::CWD,
            destination,
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .map_err(|e| common::io(e.into()))
    }
    #[cfg(not(any(target_vendor = "apple", target_os = "linux", target_os = "android")))]
    {
        let _ = (staging, destination);
        Err(app::invalid(
            "Atomic non-replacing package publication is unavailable",
        ))
    }
}
pub fn export_history_package_v1(
    input: &ExportHistoryPackageV1,
    source: &HistoryPackageSourceV1,
    asset_store_root: &Path,
) -> Result<ExportHistoryPackageReceiptV1, EditorError> {
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
    let prepared = PreparedHistoryImportV1 {
        manifest_sha256: app::digest(&manifest_bytes),
        manifest,
        archive: source.archive.clone(),
    };
    Ok(ExportHistoryPackageReceiptV1 {
        package_path: destination.to_string_lossy().into_owned(),
        package: app::inspection(&prepared),
    })
}
fn verified(
    path: &str,
    expected: Option<&str>,
) -> Result<(PackageDirectory, PreparedHistoryImportV1, u64), EditorError> {
    let root = PackageDirectory::open(&root(path)?)?;
    let bytes = bounded(root.child("manifest.json", false)?, app::MAX_MANIFEST_BYTES)?;
    let hash = app::digest(&bytes);
    if expected.is_some_and(|e| !app::valid_hash(e) || e != hash) {
        return Err(app::invalid("History package manifest SHA-256 mismatch"));
    }
    let manifest: HistoryPackageManifestV1 = serde_json::from_slice(&bytes)
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
    let archive: HistoryArchiveV1 =
        serde_json::from_slice(&history).map_err(|_| app::invalid("Invalid V4 history archive"))?;
    if history != app::canonical_bytes(&archive, app::MAX_HISTORY_BYTES)? {
        return Err(app::invalid("History is not canonical JSON"));
    }
    let size = total(&manifest, history.len() + bytes.len())?;
    let prepared = PreparedHistoryImportV1 {
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
pub fn inspect_history_package_v1(
    input: &InspectHistoryPackageV1,
) -> Result<HistoryPackageInspectionV1, EditorError> {
    let (_, prepared, _) = verified(&input.package_path, None)?;
    Ok(app::inspection(&prepared))
}
pub fn read_history_package_v1(
    input: &ImportHistoryPackageV1,
    asset_store_root: &Path,
) -> Result<PreparedHistoryImportV1, EditorError> {
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
