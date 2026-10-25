//! Native assembly for read-only subtitle authoring. Public apply owns writes.
use super::*;
use opencut_contracts::{project_public::ApplyProject, subtitle_text::*};
use opencut_editor_application::{
    classic_text_frame::{ClassicTextFrameSession, admit_text_frame},
    font_import::FontImportPolicy,
    scene::{SceneCommitIntent, SceneRead},
    scene_text_frame_jobs::TextFrameJobSubmissionStore,
};
use opencut_native_services::classic_visual::VerifiedClassicTextResources;

pub(super) fn require_head(
    host: &Automation,
    project_id: &str,
    expected_revision: &str,
    idempotency_key: &str,
) -> Result<(), EditorError> {
    let intent = ApplyProject {
        branch_name: None,
        project_id: project_id.into(),
        expected_revision: expected_revision.into(),
        idempotency_key: idempotency_key.into(),
        label: "Create editable subtitles".into(),
        operations: vec![],
    };
    host.app
        .authorize_project(SceneCommitIntent::PublicApply(&intent))?;
    let head = host.app.head_scene_source(project_id)?;
    if head.revision != expected_revision {
        return Err(EditorError::new(
            "REVISION_CONFLICT",
            "Subtitle authoring requires its original V4 head",
        ));
    }
    Ok(())
}

pub(super) fn plan(
    host: &Automation,
    input: &SubtitleTextPlanRequest,
) -> Result<SubtitleTextPlanResult, EditorError> {
    let started = std::time::Instant::now();
    require_head(
        host,
        &input.project_id,
        &input.expected_revision,
        &input.idempotency_key,
    )?;
    let selected = host
        .app
        .fixed_scene_text_frame_snapshot(&input.project_id, &input.expected_revision)?;
    let transcript = host.app.subtitle_text_transcript(input)?;
    let checkpoint = || {
        require_head(
            host,
            &input.project_id,
            &input.expected_revision,
            &input.idempotency_key,
        )?;
        let current = host
            .app
            .fixed_scene_text_frame_snapshot(&input.project_id, &input.expected_revision)?;
        if current.processing_policy != selected.processing_policy {
            return Err(EditorError::new(
                "PERMISSION_DENIED",
                "Subtitle execution policy changed",
            ));
        }
        if started.elapsed().as_millis() > u128::from(selected.processing_policy.deadline_ms) {
            return Err(EditorError::new(
                "VISUAL_BUDGET_EXCEEDED",
                "Subtitle preparation exceeded its deadline",
            ));
        }
        Ok(())
    };
    let prepared = opencut_editor_application::subtitle_text::prepare_with_checkpoint(
        &selected.snapshot,
        input,
        transcript.as_ref(),
        &mut || checkpoint(),
    )?;
    let mut layers = Vec::with_capacity(prepared.frames().len());
    for frame in prepared.frames() {
        checkpoint()?;
        admit_text_frame(frame, &selected.processing_policy)?;
        let resources = VerifiedClassicTextResources::new_subtitle_authoring_fonts(
            host.app.asset_store_root(),
            frame,
            &FontImportPolicy::default(),
            &checkpoint,
        )?;
        let plan =
            ClassicTextFrameSession::new(frame, &selected.processing_policy, resources.coverage())?
                .typed_plan()?;
        if plan.texts.len() != 1 {
            return Err(EditorError::new(
                "INVALID_VISUAL_PLAN",
                "Subtitle measurement must have one target",
            ));
        }
        layers.extend(plan.texts);
    }
    let result = opencut_editor_application::subtitle_text::finish(&prepared, input, &layers)?;
    // Original transcript authority can be revoked independently of project edit.
    host.app.subtitle_text_transcript(input)?;
    checkpoint()?;
    Ok(result)
}

pub(super) fn export(
    host: &Automation,
    input: &SubtitleTextExport,
) -> Result<opencut_api_contracts::subtitle_exchange::SubtitleExportReceipt, EditorError> {
    let fixed = opencut_editor_application::scene::inspect(
        &host.app,
        &opencut_contracts::project_inspection::InspectProject {
            project_id: input.project_id.clone(),
            revision: Some(input.revision.clone()),
        },
    )?;
    opencut_editor_application::subtitle_text_export::export(&fixed, input)
}
