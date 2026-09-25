//! Native I/O port. No workspace, project, database or job model crosses here.
use crate::MediaResult;
use std::{
    fmt::Debug,
    path::Path,
    process::{Child, Command},
};
pub trait OutputProcess {
    fn child(&mut self) -> &mut Child;
    fn output_limit(&self) -> Option<u64>;
}
pub trait OutputAdmission: Debug + Send + Sync {
    fn spawn(&self, command: Command, output: Option<&Path>)
    -> MediaResult<Box<dyn OutputProcess>>;
    fn write(&self, path: &Path, bytes: &[u8]) -> MediaResult<()>;
    /// Hold quota for a private stream-copied input until the caller drops it.
    fn reserve_private_copy(&self, _path: &Path, _bytes: u64) -> MediaResult<Box<dyn Send>> {
        Err(crate::MediaError::Unsupported(
            "Private input snapshots are unavailable".into(),
        ))
    }
}
pub(crate) struct UnmanagedProcess(pub Child);
impl OutputProcess for UnmanagedProcess {
    fn child(&mut self) -> &mut Child {
        &mut self.0
    }
    fn output_limit(&self) -> Option<u64> {
        None
    }
}
impl Drop for UnmanagedProcess {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
