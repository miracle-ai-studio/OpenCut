//! Additive scenes over the shared fixed-scene video admission kernel.
use crate::{JobRecord, JobRequest, project_v4_video_jobs as video};
use opencut_contracts::{EditorError, project_v4::ProjectSnapshotV4};
pub use opencut_job_contracts::project_v4_scene_video::*;
pub use opencut_job_execution::project_v4_scene_video::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub use video::{
    LocalVideoExecutionPolicyV1, PreparedVideoV4, V4VideoAssetCapture, V4VideoJobSubmissionStore,
};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredV4SceneVideoMp4ResultV1 {
    pub public: V4SceneVideoMp4ResultV1,
    pub artifact: opencut_job_contracts::ArtifactDescriptor,
}
pub fn prepare(
    snapshot: &ProjectSnapshotV4,
    sequence: &str,
    policy: &LocalVideoExecutionPolicyV1,
) -> Result<PreparedVideoV4, EditorError> {
    video::prepare_with_profile(
        snapshot,
        sequence,
        policy,
        video::VideoFrameProfile::SceneV1,
    )
}
pub fn validate_submission(input: &SubmitV4SceneVideoMp4V1) -> Result<(), EditorError> {
    video::validate_submission(input)
}
pub fn validate_prepared(
    input: &PreparedV4SceneVideoMp4V1,
) -> Result<PreparedVideoV4, EditorError> {
    video::validate_prepared_with_profile(input, video::VideoFrameProfile::SceneV1)
}
pub fn validate_prepared_request(
    request: &JobRequest,
    input: &PreparedV4SceneVideoMp4V1,
) -> Result<(), EditorError> {
    video::validate_prepared_request_with_profile(request, input, video::VideoFrameProfile::SceneV1)
}
pub fn submit_video_mp4(
    store: &mut impl V4VideoJobSubmissionStore,
    assets: &impl V4VideoAssetCapture,
    input: SubmitV4SceneVideoMp4V1,
) -> Result<JobRecord, EditorError> {
    video::submit_video_mp4_with_profile(store, assets, input, video::VideoFrameProfile::SceneV1)
}
pub fn public_identity(identity: &video::V4VideoIdentityV1) -> V4SceneVideoIdentityV1 {
    V4SceneVideoIdentityV1 {
        project_id: identity.project_id.clone(),
        revision: identity.revision.clone(),
        sequence_id: identity.sequence_id.clone(),
        profile: V4SceneVideoProfileV1::ClassicV31CfrH264AacSceneV1,
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
    snapshot: &ProjectSnapshotV4,
    sequence: &str,
) -> Result<String, EditorError> {
    let bytes = serde_json::to_vec(&(
        1u32,
        snapshot,
        sequence,
        V4SceneVideoProfileV1::ClassicV31CfrH264AacSceneV1,
        48000u32,
        2u32,
    ))
    .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Cannot hash fixed scene video input"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
pub fn validate_stored_result(
    input: &PreparedV4SceneVideoMp4V1,
    stored: &StoredV4SceneVideoMp4ResultV1,
) -> Result<V4SceneVideoMp4ResultV1, EditorError> {
    let invalid = || {
        EditorError::new(
            "INVALID_STORED_RESULT",
            "Scene video result differs from its fixed input and profile",
        )
    };
    let plan = validate_prepared(input).map_err(|_| invalid())?;
    let public = &stored.public;
    if public.schema_version != 1
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
    )
    .map_err(|_| invalid())?;
    Ok(public.clone())
}
