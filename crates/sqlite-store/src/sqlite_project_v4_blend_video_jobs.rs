//! New blend kind reuses the original video capture and durable queue ports.
use super::sqlite_project_v4_jobs::context;
use super::sqlite_project_v4_video_jobs::{Assets, policy, require_prepared_authority};
use super::*;
use opencut_editor_application::{audit::ExecutionContext, project_v4_blend_video_jobs as video};
use opencut_job_contracts::project_v4_blend_video::{
    SubmitV4BlendVideoMp4V1, V4_BLEND_VIDEO_MP4_KIND,
};
use opencut_job_execution::{
    project_v4_audio::TrustedJobAuthorityV1, project_v4_blend_video::PreparedV4BlendVideoMp4V1,
};
pub fn start_blend_video_mp4(
    app: &mut EditorApplication,
    input: SubmitV4BlendVideoMp4V1,
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
    let prepared: PreparedV4BlendVideoMp4V1 =
        serde_json::from_value(input.clone()).map_err(|_| {
            EditorError::new(
                "INVALID_ARGUMENT",
                "Invalid private blend video preparation",
            )
        })?;
    video::validate_prepared_request(request, &prepared)?;
    require_prepared_authority(connection, caller, &prepared)
}
pub(super) fn require_completion(
    connection: &Connection,
    job: &JobRecord,
) -> Result<(), EditorError> {
    if job.kind != V4_BLEND_VIDEO_MP4_KIND {
        return Err(EditorError::new(
            "INVALID_STORED_RESULT",
            "Blend video completion kind differs",
        ));
    }
    let prepared: PreparedV4BlendVideoMp4V1 = serde_json::from_value(job.input.clone())
        .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Invalid persisted blend video input"))?;
    require_prepared_authority(connection, &context(&prepared.authority), &prepared)?;
    let stored: video::StoredV4BlendVideoMp4ResultV1 =
        serde_json::from_value(job.result.clone().ok_or_else(|| {
            EditorError::new("INVALID_STORED_RESULT", "Missing blend video result")
        })?)
        .map_err(|_| EditorError::new("INVALID_STORED_RESULT", "Invalid blend video result"))?;
    video::validate_stored_result(&prepared, &stored)?;
    Ok(())
}
impl EditorApplication {
    pub fn revalidate_v4_blend_video_job(
        &self,
        id: &str,
        worker: &str,
        attempt: u32,
        authority: &TrustedJobAuthorityV1,
    ) -> Result<(), EditorError> {
        super::sqlite_project_v4_jobs::revalidate(
            self,
            V4_BLEND_VIDEO_MP4_KIND,
            id,
            worker,
            attempt,
            authority,
            &policy(),
        )
    }
}
