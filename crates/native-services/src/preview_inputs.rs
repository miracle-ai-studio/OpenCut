//! Private preview inputs share an inode lease with every decoder process.
use opencut_contracts::EditorError;
use opencut_storage_budget::AttemptLease;
use std::path::{Path, PathBuf};

pub(crate) struct PreviewInputs {
    // Remove files while the lease is still held.
    directory: tempfile::TempDir,
    pub(crate) lease: AttemptLease,
}
fn error(_: impl std::fmt::Display) -> EditorError {
    EditorError::new(
        "STORAGE_ERROR",
        "Preview input storage could not be accessed safely",
    )
}
fn directory(workspace: &Path) -> Result<PathBuf, EditorError> {
    let root = workspace.join("previews");
    match std::fs::create_dir(&root) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(error(e)),
    }
    if !std::fs::symlink_metadata(&root)
        .map_err(error)?
        .file_type()
        .is_dir()
    {
        return Err(error("not a directory"));
    }
    Ok(root)
}
impl PreviewInputs {
    pub(crate) fn create(workspace: &Path) -> Result<Self, EditorError> {
        let root = directory(workspace)?;
        // The parent lease closes the creation-before-child-lease recovery gap.
        let parent = AttemptLease::acquire(&root).map_err(error)?;
        if !parent.still_names(&root).map_err(error)? {
            return Err(error("replaced root"));
        }
        let directory = tempfile::Builder::new()
            .prefix(".opencut-preview-")
            .tempdir_in(&root)
            .map_err(error)?;
        let lease = AttemptLease::acquire(directory.path()).map_err(error)?;
        Ok(Self { directory, lease })
    }
    pub(crate) fn path(&self) -> &Path {
        self.directory.path()
    }
}
/// Reclaims only this managed namespace and only after the creator and all
/// inherited decoder leases are gone. Live sessions in other hosts survive.
pub(crate) fn recover(workspace: &Path) -> Result<usize, EditorError> {
    let root = directory(workspace)?;
    let Some(parent) = AttemptLease::try_cleanup(&root).map_err(error)? else {
        return Ok(0);
    };
    if !parent.still_names(&root).map_err(error)? {
        return Err(error("replaced root"));
    }
    let mut removed = 0;
    for entry in std::fs::read_dir(&root).map_err(error)? {
        let entry = entry.map_err(error)?;
        let name = entry.file_name();
        let Some(suffix) = name
            .to_str()
            .and_then(|n| n.strip_prefix(".opencut-preview-"))
        else {
            continue;
        };
        if suffix.is_empty()
            || suffix.len() > 32
            || !suffix.bytes().all(|b| b.is_ascii_alphanumeric())
            || !entry.file_type().map_err(error)?.is_dir()
        {
            continue;
        }
        let path = entry.path();
        let Some(lease) = AttemptLease::try_cleanup(&path).map_err(error)? else {
            continue;
        };
        if lease.still_names(&path).map_err(error)? {
            std::fs::remove_dir_all(&path).map_err(error)?;
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn recovery_preserves_live_inputs_and_inherited_decoder_lease() {
        let workspace = tempfile::tempdir().unwrap();
        let inputs = PreviewInputs::create(workspace.path()).unwrap();
        std::fs::write(inputs.path().join("source"), b"private media").unwrap();
        assert_eq!(recover(workspace.path()).unwrap(), 0);
        let mut command = std::process::Command::new("/bin/sleep");
        command.arg("30");
        let child = inputs.lease.clone().spawn_readonly(command).unwrap();
        let PreviewInputs { directory, lease } = inputs;
        let path = directory.keep();
        drop(lease);
        assert_eq!(recover(workspace.path()).unwrap(), 0);
        assert!(path.join("source").is_file());
        drop(child);
        assert_eq!(recover(workspace.path()).unwrap(), 1);
        assert!(!path.exists());
    }
    #[test]
    fn recovery_ignores_foreign_entries_and_symlinks() {
        let workspace = tempfile::tempdir().unwrap();
        let root = directory(workspace.path()).unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("keep"), b"keep").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join(".opencut-preview-link")).unwrap();
        std::fs::create_dir(root.join("foreign")).unwrap();
        assert_eq!(recover(workspace.path()).unwrap(), 0);
        assert!(outside.path().join("keep").is_file());
        assert!(root.join("foreign").is_dir());
    }
}
