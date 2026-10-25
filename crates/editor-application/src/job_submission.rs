//! Application dispatch; the host supplies native submission ports.
use crate::{
    JobRecord,
    agent_render::{self, RenderSubmission},
};
use opencut_api_contracts::job_submission::SubmitJob;
use opencut_contracts::EditorError;
use opencut_job_contracts::{
    JobStartInput, asset_analysis::SubmitAssetAnalysis,
    scene_transcription::SceneTranscriptionRequest,
};
pub trait JobSubmission: RenderSubmission {
    fn asset_analysis(&mut self, input: SubmitAssetAnalysis) -> Result<JobRecord, EditorError>;
    fn scene_transcription(
        &mut self,
        input: SceneTranscriptionRequest,
    ) -> Result<JobRecord, EditorError>;
    /// Only the four source operations below reach this owner-only native port.
    fn source_job(&mut self, input: JobStartInput) -> Result<JobRecord, EditorError>;
}
pub fn submit(port: &mut impl JobSubmission, input: SubmitJob) -> Result<JobRecord, EditorError> {
    match input {
        SubmitJob::Render { request } => agent_render::submit(port, request),
        SubmitJob::AssetAnalysis { request } => port.asset_analysis(request),
        SubmitJob::SceneTranscription { request } => port.scene_transcription(request),
        SubmitJob::Probe {
            idempotency_key,
            input,
        } => port.source_job(JobStartInput::Probe {
            idempotency_key,
            input,
        }),
        SubmitJob::SourceFrame {
            idempotency_key,
            input,
        } => port.source_job(JobStartInput::Frame {
            idempotency_key,
            input,
        }),
        SubmitJob::MediaExport {
            idempotency_key,
            input,
        } => port.source_job(JobStartInput::Export {
            idempotency_key,
            input,
        }),
        SubmitJob::Verify {
            idempotency_key,
            input,
        } => port.source_job(JobStartInput::Verify {
            idempotency_key,
            input,
        }),
    }
}
