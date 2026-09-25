//! V3 submissions reuse the existing jobs table and atomic acceptance transaction.
use super::*;
use opencut_api_contracts::project_v3_jobs::{SubmitV3Frame, SubmitV3Range, SubmitV3Render};
use opencut_editor_application::project_v3::AuthorizedSnapshotV3;
use opencut_editor_application::project_v3_jobs::{self, V3JobSubmissionStore};

impl V3JobSubmissionStore for EditorApplication {
    fn fixed_v3_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<AuthorizedSnapshotV3, EditorError> {
        self.execution_snapshot_v3(project_id, revision)
    }
}

pub fn start_v3_render(
    app: &mut EditorApplication,
    input: SubmitV3Render,
) -> Result<JobRecord, EditorError> {
    let root = app.asset_store_root().to_path_buf();
    project_v3_jobs::submit_render(app, &crate::jobs::native_assets(&root), input)
}
pub fn start_v3_frame(
    app: &mut EditorApplication,
    input: SubmitV3Frame,
) -> Result<JobRecord, EditorError> {
    let root = app.asset_store_root().to_path_buf();
    project_v3_jobs::submit_frame(app, &crate::jobs::native_assets(&root), input)
}
pub fn start_v3_range(
    app: &mut EditorApplication,
    input: SubmitV3Range,
) -> Result<JobRecord, EditorError> {
    let root = app.asset_store_root().to_path_buf();
    project_v3_jobs::submit_range(app, &crate::jobs::native_assets(&root), input)
}
