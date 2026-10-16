use super::*;
use crate::{classic_graphic_frame_v1::*, classic_text_frame_v2::*, classic_visual::*};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum BlendFrameCallbackV1 {
    Blend(BlendCallbackV1),
    Original(GraphicFrameCallbackV1),
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", content = "args", deny_unknown_fields)]
pub enum BlendCallbackV1 {
    #[serde(rename = "bindBlendFrameV1")]
    Bind {},
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicBlendFrameReceiptV1 {
    pub identity: opencut_contracts::text_frame_v2::TextFrameIdentityV2,
    pub engine_identity: String,
    pub execution: ClassicGraphicFrameReceiptV1,
    pub blends: Vec<LayerBlendV1>,
}
#[derive(Debug)]
pub struct ClassicBlendFrameExecutionV1 {
    pub receipt: ClassicBlendFrameReceiptV1,
    pub quota: VisualQuotaReceipt,
    pub runtime_identity: ClassicVisualRuntimeReceipt,
}
pub trait ClassicBlendFrameExecutorV1 {
    fn execute_blend_frame(
        &self,
        session: &mut ClassicBlendFrameSessionV1<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandleV2],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicBlendFrameExecutionV1, EditorError>;
}

pub struct ClassicBlendFrameSessionV1<'a> {
    prepared: &'a PreparedBlendFrameV1,
    original: ClassicGraphicFrameSessionV1<'a>,
    plan: ClassicBlendFramePlanV1,
    child: Option<BoundGraphicFrameV1>,
    bound: Option<BoundBlendFrameV1>,
}
impl<'a> ClassicBlendFrameSessionV1<'a> {
    pub fn new(
        prepared: &'a PreparedBlendFrameV1,
        policy: &LocalVisualExecutionPolicyV1,
        coverage: &[NativeTextCoverageV1],
    ) -> Result<Self, EditorError> {
        Ok(Self {
            prepared,
            original: ClassicGraphicFrameSessionV1::new(prepared.execution(), policy, coverage)?,
            plan: super::plan_v1(prepared, policy, coverage)?,
            child: None,
            bound: None,
        })
    }
    pub fn policy(&self) -> &LocalVisualExecutionPolicyV1 {
        self.original.policy()
    }
    pub fn typed_plan(&self) -> &ClassicBlendFramePlanV1 {
        &self.plan
    }
    pub fn plan(&self) -> Result<Value, EditorError> {
        serde_json::to_value(&self.plan).map_err(|_| invalid("Cannot serialize blend frame plan"))
    }
    pub fn callback(
        &mut self,
        request: BlendFrameCallbackV1,
        resources: &mut (impl ClassicTextFrameResources + ?Sized),
    ) -> Result<Value, EditorError> {
        resources.checkpoint()?;
        match request {
            BlendFrameCallbackV1::Blend(BlendCallbackV1::Bind {}) => {
                if self.bound.is_some() {
                    return Err(invalid("Blend frame binding repeated"));
                }
                let child = self.child.as_ref().ok_or_else(|| {
                    invalid("Blend binding requires successful graphic binding first")
                })?;
                let bound = bind_blend_frame_v1(self.prepared, child)?;
                let value = serde_json::to_value(&bound)
                    .map_err(|_| invalid("Cannot serialize blend frame binding"))?;
                self.bound = Some(bound);
                Ok(value)
            }
            BlendFrameCallbackV1::Original(GraphicFrameCallbackV1::Graphic(
                GraphicCallbackV1::Bind { graphics },
            )) => {
                let value = self.original.callback(
                    GraphicFrameCallbackV1::Graphic(GraphicCallbackV1::Bind { graphics }),
                    resources,
                )?;
                self.child = Some(
                    serde_json::from_value(value.clone())
                        .map_err(|_| invalid("Delegated graphic binding malformed"))?,
                );
                Ok(value)
            }
            BlendFrameCallbackV1::Original(request) => self.original.callback(request, resources),
        }
    }
    pub fn rgba_chunk(
        &mut self,
        offset: u64,
        bytes: &[u8],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<(), EditorError> {
        if self.bound.is_none() {
            return Err(invalid("Blend pixels require completed final binding"));
        }
        self.original.rgba_chunk(offset, bytes, sink)
    }
    pub fn validate_receipt(
        &self,
        receipt: &ClassicBlendFrameReceiptV1,
    ) -> Result<(), EditorError> {
        if self.bound.is_none()
            || receipt.identity != self.plan.identity
            || receipt.engine_identity != CLASSIC_BLEND_FRAME_PROFILE_V1
            || receipt.blends != self.plan.blends
        {
            return Err(invalid(
                "Blend receipt differs from fixed identity, modes or binding",
            ));
        }
        validate_blend_layers_v1(self.prepared, &receipt.blends)
            .map_err(|error| invalid(&error.message))?;
        self.original.validate_receipt(&receipt.execution)
    }
}
