//! Mask V1 adapts the existing isolated visual process. Typed application
//! sessions retain font/media authority, binding, byte and receipt fences.
use crate::classic_visual::{ClassicVisualProcess, VisualProcessSession};
use opencut_contracts::EditorError;
use opencut_editor_application::{
    classic_mask_frame::*,
    classic_text_frame::{ClassicTextFontHandle, ClassicTextFrameResources},
    classic_visual::*,
    media_frame_jobs::LocalVisualExecutionPolicy,
};
use serde_json::Value;

impl<R: ClassicTextFrameResources + ?Sized> VisualProcessSession<R>
    for ClassicMaskFrameSession<'_>
{
    type Callback = MaskFrameCallback;
    type Receipt = ClassicMaskFrameReceipt;
    type Output = ClassicMaskFrameExecution;
    fn policy(&self) -> &LocalVisualExecutionPolicy {
        ClassicMaskFrameSession::policy(self)
    }
    fn plan(&self) -> Result<Value, EditorError> {
        ClassicMaskFrameSession::plan(self)
    }
    fn callback(
        &mut self,
        request: Self::Callback,
        resources: &mut R,
    ) -> Result<Value, EditorError> {
        ClassicMaskFrameSession::callback(self, request, resources)
    }
    fn rgba_chunk(
        &mut self,
        offset: u64,
        bytes: &[u8],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<(), EditorError> {
        ClassicMaskFrameSession::rgba_chunk(self, offset, bytes, sink)
    }
    fn finish(
        &self,
        receipt: Self::Receipt,
        quota: VisualQuotaReceipt,
        runtime_identity: ClassicVisualRuntimeReceipt,
    ) -> Result<Self::Output, EditorError> {
        self.validate_receipt(&receipt)?;
        Ok(ClassicMaskFrameExecution {
            receipt,
            quota,
            runtime_identity,
        })
    }
}
impl ClassicMaskFrameExecutor for ClassicVisualProcess {
    fn execute_mask_frame(
        &self,
        session: &mut ClassicMaskFrameSession<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandle],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicMaskFrameExecution, EditorError> {
        #[cfg(unix)]
        {
            self.render_session(
                session,
                resources,
                "beginMaskFrame",
                serde_json::to_value(handles).map_err(|_| {
                    EditorError::new("INVALID_VISUAL_PLAN", "Invalid V1 media handles")
                })?,
                Some(serde_json::to_value(fonts).map_err(|_| {
                    EditorError::new("INVALID_VISUAL_PLAN", "Invalid V1 font handles")
                })?),
                sink,
            )
        }
        #[cfg(not(unix))]
        {
            let _ = (session, resources, handles, fonts, sink);
            Err(EditorError::new(
                "CAPABILITY_UNAVAILABLE",
                "Isolated mask execution requires process-group termination",
            ))
        }
    }
}
