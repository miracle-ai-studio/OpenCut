//! V4 frames joins the existing durable queue with the original trusted caller.
use super::sqlite_project_v4_jobs::{authority, context, fixed, require_scope};
use super::*;
use opencut_contracts::{AssetLockManifest, LockedAsset, project_v4::ProjectSnapshotV4};
use opencut_editor_application::project_v4_blend_frame_jobs as blend;
use opencut_editor_application::project_v4_caption_frame_jobs as caption;
use opencut_editor_application::project_v4_graphic_frame_jobs as graphic;
use opencut_editor_application::project_v4_mask_frame_jobs as mask;
use opencut_editor_application::project_v4_scene_frame_jobs as scene;
use opencut_editor_application::{
    audit::ExecutionContext,
    project_v4_text_frame_jobs::{
        self as frame, AuthorizedV4TextFrameSnapshot, V4TextFrameAssetCapture,
        V4TextFrameJobSubmissionStore,
    },
};
use opencut_job_contracts::project_v4_text_frame::{
    FontLockV2, SubmitV4TextFramePng, V4_TEXT_FRAME_PNG_KIND,
};
use opencut_job_execution::{
    project_v4_audio::TrustedJobAuthorityV1, project_v4_frame::LocalVisualExecutionPolicyV1,
    project_v4_text_frame::PreparedV4TextFramePng,
};

pub(crate) fn policy() -> LocalVisualExecutionPolicyV1 {
    LocalVisualExecutionPolicyV1 {
        max_frame_pixels: 16_777_216,
        max_decoded_pixels: 16_777_216,
        max_gpu_bytes: 512 * 1024 * 1024,
        max_rgba_bytes: 64 * 1024 * 1024,
        max_encoded_bytes: 32 * 1024 * 1024,
        max_output_bytes: 129 * 1024 * 1024,
        max_rss_bytes: 2 * 1024 * 1024 * 1024,
        deadline_ms: 120_000,
    }
}
impl V4TextFrameJobSubmissionStore for EditorApplication {
    fn fixed_v4_text_frame_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<AuthorizedV4TextFrameSnapshot, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        require_scope(&tx, &self.connection.1, project_id)?;
        let snapshot = fixed(&tx, project_id, revision)?;
        Ok(AuthorizedV4TextFrameSnapshot {
            snapshot,
            authority: authority(&self.connection.1),
            processing_policy: policy(),
        })
    }
    fn find_accepted_v4_text_frame_job(
        &self,
        request: &JobRequest,
    ) -> Result<Option<JobRecord>, EditorError> {
        self.find_existing_job(request)
    }
    fn accept_v4_text_frame_job(
        &mut self,
        request: JobRequest,
        prepared: PreparedV4TextFramePng,
    ) -> Result<JobRecord, EditorError> {
        self.create_prepared_job(
            request,
            serde_json::to_value(prepared).map_err(|_| {
                EditorError::new("SERIALIZATION_ERROR", "Could not persist frame input")
            })?,
        )
    }
}
struct Assets(opencut_asset_store::AssetStore);
impl V4TextFrameAssetCapture for Assets {
    fn capture(
        &self,
        snapshot: &ProjectSnapshotV4,
        sequence_id: &str,
        ids: &[String],
        font_ids: &[String],
    ) -> Result<(AssetLockManifest, Vec<FontLockV2>), EditorError> {
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
            fonts.push(FontLockV2 {
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
pub fn start_text_frame_png(
    app: &mut EditorApplication,
    input: SubmitV4TextFramePng,
) -> Result<JobRecord, EditorError> {
    let assets = Assets(opencut_asset_store::AssetStore::read_only(
        app.asset_store_root(),
    )?);
    frame::submit_text_frame_png(app, &assets, input)
}

/// Select either the public request or trusted private snapshot, without V3 projection.
pub(super) fn verified_target(
    connection: &Connection,
    input: &Value,
) -> Result<Option<(String, String)>, EditorError> {
    let (project, revision, snapshot) = if let Ok(public) =
        serde_json::from_value::<SubmitV4TextFramePng>(input.clone())
    {
        frame::validate_submission(&public)?;
        (public.project_id, public.revision, None)
    } else if let Ok(prepared) = serde_json::from_value::<PreparedV4TextFramePng>(input.clone()) {
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
/// Called inside the same IMMEDIATE transaction as acceptance, including owners.
pub(super) fn validate_acceptance(
    connection: &Connection,
    caller: &ExecutionContext,
    request: &JobRequest,
    input: &Value,
) -> Result<(), EditorError> {
    let prepared: PreparedV4TextFramePng = serde_json::from_value(input.clone())
        .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Invalid prepared frame input"))?;
    if request.kind == scene::V4_SCENE_FRAME_PNG_KIND {
        scene::validate_prepared_request(request, &prepared)?;
    } else if request.kind == mask::V4_MASK_FRAME_PNG_KIND {
        mask::validate_prepared_request(request, &prepared)?;
    } else if request.kind == blend::V4_BLEND_FRAME_PNG_KIND {
        blend::validate_prepared_request(request, &prepared)?;
    } else if request.kind == graphic::V4_GRAPHIC_FRAME_PNG_KIND {
        graphic::validate_prepared_request(request, &prepared)?;
    } else if request.kind == caption::V4_CAPTION_FRAME_PNG_KIND {
        caption::validate_prepared_request(request, &prepared)?;
    } else {
        frame::validate_prepared_request(request, &prepared)?;
    }
    require_scope(connection, caller, &prepared.snapshot.project.id)?;
    if prepared.authority != authority(caller)
        || prepared.processing_policy != policy()
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
pub(super) fn require_completion(
    connection: &Connection,
    job: &JobRecord,
) -> Result<(), EditorError> {
    let prepared: PreparedV4TextFramePng = serde_json::from_value(job.input.clone())
        .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Invalid persisted frame input"))?;
    require_scope(
        connection,
        &context(&prepared.authority),
        &prepared.snapshot.project.id,
    )?;
    if prepared.processing_policy != policy()
        || fixed(
            connection,
            &prepared.snapshot.project.id,
            &prepared.snapshot.revision,
        )? != prepared.snapshot
    {
        return Err(access::denied());
    }
    let value = job
        .result
        .clone()
        .ok_or_else(|| EditorError::new("INVALID_STORED_RESULT", "Missing frame result"))?;
    if job.kind == scene::V4_SCENE_FRAME_PNG_KIND {
        let result: scene::StoredV4SceneFramePngResultV1 = serde_json::from_value(value)
            .map_err(|_| EditorError::new("INVALID_STORED_RESULT", "Invalid scene frame result"))?;
        scene::validate_stored_result(&prepared, &result)?;
    } else if job.kind == mask::V4_MASK_FRAME_PNG_KIND {
        let result: mask::StoredV4MaskFramePngResultV1 = serde_json::from_value(value)
            .map_err(|_| EditorError::new("INVALID_STORED_RESULT", "Invalid mask frame result"))?;
        mask::validate_stored_result(&prepared, &result)?;
    } else if job.kind == blend::V4_BLEND_FRAME_PNG_KIND {
        let result: blend::StoredV4BlendFramePngResultV1 = serde_json::from_value(value)
            .map_err(|_| EditorError::new("INVALID_STORED_RESULT", "Invalid blend frame result"))?;
        blend::validate_stored_result(&prepared, &result)?;
    } else if job.kind == graphic::V4_GRAPHIC_FRAME_PNG_KIND {
        let result: graphic::StoredV4GraphicFramePngResultV1 = serde_json::from_value(value)
            .map_err(|_| {
                EditorError::new("INVALID_STORED_RESULT", "Invalid graphic frame result")
            })?;
        graphic::validate_stored_result(&prepared, &result)?;
    } else if job.kind == caption::V4_CAPTION_FRAME_PNG_KIND {
        let result: caption::StoredV4CaptionFramePngResultV3 = serde_json::from_value(value)
            .map_err(|_| {
                EditorError::new("INVALID_STORED_RESULT", "Invalid caption frame result")
            })?;
        caption::validate_stored_result(&prepared, &result)?;
    } else {
        let result: frame::StoredV4TextFramePngResultV2 = serde_json::from_value(value)
            .map_err(|_| EditorError::new("INVALID_STORED_RESULT", "Invalid text frame result"))?;
        frame::validate_stored_result(&prepared, &result)?;
    }
    Ok(())
}
impl EditorApplication {
    /// Small live checks share the fixed job authorization boundary.
    pub fn revalidate_v4_text_frame_job(
        &self,
        job_id: &str,
        worker: &str,
        attempt: u32,
        selected_authority: &TrustedJobAuthorityV1,
    ) -> Result<(), EditorError> {
        sqlite_project_v4_jobs::revalidate(
            self,
            V4_TEXT_FRAME_PNG_KIND,
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

impl caption::V4CaptionFrameJobSubmissionStore for EditorApplication {
    fn accept_v4_caption_frame_job(
        &mut self,
        request: JobRequest,
        prepared: caption::PreparedV4CaptionFramePng,
    ) -> Result<JobRecord, EditorError> {
        self.create_prepared_job(
            request,
            serde_json::to_value(prepared).map_err(|_| {
                EditorError::new("SERIALIZATION_ERROR", "Cannot persist caption frame")
            })?,
        )
    }
}
pub fn start_caption_frame_png(
    app: &mut EditorApplication,
    input: caption::SubmitV4CaptionFramePng,
) -> Result<JobRecord, EditorError> {
    let assets = Assets(opencut_asset_store::AssetStore::read_only(
        app.asset_store_root(),
    )?);
    caption::submit_caption_frame_png(app, &assets, input)
}
impl EditorApplication {
    pub fn revalidate_v4_caption_frame_job(
        &self,
        job_id: &str,
        worker: &str,
        attempt: u32,
        selected_authority: &TrustedJobAuthorityV1,
    ) -> Result<(), EditorError> {
        sqlite_project_v4_jobs::revalidate(
            self,
            caption::V4_CAPTION_FRAME_PNG_KIND,
            job_id,
            worker,
            attempt,
            selected_authority,
            &policy(),
        )
    }
}

impl graphic::V4GraphicFrameJobSubmissionStore for EditorApplication {
    fn accept_v4_graphic_frame_job(
        &mut self,
        request: JobRequest,
        prepared: graphic::PreparedV4GraphicFramePng,
    ) -> Result<JobRecord, EditorError> {
        self.create_prepared_job(
            request,
            serde_json::to_value(prepared).map_err(|_| {
                EditorError::new("SERIALIZATION_ERROR", "Cannot persist graphic frame")
            })?,
        )
    }
}
pub fn start_graphic_frame_png(
    app: &mut EditorApplication,
    input: graphic::SubmitV4GraphicFramePng,
) -> Result<JobRecord, EditorError> {
    let assets = Assets(opencut_asset_store::AssetStore::read_only(
        app.asset_store_root(),
    )?);
    graphic::submit_graphic_frame_png(app, &assets, input)
}
impl EditorApplication {
    pub fn revalidate_v4_graphic_frame_job(
        &self,
        job_id: &str,
        worker: &str,
        attempt: u32,
        selected_authority: &TrustedJobAuthorityV1,
    ) -> Result<(), EditorError> {
        sqlite_project_v4_jobs::revalidate(
            self,
            graphic::V4_GRAPHIC_FRAME_PNG_KIND,
            job_id,
            worker,
            attempt,
            selected_authority,
            &policy(),
        )
    }
}

impl scene::V4SceneFrameJobSubmissionStore for EditorApplication {
    fn accept_v4_scene_frame_job(
        &mut self,
        request: JobRequest,
        prepared: scene::PreparedV4SceneFramePng,
    ) -> Result<JobRecord, EditorError> {
        self.create_prepared_job(
            request,
            serde_json::to_value(prepared).map_err(|_| {
                EditorError::new("SERIALIZATION_ERROR", "Cannot persist scene frame")
            })?,
        )
    }
}
impl mask::V4MaskFrameJobSubmissionStore for EditorApplication {
    fn accept_v4_mask_frame_job(
        &mut self,
        request: JobRequest,
        prepared: mask::PreparedV4MaskFramePng,
    ) -> Result<JobRecord, EditorError> {
        self.create_prepared_job(
            request,
            serde_json::to_value(prepared).map_err(|_| {
                EditorError::new("SERIALIZATION_ERROR", "Cannot persist mask frame")
            })?,
        )
    }
}
impl blend::V4BlendFrameJobSubmissionStore for EditorApplication {
    fn accept_v4_blend_frame_job(
        &mut self,
        request: JobRequest,
        prepared: blend::PreparedV4BlendFramePng,
    ) -> Result<JobRecord, EditorError> {
        self.create_prepared_job(
            request,
            serde_json::to_value(prepared).map_err(|_| {
                EditorError::new("SERIALIZATION_ERROR", "Cannot persist blend frame")
            })?,
        )
    }
}
pub fn start_scene_frame_png(
    app: &mut EditorApplication,
    input: scene::SubmitV4SceneFramePng,
) -> Result<JobRecord, EditorError> {
    let assets = Assets(opencut_asset_store::AssetStore::read_only(
        app.asset_store_root(),
    )?);
    scene::submit_scene_frame_png(app, &assets, input)
}
pub fn start_mask_frame_png(
    app: &mut EditorApplication,
    input: mask::SubmitV4MaskFramePng,
) -> Result<JobRecord, EditorError> {
    let assets = Assets(opencut_asset_store::AssetStore::read_only(
        app.asset_store_root(),
    )?);
    mask::submit_mask_frame_png(app, &assets, input)
}
pub fn start_blend_frame_png(
    app: &mut EditorApplication,
    input: blend::SubmitV4BlendFramePng,
) -> Result<JobRecord, EditorError> {
    let assets = Assets(opencut_asset_store::AssetStore::read_only(
        app.asset_store_root(),
    )?);
    blend::submit_blend_frame_png(app, &assets, input)
}
impl EditorApplication {
    pub fn revalidate_v4_scene_frame_job(
        &self,
        job_id: &str,
        worker: &str,
        attempt: u32,
        selected_authority: &TrustedJobAuthorityV1,
    ) -> Result<(), EditorError> {
        sqlite_project_v4_jobs::revalidate(
            self,
            scene::V4_SCENE_FRAME_PNG_KIND,
            job_id,
            worker,
            attempt,
            selected_authority,
            &policy(),
        )
    }
    pub fn revalidate_v4_mask_frame_job(
        &self,
        job_id: &str,
        worker: &str,
        attempt: u32,
        selected_authority: &TrustedJobAuthorityV1,
    ) -> Result<(), EditorError> {
        sqlite_project_v4_jobs::revalidate(
            self,
            mask::V4_MASK_FRAME_PNG_KIND,
            job_id,
            worker,
            attempt,
            selected_authority,
            &policy(),
        )
    }
    pub fn revalidate_v4_blend_frame_job(
        &self,
        job_id: &str,
        worker: &str,
        attempt: u32,
        selected_authority: &TrustedJobAuthorityV1,
    ) -> Result<(), EditorError> {
        sqlite_project_v4_jobs::revalidate(
            self,
            blend::V4_BLEND_FRAME_PNG_KIND,
            job_id,
            worker,
            attempt,
            selected_authority,
            &policy(),
        )
    }
}
