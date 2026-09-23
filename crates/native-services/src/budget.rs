//! Host workspace selection; the durable native ledger is transport independent.
use opencut_contracts::EditorError;
use opencut_editor_application::{
    audit::ExecutionContext,
    budget::{
        BudgetNative, Configure, PolicyChange, RepairAction, RepairChange, RepairRequest, Report,
    },
};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
pub struct NativeBudget<'a>(pub &'a Path);
impl NativeBudget<'_> {
    fn native(&self) -> Result<opencut_storage_budget::Budget, EditorError> {
        if self.0 == Path::new(":memory:") {
            return Err(EditorError::new(
                "CAPABILITY_UNAVAILABLE",
                "Storage admission requires a file-backed workspace",
            ));
        }
        Ok(opencut_storage_budget::Budget::new(
            self.0
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new(".")),
        ))
    }
}
impl BudgetNative for NativeBudget<'_> {
    fn inspect(&self, recover: bool) -> Result<Report, EditorError> {
        self.native()?.inspect(recover)
    }
    fn configure(
        &self,
        input: Configure,
        context: &ExecutionContext,
    ) -> Result<PolicyChange, EditorError> {
        self.native()?.configure(input, context)
    }
    fn repair_transition(
        &self,
        action: RepairAction,
        input: RepairRequest,
        context: &ExecutionContext,
    ) -> Result<RepairChange, EditorError> {
        opencut_editor_application::budget::BudgetNative::repair_transition(
            &self.native()?,
            action,
            input,
            context,
        )
    }
}

/// Synchronous native file ownership. Run construction, I/O and destruction on
/// a blocking executor. The file closes and named staging is removed before its
/// lifetime reservation is released, including error and cancellation paths.
pub struct AdmittedFile {
    file: Option<File>,
    cleanup: Option<PathBuf>,
    reservation: Option<opencut_storage_budget::Reservation>,
    written: u64,
}
fn io(error: impl std::fmt::Display) -> EditorError {
    EditorError::new("STORAGE_ERROR", error.to_string())
}
impl AdmittedFile {
    pub fn create(
        database: &Path,
        path: &Path,
        category: &str,
        ceiling: u64,
        anonymous: bool,
    ) -> Result<Self, EditorError> {
        #[cfg(unix)]
        let reservation = Some(
            NativeBudget(database)
                .native()?
                .reserve(category, ceiling)?,
        );
        #[cfg(not(unix))]
        let reservation = {
            let _ = (database, category, ceiling);
            None
        };
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(io)?;
        let mut owned = Self {
            file: Some(file),
            cleanup: Some(path.into()),
            reservation,
            written: 0,
        };
        if anonymous {
            std::fs::remove_file(path).map_err(io)?;
            owned.cleanup = None;
        }
        Ok(owned)
    }
    pub fn append(&mut self, bytes: &[u8]) -> Result<(), EditorError> {
        let size = self
            .written
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| io("File size overflow"))?;
        if let Some(reservation) = &mut self.reservation {
            reservation.ensure(size)?;
        }
        self.file
            .as_mut()
            .expect("owned file")
            .write_all(bytes)
            .map_err(io)?;
        self.written = size;
        Ok(())
    }
    pub fn sync(&self) -> Result<(), EditorError> {
        self.file
            .as_ref()
            .expect("owned file")
            .sync_all()
            .map_err(io)
    }
    pub fn rewind(&mut self) -> Result<(), EditorError> {
        self.file
            .as_mut()
            .expect("owned file")
            .seek(SeekFrom::Start(0))
            .map_err(io)?;
        Ok(())
    }
    pub fn read_chunk(&mut self) -> Result<Vec<u8>, EditorError> {
        let mut bytes = vec![0; 65536];
        let count = self
            .file
            .as_mut()
            .expect("owned file")
            .read(&mut bytes)
            .map_err(io)?;
        bytes.truncate(count);
        Ok(bytes)
    }
}
impl Drop for AdmittedFile {
    fn drop(&mut self) {
        drop(self.file.take());
        if let Some(path) = self.cleanup.take() {
            let _ = std::fs::remove_file(path);
        }
        drop(self.reservation.take());
    }
}
