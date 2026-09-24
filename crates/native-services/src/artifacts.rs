//! Native read lease for a published Worker artifact.
use opencut_contracts::EditorError;
use opencut_storage_budget::AttemptLease;
use std::{
    ffi::OsString,
    fs::{self, File, Metadata},
    io::{self, Read},
    path::{Path, PathBuf},
};

pub struct ArtifactSource {
    file: File,
    _attempt: Option<AttemptLease>,
}

fn unavailable() -> EditorError {
    EditorError::new(
        "ARTIFACT_VERIFICATION_FAILED",
        "Published artifact is unavailable or has an invalid path",
    )
}

fn published_location(
    root: &Path,
    job_id: &str,
    attempt: u32,
    published: &Path,
) -> Result<(PathBuf, OsString), EditorError> {
    if job_id.is_empty()
        || job_id.len() > 128
        || !job_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        || attempt == 0
    {
        return Err(unavailable());
    }
    let root = fs::canonicalize(root).map_err(|_| unavailable())?;
    let directory = root.join(job_id).join(format!("attempt-{attempt}"));
    if fs::canonicalize(&directory).map_err(|_| unavailable())? != directory
        || fs::canonicalize(published.parent().ok_or_else(unavailable)?)
            .map_err(|_| unavailable())?
            != directory
    {
        return Err(unavailable());
    }
    let name = published
        .file_name()
        .ok_or_else(unavailable)?
        .to_os_string();
    Ok((directory, name))
}

impl ArtifactSource {
    /// The caller supplies a trusted successful JobRecord. This adapter opens
    /// only a direct file inside that job's committed attempt and holds the
    /// attempt's shared inode lock until verification finishes. Collection
    /// must acquire the same lock exclusively before deleting an attempt.
    pub fn open(
        root: &Path,
        job_id: &str,
        attempt: u32,
        published: &Path,
    ) -> Result<Self, EditorError> {
        let (directory, name) = published_location(root, job_id, attempt, published)?;
        #[cfg(unix)]
        let lease = Some(AttemptLease::acquire(&directory).map_err(|_| unavailable())?);
        #[cfg(not(unix))]
        let lease: Option<AttemptLease> = None;
        if lease
            .as_ref()
            .is_some_and(|guard| !guard.still_names(&directory).unwrap_or(false))
        {
            return Err(unavailable());
        }
        #[cfg(unix)]
        let file = lease
            .as_ref()
            .ok_or_else(unavailable)?
            .open_readonly_child(&name)
            .map_err(|_| unavailable())?;
        #[cfg(not(unix))]
        let file = File::open(directory.join(&name)).map_err(|_| unavailable())?;
        let metadata = file.metadata().map_err(|_| unavailable())?;
        if !metadata.is_file() {
            return Err(unavailable());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.nlink() != 1
                || !lease
                    .as_ref()
                    .ok_or_else(unavailable)?
                    .still_names(&directory)
                    .map_err(|_| unavailable())?
            {
                return Err(unavailable());
            }
        }
        Ok(Self {
            file,
            _attempt: lease,
        })
    }

    pub fn metadata(&self) -> io::Result<Metadata> {
        self.file.metadata()
    }
}

/// A private, exact-inode deletion candidate. A durable collector must recheck
/// all authoritative references while holding its own publication gate before
/// calling `remove_if`; this type only supplies the filesystem half of that protocol.
#[cfg(unix)]
pub struct ArtifactCandidate {
    directory: PathBuf,
    directory_device: u64,
    directory_inode: u64,
    name: OsString,
    identity: FileIdentity,
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanos: i64,
    changed_seconds: i64,
    changed_nanos: i64,
}

#[cfg(unix)]
impl FileIdentity {
    fn of(metadata: &Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            modified_seconds: metadata.mtime(),
            modified_nanos: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanos: metadata.ctime_nsec(),
        }
    }
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    Removed,
    AlreadyAbsent,
    Changed,
    Busy,
    Retained,
}

#[cfg(unix)]
impl ArtifactCandidate {
    pub fn observe(
        root: &Path,
        job_id: &str,
        attempt: u32,
        published: &Path,
    ) -> Result<Self, EditorError> {
        let source = ArtifactSource::open(root, job_id, attempt, published)?;
        let identity = FileIdentity::of(&source.metadata().map_err(|_| unavailable())?);
        let (directory, name) = published_location(root, job_id, attempt, published)?;
        use std::os::unix::fs::MetadataExt;
        let directory_metadata = fs::symlink_metadata(&directory).map_err(|_| unavailable())?;
        if !directory_metadata.is_dir()
            || !source
                ._attempt
                .as_ref()
                .ok_or_else(unavailable)?
                .still_names(&directory)
                .map_err(|_| unavailable())?
        {
            return Err(unavailable());
        }
        Ok(Self {
            directory,
            directory_device: directory_metadata.dev(),
            directory_inode: directory_metadata.ino(),
            name,
            identity,
        })
    }

    /// Nonblocking exclusive attempt lock excludes active download verification
    /// and inherited media writers. The caller must recheck durable references
    /// inside this lock, immediately before deletion. A changed file is never
    /// removed by this candidate.
    pub fn remove_if(
        &self,
        still_unreferenced: impl FnOnce() -> Result<bool, EditorError>,
    ) -> Result<Removal, EditorError> {
        let Some(guard) = AttemptLease::try_cleanup(&self.directory).map_err(|_| unavailable())?
        else {
            return match fs::symlink_metadata(&self.directory) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Removal::AlreadyAbsent),
                Ok(_) => Ok(Removal::Busy),
                Err(_) => Err(unavailable()),
            };
        };
        if !guard
            .still_names(&self.directory)
            .map_err(|_| unavailable())?
        {
            return Ok(Removal::Changed);
        }
        use std::os::unix::fs::MetadataExt;
        let directory_metadata =
            fs::symlink_metadata(&self.directory).map_err(|_| unavailable())?;
        if directory_metadata.dev() != self.directory_device
            || directory_metadata.ino() != self.directory_inode
        {
            return Ok(Removal::Changed);
        }
        let file = match guard.open_readonly_child(&self.name) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Removal::AlreadyAbsent);
            }
            Err(_) => return Ok(Removal::Changed),
        };
        let metadata = file.metadata().map_err(|_| unavailable())?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || FileIdentity::of(&metadata) != self.identity
            || !guard
                .still_names(&self.directory)
                .map_err(|_| unavailable())?
        {
            return Ok(Removal::Changed);
        }
        if !still_unreferenced()? {
            return Ok(Removal::Retained);
        }
        if !guard
            .still_names(&self.directory)
            .map_err(|_| unavailable())?
        {
            return Ok(Removal::Changed);
        }
        guard
            .unlink_child_sync(&self.name)
            .map_err(|_| unavailable())?;
        Ok(Removal::Removed)
    }
}

impl Read for ArtifactSource {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.file.read(bytes)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn published_read_holds_attempt_and_rejects_aliases() {
        let root = tempfile::tempdir().unwrap();
        let artifacts = root.path().join("artifacts");
        let attempt = artifacts.join("job_1").join("attempt-1");
        fs::create_dir_all(&attempt).unwrap();
        let output = attempt.join("frame.png");
        fs::write(&output, b"published").unwrap();
        let outside = root.path().join("outside.png");
        fs::write(&outside, b"outside").unwrap();
        assert!(ArtifactSource::open(&artifacts, "job_1", 1, &outside).is_err());
        let alias = attempt.join("alias.png");
        symlink(&output, &alias).unwrap();
        assert!(ArtifactSource::open(&artifacts, "job_1", 1, &alias).is_err());
        let mut source = ArtifactSource::open(&artifacts, "job_1", 1, &output).unwrap();
        assert!(AttemptLease::try_cleanup(&attempt).unwrap().is_none());
        let mut bytes = Vec::new();
        source.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"published");
        drop(source);
        assert!(AttemptLease::try_cleanup(&attempt).unwrap().is_some());
    }

    #[test]
    fn replacing_job_directory_cannot_redirect_a_published_read() {
        let root = tempfile::tempdir().unwrap();
        let artifacts = root.path().join("artifacts");
        let attempt = artifacts.join("job_1").join("attempt-1");
        fs::create_dir_all(&attempt).unwrap();
        let output = attempt.join("frame.png");
        fs::write(&output, b"published").unwrap();
        let moved = root.path().join("moved");
        fs::rename(artifacts.join("job_1"), &moved).unwrap();
        symlink(&moved, artifacts.join("job_1")).unwrap();
        assert!(ArtifactSource::open(&artifacts, "job_1", 1, &output).is_err());
    }

    #[test]
    fn candidate_waits_for_active_reader_then_durably_removes_only_exact_file() {
        let root = tempfile::tempdir().unwrap();
        let artifacts = root.path().join("artifacts");
        let attempt = artifacts.join("job_1").join("attempt-1");
        fs::create_dir_all(&attempt).unwrap();
        let output = attempt.join("frame.png");
        let other = attempt.join("other.png");
        fs::write(&output, b"published").unwrap();
        fs::write(&other, b"retained").unwrap();
        let candidate = ArtifactCandidate::observe(&artifacts, "job_1", 1, &output).unwrap();
        let reader = ArtifactSource::open(&artifacts, "job_1", 1, &output).unwrap();
        assert_eq!(candidate.remove_if(|| Ok(true)).unwrap(), Removal::Busy);
        assert!(output.exists());
        drop(reader);
        assert_eq!(
            candidate.remove_if(|| Ok(false)).unwrap(),
            Removal::Retained
        );
        assert!(output.exists());
        assert!(
            candidate
                .remove_if(|| Err(EditorError::new("STORAGE_ERROR", "reference scan failed")))
                .is_err()
        );
        assert!(output.exists());
        assert_eq!(candidate.remove_if(|| Ok(true)).unwrap(), Removal::Removed);
        assert_eq!(
            candidate.remove_if(|| Ok(true)).unwrap(),
            Removal::AlreadyAbsent
        );
        assert_eq!(fs::read(other).unwrap(), b"retained");
    }

    #[test]
    fn candidate_never_removes_replaced_or_aliased_file() {
        let root = tempfile::tempdir().unwrap();
        let artifacts = root.path().join("artifacts");
        let attempt = artifacts.join("job_1").join("attempt-1");
        fs::create_dir_all(&attempt).unwrap();
        let output = attempt.join("frame.png");
        fs::write(&output, b"published").unwrap();
        let candidate = ArtifactCandidate::observe(&artifacts, "job_1", 1, &output).unwrap();
        fs::remove_file(&output).unwrap();
        fs::write(&output, b"replacement").unwrap();
        assert_eq!(candidate.remove_if(|| Ok(true)).unwrap(), Removal::Changed);
        assert_eq!(fs::read(&output).unwrap(), b"replacement");
        let next = ArtifactCandidate::observe(&artifacts, "job_1", 1, &output).unwrap();
        let outside = root.path().join("outside.png");
        fs::write(&outside, b"outside").unwrap();
        fs::remove_file(&output).unwrap();
        symlink(&outside, &output).unwrap();
        assert_eq!(next.remove_if(|| Ok(true)).unwrap(), Removal::Changed);
        assert_eq!(fs::read(outside).unwrap(), b"outside");
    }

    #[test]
    fn named_pipe_cannot_block_artifact_open() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let root = tempfile::tempdir().unwrap();
        let artifacts = root.path().join("artifacts");
        let attempt = artifacts.join("job_1").join("attempt-1");
        fs::create_dir_all(&attempt).unwrap();
        let pipe = attempt.join("frame.png");
        let name = CString::new(pipe.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(ArtifactSource::open(&artifacts, "job_1", 1, &pipe).is_err());
    }

    #[test]
    fn candidate_rejects_replaced_attempt_even_when_file_inode_is_unchanged() {
        let root = tempfile::tempdir().unwrap();
        let artifacts = root.path().join("artifacts");
        let attempt = artifacts.join("job_1").join("attempt-1");
        fs::create_dir_all(&attempt).unwrap();
        let output = attempt.join("frame.png");
        fs::write(&output, b"published").unwrap();
        let candidate = ArtifactCandidate::observe(&artifacts, "job_1", 1, &output).unwrap();
        let moved = root.path().join("old-attempt");
        fs::rename(&attempt, &moved).unwrap();
        fs::create_dir(&attempt).unwrap();
        fs::rename(moved.join("frame.png"), &output).unwrap();
        assert_eq!(candidate.remove_if(|| Ok(true)).unwrap(), Removal::Changed);
        assert_eq!(fs::read(output).unwrap(), b"published");
    }
}
