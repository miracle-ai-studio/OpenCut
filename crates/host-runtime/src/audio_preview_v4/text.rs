//! Native fixed-font proof and complete frame binding share the original sound
//! session. Only one bounded frame/proof set lives in each session.
use super::*;
use opencut_api_contracts::visual_preview_text_v2::*;
use opencut_composition::project_v4::NativeTextCoverageV1;
use opencut_editor_application::{
    classic_text_frame_v2::admit_text_frame_v2, font_import_v1::FontImportPolicyV1,
    visual_preview_text_v2::PreparedTextPictureV2, visual_preview_v4::validate_picture_time,
};
use opencut_native_services::classic_visual::VerifiedClassicTextResources;

pub(super) struct CachedTextPicture {
    pub session: AudioPreviewStreamV4,
    pub prepared: Arc<PreparedTextPictureV2>,
}
pub(super) struct CachedTextCoverage {
    pub session: AudioPreviewStreamV4,
    pub coverage: Vec<NativeTextCoverageV1>,
}
pub(crate) fn text_picture(
    host: &mut Automation,
    input: VisualPreviewTextFrameV2,
) -> Result<VisualPreviewTextFrameResultV2, EditorError> {
    let e = entry(&host.database, &input.session.session_id)?;
    authorize(host, &e, true)?;
    let (view, cached, coverage) = {
        let state = lock(&e.state)?;
        state
            .machine
            .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
        let cached = state
            .text_picture
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
        let frame = opencut_editor_application::project_v4_video_jobs::prepare_frame(
            &selected.snapshot,
            &e.request.sequence_id,
            time,
        )?;
        admit_text_frame_v2(&frame, &selected.processing_policy)?;
        let proofs = coverage.filter(|proofs| {
            proofs.len() == frame.texts().len()
                && frame.texts().iter().all(|plan| {
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
            let resources = VerifiedClassicTextResources::new_fonts(
                host.app.asset_store_root(),
                &frame,
                &FontImportPolicyV1::default(),
                || e.checkpoint(&host.app),
            )?;
            resources.coverage().to_vec()
        };
        let prepared = Arc::new(PreparedTextPictureV2::new(
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
        state.caption_picture = None;
        state.text_picture = Some(CachedTextPicture {
            session: input.session,
            prepared,
        });
    }
    Ok(result)
}
/// Font reads must reference a font participating in a currently admitted
/// complete frame. Captured native proof binds its exact immutable CAS version.
fn font_version(
    host: &mut Automation,
    input: &VisualPreviewFontV2,
) -> Result<opencut_contracts::AssetVersion, EditorError> {
    let e = entry(&host.database, &input.session.session_id)?;
    authorize(host, &e, true)?;
    let state = lock(&e.state)?;
    state
        .machine
        .verify_visual_frame(&input.session, host.caller.principal_id(), now())?;
    let coverage = state
        .text_coverage
        .as_ref()
        .filter(|c| c.session == input.session)
        .ok_or_else(|| error("NOT_AUTHORIZED"))?;
    coverage
        .coverage
        .iter()
        .find(|proof| proof.font.id == input.font_id)
        .map(|proof| proof.font.content.clone())
        .ok_or_else(|| error("NOT_AUTHORIZED"))
}
pub(crate) fn authorize_font(
    host: &mut Automation,
    input: &VisualPreviewFontV2,
) -> Result<(), EditorError> {
    font_version(host, input).map(|_| ())
}
pub(crate) fn open_font(
    host: &mut Automation,
    input: VisualPreviewFontV2,
) -> Result<(std::fs::File, u64), EditorError> {
    let version = font_version(host, &input)?;
    let opened = crate::transport_io::open_verified_v4_resource(&host.database, &version)?;
    if font_version(host, &input)? != version {
        return Err(error("NOT_AUTHORIZED"));
    }
    Ok(opened)
}
