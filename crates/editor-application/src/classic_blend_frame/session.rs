use super::*;
use crate::{classic_graphic_frame::*, classic_text_frame::*, classic_visual::*};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum BlendFrameCallback {
    Blend(BlendCallback),
    Original(GraphicFrameCallback),
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", content = "args", deny_unknown_fields)]
pub enum BlendCallback {
    #[serde(rename = "bindBlendFrameV1")]
    Bind {},
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicBlendFrameReceipt {
    pub identity: opencut_contracts::text_frame::TextFrameIdentity,
    pub engine_identity: String,
    pub execution: ClassicGraphicFrameReceipt,
    pub blends: Vec<LayerBlend>,
}
#[derive(Debug)]
pub struct ClassicBlendFrameExecution {
    pub receipt: ClassicBlendFrameReceipt,
    pub quota: VisualQuotaReceipt,
    pub runtime_identity: ClassicVisualRuntimeReceipt,
}
pub trait ClassicBlendFrameExecutor {
    fn execute_blend_frame(
        &self,
        session: &mut ClassicBlendFrameSession<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandle],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicBlendFrameExecution, EditorError>;
}

pub struct ClassicBlendFrameSession<'a> {
    prepared: &'a PreparedBlendFrame,
    original: ClassicGraphicFrameSession<'a>,
    plan: BlendFramePlan,
    child: Option<BoundGraphicFrame>,
    bound: Option<BoundBlendFrame>,
}
impl<'a> ClassicBlendFrameSession<'a> {
    pub fn new(
        prepared: &'a PreparedBlendFrame,
        policy: &LocalVisualExecutionPolicy,
        coverage: &[NativeTextCoverage],
    ) -> Result<Self, EditorError> {
        Ok(Self {
            prepared,
            original: ClassicGraphicFrameSession::new(prepared.execution(), policy, coverage)?,
            plan: super::plan_frame(prepared, policy, coverage)?,
            child: None,
            bound: None,
        })
    }
    pub fn policy(&self) -> &LocalVisualExecutionPolicy {
        self.original.policy()
    }
    pub fn typed_plan(&self) -> &BlendFramePlan {
        &self.plan
    }
    pub fn plan(&self) -> Result<Value, EditorError> {
        serde_json::to_value(&self.plan).map_err(|_| invalid("Cannot serialize blend frame plan"))
    }
    pub fn callback(
        &mut self,
        request: BlendFrameCallback,
        resources: &mut (impl ClassicTextFrameResources + ?Sized),
    ) -> Result<Value, EditorError> {
        resources.checkpoint()?;
        match request {
            BlendFrameCallback::Blend(BlendCallback::Bind {}) => {
                if self.bound.is_some() {
                    return Err(invalid("Blend frame binding repeated"));
                }
                let child = self.child.as_ref().ok_or_else(|| {
                    invalid("Blend binding requires successful graphic binding first")
                })?;
                let bound = bind_blend_frame(self.prepared, child)?;
                let value = serde_json::to_value(&bound)
                    .map_err(|_| invalid("Cannot serialize blend frame binding"))?;
                self.bound = Some(bound);
                Ok(value)
            }
            BlendFrameCallback::Original(GraphicFrameCallback::Graphic(
                GraphicCallback::Bind { graphics },
            )) => {
                let value = self.original.callback(
                    GraphicFrameCallback::Graphic(GraphicCallback::Bind { graphics }),
                    resources,
                )?;
                self.child = Some(
                    serde_json::from_value(value.clone())
                        .map_err(|_| invalid("Delegated graphic binding malformed"))?,
                );
                Ok(value)
            }
            BlendFrameCallback::Original(request) => self.original.callback(request, resources),
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
        if self.bound.is_none() {
            return Err(invalid("Blend pixels require completed final binding"));
        }
        self.original.rgba_chunk(offset, bytes, sink)
    }
    pub fn validate_receipt(&self, receipt: &ClassicBlendFrameReceipt) -> Result<(), EditorError> {
        if self.bound.is_none()
            || receipt.identity != self.plan.identity
            || receipt.engine_identity != CLASSIC_BLEND_FRAME_PROFILE
            || receipt.blends != self.plan.blends
        {
            return Err(invalid(
                "Blend receipt differs from fixed identity, modes or binding",
            ));
        }
        validate_blend_layers(self.prepared, &receipt.blends)
            .map_err(|error| invalid(&error.message))?;
        self.original.validate_receipt(&receipt.execution)
    }
}
