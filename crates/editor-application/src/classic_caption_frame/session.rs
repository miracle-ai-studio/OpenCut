use super::*;
use crate::{classic_text_frame::*, classic_visual::*};
use serde_json::Value;
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum CaptionFrameCallback {
    Caption(CaptionCallback),
    Original(TextFrameCallback),
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", content = "args", deny_unknown_fields)]
pub enum CaptionCallback {
    #[serde(rename = "bindCaptionFrameV3")]
    Bind {
        rasters: Vec<TextRasterReceipt>,
        captions: Vec<CaptionLayoutReceipt>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicCaptionFrameReceipt {
    pub identity: opencut_contracts::text_frame::TextFrameIdentity,
    pub engine_identity: String,
    pub execution: ClassicTextFrameReceipt,
    pub captions: Vec<CaptionLayoutReceipt>,
}
#[derive(Debug)]
pub struct ClassicCaptionFrameExecution {
    pub receipt: ClassicCaptionFrameReceipt,
    pub quota: VisualQuotaReceipt,
    pub runtime_identity: ClassicVisualRuntimeReceipt,
}
pub trait ClassicCaptionFrameExecutor {
    fn execute_caption_frame(
        &self,
        session: &mut ClassicCaptionFrameSession<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandle],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicCaptionFrameExecution, EditorError>;
}
pub struct ClassicCaptionFrameSession<'a> {
    prepared: &'a PreparedCaptionFrame,
    original: ClassicTextFrameSession<'a>,
    plan: CaptionFramePlan,
    bound_captions: Option<Vec<CaptionLayoutReceipt>>,
}
impl<'a> ClassicCaptionFrameSession<'a> {
    pub fn new(
        prepared: &'a PreparedCaptionFrame,
        policy: &LocalVisualExecutionPolicy,
        coverage: &[NativeTextCoverage],
    ) -> Result<Self, EditorError> {
        Ok(Self {
            prepared,
            original: ClassicTextFrameSession::new(prepared.execution(), policy, coverage)?,
            plan: plan_caption_frame(prepared, policy, coverage)?,
            bound_captions: None,
        })
    }
    pub fn policy(&self) -> &LocalVisualExecutionPolicy {
        self.original.policy()
    }
    pub fn typed_plan(&self) -> &CaptionFramePlan {
        &self.plan
    }
    pub fn plan(&self) -> Result<Value, EditorError> {
        serde_json::to_value(&self.plan).map_err(|_| invalid("Cannot serialize V3 plan"))
    }
    pub fn callback(
        &mut self,
        request: CaptionFrameCallback,
        resources: &mut (impl ClassicTextFrameResources + ?Sized),
    ) -> Result<Value, EditorError> {
        resources.checkpoint()?;
        match request {
            CaptionFrameCallback::Original(TextFrameCallback::Text(TextCallback::Bind(_))) => Err(
                invalid("V3 must bind caption facts through its versioned operation"),
            ),
            CaptionFrameCallback::Original(request) => self.original.callback(request, resources),
            CaptionFrameCallback::Caption(CaptionCallback::Bind { rasters, captions }) => {
                if self.bound_captions.is_some() {
                    return Err(invalid("V3 caption binding repeated"));
                }
                let result =
                    self.original
                        .bind_caption_sources(self.prepared, &rasters, &captions)?;
                self.bound_captions = Some(captions);
                Ok(result)
            }
        }
    }
    pub fn use_canvas_output(&mut self) -> Result<(), EditorError> {
        self.original.use_canvas_output()
    }
    pub fn rgba_chunk(
        &mut self,
        offset: u64,
        bytes: &[u8],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<(), EditorError> {
        self.original.rgba_chunk(offset, bytes, sink)
    }
    pub fn validate_receipt(
        &self,
        receipt: &ClassicCaptionFrameReceipt,
    ) -> Result<(), EditorError> {
        if receipt.identity != self.plan.identity
            || receipt.engine_identity != CLASSIC_CAPTION_FRAME_PROFILE
            || self.bound_captions.as_ref() != Some(&receipt.captions)
        {
            return Err(invalid(
                "V3 receipt differs from original identity/caption binding",
            ));
        }
        self.original.validate_receipt(&receipt.execution)
    }
}
