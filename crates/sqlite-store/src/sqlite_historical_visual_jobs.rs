//! V3 submissions reuse the existing jobs table and atomic acceptance transaction.
use super::*;
use opencut_editor_application::historical_visual::HistoricalVisualAuthorizedSnapshot;
use opencut_editor_application::historical_visual_jobs::{
    self, HistoricalVisualJobSubmissionStore,
};
use opencut_job_contracts::historical_visual::{
    HistoricalVisualSubmitFrame, HistoricalVisualSubmitRange, HistoricalVisualSubmitRender,
};

impl HistoricalVisualJobSubmissionStore for EditorApplication {
    fn fixed_historical_visual_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<HistoricalVisualAuthorizedSnapshot, EditorError> {
        self.execution_snapshot_historical_visual(project_id, revision)
    }
}

pub fn start_historical_visual_render(
    app: &mut EditorApplication,
    input: HistoricalVisualSubmitRender,
) -> Result<JobRecord, EditorError> {
    let root = app.asset_store_root().to_path_buf();
    historical_visual_jobs::submit_render(app, &crate::jobs::native_assets(&root), input)
}

pub fn start_sequence_transcribe(
    app: &mut EditorApplication,
    input: opencut_job_contracts::SceneTranscriptionRequest,
) -> Result<JobRecord, EditorError> {
    let root = app.asset_store_root().to_path_buf();
    historical_visual_jobs::submit_sequence_transcribe(
        app,
        &crate::jobs::native_assets(&root),
        input,
    )
}
pub fn start_historical_visual_frame(
    app: &mut EditorApplication,
    input: HistoricalVisualSubmitFrame,
) -> Result<JobRecord, EditorError> {
    let root = app.asset_store_root().to_path_buf();
    historical_visual_jobs::submit_frame(app, &crate::jobs::native_assets(&root), input)
}
pub fn start_historical_visual_range(
    app: &mut EditorApplication,
    input: HistoricalVisualSubmitRange,
) -> Result<JobRecord, EditorError> {
    let root = app.asset_store_root().to_path_buf();
    historical_visual_jobs::submit_range(app, &crate::jobs::native_assets(&root), input)
}
