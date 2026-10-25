//! Capture, execution and completion use the same fixed revision/authority fences.
use super::*;
use opencut_editor_application::{asset_analysis::*, audit::ExecutionContext};
use opencut_job_execution::scene_audio::TrustedJobAuthority;
impl AssetAnalysisStore for EditorApplication {
    fn find_asset_analysis(&self, request: &JobRequest) -> Result<Option<JobRecord>, EditorError> {
        opencut_editor_application::media_video_jobs::MediaVideoJobSubmissionStore::find_accepted_scene_video_job(self,request)
    }
    fn accept_asset_analysis(
        &mut self,
        request: JobRequest,
        prepared: PreparedAssetAnalysis,
    ) -> Result<JobRecord, EditorError> {
        self.create_prepared_job(
            request,
            serde_json::to_value(prepared).map_err(|_| {
                EditorError::new("SERIALIZATION_ERROR", "Cannot persist asset analysis")
            })?,
        )
    }
}
pub(super) fn verified_target(
    connection: &Connection,
    input: &Value,
) -> Result<Option<(String, String)>, EditorError> {
    let target = if let Ok(public) = serde_json::from_value::<SubmitAssetAnalysis>(input.clone()) {
        validate(&public)?;
        (public.project_id, public.revision)
    } else if let Ok(prepared) = serde_json::from_value::<PreparedAssetAnalysis>(input.clone()) {
        prepare(&prepared)?;
        (prepared.snapshot.project.id, prepared.snapshot.revision)
    } else {
        return Ok(None);
    };
    super::sqlite_scene_jobs::fixed(connection, &target.0, &target.1)?;
    Ok(Some(target))
}
fn require(
    connection: &Connection,
    context: &ExecutionContext,
    input: &PreparedAssetAnalysis,
) -> Result<(), EditorError> {
    super::sqlite_scene_jobs::require_scope(connection, context, &input.snapshot.project.id)?;
    if super::sqlite_scene_jobs::authority(context) != input.authority
        || input.processing_policy != super::sqlite_source_art::policy()
        || super::sqlite_scene_jobs::fixed(
            connection,
            &input.snapshot.project.id,
            &input.snapshot.revision,
        )? != input.snapshot
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Analysis authority/policy/snapshot differs",
        ));
    }
    prepare(input)?;
    Ok(())
}
pub(super) fn validate_acceptance(
    connection: &Connection,
    context: &ExecutionContext,
    request: &JobRequest,
    input: &Value,
) -> Result<(), EditorError> {
    let prepared: PreparedAssetAnalysis = serde_json::from_value(input.clone())
        .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Invalid prepared asset analysis"))?;
    validate_request(request, &prepared)?;
    require(connection, context, &prepared)
}
pub(super) fn require_completion(
    connection: &Connection,
    job: &JobRecord,
) -> Result<(), EditorError> {
    let prepared: PreparedAssetAnalysis = serde_json::from_value(job.input.clone())
        .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Invalid persisted asset analysis"))?;
    require(
        connection,
        &super::sqlite_scene_jobs::context(&prepared.authority),
        &prepared,
    )?;
    let result: AssetAnalysisResult =
        serde_json::from_value(job.result.clone().ok_or_else(|| {
            EditorError::new("INVALID_STORED_RESULT", "Missing asset analysis result")
        })?)
        .map_err(|_| EditorError::new("INVALID_STORED_RESULT", "Invalid asset analysis result"))?;
    validate_result(&prepared, &result)
}
impl EditorApplication {
    pub fn revalidate_asset_analysis_scene(
        &self,
        id: &str,
        worker: &str,
        attempt: u32,
        authority: &TrustedJobAuthority,
    ) -> Result<(), EditorError> {
        super::sqlite_scene_jobs::revalidate(
            self,
            KIND,
            id,
            worker,
            attempt,
            authority,
            &super::sqlite_source_art::policy(),
        )
    }
}
