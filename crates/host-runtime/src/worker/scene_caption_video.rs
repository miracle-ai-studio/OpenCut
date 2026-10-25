//! Caption profile enters the same admitted PCM/CFR/encoder attempt kernel.
use super::media_video;
use opencut_contracts::EditorError;
use opencut_editor_application::media_video_jobs::VideoFrameProfile;
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
    media_video::execute_with_profile(
        database,
        root,
        worker,
        job,
        cancel,
        held,
        VideoFrameProfile::CaptionV3,
        progress,
    )
}
