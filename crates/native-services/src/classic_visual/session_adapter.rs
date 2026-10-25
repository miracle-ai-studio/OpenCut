//! Typed session adaptation only. Both frame versions use the same native
//! process lifetime, packet limits, cancellation and quota verification.
use opencut_contracts::EditorError;
use opencut_editor_application::media_frame_jobs::LocalVisualExecutionPolicy;
use opencut_editor_application::{classic_text_frame::*, classic_visual::*};
use serde::de::DeserializeOwned;
use serde_json::Value;

pub(crate) trait VisualProcessSession<R: ClassicVisualResources + ?Sized> {
    type Callback: DeserializeOwned;
    type Receipt: DeserializeOwned;
    type Output;
    fn policy(&self) -> &LocalVisualExecutionPolicy;
    fn plan(&self) -> Result<Value, EditorError>;
    fn callback(
        &mut self,
        request: Self::Callback,
        resources: &mut R,
    ) -> Result<Value, EditorError>;
    fn rgba_chunk(
        &mut self,
        offset: u64,
        bytes: &[u8],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<(), EditorError>;
    fn finish(
        &self,
        receipt: Self::Receipt,
        quota: VisualQuotaReceipt,
        runtime: ClassicVisualRuntimeReceipt,
    ) -> Result<Self::Output, EditorError>;
}
impl<R: ClassicVisualResources + ?Sized> VisualProcessSession<R> for ClassicVisualSession<'_> {
    type Callback = VisualCallback;
    type Receipt = ClassicVisualRenderReceipt;
    type Output = ClassicVisualExecutionReceipt;
    fn policy(&self) -> &LocalVisualExecutionPolicy {
        ClassicVisualSession::policy(self)
    }
    fn plan(&self) -> Result<Value, EditorError> {
        ClassicVisualSession::plan(self)
    }
    fn callback(
        &mut self,
        request: Self::Callback,
        resources: &mut R,
    ) -> Result<Value, EditorError> {
        ClassicVisualSession::callback(self, request, resources)
    }
    fn rgba_chunk(
        &mut self,
        offset: u64,
        bytes: &[u8],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<(), EditorError> {
        ClassicVisualSession::rgba_chunk(self, offset, bytes, sink)
    }
    fn finish(
        &self,
        receipt: Self::Receipt,
        quota: VisualQuotaReceipt,
        runtime_identity: ClassicVisualRuntimeReceipt,
    ) -> Result<Self::Output, EditorError> {
        self.validate_receipt(&receipt)?;
        Ok(ClassicVisualExecutionReceipt {
            receipt,
            quota,
            runtime_identity,
        })
    }
}
impl<R: ClassicTextFrameResources + ?Sized> VisualProcessSession<R>
    for ClassicTextFrameSession<'_>
{
    type Callback = TextFrameCallback;
    type Receipt = ClassicTextFrameReceipt;
    type Output = ClassicTextFrameExecution;
    fn policy(&self) -> &LocalVisualExecutionPolicy {
        ClassicTextFrameSession::policy(self)
    }
    fn plan(&self) -> Result<Value, EditorError> {
        ClassicTextFrameSession::plan(self)
    }
    fn callback(
        &mut self,
        request: Self::Callback,
        resources: &mut R,
    ) -> Result<Value, EditorError> {
        ClassicTextFrameSession::callback(self, request, resources)
    }
    fn rgba_chunk(
        &mut self,
        offset: u64,
        bytes: &[u8],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<(), EditorError> {
        ClassicTextFrameSession::rgba_chunk(self, offset, bytes, sink)
    }
    fn finish(
        &self,
        receipt: Self::Receipt,
        quota: VisualQuotaReceipt,
        runtime_identity: ClassicVisualRuntimeReceipt,
    ) -> Result<Self::Output, EditorError> {
        self.validate_receipt(&receipt)?;
        Ok(ClassicTextFrameExecution {
            receipt,
            quota,
            runtime_identity,
        })
    }
}
