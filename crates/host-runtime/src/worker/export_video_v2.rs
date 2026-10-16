//! Configurable output joins the original attempt/CAS/frame/encoder kernel.
use opencut_contracts::EditorError;
use opencut_media::CancellationToken;
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
) -> Result<Value, EditorError> {
    super::project_v4_video::execute_export_v2(database, root, worker, job, cancel, held)
}
