//! Owner-only native SQLite observation and explicit WAL maintenance.
use super::*;
use opencut_editor_application::database::{self, DatabaseMaintenance};

fn file_bytes(connection: &Connection, suffix: &str) -> Option<String> {
    let name = connection.path().filter(|name| !name.is_empty())?;
    let path = format!("{name}{suffix}");
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Some(metadata.len().to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some("0".into()),
        _ => None,
    }
}
fn number(connection: &Connection, pragma: &str) -> Result<u64, EditorError> {
    connection
        .query_row(pragma, [], |row| row.get(0))
        .map_err(storage_error)
}
impl DatabaseMaintenance for EditorApplication {
    fn inspect_database_storage(&self) -> Result<database::Report, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        let c = &self.connection.0;
        let page_size = number(c, "PRAGMA main.page_size")?;
        let pages = number(c, "PRAGMA main.page_count")?;
        let bytes = page_size.checked_mul(pages).ok_or_else(|| {
            EditorError::new("STORAGE_ERROR", "Database page arithmetic overflow")
        })?;
        Ok(database::Report {
            sqlite_version: c
                .query_row("SELECT sqlite_version()", [], |r| r.get(0))
                .map_err(storage_error)?,
            persistent: c.path().is_some_and(|name| !name.is_empty()),
            journal_mode: c
                .query_row("PRAGMA main.journal_mode", [], |r| r.get(0))
                .map_err(storage_error)?,
            page_size_bytes: page_size,
            page_count: pages.to_string(),
            freelist_pages: number(c, "PRAGMA main.freelist_count")?.to_string(),
            max_page_count: number(c, "PRAGMA main.max_page_count")?.to_string(),
            logical_database_bytes: bytes.to_string(),
            database_file_bytes: file_bytes(c, ""),
            wal_file_bytes: file_bytes(c, "-wal"),
            shm_file_bytes: file_bytes(c, "-shm"),
            connection_busy_timeout_ms: number(c, "PRAGMA busy_timeout")?,
            connection_wal_autocheckpoint_pages: number(c, "PRAGMA wal_autocheckpoint")?,
            consistency: "live_non_atomic".into(),
            enforces_total_storage_limit: false,
            managed_vfs_enabled: self.managed_vfs,
        })
    }
    fn checkpoint_database(
        &self,
        input: &database::CheckpointInput,
    ) -> Result<database::CheckpointReport, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        if input.busy_timeout_ms > 5000 {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Checkpoint busy_timeout_ms must be in 0..5000",
            ));
        }
        let c = &self.connection.0;
        let previous = number(c, "PRAGMA busy_timeout")?;
        c.busy_timeout(Duration::from_millis(input.busy_timeout_ms))
            .map_err(storage_error)?;
        let statement = match input.mode {
            database::CheckpointMode::Passive => "PRAGMA main.wal_checkpoint(PASSIVE)",
            database::CheckpointMode::Truncate => "PRAGMA main.wal_checkpoint(TRUNCATE)",
        };
        let result: rusqlite::Result<(i64, i64, i64)> =
            c.query_row(statement, [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)));
        // Restore the original connection policy even when the checkpoint fails.
        let restored = c.busy_timeout(Duration::from_millis(previous));
        let (busy, frames, copied) = result.map_err(storage_error)?;
        restored.map_err(storage_error)?;
        let outcome = if frames < 0 || copied < 0 {
            database::CheckpointOutcome::NotApplicable
        } else if busy != 0 || copied < frames {
            database::CheckpointOutcome::Blocked
        } else {
            database::CheckpointOutcome::Complete
        };
        Ok(database::CheckpointReport {
            mode: input.mode,
            outcome,
            log_frames: u64::try_from(frames).ok(),
            checkpointed_frames: u64::try_from(copied).ok(),
            observed_wal_file_bytes: file_bytes(c, "-wal"),
        })
    }
}
