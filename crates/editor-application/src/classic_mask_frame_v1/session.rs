use super::*;
use crate::{classic_blend_frame_v1::*, classic_text_frame_v2::*, classic_visual::*};
use opencut_contracts::mask_frame_v1::LayerMaskV1;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum MaskFrameCallbackV1 {
    Mask(MaskCallbackV1),
    Original(BlendFrameCallbackV1),
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", content = "args", deny_unknown_fields)]
pub enum MaskCallbackV1 {
    #[serde(rename = "bindMaskFrameV1")]
    Bind {},
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicMaskFrameReceiptV1 {
    pub identity: opencut_contracts::text_frame_v2::TextFrameIdentityV2,
    pub engine_identity: String,
    pub execution: ClassicBlendFrameReceiptV1,
    pub masks: Vec<LayerMaskV1>,
}
#[derive(Debug)]
pub struct ClassicMaskFrameExecutionV1 {
    pub receipt: ClassicMaskFrameReceiptV1,
    pub quota: VisualQuotaReceipt,
    pub runtime_identity: ClassicVisualRuntimeReceipt,
}
pub trait ClassicMaskFrameExecutorV1 {
    fn execute_mask_frame(
        &self,
        session: &mut ClassicMaskFrameSessionV1<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandleV2],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicMaskFrameExecutionV1, EditorError>;
}
pub struct ClassicMaskFrameSessionV1<'a> {
    prepared: &'a PreparedMaskFrameV1,
    policy: LocalVisualExecutionPolicyV1,
    original: ClassicBlendFrameSessionV1<'a>,
    plan: ClassicMaskFramePlanV1,
    child: Option<BoundBlendFrameV1>,
    bound: Option<BoundMaskFrameV1>,
}
impl<'a> ClassicMaskFrameSessionV1<'a> {
    pub fn new(
        prepared: &'a PreparedMaskFrameV1,
        policy: &LocalVisualExecutionPolicyV1,
        coverage: &[NativeTextCoverageV1],
    ) -> Result<Self, EditorError> {
        Ok(Self {
            prepared,
            policy: policy.clone(),
            original: ClassicBlendFrameSessionV1::new(prepared.blend(), policy, coverage)?,
            plan: super::plan_v1(prepared, policy, coverage)?,
            child: None,
            bound: None,
        })
    }
    pub fn policy(&self) -> &LocalVisualExecutionPolicyV1 {
        &self.policy
    }
    pub fn typed_plan(&self) -> &ClassicMaskFramePlanV1 {
        &self.plan
    }
    pub fn plan(&self) -> Result<Value, EditorError> {
        serde_json::to_value(&self.plan).map_err(|_| invalid("Cannot serialize mask plan"))
    }
    pub fn callback(
        &mut self,
        request: MaskFrameCallbackV1,
        resources: &mut (impl ClassicTextFrameResources + ?Sized),
    ) -> Result<Value, EditorError> {
        resources.checkpoint()?;
        match request {
            MaskFrameCallbackV1::Mask(MaskCallbackV1::Bind {}) => {
                if self.bound.is_some() {
                    return Err(invalid("Mask binding repeated"));
                }
                let child = self
                    .child
                    .as_ref()
                    .ok_or_else(|| invalid("Mask binding requires completed blend binding"))?;
                let bound = bind_mask_frame_v1(self.prepared, child)?;
                let value = serde_json::to_value(&bound)
                    .map_err(|_| invalid("Cannot serialize mask binding"))?;
                self.bound = Some(bound);
                Ok(value)
            }
            MaskFrameCallbackV1::Original(BlendFrameCallbackV1::Blend(
                BlendCallbackV1::Bind {},
            )) => {
                let value = self.original.callback(
                    BlendFrameCallbackV1::Blend(BlendCallbackV1::Bind {}),
                    resources,
                )?;
                self.child = Some(
                    serde_json::from_value(value.clone())
                        .map_err(|_| invalid("Invalid delegated blend binding"))?,
                );
                Ok(value)
            }
            MaskFrameCallbackV1::Original(request) => self.original.callback(request, resources),
        }
    }
    pub fn rgba_chunk(
        &mut self,
        offset: u64,
        bytes: &[u8],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<(), EditorError> {
        if self.bound.is_none() {
            return Err(invalid("Mask pixels require completed final binding"));
        }
        self.original.rgba_chunk(offset, bytes, sink)
    }
    pub fn validate_receipt(&self, receipt: &ClassicMaskFrameReceiptV1) -> Result<(), EditorError> {
        if self.bound.is_none()
            || receipt.identity != self.plan.identity
            || receipt.engine_identity != CLASSIC_MASK_FRAME_PROFILE_V1
            || receipt.masks != self.plan.masks
        {
            return Err(invalid("Mask receipt changed fixed identity or masks"));
        }
        validate_mask_layers_v1(self.prepared, &receipt.masks)
            .map_err(|e| EditorError::new(e.code, e.message))?;
        self.original.validate_receipt(&receipt.execution)
    }
}
