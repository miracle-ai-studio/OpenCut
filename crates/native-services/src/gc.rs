//! Bounded native CAS enumeration and durable unlink under a publication gate.
use opencut_contracts::EditorError;
use opencut_editor_application::gc::{
    self, Files, ItemState, Native, Object, Observation, PlanInput,
};
use std::path::Path;
pub struct NativeGc<'a>(pub &'a Path, pub &'a Path);
pub struct Locked {
    _gate: opencut_asset_store::gate::Gate,
}
impl Native for NativeGc<'_> {
    type Locked = Locked;
    fn exclusive(&self) -> Result<Locked, EditorError> {
        #[cfg(not(unix))]
        {
            Err(EditorError::new(
                "CAPABILITY_UNAVAILABLE",
                "CAS collection requires Unix directory locks",
            ))
        }
        #[cfg(unix)]
        {
            unix::validate_workspace(self.0, self.1)?;
            std::fs::create_dir_all(self.0).map_err(|_| failure())?;
            Ok(Locked {
                _gate: opencut_asset_store::gate::Gate::exclusive(self.0)?,
            })
        }
    }
}
fn failure() -> EditorError {
    EditorError::new(
        "STORAGE_ERROR",
        "CAS collection filesystem operation failed; inspect the plan before retrying",
    )
}
impl Files for Locked {
    fn protect_sources(
        &self,
        objects: &[Observation],
        paths: &std::collections::BTreeSet<String>,
    ) -> Result<std::collections::BTreeSet<String>, EditorError> {
        #[cfg(unix)]
        {
            unix::protect_sources(objects, paths)
        }
        #[cfg(not(unix))]
        {
            let _ = (objects, paths);
            Err(failure())
        }
    }
    fn list(&self, input: &PlanInput) -> Result<(Vec<Observation>, Option<String>), EditorError> {
        #[cfg(unix)]
        {
            unix::list(self._gate.directory()?, input)
        }
        #[cfg(not(unix))]
        {
            let _ = input;
            Err(failure())
        }
    }
    fn remove(&self, object: &Observation) -> Result<ItemState, EditorError> {
        gc::validate_object(&object.object)?;
        #[cfg(unix)]
        {
            unix::remove(self._gate.directory()?, object)
        }
        #[cfg(not(unix))]
        {
            Err(failure())
        }
    }
}
#[cfg(unix)]
mod unix {
    use super::*;
    use rustix::{
        fd::AsFd,
        fs::{self, AtFlags, FileType, Mode, OFlags, Stat},
    };
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    const FLAGS: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::CLOEXEC);
    const MAX_ENTRIES: usize = gc::MAX_NAMESPACE_ENTRIES;
    pub fn protect_sources(
        objects: &[Observation],
        paths: &std::collections::BTreeSet<String>,
    ) -> Result<std::collections::BTreeSet<String>, EditorError> {
        use std::os::unix::fs::MetadataExt;
        let mut result = std::collections::BTreeSet::new();
        let start = std::time::Instant::now();
        for path in paths {
            if start.elapsed() > std::time::Duration::from_millis(gc::MAX_REFERENCE_MS) {
                return Err(EditorError::new(
                    "RESOURCE_TOO_LARGE",
                    "Collection native reference resolution exceeded its budget",
                ));
            }
            let path = path.strip_prefix("file://").unwrap_or(path);
            if path.contains("://") {
                continue;
            }
            let metadata = match std::fs::metadata(path) {
                Ok(m) => m,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                    ) =>
                {
                    continue;
                }
                Err(_) => {
                    return Err(EditorError::new(
                        "STORAGE_ERROR",
                        "Cannot resolve a retained native source reference; collection refused",
                    ));
                }
            };
            if !metadata.is_file() {
                continue;
            }
            let identity = format!("{}:{}:", metadata.dev(), metadata.ino());
            for object in objects {
                if object.identity.starts_with(&identity) {
                    result.insert(gc::key(&object.object));
                }
            }
        }
        Ok(result)
    }

    pub fn validate_workspace(cas: &Path, database: &Path) -> Result<(), EditorError> {
        use std::io::Read;
        let unsupported = || {
            EditorError::new(
                "CAPABILITY_UNAVAILABLE",
                "CAS collection requires a dedicated workspace directory with one non-aliased SQLite database",
            )
        };
        if database == Path::new(":memory:") {
            return Err(unsupported());
        }
        let workspace = cas.parent().ok_or_else(unsupported)?;
        let folder = fs::open(workspace, FLAGS, Mode::empty()).map_err(|_| unsupported())?;
        let filename = database.file_name().ok_or_else(unsupported)?;
        let main =
            fs::statat(&folder, filename, AtFlags::SYMLINK_NOFOLLOW).map_err(|_| unsupported())?;
        if FileType::from_raw_mode(main.st_mode) != FileType::RegularFile || main.st_nlink != 1 {
            return Err(unsupported());
        }
        let mut count = 0;
        for name in names(&folder, &mut count)? {
            let meta =
                fs::statat(&folder, &name, AtFlags::SYMLINK_NOFOLLOW).map_err(|_| unsupported())?;
            let kind = FileType::from_raw_mode(meta.st_mode);
            if kind == FileType::Symlink {
                return Err(unsupported());
            }
            if kind != FileType::RegularFile
                || (meta.st_dev == main.st_dev && meta.st_ino == main.st_ino)
            {
                continue;
            }
            let fd = fs::openat(
                &folder,
                &name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| unsupported())?;
            let mut file = std::fs::File::from(fd);
            let mut header = [0u8; 16];
            match file.read_exact(&mut header) {
                Ok(()) if &header == b"SQLite format 3\0" => return Err(unsupported()),
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {}
                Err(_) => return Err(unsupported()),
            }
        }
        Ok(())
    }

    fn identity(s: &Stat) -> String {
        format!(
            "{}:{}:{}:{}:{}:{}:{}",
            s.st_dev, s.st_ino, s.st_size, s.st_mtime, s.st_mtime_nsec, s.st_ctime, s.st_ctime_nsec
        )
    }
    fn names(fd: &impl AsFd, count: &mut usize) -> Result<Vec<OsString>, EditorError> {
        let mut names = Vec::new();
        let mut dir = fs::Dir::read_from(fd).map_err(|_| failure())?;
        while let Some(entry) = dir.read() {
            let entry = entry.map_err(|_| failure())?;
            let name = entry.file_name().to_bytes();
            if name == b"." || name == b".." {
                continue;
            }
            *count += 1;
            if *count > MAX_ENTRIES {
                return Err(EditorError::new(
                    "RESOURCE_TOO_LARGE",
                    "CAS namespace exceeds collection enumeration budget",
                ));
            }
            names.push(OsString::from_vec(name.to_vec()));
        }
        names.sort();
        Ok(names)
    }
    fn observe(
        fd: &impl AsFd,
        name: &std::ffi::OsStr,
        object: Object,
    ) -> Result<Observation, EditorError> {
        let s = fs::statat(fd, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|_| failure())?;
        if FileType::from_raw_mode(s.st_mode) != FileType::RegularFile {
            return Err(EditorError::new(
                "STORAGE_ERROR",
                "Collection encountered a non-regular CAS object",
            ));
        }
        if s.st_dev != fs::fstat(fd).map_err(|_| failure())?.st_dev {
            return Err(failure());
        }
        Ok(Observation {
            object,
            size_bytes: s.st_size.to_string(),
            identity: identity(&s),
        })
    }
    pub fn list(
        root: &impl AsFd,
        input: &PlanInput,
    ) -> Result<(Vec<Observation>, Option<String>), EditorError> {
        let mut count = 0;
        let mut objects = Vec::new();
        let device = fs::fstat(root).map_err(|_| failure())?.st_dev;
        for name in names(root, &mut count)? {
            let Some(text) = name.to_str() else { continue };
            if let Some(id) = text.strip_prefix(".capture-") {
                let object = Object::Capture {
                    capture_id: id.into(),
                };
                if gc::validate_object(&object).is_ok() {
                    objects.push(observe(root, &name, object)?);
                }
            } else if text.len() == 2
                && text
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                let folder =
                    fs::openat(root, &name, FLAGS, Mode::empty()).map_err(|_| failure())?;
                if fs::fstat(&folder).map_err(|_| failure())?.st_dev != device {
                    return Err(EditorError::new(
                        "STORAGE_ERROR",
                        "Collection refuses a mounted CAS prefix",
                    ));
                }
                for name in names(&folder, &mut count)? {
                    let Some(hash) = name.to_str() else { continue };
                    if gc::valid_hash(hash) && hash.starts_with(text) {
                        objects.push(observe(
                            &folder,
                            &name,
                            Object::Content {
                                content_hash: hash.into(),
                            },
                        )?);
                    }
                }
            }
        }
        objects.sort_by_key(|o| gc::key(&o.object));
        if let Some(after) = &input.after {
            objects.retain(|o| gc::key(&o.object) > *after);
        }
        let more = objects.len() > input.limit;
        objects.truncate(input.limit);
        let next = if more {
            objects.last().map(|o| gc::key(&o.object))
        } else {
            None
        };
        Ok((objects, next))
    }
    fn unlink(
        parent: &impl AsFd,
        name: &str,
        observation: &Observation,
    ) -> Result<ItemState, EditorError> {
        let state = match fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW) {
            Err(rustix::io::Errno::NOENT) => ItemState::AlreadyAbsent,
            Err(_) => return Err(failure()),
            Ok(s) => {
                if FileType::from_raw_mode(s.st_mode) != FileType::RegularFile
                    || identity(&s) != observation.identity
                {
                    return Ok(ItemState::Changed);
                }
                fs::unlinkat(parent, name, AtFlags::empty()).map_err(|_| failure())?;
                ItemState::Removed
            }
        };
        // Also sync absence on recovery after unlink but before directory fsync/receipt.
        fs::fsync(parent).map_err(|_| failure())?;
        Ok(state)
    }
    pub fn remove(root: &impl AsFd, observation: &Observation) -> Result<ItemState, EditorError> {
        match &observation.object {
            Object::Capture { capture_id } => {
                unlink(root, &format!(".capture-{capture_id}"), observation)
            }
            Object::Content { content_hash } => {
                let folder = match fs::openat(root, &content_hash[..2], FLAGS, Mode::empty()) {
                    Ok(fd) => fd,
                    Err(rustix::io::Errno::NOENT) => {
                        fs::fsync(root).map_err(|_| failure())?;
                        return Ok(ItemState::AlreadyAbsent);
                    }
                    Err(_) => return Err(failure()),
                };
                if fs::fstat(&folder).map_err(|_| failure())?.st_dev
                    != fs::fstat(root).map_err(|_| failure())?.st_dev
                {
                    return Err(failure());
                }
                unlink(&folder, content_hash, observation)
            }
        }
    }
}
