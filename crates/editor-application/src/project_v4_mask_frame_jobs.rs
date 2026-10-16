//! Mask frames use the existing fixed-frame queue and resource capture ports.
//! Only the versioned composition/profile and its layout evidence differ.
use crate::{
    JobRecord, JobRequest, project_v4_frame_jobs as media, project_v4_text_frame_jobs as text,
};
use opencut_composition::{CompositionScope, project_v4::*};
use opencut_contracts::{EditorError, Time, project_v4::ProjectSnapshotV4};
use opencut_job_contracts::ArtifactDescriptor;
pub use opencut_job_contracts::project_v4_mask_frame::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub use text::PreparedV4TextFramePng as PreparedV4MaskFramePng;

pub trait V4MaskFrameJobSubmissionStore: text::V4TextFrameJobSubmissionStore {
    fn accept_v4_mask_frame_job(
        &mut self,
        request: JobRequest,
        prepared: PreparedV4MaskFramePng,
    ) -> Result<JobRecord, EditorError>;
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredV4MaskFramePngResultV1 {
    pub public: V4MaskFramePngResultV1,
    pub artifact: ArtifactDescriptor,
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn bad_result() -> EditorError {
    EditorError::new(
        "INVALID_STORED_RESULT",
        "Mask frame differs from its fixed input or verified layout",
    )
}
pub fn prepare(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    time: &Time,
) -> Result<PreparedMaskFrameV1, EditorError> {
    crate::font_synthesis_v1::reject_legacy_project(&snapshot.project)?;
    let graph = compile_frame_v4(snapshot, sequence_id, time, CompositionScope::Visual)
        .map_err(|e| EditorError::new(e.code, e.message))?;
    prepare_mask_frame_v1(&graph).map_err(|e| EditorError::new(e.code, e.message))
}
pub fn validate_prepared(
    input: &PreparedV4MaskFramePng,
) -> Result<PreparedMaskFrameV1, EditorError> {
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
        return Err(invalid("Invalid mask frame scope"));
    }
    let frame = prepare(&input.snapshot, &input.sequence_id, &input.time)?;
    crate::classic_mask_frame_v1::admit_mask_frame_v1(&frame, &input.processing_policy)?;
    text::validate_lock(input, frame.execution().execution().execution())?;
    Ok(frame)
}
pub fn validate_prepared_request(
    request: &JobRequest,
    prepared: &PreparedV4MaskFramePng,
) -> Result<(), EditorError> {
    let input: SubmitV4MaskFramePng = serde_json::from_value(request.input.clone())
        .map_err(|_| invalid("Invalid mask frame request"))?;
    text::validate_submission(&input)?;
    if request.kind != V4_MASK_FRAME_PNG_KIND
        || request.idempotency_key != input.idempotency_key
        || input.project_id != prepared.snapshot.project.id
        || input.revision != prepared.snapshot.revision
        || input.sequence_id != prepared.sequence_id
        || input.time != prepared.time
    {
        return Err(invalid("Mask frame request differs from preparation"));
    }
    validate_prepared(prepared)?;
    Ok(())
}
pub fn submit_mask_frame_png(
    store: &mut impl V4MaskFrameJobSubmissionStore,
    assets: &impl text::V4TextFrameAssetCapture,
    input: SubmitV4MaskFramePng,
) -> Result<JobRecord, EditorError> {
    text::validate_submission(&input)?;
    let selected = store.fixed_v4_text_frame_snapshot(&input.project_id, &input.revision)?;
    if selected.snapshot.project.id != input.project_id
        || selected.snapshot.revision != input.revision
    {
        return Err(invalid("Authorized mask snapshot differs from request"));
    }
    opencut_editor_core::project_v4::validate_project_v4(&selected.snapshot.project)?;
    let request = JobRequest {
        idempotency_key: input.idempotency_key.clone(),
        kind: V4_MASK_FRAME_PNG_KIND.into(),
        input: serde_json::to_value(&input)
            .map_err(|_| invalid("Cannot encode mask frame request"))?,
    };
    if let Some(existing) = store.find_accepted_v4_text_frame_job(&request)? {
        return Ok(existing);
    }
    text::validate_policy(&selected.processing_policy)?;
    media::validate_authority(&selected.authority)?;
    let frame = prepare(&selected.snapshot, &input.sequence_id, &input.time)?;
    crate::classic_mask_frame_v1::admit_mask_frame_v1(&frame, &selected.processing_policy)?;
    let mut assets_ids: Vec<_> = frame
        .execution()
        .execution()
        .execution()
        .media()
        .resources()
        .iter()
        .map(|r| r.asset_id.clone())
        .collect();
    let mut fonts: Vec<_> = frame
        .execution()
        .execution()
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
    let prepared = PreparedV4MaskFramePng {
        snapshot: selected.snapshot,
        sequence_id: input.sequence_id,
        time: input.time,
        asset_lock,
        font_lock,
        authority: selected.authority,
        processing_policy: selected.processing_policy,
    };
    text::validate_lock(&prepared, frame.execution().execution().execution())?;
    store.accept_v4_mask_frame_job(request, prepared)
}
pub fn public_identity(frame: &PreparedMaskFrameV1) -> V4MaskFrameIdentityV1 {
    let i = frame.execution().execution().execution().identity();
    V4MaskFrameIdentityV1 {
        project_id: i.project_id.clone(),
        revision: i.revision.clone(),
        sequence_id: i.sequence_id.clone(),
        time: i.time.clone(),
        width: i.width,
        height: i.height,
        profile: V4MaskFrameProfileV1::ClassicV31GpuMaskFrameV1,
    }
}
pub fn render_input_sha256(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    time: &Time,
) -> Result<String, EditorError> {
    let bytes = serde_json::to_vec(&(
        1u32,
        snapshot,
        sequence_id,
        time,
        V4MaskFrameProfileV1::ClassicV31GpuMaskFrameV1,
    ))
    .map_err(|_| invalid("Cannot hash mask input"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
pub fn validate_stored_result(
    input: &PreparedV4MaskFramePng,
    stored: &StoredV4MaskFramePngResultV1,
) -> Result<V4MaskFramePngResultV1, EditorError> {
    let frame = validate_prepared(input).map_err(|_| bad_result())?;
    let public = &stored.public;
    crate::classic_text_frame_v3::validate_caption_receipts_v3(
        frame.execution().execution(),
        &public.texts,
        &public.captions,
    )
    .map_err(|_| bad_result())?;
    validate_graphic_receipts_v1(frame.execution(), &public.graphics).map_err(|_| bad_result())?;
    let decoded = media::private_sources(&public.sources)?;
    crate::classic_visual::ClassicVisualSession::new(
        frame.execution().execution().execution().media(),
        &crate::classic_graphic_frame_v1::graphic_media_policy_v1(
            frame.execution(),
            &input.processing_policy,
        )?,
    )?
    .project_decoded_sources(&decoded)
    .map_err(|_| bad_result())?;
    text::validate_output_evidence(
        &input.processing_policy,
        &public.execution,
        &public.artifact,
        &stored.artifact,
    )?;
    if public.blends != frame.blends()
        || public.masks != frame.masks()
        || public.schema_version != 1
        || public.identity != public_identity(&frame)
        || public.asset_lock != input.asset_lock
        || public.font_lock != input.font_lock
        || public.render_input_sha256
            != render_input_sha256(&input.snapshot, &input.sequence_id, &input.time)?
        || public.sources
            != media::public_sources(frame.execution().execution().execution().media(), &decoded)?
    {
        return Err(bad_result());
    }
    Ok(public.clone())
}
