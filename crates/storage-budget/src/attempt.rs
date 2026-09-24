//! Inode lifetime guard for private attempts, inherited by admitted writers.
use std::{
    ffi::OsStr,
    fs::File,
    io,
    path::{Component, Path},
    sync::Arc,
};
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
    pub fn still_names(&self, path: &Path) -> io::Result<bool> {
        same_directory(&self.0, path)
    }
    /// Open a direct child of the locked attempt inode, never following a
    /// replacement of the attempt path or a symbolic link at the file name.
    pub fn open_readonly_child(&self, name: &OsStr) -> io::Result<File> {
        let mut components = Path::new(name).components();
        if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid attempt child name",
            ));
        }
        #[cfg(unix)]
        {
            use rustix::fs::{Mode, OFlags, openat};
            let fd = openat(
                self.0.as_ref(),
                Path::new(name),
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )?;
            Ok(fd.into())
        }
        #[cfg(not(unix))]
        {
            let _ = name;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Attempt child reads require Unix",
            ))
        }
    }
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
        same_directory(&self.0, path)
    }
}
fn same_directory(file: &File, path: &Path) -> io::Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let opened = file.metadata()?;
        let named = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        Ok(named.is_dir() && opened.dev() == named.dev() && opened.ino() == named.ino())
    }
    #[cfg(not(unix))]
    {
        let _ = (file, path);
        Ok(false)
    }
}
