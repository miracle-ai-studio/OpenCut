//! Caption frames use the existing fixed-frame queue and resource capture ports.
//! Only the versioned composition/profile and its layout evidence differ.
use crate::{JobRecord, JobRequest, media_frame_jobs as media, scene_text_frame_jobs as text};
use opencut_composition::{CompositionScope, scene::*};
use opencut_contracts::{EditorError, Time, scene::EditorSnapshot};
use opencut_job_contracts::ArtifactDescriptor;
pub use opencut_job_contracts::scene_caption_frame::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub use text::PreparedTextFramePng as PreparedCaptionFramePng;

pub trait CaptionFrameJobSubmissionStore: text::TextFrameJobSubmissionStore {
    fn accept_scene_caption_frame_job(
        &mut self,
        request: JobRequest,
        prepared: PreparedCaptionFramePng,
    ) -> Result<JobRecord, EditorError>;
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredCaptionFramePngResult {
    pub public: CaptionFramePngResult,
    pub artifact: ArtifactDescriptor,
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn bad_result() -> EditorError {
    EditorError::new(
        "INVALID_STORED_RESULT",
        "Caption frame differs from its fixed input or verified layout",
    )
}
pub fn prepare(
    snapshot: &EditorSnapshot,
    sequence_id: &str,
    time: &Time,
) -> Result<PreparedCaptionFrame, EditorError> {
    crate::font_synthesis::reject_legacy_project(&snapshot.project)?;
    let graph = compile_frame_scene(snapshot, sequence_id, time, CompositionScope::Visual)
        .map_err(|e| EditorError::new(e.code, e.message))?;
    prepare_caption_frame(&graph).map_err(|e| EditorError::new(e.code, e.message))
}
pub fn validate_prepared(
    input: &PreparedCaptionFramePng,
) -> Result<PreparedCaptionFrame, EditorError> {
    text::validate_policy(&input.processing_policy)?;
    media::validate_authority(&input.authority)?;
    if [
        &input.snapshot.project.id,
        &input.snapshot.revision,
        &input.sequence_id,
    ]
    .into_iter()
    .any(|v| !media::valid_id(v))
    {
        return Err(invalid("Invalid caption frame scope"));
    }
    let frame = prepare(&input.snapshot, &input.sequence_id, &input.time)?;
    crate::classic_text_frame::admit_text_frame(frame.execution(), &input.processing_policy)?;
    text::validate_lock(input, frame.execution())?;
    Ok(frame)
}
pub fn validate_prepared_request(
    request: &JobRequest,
    prepared: &PreparedCaptionFramePng,
) -> Result<(), EditorError> {
    let input: FrameRenderInput = serde_json::from_value(request.input.clone())
        .map_err(|_| invalid("Invalid caption frame request"))?;
    text::validate_submission(&input)?;
    if request.kind != CAPTION_FRAME_PNG_KIND
        || request.idempotency_key != input.idempotency_key
        || input.project_id != prepared.snapshot.project.id
        || input.revision != prepared.snapshot.revision
        || input.sequence_id != prepared.sequence_id
        || input.time != prepared.time
    {
        return Err(invalid("Caption frame request differs from preparation"));
    }
    validate_prepared(prepared)?;
    Ok(())
}
pub fn submit_caption_frame_png(
    store: &mut impl CaptionFrameJobSubmissionStore,
    assets: &impl text::TextFrameAssetCapture,
    input: FrameRenderInput,
) -> Result<JobRecord, EditorError> {
    text::validate_submission(&input)?;
    let selected = store.fixed_scene_text_frame_snapshot(&input.project_id, &input.revision)?;
    if selected.snapshot.project.id != input.project_id
        || selected.snapshot.revision != input.revision
    {
        return Err(invalid("Authorized caption snapshot differs from request"));
    }
    opencut_editor_core::scene::validate_project_scene(&selected.snapshot.project)?;
    let request = JobRequest {
        idempotency_key: input.idempotency_key.clone(),
        kind: CAPTION_FRAME_PNG_KIND.into(),
        input: serde_json::to_value(&input)
            .map_err(|_| invalid("Cannot encode caption frame request"))?,
    };
    if let Some(existing) = store.find_accepted_scene_text_frame_job(&request)? {
        return Ok(existing);
    }
    text::validate_policy(&selected.processing_policy)?;
    media::validate_authority(&selected.authority)?;
    let frame = prepare(&selected.snapshot, &input.sequence_id, &input.time)?;
    crate::classic_text_frame::admit_text_frame(frame.execution(), &selected.processing_policy)?;
    let mut assets_ids: Vec<_> = frame
        .execution()
        .media()
        .resources()
        .iter()
        .map(|r| r.asset_id.clone())
        .collect();
    let mut fonts: Vec<_> = frame
        .execution()
        .graph()
        .fonts
        .iter()
        .map(|f| f.id.clone())
        .collect();
    assets_ids.sort();
    fonts.sort();
    let (asset_lock, font_lock) =
        assets.capture(&selected.snapshot, &input.sequence_id, &assets_ids, &fonts)?;
    let prepared = PreparedCaptionFramePng {
        snapshot: selected.snapshot,
        sequence_id: input.sequence_id,
        time: input.time,
        asset_lock,
        font_lock,
        authority: selected.authority,
        processing_policy: selected.processing_policy,
    };
    text::validate_lock(&prepared, frame.execution())?;
    store.accept_scene_caption_frame_job(request, prepared)
}
pub fn public_identity(frame: &PreparedCaptionFrame) -> CaptionFrameIdentity {
    let i = frame.execution().identity();
    CaptionFrameIdentity {
        project_id: i.project_id.clone(),
        revision: i.revision.clone(),
        sequence_id: i.sequence_id.clone(),
        time: i.time.clone(),
        width: i.width,
        height: i.height,
        profile: CaptionFrameProfile::ClassicV31GpuNormalCaptionFrameV3,
    }
}
pub fn render_input_sha256(
    snapshot: &EditorSnapshot,
    sequence_id: &str,
    time: &Time,
) -> Result<String, EditorError> {
    let bytes = serde_json::to_vec(&(
        3u32,
        snapshot,
        sequence_id,
        time,
        CaptionFrameProfile::ClassicV31GpuNormalCaptionFrameV3,
    ))
    .map_err(|_| invalid("Cannot hash caption input"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
pub fn validate_stored_result(
    input: &PreparedCaptionFramePng,
    stored: &StoredCaptionFramePngResult,
) -> Result<CaptionFramePngResult, EditorError> {
    let frame = validate_prepared(input).map_err(|_| bad_result())?;
    let public = &stored.public;
    crate::classic_caption_frame::validate_caption_receipts(
        &frame,
        &public.texts,
        &public.captions,
    )
    .map_err(|_| bad_result())?;
    let decoded = media::private_sources(&public.sources)?;
    crate::classic_visual::ClassicVisualSession::new(
        frame.execution().media(),
        &crate::classic_text_frame::admit_text_frame(frame.execution(), &input.processing_policy)?,
    )?
    .project_decoded_sources(&decoded)
    .map_err(|_| bad_result())?;
    text::validate_output_evidence(
        &input.processing_policy,
        &public.execution,
        &public.artifact,
        &stored.artifact,
    )?;
    if public.schema_version != 3
        || public.identity != public_identity(&frame)
        || public.asset_lock != input.asset_lock
        || public.font_lock != input.font_lock
        || public.render_input_sha256
            != render_input_sha256(&input.snapshot, &input.sequence_id, &input.time)?
        || public.sources != media::public_sources(frame.execution().media(), &decoded)?
    {
        return Err(bad_result());
    }
    Ok(public.clone())
}
