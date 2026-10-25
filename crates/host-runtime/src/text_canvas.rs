//! Canvas assembly reuses the fixed-snapshot font inspection and live fences.
use super::*;
use opencut_contracts::text_canvas::{TextCanvasPlanRequest, TextCanvasPlanResult};
use opencut_editor_application::{
    classic_text_frame::admit_text_frame,
    font_import::FontImportPolicy,
    scene_text_frame_jobs::TextFrameJobSubmissionStore,
    text_canvas::{canvas_plan_result, prepare_canvas_plan},
};
use opencut_native_services::classic_visual::VerifiedClassicTextResources;

pub(super) fn plan(
    host: &Automation,
    request: &TextCanvasPlanRequest,
) -> Result<TextCanvasPlanResult, EditorError> {
    let selected = host
        .app
        .fixed_scene_text_frame_snapshot(&request.project_id, &request.revision)?;
    let prepared = prepare_canvas_plan(&selected.snapshot, request)?;
    admit_text_frame(prepared.execution(), &selected.processing_policy)?;
    let started = std::time::Instant::now();
    let checkpoint = || {
        let current = host
            .app
            .fixed_scene_text_frame_snapshot(&request.project_id, &request.revision)?;
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
        prepared.execution(),
        &FontImportPolicy::default(),
        &checkpoint,
    )?;
    let result = canvas_plan_result(
        &prepared,
        request,
        &selected.processing_policy,
        resources.coverage(),
    )?;
    checkpoint()?;
    Ok(result)
}
