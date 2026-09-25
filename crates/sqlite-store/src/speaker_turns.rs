use crate::EditorApplication;
use opencut_contracts::EditorError;
use opencut_editor_application::JobRecord;
use opencut_job_contracts::SpeakerTurnsJobInput;
use std::path::Path;

pub fn start(
    app: &mut EditorApplication,
    asset_store_root: &Path,
    input: SpeakerTurnsJobInput,
) -> Result<JobRecord, EditorError> {
    opencut_editor_application::speaker_turns::submit(
        app,
        &super::jobs::native_assets(asset_store_root),
        input,
    )
}
