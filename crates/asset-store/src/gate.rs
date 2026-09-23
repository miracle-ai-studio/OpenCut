//! A directory-inode lock fences cooperating CAS publication and collection.
//! It creates no sidecar during read-only queries. Upgrade with old hosts stopped.
#[cfg(unix)]
use crate::io_error;
use crate::{EditorError, File, Path};
#[derive(Debug)]
pub struct Gate {
    _file: Option<File>,
}
impl Gate {
    pub fn shared(root: &Path) -> Result<Self, EditorError> {
        #[cfg(unix)]
        {
            let file = match File::open(root) {
                Ok(file) => file,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(Self { _file: None });
                }
                Err(e) => return Err(io_error(e)),
            };
            fs2::FileExt::lock_shared(&file).map_err(io_error)?;
            Ok(Self { _file: Some(file) })
        }
        #[cfg(not(unix))]
        {
            let _ = root;
            Ok(Self { _file: None })
        }
    }
    pub fn directory(&self) -> Result<&File, EditorError> {
        self._file.as_ref().ok_or_else(|| {
            EditorError::new("CAPABILITY_UNAVAILABLE", "Directory locking is unavailable")
        })
    }
    pub fn exclusive(root: &Path) -> Result<Self, EditorError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC)
                .open(root)
                .map_err(io_error)?;
            fs2::FileExt::try_lock_exclusive(&file).map_err(|e| {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    EditorError::new(
                        "STORAGE_BUSY",
                        "CAS publication or reading is active; retry collection later",
                    )
                } else {
                    io_error(e)
                }
            })?;
            Ok(Self { _file: Some(file) })
        }
        #[cfg(not(unix))]
        {
            let _ = root;
            Err(EditorError::new(
                "CAPABILITY_UNAVAILABLE",
                "CAS collection requires Unix directory locks",
            ))
        }
    }
}
