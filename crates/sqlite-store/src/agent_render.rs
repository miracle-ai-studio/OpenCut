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
