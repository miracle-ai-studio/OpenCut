//! Native fixed-font proof and complete frame binding share the original sound
//! session. Only one bounded frame/proof set lives in each session.
use super::text::CachedTextCoverage;
use super::*;
use opencut_api_contracts::visual_preview_caption_v3::*;
use opencut_editor_application::{
    classic_text_frame_v2::admit_text_frame_v2, font_import_v1::FontImportPolicyV1,
    visual_preview_caption_v3::PreparedCaptionPictureV3, visual_preview_v4::validate_picture_time,
};
use opencut_native_services::classic_visual::VerifiedClassicTextResources;

pub(super) struct CachedCaptionPicture {
    pub session: AudioPreviewStreamV4,
    pub prepared: Arc<PreparedCaptionPictureV3>,
}
pub(crate) fn caption_picture(
    host: &mut Automation,
    input: VisualPreviewCaptionFrameV3,
) -> Result<VisualPreviewCaptionFrameResultV3, EditorError> {
    let e = entry(&host.database, &input.session.session_id)?;
    authorize(host, &e, true)?;
    let (view, cached, coverage) = {
        let state = lock(&e.state)?;
        state
            .machine
            .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
        let cached = state
            .caption_picture
            .as_ref()
            .filter(|c| {
                input.decoded.is_some()
                    && c.session == input.session
                    && input.time.as_ref() == Some(c.prepared.time())
            })
            .map(|c| c.prepared.clone());
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
        let frame = opencut_editor_application::project_v4_caption_frame_jobs::prepare(
            &selected.snapshot,
            &e.request.sequence_id,
            time,
        )?;
        admit_text_frame_v2(frame.execution(), &selected.processing_policy)?;
        let proofs = coverage.filter(|proofs| {
            proofs.len() == frame.execution().texts().len()
                && frame.execution().texts().iter().all(|plan| {
                    proofs.iter().any(|proof| {
                        proof.element_id == plan.identity.element_id
                            && proof.font == plan.font
                            && proof.content == plan.text.content
                    })
                })
        });
        let proofs = if let Some(proofs) = proofs {
            proofs
        } else {
            let resources = VerifiedClassicTextResources::new_caption_fonts_v3(
                host.app.asset_store_root(),
                &frame,
                &FontImportPolicyV1::default(),
                || e.checkpoint(&host.app),
            )?;
            resources.coverage().to_vec()
        };
        let prepared = Arc::new(PreparedCaptionPictureV3::new(
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
        state.text_picture = None;
        state.caption_picture = Some(CachedCaptionPicture {
            session: input.session,
            prepared,
        });
    }
    Ok(result)
}
