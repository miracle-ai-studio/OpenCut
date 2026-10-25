//! Bounded, data-only preset packages; no WASM or shell runtime is loaded.
use opencut_api_contracts::presets::{
    InstalledPresetPackage, PackageInput, PresetDeclaration, PresetInput, PresetManifest,
    PresetSpec,
};
use opencut_contracts::EditorError;
use opencut_editor_application::presets::{self, PresetCatalog};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_MANIFEST: u64 = 64 * 1024;
const MAX_SPEC: u64 = 512 * 1024;
const MAX_PACKAGE: u64 = 8 * 1024 * 1024;
type PackageData = (PresetManifest, BTreeMap<PathBuf, Vec<u8>>, String);

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_PLUGIN", message)
}
fn io(error: std::io::Error) -> EditorError {
    EditorError::new("PLUGIN_IO_ERROR", error.to_string())
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn ensure_dir(path: &Path) -> Result<(), EditorError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => Err(invalid("Package directory is not a regular directory")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(io)?;
            if !fs::symlink_metadata(path).map_err(io)?.file_type().is_dir() {
                return Err(invalid("Package directory changed"));
            }
            Ok(())
        }
        Err(error) => Err(io(error)),
    }
}
#[cfg(unix)]
fn nofollow_read(path: &Path) -> Result<File, EditorError> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(io)
}
#[cfg(not(unix))]
fn nofollow_read(path: &Path) -> Result<File, EditorError> {
    File::open(path).map_err(io)
}
fn regular_bytes(path: &Path, ceiling: u64) -> Result<Vec<u8>, EditorError> {
    let metadata = fs::symlink_metadata(path).map_err(io)?;
    if !metadata.file_type().is_file() {
        return Err(invalid("Preset entry is not a regular file"));
    }
    let file = nofollow_read(path)?;
    let opened = file.metadata().map_err(io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.dev() != opened.dev() || metadata.ino() != opened.ino() {
            return Err(invalid("Preset entry changed while opening"));
        }
    }
    if !opened.is_file() || opened.len() > ceiling {
        return Err(invalid("Preset entry exceeds size limit"));
    }
    let mut bytes = Vec::new();
    file.take(ceiling + 1).read_to_end(&mut bytes).map_err(io)?;
    if bytes.len() as u64 > ceiling {
        return Err(invalid("Preset entry grew beyond size limit"));
    }
    Ok(bytes)
}
fn spec_path(declaration: &PresetDeclaration) -> Result<PathBuf, EditorError> {
    let path = Path::new(&declaration.spec_path);
    let parts: Vec<_> = path.components().collect();
    if parts.len() != 2
        || parts[0].as_os_str() != "specs"
        || !matches!(parts[1], std::path::Component::Normal(_))
        || !declaration.spec_path.ends_with(".json")
        || !presets::valid_hash(&declaration.spec_sha256)
    {
        return Err(invalid("Invalid preset spec path or digest"));
    }
    Ok(path.to_path_buf())
}
fn validate_manifest(manifest: &PresetManifest) -> Result<(), EditorError> {
    if manifest.manifest_version != 2
        || !presets::valid_package_id(&manifest.id)
        || !presets::valid_version(&manifest.version)
        || manifest.display_name.is_empty()
        || manifest.display_name.len() > 256
        || manifest.presets.is_empty()
        || manifest.presets.len() > presets::MAX_PRESETS
    {
        return Err(invalid("Invalid Plugin V2 manifest"));
    }
    let mut names = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for preset in &manifest.presets {
        if !presets::valid_name(&preset.name) || !names.insert(&preset.name) {
            return Err(invalid("Duplicate or invalid preset name"));
        }
        if !paths.insert(spec_path(preset)?) {
            return Err(invalid("Duplicate spec path"));
        }
    }
    Ok(())
}
fn validate_spec(spec: &PresetSpec) -> Result<(), EditorError> {
    match spec {
        PresetSpec::CaptionStyle { .. } => Ok(()),
        PresetSpec::EditTemplate {
            operations,
            clip_bindings,
        } => {
            if operations.is_empty()
                || operations.len() > presets::MAX_OPERATIONS
                || clip_bindings.len() > presets::MAX_OPERATIONS
            {
                return Err(invalid("Template exceeds operation limit"));
            }
            for op in operations {
                match op {
                    opencut_contracts::Operation::AddTrack { track, .. }
                        if track.clips.is_empty() => {}
                    opencut_contracts::Operation::InsertClip { clip, .. }
                        if clip.asset_id.is_none() && clip_bindings.contains_key(&clip.id) => {}
                    opencut_contracts::Operation::InsertCaption { .. } => {}
                    _ => return Err(invalid("Template operation is not declarative")),
                }
            }
            Ok(())
        }
    }
}
fn read_package(root: &Path) -> Result<PackageData, EditorError> {
    if !fs::symlink_metadata(root).map_err(io)?.file_type().is_dir() {
        return Err(invalid("Package root is not a directory"));
    }
    let mut files = BTreeMap::new();
    let manifest_bytes = regular_bytes(&root.join("manifest_v2.json"), MAX_MANIFEST)?;
    let manifest: PresetManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| invalid("Invalid manifest_v2.json"))?;
    validate_manifest(&manifest)?;
    files.insert(PathBuf::from("manifest_v2.json"), manifest_bytes);
    if !fs::symlink_metadata(root.join("specs"))
        .map_err(io)?
        .file_type()
        .is_dir()
    {
        return Err(invalid("Spec directory is not regular"));
    }
    for declaration in &manifest.presets {
        let path = spec_path(declaration)?;
        let bytes = regular_bytes(&root.join(&path), MAX_SPEC)?;
        if digest(&bytes) != declaration.spec_sha256 {
            return Err(invalid("Preset spec digest differs"));
        }
        let spec: PresetSpec =
            serde_json::from_slice(&bytes).map_err(|_| invalid("Invalid preset spec"))?;
        validate_spec(&spec)?;
        files.insert(path, bytes);
    }
    let mut actual = BTreeSet::new();
    for entry in fs::read_dir(root).map_err(io)? {
        let entry = entry.map_err(io)?;
        let name = entry.file_name();
        if name != "manifest_v2.json" && name != "specs" {
            return Err(invalid("Undeclared package entry"));
        }
        actual.insert(name);
    }
    if actual.len() != 2 {
        return Err(invalid("Package files are incomplete"));
    }
    let expected_specs: BTreeSet<_> = files
        .keys()
        .filter_map(|p| p.strip_prefix("specs").ok())
        .map(|p| p.to_path_buf())
        .collect();
    let mut actual_specs = BTreeSet::new();
    for entry in fs::read_dir(root.join("specs")).map_err(io)? {
        let entry = entry.map_err(io)?;
        if !entry.file_type().map_err(io)?.is_file() {
            return Err(invalid("Undeclared spec entry"));
        }
        actual_specs.insert(PathBuf::from(entry.file_name()));
    }
    if expected_specs != actual_specs {
        return Err(invalid("Undeclared or missing spec file"));
    }
    let total: usize = files.values().map(Vec::len).sum();
    if total as u64 > MAX_PACKAGE {
        return Err(invalid("Package exceeds size limit"));
    }
    let mut hash = Sha256::new();
    for (path, bytes) in &files {
        hash.update(
            path.to_str()
                .ok_or_else(|| invalid("Non-UTF-8 package name"))?
                .as_bytes(),
        );
        hash.update([0]);
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    Ok((manifest, files, format!("{:x}", hash.finalize())))
}
fn write_file(path: &Path, bytes: &[u8]) -> Result<(), EditorError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io)?;
    file.write_all(bytes).map_err(io)?;
    file.sync_all().map_err(io)
}
#[cfg(unix)]
fn lock(file: &File, exclusive: bool) -> Result<(), EditorError> {
    use std::os::fd::AsRawFd;
    let operation = if exclusive {
        libc::LOCK_EX
    } else {
        libc::LOCK_SH
    };
    if unsafe { libc::flock(file.as_raw_fd(), operation) } != 0 {
        return Err(io(std::io::Error::last_os_error()));
    }
    Ok(())
}
#[cfg(not(unix))]
fn lock(_: &File, _: bool) -> Result<(), EditorError> {
    Err(EditorError::new(
        "CAPABILITY_UNAVAILABLE",
        "Plugin V2 installation requires native file locks",
    ))
}

pub struct PresetPackages {
    root: PathBuf,
}
impl PresetPackages {
    pub fn new(workspace: impl AsRef<Path>) -> Self {
        Self {
            root: workspace.as_ref().join("plugin-packages-v2"),
        }
    }

    /// Copies one verified installed package into a project package without changing it.
    pub fn copy_installed_package(
        &self,
        id: &str,
        version: &str,
        expected_sha256: &str,
        destination: impl AsRef<Path>,
    ) -> Result<u64, EditorError> {
        let (package, source) = self.installed(&PackageInput {
            id: id.to_owned(),
            version: version.to_owned(),
        })?;
        if package.package_sha256 != expected_sha256 {
            return Err(invalid(
                "Installed preset package differs from the project grant identity",
            ));
        }
        let destination = destination.as_ref();
        if destination.exists() {
            return Err(invalid("Plugin bundle destination already exists"));
        }
        let (_, files, _) = read_package(&source)?;
        let total_size = files.values().map(|bytes| bytes.len() as u64).sum();
        fs::create_dir_all(destination.join("specs")).map_err(io)?;
        for (relative, bytes) in files {
            let target = destination.join(relative);
            write_file(&target, &bytes)?;
        }
        File::open(destination.join("specs"))
            .map_err(io)?
            .sync_all()
            .map_err(io)?;
        File::open(destination)
            .map_err(io)?
            .sync_all()
            .map_err(io)?;
        let (_, _, copied_hash) = read_package(destination)?;
        if copied_hash != expected_sha256 {
            return Err(invalid("Copied plugin package identity differs"));
        }
        Ok(total_size)
    }

    /// Validates an uninstalled package directory and returns its identity.
    pub fn verify_package_root(
        root: impl AsRef<Path>,
    ) -> Result<(PresetManifest, String), EditorError> {
        let (manifest, _, digest) = read_package(root.as_ref())?;
        Ok((manifest, digest))
    }
    fn version_lock(&self, id: &str, version: &str, exclusive: bool) -> Result<File, EditorError> {
        if !presets::valid_package_id(id) || !presets::valid_version(version) {
            return Err(invalid("Invalid package identity"));
        }
        ensure_dir(&self.root)?;
        ensure_dir(&self.root.join(".locks"))?;
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        let file = options
            .open(self.root.join(".locks").join(format!("{id}@{version}")))
            .map_err(io)?;
        lock(&file, exclusive)?;
        Ok(file)
    }
    fn installed(
        &self,
        input: &PackageInput,
    ) -> Result<(InstalledPresetPackage, PathBuf), EditorError> {
        let _lock = self.version_lock(&input.id, &input.version, false)?;
        let version = self.root.join(&input.id).join(&input.version);
        if !fs::symlink_metadata(self.root.join(&input.id))
            .map_err(io)?
            .file_type()
            .is_dir()
            || !fs::symlink_metadata(&version)
                .map_err(io)?
                .file_type()
                .is_dir()
        {
            return Err(invalid("Package directory is not regular"));
        }
        let entries = fs::read_dir(&version)
            .map_err(io)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(io)?;
        let installed: Vec<_> = entries
            .into_iter()
            .filter(|entry| !entry.file_name().to_string_lossy().starts_with(".stage-"))
            .collect();
        if installed.len() != 1 {
            return Err(invalid("Package version has multiple identities"));
        }
        let entry = &installed[0];
        let path = entry.path();
        let (manifest, _, hash) = read_package(&path)?;
        if manifest.id != input.id
            || manifest.version != input.version
            || entry.file_name() != hash.as_str()
        {
            return Err(invalid("Installed package identity differs"));
        }
        Ok((
            InstalledPresetPackage {
                manifest,
                package_sha256: hash,
            },
            path,
        ))
    }
}
impl PresetPackages {
    fn install_package(
        &self,
        package_root: &str,
        expected_sha256: Option<&str>,
    ) -> Result<InstalledPresetPackage, EditorError> {
        let source = Path::new(package_root);
        if !source.is_absolute() {
            return Err(invalid(
                "Installation requires an absolute owner-local path",
            ));
        }
        let source = source.canonicalize().map_err(io)?;
        let (manifest, files, hash) = read_package(&source)?;
        if expected_sha256.is_some_and(|expected| expected != hash) {
            return Err(EditorError::new(
                "PLUGIN_INTEGRITY",
                "Preset package differs from the expected content identity",
            ));
        }
        let _lock = self.version_lock(&manifest.id, &manifest.version, true)?;
        let id_root = self.root.join(&manifest.id);
        ensure_dir(&id_root)?;
        let version = id_root.join(&manifest.version);
        ensure_dir(&version)?;
        for entry in fs::read_dir(&version).map_err(io)? {
            let entry = entry.map_err(io)?;
            if entry.file_name().to_string_lossy().starts_with(".stage-") {
                if !entry.file_type().map_err(io)?.is_dir() {
                    return Err(invalid("Invalid interrupted staging entry"));
                }
                fs::remove_dir_all(entry.path()).map_err(io)?;
            }
        }
        let entries = fs::read_dir(&version)
            .map_err(io)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(io)?;
        if let Some(entry) = entries.first() {
            if entries.len() != 1 || entry.file_name() != hash.as_str() {
                return Err(EditorError::new(
                    "PLUGIN_VERSION_CONFLICT",
                    "Version already has different package content",
                ));
            }
            let (_, _, installed_hash) = read_package(&entry.path())?;
            if installed_hash != hash {
                return Err(invalid("Installed package integrity failed"));
            }
            return Ok(InstalledPresetPackage {
                manifest,
                package_sha256: hash,
            });
        }
        let _reservation = opencut_storage_budget::Budget::new(
            self.root
                .parent()
                .ok_or_else(|| invalid("Invalid workspace root"))?,
        )
        .reserve(
            "plugin_package",
            files.values().map(|v| v.len() as u64).sum(),
        )?;
        let staging = tempfile::Builder::new()
            .prefix(".stage-")
            .tempdir_in(&version)
            .map_err(io)?;
        fs::create_dir(staging.path().join("specs")).map_err(io)?;
        for (path, bytes) in &files {
            write_file(&staging.path().join(path), bytes)?;
        }
        File::open(staging.path().join("specs"))
            .map_err(io)?
            .sync_all()
            .map_err(io)?;
        File::open(staging.path())
            .map_err(io)?
            .sync_all()
            .map_err(io)?;
        let (_, _, copied_hash) = read_package(staging.path())?;
        if copied_hash != hash {
            return Err(invalid("Staged package integrity failed"));
        }
        fs::rename(staging.path(), version.join(&hash)).map_err(io)?;
        File::open(&version).map_err(io)?.sync_all().map_err(io)?;
        Ok(InstalledPresetPackage {
            manifest,
            package_sha256: hash,
        })
    }
}
impl PresetCatalog for PresetPackages {
    fn install(
        &self,
        input: &opencut_api_contracts::presets::InstallInput,
    ) -> Result<InstalledPresetPackage, EditorError> {
        self.install_package(&input.package_root, None)
    }
    fn install_pinned(
        &self,
        input: &opencut_api_contracts::presets::InstallPinnedInput,
    ) -> Result<InstalledPresetPackage, EditorError> {
        self.install_package(&input.package_root, Some(&input.expected_package_sha256))
    }
    fn list(&self) -> Result<Vec<InstalledPresetPackage>, EditorError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        if !fs::symlink_metadata(&self.root)
            .map_err(io)?
            .file_type()
            .is_dir()
        {
            return Err(invalid("Package root is not regular"));
        }
        let mut packages = Vec::new();
        for id in fs::read_dir(&self.root).map_err(io)? {
            let id = id.map_err(io)?;
            let Some(name) = id.file_name().to_str().map(str::to_owned) else {
                return Err(invalid("Invalid package name"));
            };
            if name == ".locks" {
                continue;
            }
            if !presets::valid_package_id(&name) {
                return Err(invalid("Invalid package directory"));
            }
            if !id.file_type().map_err(io)?.is_dir() {
                return Err(invalid("Package id directory is not regular"));
            }
            for version in fs::read_dir(id.path()).map_err(io)? {
                let version = version.map_err(io)?;
                let version_name = version
                    .file_name()
                    .to_str()
                    .ok_or_else(|| invalid("Invalid version name"))?
                    .to_owned();
                packages.push(self.inspect(&PackageInput {
                    id: name.clone(),
                    version: version_name,
                })?);
            }
        }
        packages.sort_by(|a, b| {
            (&a.manifest.id, &a.manifest.version).cmp(&(&b.manifest.id, &b.manifest.version))
        });
        Ok(packages)
    }
    fn inspect(&self, input: &PackageInput) -> Result<InstalledPresetPackage, EditorError> {
        self.installed(input).map(|(package, _)| package)
    }
    fn load_spec(
        &self,
        input: &PresetInput,
        expected_package_sha256: &str,
    ) -> Result<(PresetDeclaration, PresetSpec), EditorError> {
        let (package, path) = self.installed(&PackageInput {
            id: input.target.id.clone(),
            version: input.target.version.clone(),
        })?;
        if package.package_sha256 != expected_package_sha256 {
            return Err(invalid("Grant package identity differs"));
        }
        let declaration = package
            .manifest
            .presets
            .into_iter()
            .find(|p| p.name == input.preset_name)
            .ok_or_else(|| invalid("Unknown preset"))?;
        let bytes = regular_bytes(&path.join(spec_path(&declaration)?), MAX_SPEC)?;
        if digest(&bytes) != declaration.spec_sha256 {
            return Err(invalid("Preset spec digest differs"));
        }
        let spec = serde_json::from_slice(&bytes).map_err(|_| invalid("Invalid preset spec"))?;
        Ok((declaration, spec))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_api_contracts::presets::InstallInput;

    fn fixture(root: &Path) -> (InstallInput, PathBuf) {
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        fs::create_dir(source.join("specs")).unwrap();
        let spec = serde_json::json!({"kind":"caption_style","style":{
            "font_family":"Arial","font_size":28.0,"color":"#ffffff",
            "background_color":null
        }});
        let spec_bytes = serde_json::to_vec(&spec).unwrap();
        fs::write(source.join("specs/title.json"), &spec_bytes).unwrap();
        let manifest = serde_json::json!({
            "manifest_version":2,"id":"example.style","version":"1.0.0",
            "display_name":"Example Style", "presets":[{
                "name":"title","spec_path":"specs/title.json",
                "spec_sha256":digest(&spec_bytes)
            }]
        });
        fs::write(
            source.join("manifest_v2.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        (
            InstallInput {
                package_root: source.to_string_lossy().into_owned(),
            },
            source,
        )
    }

    #[test]
    fn installed_identity_is_verified_and_retry_cleans_interrupted_stage() {
        let temp = tempfile::tempdir().unwrap();
        let (input, _) = fixture(temp.path());
        let catalog = PresetPackages::new(temp.path());
        let installed = catalog.install(&input).unwrap();
        assert_eq!(installed.manifest.version, "1.0.0");
        assert_eq!(catalog.list().unwrap().len(), 1);
        let version = temp.path().join("plugin-packages-v2/example.style/1.0.0");
        fs::create_dir(version.join(".stage-interrupted")).unwrap();
        assert_eq!(
            catalog.install(&input).unwrap().package_sha256,
            installed.package_sha256
        );
        assert!(!version.join(".stage-interrupted").exists());
        let path = version
            .join(&installed.package_sha256)
            .join("specs/title.json");
        fs::write(path, b"{}").unwrap();
        assert_eq!(
            catalog
                .inspect(&PackageInput {
                    id: "example.style".into(),
                    version: "1.0.0".into()
                })
                .unwrap_err()
                .code,
            "INVALID_PLUGIN"
        );
    }

    #[cfg(unix)]
    #[test]
    fn pinned_install_rejects_different_package_bytes_before_installing() {
        let workspace = tempfile::tempdir().unwrap();
        let package_root = tempfile::tempdir().unwrap();
        let (input, _) = fixture(package_root.path());
        let pinned = opencut_api_contracts::presets::InstallPinnedInput {
            package_root: input.package_root,
            expected_package_sha256: "0".repeat(64),
        };
        let catalog = PresetPackages::new(workspace.path());
        let error = catalog.install_pinned(&pinned).unwrap_err();
        assert_eq!(error.code, "PLUGIN_INTEGRITY");
        assert!(catalog.list().unwrap().is_empty());
    }

    #[test]
    fn source_symlink_and_extra_files_are_rejected() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let (input, source) = fixture(temp.path());
        let catalog = PresetPackages::new(temp.path());
        fs::write(source.join("unlisted.txt"), b"hidden").unwrap();
        assert_eq!(catalog.install(&input).unwrap_err().code, "INVALID_PLUGIN");
        fs::remove_file(source.join("unlisted.txt")).unwrap();
        fs::remove_file(source.join("specs/title.json")).unwrap();
        symlink("../manifest_v2.json", source.join("specs/title.json")).unwrap();
        assert_eq!(catalog.install(&input).unwrap_err().code, "INVALID_PLUGIN");
    }
}
