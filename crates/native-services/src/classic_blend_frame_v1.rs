//! Blend V1 adapts the existing isolated visual process. Typed application
//! sessions retain font/media authority, binding, byte and receipt fences.
use crate::classic_visual::{ClassicVisualProcess, VisualProcessSession};
use opencut_contracts::EditorError;
use opencut_editor_application::{
    classic_blend_frame_v1::*,
    classic_text_frame_v2::{ClassicTextFontHandleV2, ClassicTextFrameResources},
    classic_visual::*,
    project_v4_frame_jobs::LocalVisualExecutionPolicyV1,
};
use serde_json::Value;

impl<R: ClassicTextFrameResources + ?Sized> VisualProcessSession<R>
    for ClassicBlendFrameSessionV1<'_>
{
    type Callback = BlendFrameCallbackV1;
    type Receipt = ClassicBlendFrameReceiptV1;
    type Output = ClassicBlendFrameExecutionV1;
    fn policy(&self) -> &LocalVisualExecutionPolicyV1 {
        ClassicBlendFrameSessionV1::policy(self)
    }
    fn plan(&self) -> Result<Value, EditorError> {
        ClassicBlendFrameSessionV1::plan(self)
    }
    fn callback(
        &mut self,
        request: Self::Callback,
        resources: &mut R,
    ) -> Result<Value, EditorError> {
        ClassicBlendFrameSessionV1::callback(self, request, resources)
    }
    fn rgba_chunk(
        &mut self,
        offset: u64,
        bytes: &[u8],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<(), EditorError> {
        ClassicBlendFrameSessionV1::rgba_chunk(self, offset, bytes, sink)
    }
    fn finish(
        &self,
        receipt: Self::Receipt,
        quota: VisualQuotaReceipt,
        runtime_identity: ClassicVisualRuntimeReceipt,
    ) -> Result<Self::Output, EditorError> {
        self.validate_receipt(&receipt)?;
        Ok(ClassicBlendFrameExecutionV1 {
            receipt,
            quota,
            runtime_identity,
        })
    }
}
impl ClassicBlendFrameExecutorV1 for ClassicVisualProcess {
    fn execute_blend_frame(
        &self,
        session: &mut ClassicBlendFrameSessionV1<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandleV2],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicBlendFrameExecutionV1, EditorError> {
        #[cfg(unix)]
        {
            self.render_session(
                session,
                resources,
                "beginBlendFrameV1",
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
                "Isolated blend execution requires process-group termination",
            ))
        }
    }
}
