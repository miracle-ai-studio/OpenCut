//! New graphic kind reuses the original video capture and durable queue ports.
use super::sqlite_media_video_jobs::{Assets, policy, require_prepared_authority};
use super::sqlite_scene_jobs::context;
use super::*;
use opencut_editor_application::{audit::ExecutionContext, scene_graphic_video_jobs as video};
use opencut_job_contracts::scene_graphic_video::{FixedVideoInput, GRAPHIC_VIDEO_MP4_KIND};
use opencut_job_execution::{
    scene_audio::TrustedJobAuthority, scene_graphic_video::PreparedGraphicVideoMp4,
};
pub fn start_graphic_video_mp4(
    app: &mut EditorApplication,
    input: FixedVideoInput,
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
    super::sqlite_media_video_jobs::verified_target(connection, input)
}
pub(super) fn validate_acceptance(
    connection: &Connection,
    caller: &ExecutionContext,
    request: &JobRequest,
    input: &Value,
) -> Result<(), EditorError> {
    let prepared: PreparedGraphicVideoMp4 =
        serde_json::from_value(input.clone()).map_err(|_| {
            EditorError::new(
                "INVALID_ARGUMENT",
                "Invalid private graphic video preparation",
            )
        })?;
    video::validate_prepared_request(request, &prepared)?;
    require_prepared_authority(connection, caller, &prepared)
}
pub(super) fn require_completion(
    connection: &Connection,
    job: &JobRecord,
) -> Result<(), EditorError> {
    if job.kind != GRAPHIC_VIDEO_MP4_KIND {
        return Err(EditorError::new(
            "INVALID_STORED_RESULT",
            "Graphic video completion kind differs",
        ));
    }
    let prepared: PreparedGraphicVideoMp4 =
        serde_json::from_value(job.input.clone()).map_err(|_| {
            EditorError::new("INVALID_ARGUMENT", "Invalid persisted graphic video input")
        })?;
    require_prepared_authority(connection, &context(&prepared.authority), &prepared)?;
    let stored: video::StoredGraphicVideoMp4Result =
        serde_json::from_value(job.result.clone().ok_or_else(|| {
            EditorError::new("INVALID_STORED_RESULT", "Missing graphic video result")
        })?)
        .map_err(|_| EditorError::new("INVALID_STORED_RESULT", "Invalid graphic video result"))?;
    video::validate_stored_result(&prepared, &stored)?;
    Ok(())
}
impl EditorApplication {
    pub fn revalidate_scene_graphic_video_job(
        &self,
        id: &str,
        worker: &str,
        attempt: u32,
        authority: &TrustedJobAuthority,
    ) -> Result<(), EditorError> {
        super::sqlite_scene_jobs::revalidate(
            self,
            GRAPHIC_VIDEO_MP4_KIND,
            id,
            worker,
            attempt,
            authority,
            &policy(),
        )
    }
}
