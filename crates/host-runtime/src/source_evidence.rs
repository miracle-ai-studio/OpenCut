//! Host assembly reuses the existing decoder/cache/waiter cancellation lifecycle.
use crate::Automation;
use opencut_contracts::{EditorError, source_art::*, source_evidence::*};
struct Port<'a>(&'a mut Automation);
impl opencut_editor_application::source_evidence::SourceEvidencePort for Port<'_> {
    fn frame(&mut self, input: SourceFrameRequest) -> Result<SourceFrameResponse, EditorError> {
        crate::source_art::source_frame(self.0, input)
    }
    fn waveform(&mut self, input: SourceArtRequest) -> Result<SourceArtResponse, EditorError> {
        crate::source_art::get(self.0, input)
    }
}
pub(super) fn create(
    host: &mut Automation,
    input: SourceEvidenceRequest,
) -> Result<SourceEvidenceResult, EditorError> {
    opencut_editor_application::source_evidence::create(&mut Port(host), input)
}

pub(super) fn cancel(
    host: &mut Automation,
    input: SourceArtCancel,
) -> Result<SourceArtCancelResponse, EditorError> {
    // Tombstone later children before waiting for the active decoder to exit.
    // The same authenticated scope and fixed revision are checked by every call.
    let mut requested = false;
    for (index, kind) in (0..8)
        .rev()
        .map(|i| (i, "wave"))
        .chain((0..12).rev().map(|i| (i, "frame")))
    {
        let result = crate::source_art::cancel(
            host,
            SourceArtCancel {
                project_id: input.project_id.clone(),
                revision: input.revision.clone(),
                request_id: format!("{}:{kind}:{index}", input.request_id),
            },
        )?;
        requested |= result.cancel_requested;
    }
    Ok(SourceArtCancelResponse {
        cancel_requested: requested,
    })
}
