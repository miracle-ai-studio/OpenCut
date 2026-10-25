//! V4 frames joins the existing durable queue with the original trusted caller.
use super::sqlite_scene_jobs::{authority, context, fixed, require_scope};
use super::*;
use opencut_contracts::{
    AssetLockManifest, LockedAsset, scene::EditorSnapshot, text_frame::FontLock,
};
use opencut_editor_application::{
    audit::ExecutionContext,
    media_video_jobs::{
        self as frame, AuthorizedVideoSnapshot, MediaVideoAssetCapture,
        MediaVideoJobSubmissionStore,
    },
};
use opencut_job_contracts::media_video::{FixedVideoInput, MEDIA_VIDEO_MP4_KIND};
use opencut_job_execution::{
    media_video::LocalVideoExecutionPolicy, media_video::PreparedMediaVideoMp4,
    scene_audio::TrustedJobAuthority,
};

pub(crate) fn policy() -> LocalVideoExecutionPolicy {
    LocalVideoExecutionPolicy {
        visual: sqlite_scene_text_frame_jobs::policy(),
        audio: sqlite_scene_audio_jobs::policy(),
        max_frames: opencut_job_execution::media_video::MAX_VIDEO_FRAMES,
        // The existing ledger chooses an actual physical/explicit-policy
        // ceiling and enforces it in the child. No fixed product output quota.
        max_output_bytes: i64::MAX as u64,
        deadline_ms: None,
        stalled_after_ms: Some(300_000),
    }
}
impl MediaVideoJobSubmissionStore for EditorApplication {
    fn fixed_scene_video_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<AuthorizedVideoSnapshot, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        require_scope(&tx, &self.connection.1, project_id)?;
        let snapshot = fixed(&tx, project_id, revision)?;
        Ok(AuthorizedVideoSnapshot {
            snapshot,
            authority: authority(&self.connection.1),
            processing_policy: policy(),
        })
    }
    fn find_accepted_scene_video_job(
        &self,
        request: &JobRequest,
    ) -> Result<Option<JobRecord>, EditorError> {
        self.find_existing_job(request)
    }
    fn accept_scene_video_job(
        &mut self,
        request: JobRequest,
        prepared: PreparedMediaVideoMp4,
    ) -> Result<JobRecord, EditorError> {
        self.create_prepared_job(
            request,
            serde_json::to_value(prepared).map_err(|_| {
                EditorError::new("SERIALIZATION_ERROR", "Could not persist frame input")
            })?,
        )
    }
}
pub(super) struct Assets(pub(super) opencut_asset_store::AssetStore);
impl MediaVideoAssetCapture for Assets {
    fn capture(
        &self,
        snapshot: &EditorSnapshot,
        sequence_id: &str,
        ids: &[String],
        font_ids: &[String],
    ) -> Result<(AssetLockManifest, Vec<FontLock>), EditorError> {
        let mut assets = Vec::with_capacity(ids.len());
        for id in ids {
            let asset = snapshot
                .project
                .assets
                .iter()
                .find(|asset| &asset.id == id)
                .ok_or_else(|| {
                    EditorError::new("ASSET_LOCK_INVALID", "Fixed frame asset is missing")
                })?;
            self.0.open_version_verified(&asset.version)?;
            assets.push(LockedAsset {
                asset_id: id.clone(),
                content_hash: asset.version.content_hash.clone(),
                size: asset.version.size,
            });
        }
        let mut fonts = Vec::with_capacity(font_ids.len());
        for id in font_ids {
            let font = snapshot
                .project
                .fonts
                .iter()
                .find(|f| &f.id == id)
                .ok_or_else(|| EditorError::new("FONT_LOCK_INVALID", "Fixed font missing"))?;
            self.0.open_version_verified(&font.content)?;
            fonts.push(FontLock {
                font_id: id.clone(),
                content_hash: font.content.content_hash.clone(),
                size: font.content.size,
            });
        }
        Ok((
            AssetLockManifest {
                version: 1,
                revision: snapshot.revision.clone(),
                sequence_id: sequence_id.into(),
                assets,
            },
            fonts,
        ))
    }
}
pub fn start_video_mp4(
    app: &mut EditorApplication,
    input: FixedVideoInput,
) -> Result<JobRecord, EditorError> {
    let assets = Assets(opencut_asset_store::AssetStore::read_only(
        app.asset_store_root(),
    )?);
    frame::submit_video_mp4(app, &assets, input)
}

/// Select either the public request or trusted private snapshot, without V3 projection.
pub(super) fn verified_target(
    connection: &Connection,
    input: &Value,
) -> Result<Option<(String, String)>, EditorError> {
    let (project, revision, snapshot) = if let Ok(public) =
        serde_json::from_value::<FixedVideoInput>(input.clone())
    {
        frame::validate_submission(&public)?;
        (public.project_id, public.revision, None)
    } else if let Ok(prepared) = serde_json::from_value::<PreparedMediaVideoMp4>(input.clone()) {
        (
            prepared.snapshot.project.id.clone(),
            prepared.snapshot.revision.clone(),
            Some(prepared.snapshot),
        )
    } else {
        return Ok(None);
    };
    let actual = fixed(connection, &project, &revision)?;
    if snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot != &actual)
    {
        return Ok(None);
    }
    Ok(Some((project, revision)))
}
pub(super) fn require_prepared_authority(
    connection: &Connection,
    caller: &ExecutionContext,
    prepared: &PreparedMediaVideoMp4,
) -> Result<(), EditorError> {
    require_scope(connection, caller, &prepared.snapshot.project.id)?;
    if prepared.authority != authority(caller)
        || !sqlite_scene_jobs::matches_policy(&prepared.processing_policy, &policy())?
        || fixed(
            connection,
            &prepared.snapshot.project.id,
            &prepared.snapshot.revision,
        )? != prepared.snapshot
    {
        return Err(access::denied());
    }
    Ok(())
}
/// Called inside the same IMMEDIATE transaction as acceptance, including owners.
pub(super) fn validate_acceptance(
    connection: &Connection,
    caller: &ExecutionContext,
    request: &JobRequest,
    input: &Value,
) -> Result<(), EditorError> {
    let prepared: PreparedMediaVideoMp4 = serde_json::from_value(input.clone())
        .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Invalid prepared frame input"))?;
    frame::validate_prepared_request(request, &prepared)?;
    require_scope(connection, caller, &prepared.snapshot.project.id)?;
    if prepared.authority != authority(caller)
        || !sqlite_scene_jobs::matches_policy(&prepared.processing_policy, &policy())?
        || fixed(
            connection,
            &prepared.snapshot.project.id,
            &prepared.snapshot.revision,
        )? != prepared.snapshot
    {
        return Err(access::denied());
    }
    frame::validate_prepared(&prepared)?;
    Ok(())
}
pub(super) fn require_completion(
    connection: &Connection,
    job: &JobRecord,
) -> Result<(), EditorError> {
    let prepared: PreparedMediaVideoMp4 = serde_json::from_value(job.input.clone())
        .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Invalid persisted frame input"))?;
    require_scope(
        connection,
        &context(&prepared.authority),
        &prepared.snapshot.project.id,
    )?;
    if !sqlite_scene_jobs::matches_policy(&prepared.processing_policy, &policy())?
        || fixed(
            connection,
            &prepared.snapshot.project.id,
            &prepared.snapshot.revision,
        )? != prepared.snapshot
    {
        return Err(access::denied());
    }
    let result: frame::StoredVideoMp4Result = serde_json::from_value(
        job.result
            .clone()
            .ok_or_else(|| EditorError::new("INVALID_STORED_RESULT", "Missing frame result"))?,
    )
    .map_err(|_| EditorError::new("INVALID_STORED_RESULT", "Invalid frame result"))?;
    frame::validate_stored_result(&prepared, &result)?;
    Ok(())
}
impl EditorApplication {
    /// Small live checks share the fixed job authorization boundary.
    pub fn revalidate_scene_video_job(
        &self,
        job_id: &str,
        worker: &str,
        attempt: u32,
        selected_authority: &TrustedJobAuthority,
    ) -> Result<(), EditorError> {
        sqlite_scene_jobs::revalidate(
            self,
            MEDIA_VIDEO_MP4_KIND,
            job_id,
            worker,
            attempt,
            selected_authority,
            &policy(),
        )
    }
}

#[cfg(test)]
mod tests;
