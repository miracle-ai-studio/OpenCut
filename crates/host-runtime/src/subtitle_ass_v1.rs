//! Host assembly for pure ASS plans with authorized pinned-font observations.
use super::*;
use opencut_contracts::subtitle_ass_v1::*;
use opencut_editor_application::{
    classic_text_frame_v2::{ClassicTextFrameSessionV2, admit_text_frame_v2},
    font_import_v1::FontImportPolicyV1,
    project_v4_frame_jobs::V4FrameJobSubmissionStore,
};
use opencut_native_services::classic_visual::VerifiedClassicTextResources;

pub(super) fn plan(
    host: &Automation,
    input: &SubtitleAssPlanRequestV1,
) -> Result<SubtitleAssPlanResultV1, EditorError> {
    let started = std::time::Instant::now();
    let head = || {
        super::subtitle_text_v1::require_head(
            host,
            &input.project_id,
            &input.expected_revision,
            &input.idempotency_key,
        )
    };
    head()?;
    let selected = host
        .app
        .fixed_v4_frame_snapshot(&input.project_id, &input.expected_revision)?;
    let checkpoint = || {
        head()?;
        let current = host
            .app
            .fixed_v4_frame_snapshot(&input.project_id, &input.expected_revision)?;
        if current.processing_policy != selected.processing_policy {
            return Err(EditorError::new(
                "PERMISSION_DENIED",
                "ASS execution policy changed",
            ));
        }
        if started.elapsed().as_millis() > u128::from(selected.processing_policy.deadline_ms) {
            return Err(EditorError::new(
                "VISUAL_BUDGET_EXCEEDED",
                "ASS preparation exceeded its deadline",
            ));
        }
        Ok(())
    };
    let prepared = opencut_editor_application::subtitle_ass_v1::prepare_with_checkpoint(
        &selected.snapshot,
        input,
        &mut || checkpoint(),
    )?;
    let mut layers = Vec::with_capacity(prepared.frames().len());
    for frame in prepared.frames() {
        checkpoint()?;
        admit_text_frame_v2(frame, &selected.processing_policy)?;
        let resources = VerifiedClassicTextResources::new_fonts(
            host.app.asset_store_root(),
            frame,
            &FontImportPolicyV1::default(),
            &checkpoint,
        )?;
        let plan = ClassicTextFrameSessionV2::new(
            frame,
            &selected.processing_policy,
            resources.coverage(),
        )?
        .typed_plan()?;
        if plan.texts.len() != 1 {
            return Err(EditorError::new(
                "INVALID_VISUAL_PLAN",
                "ASS measurement must have one target",
            ));
        }
        layers.extend(plan.texts);
    }
    let result = opencut_editor_application::subtitle_ass_v1::finish(&prepared, input, &layers)?;
    checkpoint()?;
    Ok(result)
}
