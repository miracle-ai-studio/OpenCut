//! Shared render dispatch is an application port, not a tool/CLI invocation.
use crate::JobRecord;
use opencut_api_contracts::agent_render::*;
use opencut_contracts::EditorError;
pub trait RenderSubmission {
    fn frame(&mut self, input: FrameRenderInput) -> Result<JobRecord, EditorError>;
    fn audio(&mut self, input: AudioRenderInput) -> Result<JobRecord, EditorError>;
    fn video(&mut self, input: VideoRenderInput) -> Result<JobRecord, EditorError>;
    fn cached_video(&mut self,input:VideoRenderInput)->Result<Option<JobRecord>,EditorError>;
}
pub fn submit(
    port: &mut impl RenderSubmission,
    input: RenderRequest,
) -> Result<JobRecord, EditorError> {
    match input {
        RenderRequest::Frame(input) => port.frame(input),
        RenderRequest::Audio(input) => port.audio(input),
        RenderRequest::Video(input) => port.video(input),
    }
}
