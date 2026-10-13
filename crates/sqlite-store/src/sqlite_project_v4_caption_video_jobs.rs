//! New caption kind reuses the original video capture and durable queue ports.
use super::sqlite_project_v4_jobs::context;
use super::sqlite_project_v4_video_jobs::{Assets, policy, require_prepared_authority};
use super::*;
use opencut_editor_application::{audit::ExecutionContext, project_v4_caption_video_jobs as video};
use opencut_job_contracts::project_v4_caption_video::{
    SubmitV4CaptionVideoMp4V2, V4_CAPTION_VIDEO_MP4_KIND,
};
use opencut_job_execution::{
    project_v4_audio::TrustedJobAuthorityV1, project_v4_caption_video::PreparedV4CaptionVideoMp4V2,
};
pub fn start_caption_video_mp4(
    app: &mut EditorApplication,
    input: SubmitV4CaptionVideoMp4V2,
) -> Result<JobRecord, EditorError> {
    let assets = Assets(opencut_asset_store::AssetStore::read_only(
        app.asset_store_root(),
    )?);
    video::submit_video_mp4(app, &assets, input)
}
pub(super) fn verified_target(
    connection: &Connection,
    input: &Value,
) -> Result<Option<(String, String)>, EditorError> {
    super::sqlite_project_v4_video_jobs::verified_target(connection, input)
}
pub(super) fn validate_acceptance(
    connection: &Connection,
    caller: &ExecutionContext,
    request: &JobRequest,
    input: &Value,
) -> Result<(), EditorError> {
    let prepared: PreparedV4CaptionVideoMp4V2 =
        serde_json::from_value(input.clone()).map_err(|_| {
            EditorError::new(
                "INVALID_ARGUMENT",
                "Invalid private caption video preparation",
            )
        })?;
    video::validate_prepared_request(request, &prepared)?;
    require_prepared_authority(connection, caller, &prepared)
}
pub(super) fn require_completion(
    connection: &Connection,
    job: &JobRecord,
) -> Result<(), EditorError> {
    if job.kind != V4_CAPTION_VIDEO_MP4_KIND {
        return Err(EditorError::new(
            "INVALID_STORED_RESULT",
            "Caption video completion kind differs",
        ));
    }
    let prepared: PreparedV4CaptionVideoMp4V2 =
        serde_json::from_value(job.input.clone()).map_err(|_| {
            EditorError::new("INVALID_ARGUMENT", "Invalid persisted caption video input")
        })?;
    require_prepared_authority(connection, &context(&prepared.authority), &prepared)?;
    let stored: video::StoredV4CaptionVideoMp4ResultV2 =
        serde_json::from_value(job.result.clone().ok_or_else(|| {
            EditorError::new("INVALID_STORED_RESULT", "Missing caption video result")
        })?)
        .map_err(|_| EditorError::new("INVALID_STORED_RESULT", "Invalid caption video result"))?;
    video::validate_stored_result(&prepared, &stored)?;
    Ok(())
}
impl EditorApplication {
    pub fn revalidate_v4_caption_video_job(
        &self,
        id: &str,
        worker: &str,
        attempt: u32,
        authority: &TrustedJobAuthorityV1,
    ) -> Result<(), EditorError> {
        super::sqlite_project_v4_jobs::revalidate(
            self,
            V4_CAPTION_VIDEO_MP4_KIND,
            id,
            worker,
            attempt,
            authority,
            &policy(),
        )
    }
}
