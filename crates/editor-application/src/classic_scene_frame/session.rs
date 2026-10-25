use super::*;
use crate::{
    classic_blend_frame::*, classic_caption_frame::*, classic_graphic_frame::*,
    classic_text_frame::*, classic_visual::*,
};
use opencut_contracts::scene_frame::{SceneBackground, SceneLayerStack, SceneStep};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum SceneFrameCallback {
    Scene(SceneCallback),
    Original(BlendFrameCallback),
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", content = "args", deny_unknown_fields)]
pub enum SceneCallback {
    #[serde(rename = "bindSceneFrameV1")]
    Bind {},
    #[serde(rename = "resolveVerifiedFontBytes")]
    Resolve(TextFontResolve),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicSceneFrameReceipt {
    pub identity: opencut_contracts::text_frame::TextFrameIdentity,
    pub engine_identity: String,
    pub execution: ClassicBlendFrameReceipt,
    pub stacks: Vec<SceneLayerStack>,
    pub mask_fonts: Vec<opencut_contracts::scene_frame::SceneTextMaskFont>,
    pub font_synthesis: Vec<opencut_contracts::scene_frame::SceneFontSynthesis>,
    pub steps: Vec<SceneStep>,
    pub background: SceneBackground,
}
#[derive(Debug)]
pub struct ClassicSceneFrameExecution {
    pub receipt: ClassicSceneFrameReceipt,
    pub quota: VisualQuotaReceipt,
    pub runtime_identity: ClassicVisualRuntimeReceipt,
}
pub trait ClassicSceneFrameExecutor {
    fn execute_scene_frame(
        &self,
        session: &mut ClassicSceneFrameSession<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandle],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicSceneFrameExecution, EditorError>;
}
pub struct ClassicSceneFrameSession<'a> {
    prepared: &'a PreparedSceneFrame,
    policy: LocalVisualExecutionPolicy,
    original: ClassicBlendFrameSession<'a>,
    plan: SceneFramePlan,
    child: Option<BoundBlendFrame>,
    resolved_mask_fonts: BTreeSet<String>,
    bound: Option<BoundSceneFrame>,
}
impl<'a> ClassicSceneFrameSession<'a> {
    pub fn new(
        prepared: &'a PreparedSceneFrame,
        policy: &LocalVisualExecutionPolicy,
        coverage: &[NativeTextCoverage],
    ) -> Result<Self, EditorError> {
        Ok(Self {
            prepared,
            policy: policy.clone(),
            original: ClassicBlendFrameSession::new(
                prepared.blend(),
                policy,
                &super::title_coverage(prepared, coverage)?,
            )?,
            plan: super::plan_frame(prepared, policy, coverage)?,
            child: None,
            resolved_mask_fonts: BTreeSet::new(),
            bound: None,
        })
    }
    pub fn policy(&self) -> &LocalVisualExecutionPolicy {
        &self.policy
    }
    pub fn typed_plan(&self) -> &SceneFramePlan {
        &self.plan
    }
    pub fn plan(&self) -> Result<Value, EditorError> {
        serde_json::to_value(&self.plan).map_err(|_| invalid("Cannot serialize scene plan"))
    }
    pub fn callback(
        &mut self,
        request: SceneFrameCallback,
        resources: &mut (impl ClassicTextFrameResources + ?Sized),
    ) -> Result<Value, EditorError> {
        resources.checkpoint()?;
        match request {
            SceneFrameCallback::Original(
                request @ BlendFrameCallback::Original(GraphicFrameCallback::Original(
                    CaptionFrameCallback::Original(TextFrameCallback::Media(
                        VisualCallback::Admit(_),
                    )),
                )),
            ) => {
                // Scene scratch/masks/backdrop, graphics and text all remain
                // reserved before the adapter allocates any media decoder.
                let element_id = match &request {
                    BlendFrameCallback::Original(GraphicFrameCallback::Original(
                        CaptionFrameCallback::Original(TextFrameCallback::Media(
                            VisualCallback::Admit(v),
                        )),
                    )) => &v.element_id,
                    _ => unreachable!("Matched decode admission"),
                };
                let scene_policy = super::scene_execution_policy(self.prepared, &self.policy)?;
                let media_policy = crate::classic_graphic_frame::graphic_media_policy(
                    self.prepared.execution(),
                    &scene_policy,
                )?;
                let admission = ClassicVisualSession::new(
                    self.prepared.execution().execution().execution().media(),
                    &media_policy,
                )?
                .decode_admission_for(element_id)?;
                self.original.callback(request, resources)?;
                serde_json::to_value(admission)
                    .map_err(|_| invalid("Cannot serialize scene media admission"))
            }
            SceneFrameCallback::Scene(SceneCallback::Bind {}) => {
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
                let bound = bind_scene_frame(self.prepared, child)?;
                let value = serde_json::to_value(&bound)
                    .map_err(|_| invalid("Cannot serialize scene binding"))?;
                self.bound = Some(bound);
                Ok(value)
            }
            SceneFrameCallback::Scene(SceneCallback::Resolve(request)) => {
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
                        BlendFrameCallback::Original(GraphicFrameCallback::Original(
                            CaptionFrameCallback::Original(TextFrameCallback::Text(
                                TextCallback::Resolve(request),
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
            SceneFrameCallback::Original(BlendFrameCallback::Blend(BlendCallback::Bind {})) => {
                let value = self
                    .original
                    .callback(BlendFrameCallback::Blend(BlendCallback::Bind {}), resources)?;
                self.child = Some(
                    serde_json::from_value(value.clone())
                        .map_err(|_| invalid("Invalid delegated blend binding"))?,
                );
                Ok(value)
            }
            SceneFrameCallback::Original(request) => self.original.callback(request, resources),
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
            return Err(invalid("Scene pixels require completed final binding"));
        }
        self.original.rgba_chunk(offset, bytes, sink)
    }
    pub fn validate_receipt(&self, receipt: &ClassicSceneFrameReceipt) -> Result<(), EditorError> {
        if self.bound.is_none()
            || receipt.identity != self.plan.identity
            || receipt.engine_identity != CLASSIC_SCENE_FRAME_PROFILE
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
