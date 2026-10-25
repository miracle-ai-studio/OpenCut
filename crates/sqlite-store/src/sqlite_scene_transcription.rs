//! Original caller, fixed V4 revision and one durable leased transcription job.
use super::sqlite_scene_jobs::{authority, context, fixed, require_scope};
use super::*;
use opencut_contracts::AssetLockManifest;
use opencut_editor_application::{
    audit::ExecutionContext,
    scene_audio_jobs as audio,
    scene_transcription::{
        self as transcription, PreparedSceneTranscription, SceneTranscriptionStore,
    },
};
use opencut_job_contracts::scene_transcription::*;

impl SceneTranscriptionStore for EditorApplication {
    fn accept_scene_transcription(
        &mut self,
        request: JobRequest,
        prepared: PreparedSceneTranscription,
    ) -> Result<JobRecord, EditorError> {
        self.create_prepared_job(
            request,
            serde_json::to_value(prepared).map_err(|_| {
                EditorError::new("SERIALIZATION_ERROR", "Could not persist V4 transcription")
            })?,
        )
    }
}
struct Assets(opencut_asset_store::AssetStore);
impl audio::AudioAssetCapture for Assets {
    fn capture(
        &self,
        snapshot: &opencut_contracts::scene::EditorSnapshot,
        sequence: &str,
        ids: &[String],
    ) -> Result<AssetLockManifest, EditorError> {
        let mut assets = Vec::with_capacity(ids.len());
        for id in ids {
            let a = snapshot
                .project
                .assets
                .iter()
                .find(|a| &a.id == id)
                .ok_or_else(|| {
                    EditorError::new("ASSET_LOCK_INVALID", "Fixed audio resource is missing")
                })?;
            self.0.open_version_verified(&a.version)?;
            assets.push(opencut_contracts::LockedAsset {
                asset_id: id.clone(),
                content_hash: a.version.content_hash.clone(),
                size: a.version.size,
            });
        }
        Ok(AssetLockManifest {
            version: 1,
            revision: snapshot.revision.clone(),
            sequence_id: sequence.into(),
            assets,
        })
    }
}
pub fn start(
    app: &mut EditorApplication,
    input: SceneTranscriptionRequest,
) -> Result<JobRecord, EditorError> {
    let assets = Assets(opencut_asset_store::AssetStore::read_only(
        app.asset_store_root(),
    )?);
    transcription::submit(app, &assets, input)
}
pub(super) fn verified_target(
    connection: &Connection,
    input: &Value,
) -> Result<Option<(String, String)>, EditorError> {
    let (project, revision, snapshot) = if let Ok(public) =
        serde_json::from_value::<SceneTranscriptionRequest>(input.clone())
    {
        transcription::validate_submission(&public)?;
        (public.project_id, public.revision, None)
    } else if let Ok(prepared) = serde_json::from_value::<PreparedSceneTranscription>(input.clone())
    {
        transcription::validate_prepared(&prepared)?;
        (
            prepared.audio.snapshot.project.id.clone(),
            prepared.audio.snapshot.revision.clone(),
            Some(prepared.audio.snapshot),
        )
    } else {
        return Ok(None);
    };
    let actual = fixed(connection, &project, &revision)?;
    if snapshot.as_ref().is_some_and(|s| s != &actual) {
        return Ok(None);
    }
    Ok(Some((project, revision)))
}
pub(super) fn validate_acceptance(
    connection: &Connection,
    caller: &ExecutionContext,
    request: &JobRequest,
    input: &Value,
) -> Result<(), EditorError> {
    let prepared: PreparedSceneTranscription = serde_json::from_value(input.clone())
        .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Invalid prepared V4 transcription"))?;
    transcription::validate_request(request, &prepared)?;
    require_scope(connection, caller, &prepared.audio.snapshot.project.id)?;
    if prepared.audio.authority != authority(caller)
        || prepared.audio.processing_policy != sqlite_scene_audio_jobs::policy()
        || fixed(
            connection,
            &prepared.audio.snapshot.project.id,
            &prepared.audio.snapshot.revision,
        )? != prepared.audio.snapshot
    {
        return Err(access::denied());
    }
    Ok(())
}
pub(super) fn require_completion(
    connection: &Connection,
    job: &JobRecord,
) -> Result<(), EditorError> {
    let prepared: PreparedSceneTranscription = serde_json::from_value(job.input.clone())
        .map_err(|_| EditorError::new("INVALID_ARGUMENT", "Invalid persisted V4 transcription"))?;
    require_scope(
        connection,
        &context(&prepared.audio.authority),
        &prepared.audio.snapshot.project.id,
    )?;
    if prepared.audio.processing_policy != sqlite_scene_audio_jobs::policy()
        || fixed(
            connection,
            &prepared.audio.snapshot.project.id,
            &prepared.audio.snapshot.revision,
        )? != prepared.audio.snapshot
    {
        return Err(access::denied());
    }
    transcription::validate_stored_result(
        &prepared,
        job.result
            .clone()
            .ok_or_else(|| EditorError::new("INVALID_STORED_RESULT", "Missing V4 transcript"))?,
    )?;
    Ok(())
}
impl EditorApplication {
    pub fn revalidate_scene_transcription(
        &self,
        id: &str,
        worker: &str,
        attempt: u32,
        selected: &audio::TrustedJobAuthority,
    ) -> Result<(), EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let row:(String,u32,Option<String>,Option<i64>,bool,String)=tx.query_row("SELECT state,attempt,lease_owner,lease_until,cancel_requested,input_json FROM jobs WHERE job_id=?1 AND kind=?2",params![id,SCENE_TRANSCRIPTION_KIND],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).map_err(storage_error)?;
        let now = lifecycle::JobClock::now_ms(&sqlite_jobs::SystemClock)?;
        if row.0 != "running"
            || row.1 != attempt
            || row.2.as_deref() != Some(worker)
            || row.3.is_none_or(|until| until <= now)
        {
            return Err(EditorError::new(
                "JOB_LEASE_LOST",
                "V4 transcription attempt is no longer current",
            ));
        }
        if row.4 {
            return Err(EditorError::new(
                "JOB_CANCELLED",
                "V4 transcription was cancelled",
            ));
        }
        let prepared: PreparedSceneTranscription = decode_json(&row.5, "V4 transcription input")?;
        if &prepared.audio.authority != selected
            || prepared.audio.processing_policy != sqlite_scene_audio_jobs::policy()
        {
            return Err(access::denied());
        }
        require_scope(&tx, &context(selected), &prepared.audio.snapshot.project.id)?;
        if fixed(
            &tx,
            &prepared.audio.snapshot.project.id,
            &prepared.audio.snapshot.revision,
        )? != prepared.audio.snapshot
        {
            return Err(access::denied());
        }
        Ok(())
    }
}
