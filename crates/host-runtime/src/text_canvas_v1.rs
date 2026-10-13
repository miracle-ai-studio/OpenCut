//! Read-only canvas facts assembled from the original caller's fixed snapshot.
use super::*;
use opencut_contracts::text_canvas_v1::*;
use opencut_editor_application::{
    classic_text_frame_v2::admit_text_frame_v2,
    font_import_v1::FontImportPolicyV1,
    project_v4_frame_jobs::V4FrameJobSubmissionStore,
    text_canvas_v1::{canvas_plan_result_v1, prepare_canvas_plan_v1},
};
use opencut_native_services::classic_visual::VerifiedClassicTextResources;

pub(super) fn plan(
    host: &Automation,
    request: &TextCanvasPlanRequestV1,
) -> Result<TextCanvasPlanResultV1, EditorError> {
    let selected = host
        .app
        .fixed_v4_frame_snapshot(&request.project_id, &request.revision)?;
    let prepared = prepare_canvas_plan_v1(&selected.snapshot, request)?;
    admit_text_frame_v2(&prepared, &selected.processing_policy)?;
    let started = std::time::Instant::now();
    let checkpoint = || {
        let current = host
            .app
            .fixed_v4_frame_snapshot(&request.project_id, &request.revision)?;
        if current.processing_policy != selected.processing_policy {
            return Err(EditorError::new(
                "PERMISSION_DENIED",
                "Canvas execution policy changed",
            ));
        }
        if started.elapsed().as_millis() > u128::from(selected.processing_policy.deadline_ms) {
            return Err(EditorError::new(
                "VISUAL_BUDGET_EXCEEDED",
                "Canvas font inspection exceeded its deadline",
            ));
        }
        Ok(())
    };
    let resources = VerifiedClassicTextResources::new_fonts(
        host.app.asset_store_root(),
        &prepared,
        &FontImportPolicyV1::default(),
        &checkpoint,
    )?;
    let result = canvas_plan_result_v1(
        &prepared,
        request,
        &selected.processing_policy,
        resources.coverage(),
    )?;
    checkpoint()?;
    Ok(result)
}
