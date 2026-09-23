//! Inode lifetime guard for private attempts, inherited by admitted writers.
use std::{fs::File, io, path::Path, sync::Arc};
#[derive(Clone, Debug)]
pub struct AttemptLease(pub(crate) Arc<File>);
pub struct CleanupLease(File);
#[cfg(unix)]
fn open(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}
#[cfg(not(unix))]
fn open(_: &Path) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Attempt lifetime locking requires Unix",
    ))
}
impl AttemptLease {
    pub fn spawn_readonly(
        self,
        command: std::process::Command,
    ) -> io::Result<crate::ReservedChild> {
        crate::child::spawn(None, command, Some(self))
    }
    pub fn acquire(path: &Path) -> io::Result<Self> {
        let file = open(path)?;
        fs2::FileExt::lock_shared(&file)?;
        Ok(Self(Arc::new(file)))
    }
    pub fn try_cleanup(path: &Path) -> io::Result<Option<CleanupLease>> {
        let file = match open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        match fs2::FileExt::try_lock_exclusive(&file) {
            Ok(()) => Ok(Some(CleanupLease(file))),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error),
        }
    }
}
impl CleanupLease {
    pub fn still_names(&self, path: &Path) -> io::Result<bool> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let opened = self.0.metadata()?;
            let named = match std::fs::symlink_metadata(path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
                Err(error) => return Err(error),
            };
            Ok(named.is_dir() && opened.dev() == named.dev() && opened.ino() == named.ino())
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Ok(false)
        }
    }
}
