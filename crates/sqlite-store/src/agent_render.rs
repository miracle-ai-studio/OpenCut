//! Adapt one public request to the original queue identity and publication fences.
use crate::*;
use opencut_api_contracts::agent_render::*;
use opencut_job_contracts::{export_video::*, scene_audio::*};
impl opencut_editor_application::agent_render::RenderSubmission for EditorApplication {
    fn frame(&mut self, input: FrameRenderInput) -> Result<JobRecord, EditorError> {
        sqlite_scene_text_frame_jobs::start_scene_frame_png(self, input)
    }
    fn audio(&mut self, input: AudioRenderInput) -> Result<JobRecord, EditorError> {
        match input.range {
            Some(range) => sqlite_scene_audio_jobs::start_audio_range_wav(
                self,
                SubmitAudioRangeWav {
                    idempotency_key: input.idempotency_key,
                    project_id: input.project_id,
                    revision: input.revision,
                    sequence_id: input.sequence_id,
                    sample_rate: input.sample_rate,
                    range,
                },
            ),
            None => sqlite_scene_audio_jobs::start_audio_wav(
                self,
                SubmitAudioWav {
                    idempotency_key: input.idempotency_key,
                    project_id: input.project_id,
                    revision: input.revision,
                    sequence_id: input.sequence_id,
                    sample_rate: input.sample_rate,
                },
            ),
        }
    }
    fn cached_video(&mut self,input:VideoRenderInput)->Result<Option<JobRecord>,EditorError>{
        let range=input.range.ok_or_else(||EditorError::new("INVALID_ARGUMENT","Preview reuse requires an exact admitted range"))?;
        opencut_editor_application::export_video::cached_range(self,SubmitExportVideoRange{idempotency_key:input.idempotency_key,project_id:input.project_id,revision:input.revision,sequence_id:input.sequence_id,options:input.options,range})
    }
    fn preview_video(&mut self,input:VideoRenderInput,content_key:&str,reuse_only:bool)->Result<Option<JobRecord>,EditorError>{
        use opencut_editor_application::{media_video_jobs::MediaVideoJobSubmissionStore,preview_cache::segment_key};
        // Both revisions stay immutable. Current submission access is checked
        // before looking up any old receipt, then old-job read access is checked
        // again. Reuse never rewrites its revision, result or publication proof.
        self.fixed_scene_video_snapshot(&input.project_id,&input.revision)?;
        let stored:Option<String>=self.connection.query_row(
            "SELECT result_json FROM idempotency WHERE operation_family='create_job' AND scope='workspace' AND idempotency_key=?1 AND result_kind='job'",
            [&input.idempotency_key],|row|row.get(0)).optional().map_err(storage_error)?;
        if let Some(stored)=stored {
            let original:JobRecord=decode_json(&stored,"preview receipt")?;
            let job=self.get_job(&original.job_id)?;
            let prepared:opencut_editor_application::export_video::PreparedExportVideo=decode_json(&serde_json::to_string(&job.input).map_err(|_|EditorError::new("INVALID_STORED_RESULT","Invalid preview input"))?,"preview input")?;
            let range=prepared.range.as_ref().ok_or_else(||EditorError::new("INVALID_STORED_RESULT","Missing preview interval"))?;
            if job.kind!=EXPORT_VIDEO_RANGE_KIND||prepared.visual_profile!=opencut_contracts::export_video::ExportVisualProfile::SceneV1
                ||prepared.execution.snapshot.project.id!=input.project_id||prepared.execution.sequence_id!=input.sequence_id
                ||prepared.options!=input.options||Some(range)!=input.range.as_ref()
                ||segment_key(&prepared.execution.snapshot,&prepared.execution.sequence_id,&prepared.execution.processing_policy,&prepared.options,range)?!=content_key {
                return Err(EditorError::new("INVALID_STORED_RESULT","Preview receipt differs from the authorized visual window"));
            }
            return Ok(Some(job));
        }
        if reuse_only {return Ok(None);}
        match self.video(input.clone()) {
            Ok(job)=>Ok(Some(job)),
            // Another fixed revision may win acceptance between lookup and
            // commit. Only the same content-verified receipt can resolve it.
            Err(error) if error.code=="IDEMPOTENCY_KEY_REUSED"=>self.preview_video(input,content_key,true)?.map(Some).ok_or(error),
            Err(error)=>Err(error),
        }
    }
    fn video(&mut self, input: VideoRenderInput) -> Result<JobRecord, EditorError> {
        match input.range {
            Some(range) => export_video::start_range(
                self,
                SubmitExportVideoRange {
                    idempotency_key: input.idempotency_key,
                    project_id: input.project_id,
                    revision: input.revision,
                    sequence_id: input.sequence_id,
                    options: input.options,
                    range,
                },
            ),
            None => export_video::start_export(
                self,
                SubmitExportVideo {
                    idempotency_key: input.idempotency_key,
                    project_id: input.project_id,
                    revision: input.revision,
                    sequence_id: input.sequence_id,
                    options: input.options,
                },
            ),
        }
    }
}
