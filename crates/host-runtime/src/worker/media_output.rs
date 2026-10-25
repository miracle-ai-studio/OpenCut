//! Workspace admission adapter for the media backend's native resource port.
use opencut_editor_application::budget::Writer;
use opencut_media::{MediaError, MediaResult, OutputAdmission, OutputProcess};
use opencut_storage_budget::{AttemptLease, Budget, ReservedChild};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command},
};

#[derive(Debug)]
pub(super) struct TaskOutput {
    pub budget: Budget,
    pub attempt: AttemptLease,
    pub directory: PathBuf,
    pub writer: Writer,
}
fn admission(error: opencut_contracts::EditorError) -> MediaError {
    MediaError::Admission {
        code: error.code,
        message: error.message,
    }
}
impl TaskOutput {
    fn check(&self, path: &Path) -> MediaResult<()> {
        if path.parent() != Some(self.directory.as_path()) {
            return Err(MediaError::InvalidRequest(
                "Output must belong to the active private attempt".into(),
            ));
        }
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
            Ok(_) => Err(MediaError::OutputExists(
                "Private output already exists".into(),
            )),
        }
    }
}
struct Process {
    inner: ReservedChild,
    ceiling: Option<u64>,
}
impl OutputProcess for Process {
    fn child(&mut self) -> &mut Child {
        self.inner.process()
    }
    fn output_limit(&self) -> Option<u64> {
        self.ceiling
    }
}
impl OutputAdmission for TaskOutput {
    fn reserve_private_copy(&self, path: &Path, bytes: u64) -> MediaResult<Box<dyn Send>> {
        self.check(path)?;
        let reservation = self
            .budget
            .reserve("whisper_model_snapshot", bytes)
            .map_err(admission)?;
        Ok(Box::new(reservation))
    }
    fn spawn(
        &self,
        command: Command,
        output: Option<&Path>,
    ) -> MediaResult<Box<dyn OutputProcess>> {
        let Some(output) = output else {
            return Ok(Box::new(Process {
                inner: self.attempt.clone().spawn_readonly(command)?,
                ceiling: None,
            }));
        };
        self.check(output)?;
        let reserved = self
            .budget
            .reserve_output(self.writer.clone())
            .map_err(admission)?;
        let ceiling = reserved.reserved_bytes();
        let inner = reserved.spawn_in_attempt(command, self.attempt.clone())?;
        Ok(Box::new(Process {
            inner,
            ceiling: Some(ceiling),
        }))
    }
    fn write(&self, path: &Path, bytes: &[u8]) -> MediaResult<()> {
        self.check(path)?;
        let _reservation = self
            .budget
            .reserve_caption(bytes.len() as u64, self.writer.clone())
            .map_err(admission)?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    }
}

/// Tracks actual child lifetime for an owning worker's captured RSS monitor.
#[derive(Debug)]
pub(super) struct TrackedTaskOutput {
    pub inner: std::sync::Arc<TaskOutput>,
    pub pids: std::sync::Arc<std::sync::Mutex<std::collections::BTreeSet<u32>>>,
}
struct TrackedProcess {
    inner: Option<Box<dyn OutputProcess>>,
    pid: u32,
    pids: std::sync::Arc<std::sync::Mutex<std::collections::BTreeSet<u32>>>,
}
impl OutputProcess for TrackedProcess {
    fn child(&mut self) -> &mut Child {
        self.inner.as_mut().expect("owned process").child()
    }
    fn output_limit(&self) -> Option<u64> {
        self.inner.as_ref().expect("owned process").output_limit()
    }
}
impl Drop for TrackedProcess {
    fn drop(&mut self) {
        drop(self.inner.take());
        if let Ok(mut pids) = self.pids.lock() {
            pids.remove(&self.pid);
        }
    }
}
impl OutputAdmission for TrackedTaskOutput {
    fn spawn(
        &self,
        command: Command,
        output: Option<&Path>,
    ) -> MediaResult<Box<dyn OutputProcess>> {
        let mut inner = self.inner.spawn(command, output)?;
        let pid = inner.child().id();
        self.pids
            .lock()
            .map_err(|_| {
                MediaError::InvalidRequest("Process observation registry unavailable".into())
            })?
            .insert(pid);
        Ok(Box::new(TrackedProcess {
            inner: Some(inner),
            pid,
            pids: self.pids.clone(),
        }))
    }
    fn write(&self, path: &Path, bytes: &[u8]) -> MediaResult<()> {
        self.inner.write(path, bytes)
    }
    fn reserve_private_copy(&self, path: &Path, bytes: u64) -> MediaResult<Box<dyn Send>> {
        self.inner.reserve_private_copy(path, bytes)
    }
}
