use super::*;
use crate::{classic_text_frame_v2::*, classic_visual::*};
use serde_json::Value;
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum CaptionFrameCallbackV3 {
    Caption(CaptionCallbackV3),
    Original(TextFrameCallbackV2),
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", content = "args", deny_unknown_fields)]
pub enum CaptionCallbackV3 {
    #[serde(rename = "bindCaptionFrameV3")]
    Bind {
        rasters: Vec<TextRasterReceiptV2>,
        captions: Vec<CaptionLayoutReceiptV3>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicCaptionFrameReceiptV3 {
    pub identity: opencut_contracts::text_frame_v2::TextFrameIdentityV2,
    pub engine_identity: String,
    pub execution: ClassicTextFrameReceiptV2,
    pub captions: Vec<CaptionLayoutReceiptV3>,
}
#[derive(Debug)]
pub struct ClassicCaptionFrameExecutionV3 {
    pub receipt: ClassicCaptionFrameReceiptV3,
    pub quota: VisualQuotaReceipt,
    pub runtime_identity: ClassicVisualRuntimeReceipt,
}
pub trait ClassicCaptionFrameExecutorV3 {
    fn execute_caption_frame(
        &self,
        session: &mut ClassicCaptionFrameSessionV3<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandleV2],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicCaptionFrameExecutionV3, EditorError>;
}
pub struct ClassicCaptionFrameSessionV3<'a> {
    prepared: &'a PreparedTextFrameV3,
    original: ClassicTextFrameSessionV2<'a>,
    plan: ClassicTextFramePlanV3,
    bound_captions: Option<Vec<CaptionLayoutReceiptV3>>,
}
impl<'a> ClassicCaptionFrameSessionV3<'a> {
    pub fn new(
        prepared: &'a PreparedTextFrameV3,
        policy: &LocalVisualExecutionPolicyV1,
        coverage: &[NativeTextCoverageV1],
    ) -> Result<Self, EditorError> {
        Ok(Self {
            prepared,
            original: ClassicTextFrameSessionV2::new(prepared.execution(), policy, coverage)?,
            plan: plan_v3(prepared, policy, coverage)?,
            bound_captions: None,
        })
    }
    pub fn policy(&self) -> &LocalVisualExecutionPolicyV1 {
        self.original.policy()
    }
    pub fn typed_plan(&self) -> &ClassicTextFramePlanV3 {
        &self.plan
    }
    pub fn plan(&self) -> Result<Value, EditorError> {
        serde_json::to_value(&self.plan).map_err(|_| invalid("Cannot serialize V3 plan"))
    }
    pub fn callback(
        &mut self,
        request: CaptionFrameCallbackV3,
        resources: &mut (impl ClassicTextFrameResources + ?Sized),
    ) -> Result<Value, EditorError> {
        resources.checkpoint()?;
        match request {
            CaptionFrameCallbackV3::Original(TextFrameCallbackV2::Text(TextCallbackV2::Bind(
                _,
            ))) => Err(invalid(
                "V3 must bind caption facts through its versioned operation",
            )),
            CaptionFrameCallbackV3::Original(request) => self.original.callback(request, resources),
            CaptionFrameCallbackV3::Caption(CaptionCallbackV3::Bind { rasters, captions }) => {
                if self.bound_captions.is_some() {
                    return Err(invalid("V3 caption binding repeated"));
                }
                let result =
                    self.original
                        .bind_caption_sources_v3(self.prepared, &rasters, &captions)?;
                self.bound_captions = Some(captions);
                Ok(result)
            }
        }
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
        receipt: &ClassicCaptionFrameReceiptV3,
    ) -> Result<(), EditorError> {
        if receipt.identity != self.plan.identity
            || receipt.engine_identity != CLASSIC_TEXT_FRAME_PROFILE_V3
            || self.bound_captions.as_ref() != Some(&receipt.captions)
        {
            return Err(invalid(
                "V3 receipt differs from original identity/caption binding",
            ));
        }
        self.original.validate_receipt(&receipt.execution)
    }
}
