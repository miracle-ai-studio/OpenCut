//! Anchored directory reads and non-replacing publication shared by package readers.
use crate::project_package as common;
use opencut_api_contracts::project_archive::ArchiveBlob;
use opencut_contracts::EditorError;
use opencut_editor_application::project_archive as app;
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};
// Anchor all package reads to an opened directory. A concurrent rename or
// symlink replacement cannot redirect later verification/capture outside it.
pub(crate) struct PackageDirectory {
    pub(crate) file: File,
}
impl PackageDirectory {
    #[cfg(unix)]
    pub(crate) fn open(path: &Path) -> Result<Self, EditorError> {
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
    pub(crate) fn open(_path: &Path) -> Result<Self, EditorError> {
        Err(EditorError::new(
            "CAPABILITY_UNAVAILABLE",
            "Anchored history package I/O requires Unix directory descriptors",
        ))
    }
    #[cfg(unix)]
    pub(crate) fn child(&self, name: &str, directory: bool) -> Result<File, EditorError> {
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
    pub(crate) fn child(&self, _name: &str, _directory: bool) -> Result<File, EditorError> {
        Err(EditorError::new(
            "CAPABILITY_UNAVAILABLE",
            "Anchored package I/O is unavailable",
        ))
    }
    pub(crate) fn resource(&self, path: &str) -> Result<File, EditorError> {
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
    pub(crate) fn names(&self) -> Result<BTreeSet<String>, EditorError> {
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
    pub(crate) fn names(&self) -> Result<BTreeSet<String>, EditorError> {
        Err(EditorError::new(
            "CAPABILITY_UNAVAILABLE",
            "Anchored package I/O is unavailable",
        ))
    }
}
pub(crate) fn bounded(file: File, max: usize) -> Result<Vec<u8>, EditorError> {
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
pub(crate) fn verify_blob(mut file: File, blob: &ArchiveBlob) -> Result<(), EditorError> {
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
pub(crate) fn root(path: &str) -> Result<PathBuf, EditorError> {
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
pub(crate) fn publish(staging: &Path, destination: &Path) -> Result<(), EditorError> {
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
