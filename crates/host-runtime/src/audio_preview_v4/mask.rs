//! Box-mask frame binding on the original fixed consumer session.
use super::blend::complete_coverage;
use super::text::CachedTextCoverage;
use super::*;
use opencut_api_contracts::visual_preview_mask_v1::*;
use opencut_editor_application::{
    classic_mask_frame_v1::admit_mask_frame_v1, font_import_v1::FontImportPolicyV1,
    visual_preview_mask_v1::PreparedMaskPictureV1, visual_preview_v4::validate_picture_time,
};
use opencut_native_services::classic_visual::VerifiedClassicTextResources;
pub(crate) fn mask_picture(
    host: &mut Automation,
    input: VisualPreviewMaskFrameV1,
) -> Result<VisualPreviewMaskFrameResultV1, EditorError> {
    let e = entry(&host.database, &input.session.session_id)?;
    authorize(host, &e, true)?;
    let (view, cached, coverage) = {
        let state = lock(&e.state)?;
        state
            .machine
            .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
        let cached = state
            .picture
            .as_ref()
            .filter(|c| {
                input.decoded.is_some()
                    && c.session == input.session
                    && input.time.as_ref() == Some(c.prepared.time())
            })
            .and_then(|c| match &c.prepared {
                PreparedPicture::Mask(prepared) => Some(prepared.clone()),
                _ => None,
            });
        let coverage = state
            .text_coverage
            .as_ref()
            .filter(|c| c.session == input.session)
            .map(|c| c.coverage.clone());
        (state.machine.snapshot(now()), cached, coverage)
    };
    let time = input.time.as_ref().unwrap_or(&view.authoritative_time);
    validate_picture_time(time, &view.authoritative_time)?;
    let (prepared, new_coverage) = if let Some(cached) = cached {
        (cached, None)
    } else {
        let selected = host
            .app
            .fixed_v4_frame_snapshot(&e.request.project_id, &e.request.revision)?;
        let frame = opencut_editor_application::project_v4_mask_frame_jobs::prepare(
            &selected.snapshot,
            &e.request.sequence_id,
            time,
        )?;
        admit_mask_frame_v1(&frame, &selected.processing_policy)?;
        let proofs = complete_coverage(
            frame
                .execution()
                .execution()
                .execution()
                .texts()
                .iter()
                .map(|plan| {
                    (
                        plan.identity.element_id.as_str(),
                        &plan.font,
                        plan.text.content.as_str(),
                    )
                }),
            coverage.as_deref().unwrap_or_default(),
            |missing| {
                VerifiedClassicTextResources::new_caption_fonts_subset_v3(
                    host.app.asset_store_root(),
                    frame.execution().execution(),
                    missing,
                    &FontImportPolicyV1::default(),
                    || e.checkpoint(&host.app),
                )
            },
        )?;
        let prepared = Arc::new(PreparedMaskPictureV1::new(
            frame,
            selected.processing_policy,
            proofs.clone(),
        )?);
        (prepared, Some(proofs))
    };
    let result = prepared.result(&input, &view.authoritative_time)?;
    authorize(host, &e, true)?;
    let mut state = lock(&e.state)?;
    state
        .machine
        .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
    validate_picture_time(
        &result.plan.identity.time,
        &state.machine.snapshot(now()).authoritative_time,
    )?;
    if let Some(coverage) = new_coverage {
        state.text_coverage = Some(CachedTextCoverage {
            session: input.session.clone(),
            coverage,
        });
    }
    if input.decoded.is_none() {
        state.picture = Some(CachedPicture {
            session: input.session,
            prepared: PreparedPicture::Mask(prepared),
        });
    }
    Ok(result)
}
