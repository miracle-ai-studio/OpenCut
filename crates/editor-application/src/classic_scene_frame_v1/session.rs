use super::*;
use crate::{
    classic_blend_frame_v1::*, classic_graphic_frame_v1::*, classic_text_frame_v2::*,
    classic_text_frame_v3::*, classic_visual::*,
};
use opencut_contracts::scene_frame_v1::{SceneBackgroundV1, SceneLayerStackV1, SceneStepV1};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum SceneFrameCallbackV1 {
    Scene(SceneCallbackV1),
    Original(BlendFrameCallbackV1),
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", content = "args", deny_unknown_fields)]
pub enum SceneCallbackV1 {
    #[serde(rename = "bindSceneFrameV1")]
    Bind {},
    #[serde(rename = "resolveVerifiedFontBytes")]
    Resolve(TextFontResolveV2),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicSceneFrameReceiptV1 {
    pub identity: opencut_contracts::text_frame_v2::TextFrameIdentityV2,
    pub engine_identity: String,
    pub execution: ClassicBlendFrameReceiptV1,
    pub stacks: Vec<SceneLayerStackV1>,
    pub mask_fonts: Vec<opencut_contracts::scene_frame_v1::SceneTextMaskFontV1>,
    pub font_synthesis: Vec<opencut_contracts::scene_frame_v1::SceneFontSynthesisV1>,
    pub steps: Vec<SceneStepV1>,
    pub background: SceneBackgroundV1,
}
#[derive(Debug)]
pub struct ClassicSceneFrameExecutionV1 {
    pub receipt: ClassicSceneFrameReceiptV1,
    pub quota: VisualQuotaReceipt,
    pub runtime_identity: ClassicVisualRuntimeReceipt,
}
pub trait ClassicSceneFrameExecutorV1 {
    fn execute_scene_frame(
        &self,
        session: &mut ClassicSceneFrameSessionV1<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandleV2],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicSceneFrameExecutionV1, EditorError>;
}
pub struct ClassicSceneFrameSessionV1<'a> {
    prepared: &'a PreparedSceneFrameV1,
    policy: LocalVisualExecutionPolicyV1,
    original: ClassicBlendFrameSessionV1<'a>,
    plan: ClassicSceneFramePlanV1,
    child: Option<BoundBlendFrameV1>,
    resolved_mask_fonts: BTreeSet<String>,
    bound: Option<BoundSceneFrameV1>,
}
impl<'a> ClassicSceneFrameSessionV1<'a> {
    pub fn new(
        prepared: &'a PreparedSceneFrameV1,
        policy: &LocalVisualExecutionPolicyV1,
        coverage: &[NativeTextCoverageV1],
    ) -> Result<Self, EditorError> {
        Ok(Self {
            prepared,
            policy: policy.clone(),
            original: ClassicBlendFrameSessionV1::new(
                prepared.blend(),
                policy,
                &super::title_coverage(prepared, coverage)?,
            )?,
            plan: super::plan_v1(prepared, policy, coverage)?,
            child: None,
            resolved_mask_fonts: BTreeSet::new(),
            bound: None,
        })
    }
    pub fn policy(&self) -> &LocalVisualExecutionPolicyV1 {
        &self.policy
    }
    pub fn typed_plan(&self) -> &ClassicSceneFramePlanV1 {
        &self.plan
    }
    pub fn plan(&self) -> Result<Value, EditorError> {
        serde_json::to_value(&self.plan).map_err(|_| invalid("Cannot serialize scene plan"))
    }
    pub fn callback(
        &mut self,
        request: SceneFrameCallbackV1,
        resources: &mut (impl ClassicTextFrameResources + ?Sized),
    ) -> Result<Value, EditorError> {
        resources.checkpoint()?;
        match request {
            SceneFrameCallbackV1::Scene(SceneCallbackV1::Bind {}) => {
                if self.bound.is_some() {
                    return Err(invalid("Scene binding repeated"));
                }
                if self
                    .plan
                    .mask_fonts
                    .iter()
                    .any(|m| !self.resolved_mask_fonts.contains(&m.font.font_id))
                {
                    return Err(invalid("Scene binding requires every fixed text-mask font"));
                }
                let child = self
                    .child
                    .as_ref()
                    .ok_or_else(|| invalid("Scene binding requires completed blend binding"))?;
                let bound = bind_scene_frame_v1(self.prepared, child)?;
                let value = serde_json::to_value(&bound)
                    .map_err(|_| invalid("Cannot serialize scene binding"))?;
                self.bound = Some(bound);
                Ok(value)
            }
            SceneFrameCallbackV1::Scene(SceneCallbackV1::Resolve(request)) => {
                if self.bound.is_some() {
                    return Err(invalid("Scene font resolution after final binding"));
                }
                let mask = self
                    .plan
                    .mask_fonts
                    .iter()
                    .find(|m| m.font.font_id == request.font.font_id);
                if let Some(mask) = mask {
                    if super::resource(&mask.font) != request.font
                        || self.resolved_mask_fonts.contains(&request.font.font_id)
                    {
                        return Err(invalid("Scene mask font resolution changed or repeated"));
                    }
                }
                let title = self
                    .prepared
                    .execution()
                    .execution()
                    .execution()
                    .texts()
                    .iter()
                    .any(|p| p.font.id == request.font.font_id);
                let font_id = request.font.font_id.clone();
                let result = if title {
                    self.original.callback(
                        BlendFrameCallbackV1::Original(GraphicFrameCallbackV1::Original(
                            CaptionFrameCallbackV3::Original(TextFrameCallbackV2::Text(
                                TextCallbackV2::Resolve(request),
                            )),
                        )),
                        resources,
                    )?
                } else {
                    let mask = mask.ok_or_else(|| invalid("Font is outside the fixed scene"))?;
                    let (transfer, bytes) =
                        resources.resolve_font(&request.font, &request.handle)?;
                    if bytes.to_string() != mask.font.size {
                        return Err(invalid("Resolved scene mask font size changed"));
                    }
                    serde_json::json!({"transferId":transfer,"byteLength":bytes.to_string()})
                };
                if mask.is_some() {
                    self.resolved_mask_fonts.insert(font_id);
                }
                Ok(result)
            }
            SceneFrameCallbackV1::Original(BlendFrameCallbackV1::Blend(
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
            SceneFrameCallbackV1::Original(request) => self.original.callback(request, resources),
        }
    }
    pub fn rgba_chunk(
        &mut self,
        offset: u64,
        bytes: &[u8],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<(), EditorError> {
        if self.bound.is_none() {
            return Err(invalid("Scene pixels require completed final binding"));
        }
        self.original.rgba_chunk(offset, bytes, sink)
    }
    pub fn validate_receipt(
        &self,
        receipt: &ClassicSceneFrameReceiptV1,
    ) -> Result<(), EditorError> {
        if self.bound.is_none()
            || receipt.identity != self.plan.identity
            || receipt.engine_identity != CLASSIC_SCENE_FRAME_PROFILE_V1
            || receipt.stacks != self.plan.stacks
            || receipt.mask_fonts != self.plan.mask_fonts
            || receipt.font_synthesis != self.plan.font_synthesis
            || receipt.steps != self.plan.steps
            || receipt.background != self.plan.background
        {
            return Err(invalid("Scene receipt changed fixed identity or scenes"));
        }
        self.original.validate_receipt(&receipt.execution)
    }
}
