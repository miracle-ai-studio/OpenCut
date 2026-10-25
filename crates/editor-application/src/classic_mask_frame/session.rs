use super::*;
use crate::{classic_blend_frame::*, classic_text_frame::*, classic_visual::*};
use opencut_contracts::mask_frame::LayerMask;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum MaskFrameCallback {
    Mask(MaskCallback),
    Original(BlendFrameCallback),
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", content = "args", deny_unknown_fields)]
pub enum MaskCallback {
    #[serde(rename = "bindMaskFrameV1")]
    Bind {},
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicMaskFrameReceipt {
    pub identity: opencut_contracts::text_frame::TextFrameIdentity,
    pub engine_identity: String,
    pub execution: ClassicBlendFrameReceipt,
    pub masks: Vec<LayerMask>,
}
#[derive(Debug)]
pub struct ClassicMaskFrameExecution {
    pub receipt: ClassicMaskFrameReceipt,
    pub quota: VisualQuotaReceipt,
    pub runtime_identity: ClassicVisualRuntimeReceipt,
}
pub trait ClassicMaskFrameExecutor {
    fn execute_mask_frame(
        &self,
        session: &mut ClassicMaskFrameSession<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandle],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicMaskFrameExecution, EditorError>;
}
pub struct ClassicMaskFrameSession<'a> {
    prepared: &'a PreparedMaskFrame,
    policy: LocalVisualExecutionPolicy,
    original: ClassicBlendFrameSession<'a>,
    plan: MaskFramePlan,
    child: Option<BoundBlendFrame>,
    bound: Option<BoundMaskFrame>,
}
impl<'a> ClassicMaskFrameSession<'a> {
    pub fn new(
        prepared: &'a PreparedMaskFrame,
        policy: &LocalVisualExecutionPolicy,
        coverage: &[NativeTextCoverage],
    ) -> Result<Self, EditorError> {
        Ok(Self {
            prepared,
            policy: policy.clone(),
            original: ClassicBlendFrameSession::new(prepared.blend(), policy, coverage)?,
            plan: super::plan_frame(prepared, policy, coverage)?,
            child: None,
            bound: None,
        })
    }
    pub fn policy(&self) -> &LocalVisualExecutionPolicy {
        &self.policy
    }
    pub fn typed_plan(&self) -> &MaskFramePlan {
        &self.plan
    }
    pub fn plan(&self) -> Result<Value, EditorError> {
        serde_json::to_value(&self.plan).map_err(|_| invalid("Cannot serialize mask plan"))
    }
    pub fn callback(
        &mut self,
        request: MaskFrameCallback,
        resources: &mut (impl ClassicTextFrameResources + ?Sized),
    ) -> Result<Value, EditorError> {
        resources.checkpoint()?;
        match request {
            MaskFrameCallback::Mask(MaskCallback::Bind {}) => {
                if self.bound.is_some() {
                    return Err(invalid("Mask binding repeated"));
                }
                let child = self
                    .child
                    .as_ref()
                    .ok_or_else(|| invalid("Mask binding requires completed blend binding"))?;
                let bound = bind_mask_frame(self.prepared, child)?;
                let value = serde_json::to_value(&bound)
                    .map_err(|_| invalid("Cannot serialize mask binding"))?;
                self.bound = Some(bound);
                Ok(value)
            }
            MaskFrameCallback::Original(BlendFrameCallback::Blend(BlendCallback::Bind {})) => {
                let value = self
                    .original
                    .callback(BlendFrameCallback::Blend(BlendCallback::Bind {}), resources)?;
                self.child = Some(
                    serde_json::from_value(value.clone())
                        .map_err(|_| invalid("Invalid delegated blend binding"))?,
                );
                Ok(value)
            }
            MaskFrameCallback::Original(request) => self.original.callback(request, resources),
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
    pub fn validate_receipt(&self, receipt: &ClassicMaskFrameReceipt) -> Result<(), EditorError> {
        if self.bound.is_none()
            || receipt.identity != self.plan.identity
            || receipt.engine_identity != CLASSIC_MASK_FRAME_PROFILE
            || receipt.masks != self.plan.masks
        {
            return Err(invalid("Mask receipt changed fixed identity or masks"));
        }
        validate_mask_layers(self.prepared, &receipt.masks)
            .map_err(|e| EditorError::new(e.code, e.message))?;
        self.original.validate_receipt(&receipt.execution)
    }
}
