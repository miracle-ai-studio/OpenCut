//! Caption profile enters the same admitted PCM/CFR/encoder attempt kernel.
use super::project_v4_video;
use opencut_contracts::EditorError;
use opencut_editor_application::project_v4_video_jobs::VideoFrameProfile;
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
    project_v4_video::execute_with_profile(
        database,
        root,
        worker,
        job,
        cancel,
        held,
        VideoFrameProfile::CaptionV3,
    )
}
