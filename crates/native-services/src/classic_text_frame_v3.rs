//! Caption V3 adapts the existing isolated visual process. Typed application
//! sessions retain font/media authority, binding, byte and receipt fences.
use crate::classic_visual::{ClassicVisualProcess, VisualProcessSession};
use opencut_contracts::EditorError;
use opencut_editor_application::{
    classic_text_frame_v2::{ClassicTextFontHandleV2, ClassicTextFrameResources},
    classic_text_frame_v3::*,
    classic_visual::*,
    project_v4_frame_jobs::LocalVisualExecutionPolicyV1,
};
use serde_json::Value;

impl<R: ClassicTextFrameResources + ?Sized> VisualProcessSession<R>
    for ClassicCaptionFrameSessionV3<'_>
{
    type Callback = CaptionFrameCallbackV3;
    type Receipt = ClassicCaptionFrameReceiptV3;
    type Output = ClassicCaptionFrameExecutionV3;
    fn policy(&self) -> &LocalVisualExecutionPolicyV1 {
        ClassicCaptionFrameSessionV3::policy(self)
    }
    fn plan(&self) -> Result<Value, EditorError> {
        ClassicCaptionFrameSessionV3::plan(self)
    }
    fn callback(
        &mut self,
        request: Self::Callback,
        resources: &mut R,
    ) -> Result<Value, EditorError> {
        ClassicCaptionFrameSessionV3::callback(self, request, resources)
    }
    fn rgba_chunk(
        &mut self,
        offset: u64,
        bytes: &[u8],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<(), EditorError> {
        ClassicCaptionFrameSessionV3::rgba_chunk(self, offset, bytes, sink)
    }
    fn finish(
        &self,
        receipt: Self::Receipt,
        quota: VisualQuotaReceipt,
        runtime_identity: ClassicVisualRuntimeReceipt,
    ) -> Result<Self::Output, EditorError> {
        self.validate_receipt(&receipt)?;
        Ok(ClassicCaptionFrameExecutionV3 {
            receipt,
            quota,
            runtime_identity,
        })
    }
}
impl ClassicCaptionFrameExecutorV3 for ClassicVisualProcess {
    fn execute_caption_frame(
        &self,
        session: &mut ClassicCaptionFrameSessionV3<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandleV2],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicCaptionFrameExecutionV3, EditorError> {
        #[cfg(unix)]
        {
            self.render_session(
                session,
                resources,
                "beginCaptionFrameV3",
                serde_json::to_value(handles).map_err(|_| {
                    EditorError::new("INVALID_VISUAL_PLAN", "Invalid V3 media handles")
                })?,
                Some(serde_json::to_value(fonts).map_err(|_| {
                    EditorError::new("INVALID_VISUAL_PLAN", "Invalid V3 font handles")
                })?),
                sink,
            )
        }
        #[cfg(not(unix))]
        {
            let _ = (session, resources, handles, fonts, sink);
            Err(EditorError::new(
                "CAPABILITY_UNAVAILABLE",
                "Isolated caption execution requires process-group termination",
            ))
        }
    }
}
