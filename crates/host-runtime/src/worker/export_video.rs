//! Configurable output joins the original attempt/CAS/frame/encoder kernel.
use opencut_contracts::EditorError;
use opencut_media::{CancellationToken, ProgressUpdate};
use opencut_sqlite_store::JobRecord;
use opencut_storage_budget::AttemptLease;
use serde_json::Value;
use std::{path::Path, sync::Mutex};
pub(super) fn execute(
    database: &Path,
    root: &Path,
    worker: &str,
    job: &JobRecord,
    cancel: &CancellationToken,
    held: &Mutex<Option<AttemptLease>>,
    progress: &mut dyn FnMut(ProgressUpdate),
) -> Result<Value, EditorError> {
    super::media_video::execute_export(database, root, worker, job, cancel, held, progress)
}
