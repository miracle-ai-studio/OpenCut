//! Owner-local directory package I/O. Only verified bytes reach the CAS adapter.
use opencut_api_contracts::project_package::{
    ExportPackage, ExportReceipt, ImportPackage, PackageAsset, PackageManifest, PackageMode,
    PreparedImport,
};
use opencut_asset_store::AssetStore;
use opencut_contracts::{Asset, EditorError, ProjectDocument};
use opencut_editor_application::project_package::{PackageExportSource, dependencies};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) const MAX_DOCUMENT_BYTES: u64 = 16 * 1024 * 1024;
pub(super) const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
pub(super) const MAX_ASSET_BYTES: u64 = opencut_api_contracts::uploads::MAX_UPLOAD_BYTES;
// Directory packages are streamed in 1 MiB blocks, not loaded into memory.
// A short multi-clip 4K project already exceeds 4 GiB. Keep a bounded project
// budget while retaining the independent per-asset and document limits.
pub(super) const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024 * 1024;

fn validate_media_sizes(sizes: impl IntoIterator<Item = u64>) -> Result<(), EditorError> {
    let mut total = 0_u64;
    for size in sizes {
        if size > MAX_ASSET_BYTES {
            return Err(invalid("Asset exceeds package size limit"));
        }
        total = total
            .checked_add(size)
            .ok_or_else(|| invalid("Package media overflow"))?;
        if total > MAX_TOTAL_BYTES {
            return Err(invalid("Package media exceeds total limit"));
        }
    }
    Ok(())
}

pub(super) fn invalid(message: &str) -> EditorError {
    EditorError::new("PACKAGE_INVALID", message)
}
pub(super) fn io(error: std::io::Error) -> EditorError {
    EditorError::new("PACKAGE_IO_ERROR", error.to_string())
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
pub(super) fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
pub(super) fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn regular(path: &Path) -> Result<File, EditorError> {
    if !fs::symlink_metadata(path)
        .map_err(io)?
        .file_type()
        .is_file()
    {
        return Err(invalid("Package entry is not a regular file"));
    }
    File::open(path).map_err(io)
}
pub(super) fn bounded_bytes(path: &Path, max: u64) -> Result<Vec<u8>, EditorError> {
    let file = regular(path)?;
    if file.metadata().map_err(io)?.len() > max {
        return Err(invalid("Package entry exceeds size limit"));
    }
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes).map_err(io)?;
    if bytes.len() as u64 > max {
        return Err(invalid("Package entry grew beyond size limit"));
    }
    Ok(bytes)
}
pub(super) fn write_file(path: &Path, bytes: &[u8]) -> Result<(), EditorError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io)?;
    file.write_all(bytes).map_err(io)?;
    file.sync_all().map_err(io)
}
pub(super) fn sync_dir(path: &Path) -> Result<(), EditorError> {
    File::open(path).map_err(io)?.sync_all().map_err(io)
}
pub(super) fn relative(root: &Path, text: &str) -> Result<PathBuf, EditorError> {
    let path = Path::new(text);
    if path.components().next().is_none()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(invalid(
            "Package path must contain only relative file names",
        ));
    }
    let mut current = root.to_path_buf();
    for part in path.components() {
        current.push(part);
        if fs::symlink_metadata(&current)
            .map_err(io)?
            .file_type()
            .is_symlink()
        {
            return Err(invalid("Symbolic links are not allowed in a package"));
        }
    }
    Ok(current)
}
fn local_link(path: &str) -> Result<PathBuf, EditorError> {
    let path = PathBuf::from(path);
    if !path.is_absolute() || fs::canonicalize(&path).map_err(io)? != path {
        return Err(invalid("Linked source must be a canonical local path"));
    }
    regular(&path)?;
    Ok(path)
}
pub(super) fn verify_file(
    path: &Path,
    expected_hash: &str,
    expected_size: u64,
) -> Result<(), EditorError> {
    if !valid_hash(expected_hash) || expected_size > MAX_ASSET_BYTES {
        return Err(invalid("Invalid package content identity or size"));
    }
    let mut file = regular(path)?;
    if file.metadata().map_err(io)?.len() != expected_size {
        return Err(invalid("Package media size differs from manifest"));
    }
    let mut hash = Sha256::new();
    let mut seen = 0_u64;
    let mut block = [0_u8; 1024 * 1024];
    loop {
        let n = file.read(&mut block).map_err(io)?;
        if n == 0 {
            break;
        }
        seen = seen
            .checked_add(n as u64)
            .ok_or_else(|| invalid("Package media overflow"))?;
        if seen > expected_size {
            return Err(invalid("Package media changed during verification"));
        }
        hash.update(&block[..n]);
    }
    if seen != expected_size || hex(&hash.finalize()) != expected_hash {
        return Err(invalid("Package media SHA-256 mismatch"));
    }
    Ok(())
}
pub(super) fn copy_verified(
    source: &mut File,
    destination: &Path,
    hash: &str,
    size: u64,
) -> Result<(), EditorError> {
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(io)?;
    let mut digest = Sha256::new();
    let mut copied = 0_u64;
    let mut block = [0_u8; 1024 * 1024];
    loop {
        let n = source.read(&mut block).map_err(io)?;
        if n == 0 {
            break;
        }
        copied = copied
            .checked_add(n as u64)
            .ok_or_else(|| invalid("Package media overflow"))?;
        if copied > size {
            return Err(invalid("Source media changed during package export"));
        }
        digest.update(&block[..n]);
        output.write_all(&block[..n]).map_err(io)?;
    }
    if copied != size || hex(&digest.finalize()) != hash {
        return Err(invalid("Source media changed during package export"));
    }
    output.sync_all().map_err(io)
}

pub(super) fn export_media(
    document: &mut ProjectDocument,
    mode: &PackageMode,
    staging: &Path,
    store: &AssetStore,
) -> Result<Vec<PackageAsset>, EditorError> {
    let unsupported: Vec<_> = document
        .assets
        .iter()
        .filter(|asset| {
            !matches!(
                asset.kind,
                opencut_contracts::AssetKind::Video
                    | opencut_contracts::AssetKind::Audio
                    | opencut_contracts::AssetKind::Image
            )
        })
        .map(|asset| format!("{}:{:?}", asset.id, asset.kind))
        .collect();
    if !unsupported.is_empty() {
        return Err(EditorError::new(
            "PACKAGE_DEPENDENCY_UNSUPPORTED",
            format!("Unsupported asset kinds: {}", unsupported.join(", ")),
        ));
    }
    let mut inventory = Vec::new();
    let mut copied = BTreeSet::new();
    let pinned_assets = document
        .assets
        .iter()
        .map(|asset| store.pin_asset(asset))
        .collect::<Result<Vec<_>, _>>()?;
    let sizes = pinned_assets
        .iter()
        .map(|asset| {
            asset
                .version
                .as_ref()
                .map(|version| version.size)
                .ok_or_else(|| invalid("Asset could not be pinned"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    // Reject oversized inventories before copying any payload into staging.
    validate_media_sizes(sizes)?;
    if *mode == PackageMode::SelfContained {
        fs::create_dir(staging.join("media")).map_err(io)?;
    }
    for (asset, pinned) in document.assets.iter_mut().zip(pinned_assets) {
        let version = pinned
            .version
            .clone()
            .ok_or_else(|| invalid("Asset could not be pinned"))?;
        let (mut bytes, verified_size) = store.open_verified(&pinned)?;
        if verified_size != version.size {
            return Err(invalid("Asset version size changed"));
        }
        let resolved = store.resolve_asset(&pinned)?;
        let entry = match mode {
            PackageMode::SelfContained => {
                let member = format!("media/{}", version.content_hash);
                if copied.insert(version.content_hash.clone()) {
                    copy_verified(
                        &mut bytes,
                        &staging.join(&member),
                        &version.content_hash,
                        version.size,
                    )?;
                }
                PackageAsset {
                    asset_id: asset.id.clone(),
                    content_hash: version.content_hash.clone(),
                    size: version.size,
                    media_path: Some(member),
                    linked_hint: None,
                }
            }
            PackageMode::Linked => PackageAsset {
                asset_id: asset.id.clone(),
                content_hash: version.content_hash.clone(),
                size: version.size,
                media_path: None,
                linked_hint: Some(resolved.uri),
            },
        };
        asset.version = Some(version);
        inventory.push(entry);
    }
    inventory.sort_by(|a, b| a.asset_id.cmp(&b.asset_id));
    Ok(inventory)
}

pub fn export_package(
    input: &ExportPackage,
    source: &PackageExportSource,
    asset_store_root: &Path,
) -> Result<ExportReceipt, EditorError> {
    if source.snapshot.project.id != input.project_id || source.snapshot.revision != input.revision
    {
        return Err(invalid("Export source is not the requested fixed revision"));
    }
    let requested = Path::new(&input.destination);
    if !requested.is_absolute() || requested.exists() {
        return Err(invalid(
            "Package destination must be a new absolute directory",
        ));
    }
    let parent = fs::canonicalize(
        requested
            .parent()
            .ok_or_else(|| invalid("Invalid package destination"))?,
    )
    .map_err(io)?;
    let name = requested
        .file_name()
        .ok_or_else(|| invalid("Invalid package destination name"))?;
    let destination = parent.join(name);
    if destination.exists() {
        return Err(invalid("Package destination already exists"));
    }
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid("System clock is invalid"))?
        .as_nanos();
    let staging = parent.join(format!(".opencut-package-{}-{suffix}", std::process::id()));
    fs::create_dir(&staging).map_err(io)?;
    let result = (|| {
        let store = AssetStore::open_managed(asset_store_root)?;
        let mut document = source.snapshot.project.clone();
        document.schema_version = 2;
        let inventory = export_media(&mut document, &input.mode, &staging, &store)?;
        opencut_editor_application::project_package::validate_export_document(&document)?;
        let document_bytes = serde_json::to_vec(&document)
            .map_err(|_| invalid("Could not serialize project document"))?;
        if document_bytes.len() as u64 > MAX_DOCUMENT_BYTES {
            return Err(invalid("Project document exceeds package size limit"));
        }
        write_file(&staging.join("project.json"), &document_bytes)?;
        let dependencies = dependencies(source);
        let manifest = PackageManifest {
            schema_version: 1,
            mode: input.mode.clone(),
            source_project_id: input.project_id.clone(),
            source_revision: input.revision.clone(),
            project_path: "project.json".into(),
            project_sha256: digest(&document_bytes),
            assets: inventory,
            dependencies: dependencies.clone(),
            self_contained_media: input.mode == PackageMode::SelfContained,
            fully_reproducible: input.mode == PackageMode::SelfContained
                && dependencies.fonts_not_bundled.is_empty()
                && dependencies.plugins_not_bundled.is_empty()
                && dependencies.tools_not_locked.is_empty()
                && dependencies.unsupported.is_empty(),
        };
        let manifest_bytes = serde_json::to_vec(&manifest)
            .map_err(|_| invalid("Could not serialize package manifest"))?;
        if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(invalid("Package manifest exceeds size limit"));
        }
        write_file(&staging.join("manifest.json"), &manifest_bytes)?;
        if input.mode == PackageMode::SelfContained {
            sync_dir(&staging.join("media"))?;
        }
        sync_dir(&staging)?;
        if destination.exists() {
            return Err(invalid("Package destination appeared during export"));
        }
        fs::rename(&staging, &destination).map_err(io)?;
        sync_dir(&parent)?;
        Ok(ExportReceipt {
            package_path: destination.to_string_lossy().into_owned(),
            manifest_sha256: digest(&manifest_bytes),
            source_revision: input.revision.clone(),
            asset_count: document.assets.len(),
            dependencies,
            self_contained_media: manifest.self_contained_media,
            fully_reproducible: manifest.fully_reproducible,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

pub fn read_package(
    input: &ImportPackage,
    asset_store_root: &Path,
) -> Result<PreparedImport, EditorError> {
    let root = Path::new(&input.package_path);
    if !root.is_absolute()
        || fs::canonicalize(root).map_err(io)? != root
        || !fs::symlink_metadata(root).map_err(io)?.file_type().is_dir()
    {
        return Err(invalid(
            "Package root must be a canonical directory without symlinks",
        ));
    }
    let manifest_bytes = bounded_bytes(&relative(root, "manifest.json")?, MAX_MANIFEST_BYTES)?;
    let manifest_sha256 = digest(&manifest_bytes);
    if !valid_hash(&input.expected_manifest_sha256)
        || manifest_sha256 != input.expected_manifest_sha256
    {
        return Err(invalid("Package manifest SHA-256 mismatch"));
    }
    let manifest: PackageManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| invalid("Package manifest is invalid"))?;
    if manifest.schema_version != 1
        || manifest.project_path != "project.json"
        || manifest.self_contained_media != (manifest.mode == PackageMode::SelfContained)
        || manifest.fully_reproducible
            != (manifest.mode == PackageMode::SelfContained
                && manifest.dependencies.fonts_not_bundled.is_empty()
                && manifest.dependencies.plugins_not_bundled.is_empty()
                && manifest.dependencies.tools_not_locked.is_empty()
                && manifest.dependencies.unsupported.is_empty())
    {
        return Err(invalid("Package schema or mode is unsupported"));
    }
    let document_bytes =
        bounded_bytes(&relative(root, &manifest.project_path)?, MAX_DOCUMENT_BYTES)?;
    if digest(&document_bytes) != manifest.project_sha256 {
        return Err(invalid("Project document SHA-256 mismatch"));
    }
    let document: ProjectDocument = serde_json::from_slice(&document_bytes)
        .map_err(|_| invalid("Project document is invalid"))?;
    if document.id != manifest.source_project_id {
        return Err(invalid("Project identity differs from manifest"));
    }
    capture_media(root, &manifest, &document, asset_store_root)?;
    Ok(PreparedImport {
        manifest_sha256,
        manifest,
        document,
    })
}

pub(super) fn capture_media(
    root: &Path,
    manifest: &PackageManifest,
    document: &ProjectDocument,
    asset_store_root: &Path,
) -> Result<(), EditorError> {
    capture_media_with_plugins(root, manifest, document, asset_store_root, &[])
}

pub(super) fn capture_media_with_plugins(
    root: &Path,
    manifest: &PackageManifest,
    document: &ProjectDocument,
    asset_store_root: &Path,
    plugin_bundle_paths: &[String],
) -> Result<(), EditorError> {
    let mut permitted = BTreeSet::from(["manifest.json".to_string(), "project.json".to_string()]);
    let mut seen = BTreeSet::new();
    validate_media_sizes(manifest.assets.iter().map(|asset| asset.size))?;
    let store = AssetStore::open_managed(asset_store_root)?;
    for asset in &manifest.assets {
        if !seen.insert(asset.asset_id.clone())
            || !valid_hash(&asset.content_hash)
            || asset.size > MAX_ASSET_BYTES
        {
            return Err(invalid("Package asset inventory is invalid"));
        }
        let source = match &manifest.mode {
            PackageMode::SelfContained => {
                let expected = format!("media/{}", asset.content_hash);
                if asset.media_path.as_deref() != Some(expected.as_str())
                    || asset.linked_hint.is_some()
                {
                    return Err(invalid("Self-contained media path does not match hash"));
                }
                permitted.insert(expected.clone());
                relative(root, &expected)?
            }
            PackageMode::Linked => {
                if asset.media_path.is_some() {
                    return Err(invalid("Linked package embeds media"));
                }
                local_link(
                    asset
                        .linked_hint
                        .as_deref()
                        .ok_or_else(|| invalid("Linked hint is missing"))?,
                )?
            }
        };
        verify_file(&source, &asset.content_hash, asset.size)?;
        let original = document
            .assets
            .iter()
            .find(|item| item.id == asset.asset_id)
            .ok_or_else(|| invalid("Package inventory refers to an absent asset"))?;
        let capture = Asset {
            version: None,
            uri: source.to_string_lossy().into_owned(),
            ..original.clone()
        };
        let captured = store.pin_asset(&capture)?;
        if captured.version.as_ref().is_none_or(|version| {
            version.content_hash != asset.content_hash || version.size != asset.size
        }) {
            return Err(invalid("CAS capture differs from package inventory"));
        }
    }
    if !plugin_bundle_paths.is_empty() {
        permitted.insert("plugins".to_string());
    }
    // Reject extra entries and symlinks so no unreviewed content is carried as
    // an apparent part of this package. Linked packages have no media directory.
    for entry in fs::read_dir(root).map_err(io)? {
        let entry = entry.map_err(io)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "media" && manifest.mode == PackageMode::SelfContained {
            if !entry.file_type().map_err(io)?.is_dir() {
                return Err(invalid("Invalid media directory"));
            }
            for child in fs::read_dir(entry.path()).map_err(io)? {
                let child = child.map_err(io)?;
                let member = format!("media/{}", child.file_name().to_string_lossy());
                if !permitted.contains(&member) || !child.file_type().map_err(io)?.is_file() {
                    return Err(invalid("Unexpected package media entry"));
                }
            }
        } else if name == "plugins" && !plugin_bundle_paths.is_empty() {
            if !entry.file_type().map_err(io)?.is_dir() {
                return Err(invalid("Invalid plugin bundle directory"));
            }
            validate_plugin_bundle_tree(root, &entry.path(), plugin_bundle_paths)?;
        } else if !permitted.contains(&name) || !entry.file_type().map_err(io)?.is_file() {
            return Err(invalid("Unexpected package entry"));
        }
    }
    Ok(())
}

fn validate_plugin_bundle_tree(
    root: &Path,
    current: &Path,
    expected_roots: &[String],
) -> Result<(), EditorError> {
    let current_relative = current
        .strip_prefix(root)
        .map_err(|_| invalid("Plugin bundle path escaped package root"))?
        .to_string_lossy()
        .replace('\\', "/");
    let is_expected_root = expected_roots.iter().any(|path| path == &current_relative);
    let is_expected_parent = expected_roots
        .iter()
        .any(|path| path.starts_with(&(current_relative.clone() + "/")));
    let is_expected_descendant = expected_roots
        .iter()
        .any(|path| current_relative.starts_with(&(path.clone() + "/")));
    if current_relative != "plugins"
        && !is_expected_root
        && !is_expected_parent
        && !is_expected_descendant
    {
        return Err(invalid("Unexpected plugin package entry"));
    }
    for entry in fs::read_dir(current).map_err(io)? {
        let entry = entry.map_err(io)?;
        let file_type = entry.file_type().map_err(io)?;
        if file_type.is_dir() {
            validate_plugin_bundle_tree(root, &entry.path(), expected_roots)?;
        } else if file_type.is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| invalid("Plugin bundle path escaped package root"))?
                .to_string_lossy()
                .replace('\\', "/");
            if !expected_roots
                .iter()
                .any(|path| relative.starts_with(&(path.clone() + "/")))
            {
                return Err(invalid("Unexpected file in plugin package inventory"));
            }
        } else {
            return Err(invalid(
                "Plugin package entries must be regular files or directories",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{CreateProject, FrameRate, TransactionRequest};
    use opencut_sqlite_store::EditorApplication;

    #[test]
    fn multi_clip_4k_inventory_keeps_bounded_package_limits() {
        // Ten individually supported camera clips can exceed the old 4 GiB
        // project ceiling without requiring any single large allocation.
        assert!(validate_media_sizes([512 * 1024 * 1024; 10]).is_ok());
        assert!(validate_media_sizes([MAX_ASSET_BYTES; 4]).is_ok());
        assert!(validate_media_sizes([MAX_ASSET_BYTES; 5]).is_err());
        assert!(validate_media_sizes([MAX_ASSET_BYTES + 1]).is_err());
        assert!(validate_media_sizes([u64::MAX]).is_err());
    }
    use serde_json::json;

    #[test]
    fn self_contained_package_survives_source_removal_and_rejects_tampering() {
        let directory = tempfile::tempdir().unwrap();
        let origin = directory.path().join("origin");
        let target = directory.path().join("target");
        fs::create_dir(&origin).unwrap();
        fs::create_dir(&target).unwrap();
        let sample = origin.join("voice.wav");
        fs::write(&sample, b"fixed media bytes for package portability").unwrap();
        let mut source = EditorApplication::open(origin.join("workspace.sqlite")).unwrap();
        let created = source
            .create_project(
                CreateProject {
                    id: "source".into(),
                    name: "Source".into(),
                    sequence_id: "main".into(),
                    width: 64,
                    height: 48,
                    fps: FrameRate {
                        numerator: 24,
                        denominator: 1,
                    },
                },
                "create-source",
            )
            .unwrap();
        let edit: TransactionRequest = serde_json::from_value(json!({
            "expected_revision":created.revision,"idempotency_key":"bind","label":"Bind voice",
            "operations":[{"type":"asset.bind","asset":{"id":"voice","name":"Voice",
                "kind":"audio","uri":sample,"duration":{"ticks":"1","timebase":1},
                "metadata":{}}}]
        }))
        .unwrap();
        let bound = source.apply("source", &edit).unwrap();
        let linked_package = directory.path().join("voice-linked.opencut");
        let linked_export = export_package(
            &ExportPackage {
                project_id: "source".into(),
                revision: bound.revision.clone(),
                destination: linked_package.to_string_lossy().into(),
                mode: PackageMode::Linked,
            },
            &source
                .package_export_source("source", &bound.revision)
                .unwrap(),
            source.asset_store_root(),
        )
        .unwrap();
        assert!(!linked_export.self_contained_media);
        let linked_request = ImportPackage {
            package_path: linked_export.package_path.clone(),
            target_project_id: "linked".into(),
            expected_manifest_sha256: linked_export.manifest_sha256,
            idempotency_key: "linked-once".into(),
        };
        read_package(&linked_request, source.asset_store_root()).unwrap();
        let package = directory.path().join("voice.opencut");
        let export = export_package(
            &ExportPackage {
                project_id: "source".into(),
                revision: bound.revision.clone(),
                destination: package.to_string_lossy().into(),
                mode: PackageMode::SelfContained,
            },
            &source
                .package_export_source("source", &bound.revision)
                .unwrap(),
            source.asset_store_root(),
        )
        .unwrap();
        assert!(export.self_contained_media);
        assert!(!export.fully_reproducible);
        drop(source);
        fs::remove_dir_all(&origin).unwrap();
        assert!(read_package(&linked_request, &target.join("asset-store")).is_err());
        let mut imported = EditorApplication::open(target.join("workspace.sqlite")).unwrap();
        let request = ImportPackage {
            package_path: export.package_path.clone(),
            target_project_id: "restored".into(),
            expected_manifest_sha256: export.manifest_sha256.clone(),
            idempotency_key: "import-once".into(),
        };
        let prepared = read_package(&request, imported.asset_store_root()).unwrap();
        let receipt = imported.package_import_commit(&request, &prepared).unwrap();
        let replay = imported.package_import_commit(&request, &prepared).unwrap();
        assert_eq!(receipt.revision, replay.revision);
        let reopened = imported.snapshot("restored").unwrap();
        assert_eq!(reopened.revision, receipt.revision);
        let asset = &reopened.project.assets[0];
        let (mut bytes, _) = AssetStore::read_only(imported.asset_store_root())
            .unwrap()
            .open_verified(asset)
            .unwrap();
        let mut copied = Vec::new();
        bytes.read_to_end(&mut copied).unwrap();
        assert_eq!(copied, b"fixed media bytes for package portability");
        let next: TransactionRequest = serde_json::from_value(json!({
            "expected_revision":receipt.revision,"idempotency_key":"continue-edit",
            "label":"Continue after import","operations":[{"type":"track.create",
                "sequence_id":"main","track":{"id":"voice-track","name":"Voice",
                "kind":"audio","enabled":true,"muted":false,"locked":false,
                "clips":[],"params":{}}}]
        }))
        .unwrap();
        imported.apply("restored", &next).unwrap();
        assert_eq!(
            imported.snapshot("restored").unwrap().project.sequences[0]
                .tracks
                .len(),
            1
        );
        let conflicting = ImportPackage {
            target_project_id: "another".into(),
            ..request.clone()
        };
        assert_eq!(
            imported
                .package_import_commit(&conflicting, &prepared)
                .unwrap_err()
                .code,
            "IDEMPOTENCY_KEY_REUSED"
        );

        let media = Path::new(&export.package_path).join(format!(
            "media/{}",
            asset.version.as_ref().unwrap().content_hash
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let extra = Path::new(&export.package_path).join("media/unreviewed-link");
            symlink(&media, &extra).unwrap();
            assert_eq!(
                read_package(&request, imported.asset_store_root())
                    .unwrap_err()
                    .code,
                "PACKAGE_INVALID"
            );
            fs::remove_file(extra).unwrap();
        }
        fs::write(&media, b"tampered").unwrap();
        assert_eq!(
            read_package(&request, imported.asset_store_root())
                .unwrap_err()
                .code,
            "PACKAGE_INVALID"
        );
    }
}
