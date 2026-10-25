//! Additive captions over the shared fixed-scene video admission kernel.
use crate::{JobRecord, JobRequest, media_video_jobs as video};
use opencut_contracts::{EditorError, scene::EditorSnapshot};
pub use opencut_job_contracts::scene_caption_video::*;
pub use opencut_job_execution::scene_caption_video::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub use video::{
    LocalVideoExecutionPolicy, MediaVideoAssetCapture, MediaVideoJobSubmissionStore, PreparedVideo,
};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredCaptionVideoMp4Result {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_decoder_evidence_v1: Option<crate::classic_audio::ClassicAudioDecoderEvidence>,
    pub public: CaptionVideoMp4Result,
    pub artifact: opencut_job_contracts::ArtifactDescriptor,
}
pub fn prepare(
    snapshot: &EditorSnapshot,
    sequence: &str,
    policy: &LocalVideoExecutionPolicy,
) -> Result<PreparedVideo, EditorError> {
    video::prepare_with_profile(
        snapshot,
        sequence,
        policy,
        video::VideoFrameProfile::CaptionV3,
    )
}
pub fn validate_submission(input: &FixedVideoInput) -> Result<(), EditorError> {
    video::validate_submission(input)
}
pub fn validate_prepared(input: &PreparedCaptionVideoMp4) -> Result<PreparedVideo, EditorError> {
    video::validate_prepared_with_profile(input, video::VideoFrameProfile::CaptionV3)
}
pub fn validate_prepared_request(
    request: &JobRequest,
    input: &PreparedCaptionVideoMp4,
) -> Result<(), EditorError> {
    video::validate_prepared_request_with_profile(
        request,
        input,
        video::VideoFrameProfile::CaptionV3,
    )
}
pub fn submit_video_mp4(
    store: &mut impl MediaVideoJobSubmissionStore,
    assets: &impl MediaVideoAssetCapture,
    input: FixedVideoInput,
) -> Result<JobRecord, EditorError> {
    video::submit_video_mp4_with_profile(store, assets, input, video::VideoFrameProfile::CaptionV3)
}
pub fn public_identity(identity: &video::MediaVideoIdentity) -> CaptionVideoIdentity {
    CaptionVideoIdentity {
        project_id: identity.project_id.clone(),
        revision: identity.revision.clone(),
        sequence_id: identity.sequence_id.clone(),
        profile: CaptionVideoProfile::ClassicV31CfrH264AacCaptionV2,
        width: identity.width,
        height: identity.height,
        fps: identity.fps.clone(),
        frame_count: identity.frame_count.clone(),
        timeline_duration: identity.timeline_duration.clone(),
        video_duration: identity.video_duration.clone(),
        sample_rate: identity.sample_rate,
        channels: identity.channels,
        audio_frame_count: identity.audio_frame_count.clone(),
    }
}
pub fn render_input_sha256(
    snapshot: &EditorSnapshot,
    sequence: &str,
) -> Result<String, EditorError> {
    let bytes = serde_json::to_vec(&(
        2u32,
        snapshot,
        sequence,
        CaptionVideoProfile::ClassicV31CfrH264AacCaptionV2,
        48000u32,
        2u32,
    ))
    .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Cannot hash fixed caption video input"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
pub fn validate_stored_result(
    input: &PreparedCaptionVideoMp4,
    stored: &StoredCaptionVideoMp4Result,
) -> Result<CaptionVideoMp4Result, EditorError> {
    let invalid = || {
        EditorError::new(
            "INVALID_STORED_RESULT",
            "Caption video result differs from its fixed input and profile",
        )
    };
    let plan = validate_prepared(input).map_err(|_| invalid())?;
    let public = &stored.public;
    if public.schema_version != 2
        || public.identity != public_identity(&plan.identity)
        || public.asset_lock != input.asset_lock
        || public.font_lock != input.font_lock
        || public.render_input_sha256 != render_input_sha256(&input.snapshot, &input.sequence_id)?
    {
        return Err(invalid());
    }
    video::validate_result_evidence(
        input,
        &plan,
        &public.execution,
        &public.artifact,
        &stored.artifact,
        stored.audio_decoder_evidence_v1.as_ref(),
    )
    .map_err(|_| invalid())?;
    Ok(public.clone())
}
#[cfg(test)]
mod tests;
