//! Native assembly for read-only subtitle authoring. Public apply owns writes.
use super::*;
use opencut_contracts::{project_v4_public::ApplyPublicV4Project, subtitle_text_v1::*};
use opencut_editor_application::{
    classic_text_frame_v2::{ClassicTextFrameSessionV2, admit_text_frame_v2},
    font_import_v1::FontImportPolicyV1,
    project_v4::{ProjectV4Read, V4Intent},
    project_v4_frame_jobs::V4FrameJobSubmissionStore,
};
use opencut_native_services::classic_visual::VerifiedClassicTextResources;

pub(super) fn require_head(
    host: &Automation,
    project_id: &str,
    expected_revision: &str,
    idempotency_key: &str,
) -> Result<(), EditorError> {
    let intent = ApplyPublicV4Project {
        project_id: project_id.into(),
        expected_revision: expected_revision.into(),
        idempotency_key: idempotency_key.into(),
        label: "Create editable subtitles".into(),
        operations: vec![],
    };
    host.app.authorize_v4(V4Intent::PublicApply(&intent))?;
    let head = host.app.head_v4_source(project_id)?;
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
    input: &SubtitleTextPlanRequestV1,
) -> Result<SubtitleTextPlanResultV1, EditorError> {
    let started = std::time::Instant::now();
    require_head(
        host,
        &input.project_id,
        &input.expected_revision,
        &input.idempotency_key,
    )?;
    let selected = host
        .app
        .fixed_v4_frame_snapshot(&input.project_id, &input.expected_revision)?;
    let transcript = host.app.subtitle_text_transcript_v1(input)?;
    let checkpoint = || {
        require_head(
            host,
            &input.project_id,
            &input.expected_revision,
            &input.idempotency_key,
        )?;
        let current = host
            .app
            .fixed_v4_frame_snapshot(&input.project_id, &input.expected_revision)?;
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
    let prepared = opencut_editor_application::subtitle_text_v1::prepare_with_checkpoint(
        &selected.snapshot,
        input,
        transcript.as_ref(),
        &mut || checkpoint(),
    )?;
    let mut layers = Vec::with_capacity(prepared.frames().len());
    for frame in prepared.frames() {
        checkpoint()?;
        admit_text_frame_v2(frame, &selected.processing_policy)?;
        let resources = VerifiedClassicTextResources::new_subtitle_authoring_fonts_v1(
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
                "Subtitle measurement must have one target",
            ));
        }
        layers.extend(plan.texts);
    }
    let result = opencut_editor_application::subtitle_text_v1::finish(&prepared, input, &layers)?;
    // Original transcript authority can be revoked independently of project edit.
    host.app.subtitle_text_transcript_v1(input)?;
    checkpoint()?;
    Ok(result)
}

pub(super) fn export(
    host: &Automation,
    input: &SubtitleTextExportV1,
) -> Result<opencut_api_contracts::subtitle_exchange::SubtitleExportReceipt, EditorError> {
    let fixed = opencut_editor_application::project_v4::inspect(
        &host.app,
        &opencut_contracts::project_v4_ops::InspectV4Project {
            project_id: input.project_id.clone(),
            revision: Some(input.revision.clone()),
        },
    )?;
    opencut_editor_application::subtitle_text_export_v1::export(&fixed, input)
}
