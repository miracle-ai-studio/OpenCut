//! Bounded trusted browser installation checks shared by host assemblies.
use serde_json::Value;
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};
pub(super) const METADATA_BYTES: u64 = 64 * 1024;
const SOURCE_BYTES: u64 = 8 * 1024 * 1024;
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Unavailable {
    NotConfigured,
    Unavailable,
}
pub(super) fn dependency(
    package: &Path,
    name: &str,
    version: &str,
    entry: &str,
) -> Result<(), Unavailable> {
    let root = absolute(&package.join("node_modules").join(name))?;
    let manifest = metadata(&root.join("package.json"))?;
    if manifest["name"] != name || manifest["version"] != version {
        return Err(Unavailable::Unavailable);
    }
    package_file(&root, entry)?;
    Ok(())
}
pub(super) fn absolute(path: &Path) -> Result<PathBuf, Unavailable> {
    if !path.is_absolute() {
        return Err(Unavailable::Unavailable);
    }
    fs::canonicalize(path).map_err(|_| Unavailable::Unavailable)
}
pub(super) fn package_file(root: &Path, relative: &str) -> Result<PathBuf, Unavailable> {
    let filename = absolute(&root.join(relative))?;
    let stat = fs::metadata(&filename).map_err(|_| Unavailable::Unavailable)?;
    if !filename.starts_with(root)
        || !stat.is_file()
        || stat.len() == 0
        || stat.len() > SOURCE_BYTES
    {
        return Err(Unavailable::Unavailable);
    }
    Ok(filename)
}
pub(super) fn executable(path: &Path) -> Result<PathBuf, Unavailable> {
    let path = absolute(path)?;
    let stat = fs::metadata(&path).map_err(|_| Unavailable::Unavailable)?;
    if !stat.is_file() || stat.len() == 0 {
        return Err(Unavailable::Unavailable);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if stat.permissions().mode() & 0o111 == 0 {
            return Err(Unavailable::Unavailable);
        }
    }
    Ok(path)
}
pub(super) fn metadata(path: &Path) -> Result<Value, Unavailable> {
    let file = File::open(path).map_err(|_| Unavailable::Unavailable)?;
    let stat = file.metadata().map_err(|_| Unavailable::Unavailable)?;
    if !stat.is_file() || stat.len() == 0 || stat.len() > METADATA_BYTES {
        return Err(Unavailable::Unavailable);
    }
    let mut bytes = Vec::new();
    file.take(METADATA_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Unavailable::Unavailable)?;
    if bytes.len() as u64 > METADATA_BYTES {
        return Err(Unavailable::Unavailable);
    }
    serde_json::from_slice(&bytes).map_err(|_| Unavailable::Unavailable)
}
