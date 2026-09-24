//! Native read lease for a published Worker artifact.
use opencut_contracts::EditorError;
use opencut_storage_budget::AttemptLease;
use std::{
    fs::{self, File, Metadata},
    io::{self, Read},
    path::Path,
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
        #[cfg(unix)]
        let lease = Some(AttemptLease::acquire(&directory).map_err(|_| unavailable())?);
        #[cfg(not(unix))]
        let lease: Option<AttemptLease> = None;
        if fs::canonicalize(&directory).map_err(|_| unavailable())? != directory
            || fs::canonicalize(published.parent().ok_or_else(unavailable)?)
                .map_err(|_| unavailable())?
                != directory
        {
            return Err(unavailable());
        }
        let name = published.file_name().ok_or_else(unavailable)?;
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
            .open_readonly_child(name)
            .map_err(|_| unavailable())?;
        #[cfg(not(unix))]
        let file = File::open(directory.join(name)).map_err(|_| unavailable())?;
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
}
