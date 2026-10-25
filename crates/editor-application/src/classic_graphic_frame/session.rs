use super::*;
use crate::{classic_caption_frame::*, classic_text_frame::*, classic_visual::*};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum GraphicFrameCallback {
    Graphic(GraphicCallback),
    Original(CaptionFrameCallback),
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", content = "args", deny_unknown_fields)]
pub enum GraphicCallback {
    #[serde(rename = "bindGraphicFrameV1")]
    Bind { graphics: Vec<GraphicRasterReceipt> },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicGraphicFrameReceipt {
    pub identity: TextFrameIdentity,
    pub engine_identity: String,
    pub execution: ClassicCaptionFrameReceipt,
    pub graphics: Vec<GraphicRasterReceipt>,
}
#[derive(Debug)]
pub struct ClassicGraphicFrameExecution {
    pub receipt: ClassicGraphicFrameReceipt,
    pub quota: VisualQuotaReceipt,
    pub runtime_identity: ClassicVisualRuntimeReceipt,
}
pub trait ClassicGraphicFrameExecutor {
    fn execute_graphic_frame(
        &self,
        session: &mut ClassicGraphicFrameSession<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandle],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicGraphicFrameExecution, EditorError>;
}
pub struct ClassicGraphicFrameSession<'a> {
    prepared: &'a PreparedGraphicFrame,
    original: ClassicCaptionFrameSession<'a>,
    plan: GraphicFramePlan,
    child: Option<BoundTextFrame>,
    captions: Vec<opencut_contracts::caption_frame::CaptionLayoutReceipt>,
    bound: Option<Vec<GraphicRasterReceipt>>,
}
impl<'a> ClassicGraphicFrameSession<'a> {
    pub fn new(
        prepared: &'a PreparedGraphicFrame,
        policy: &LocalVisualExecutionPolicy,
        coverage: &[NativeTextCoverage],
    ) -> Result<Self, EditorError> {
        let plan = plan_frame(prepared, policy, coverage)?;
        // Admission subtracts all generated raster allocation before delegated admission.
        // The one aggregate GPU peak belongs to the full policy, including its child
        // receipt; no second execution or reduced/fabricated peak is claimed.
        Ok(Self {
            prepared,
            original: ClassicCaptionFrameSession::new(prepared.execution(), policy, coverage)?,
            plan,
            child: None,
            captions: Vec::new(),
            bound: None,
        })
    }
    pub fn policy(&self) -> &LocalVisualExecutionPolicy {
        self.original.policy()
    }
    pub fn typed_plan(&self) -> &GraphicFramePlan {
        &self.plan
    }
    pub fn plan(&self) -> Result<Value, EditorError> {
        serde_json::to_value(&self.plan).map_err(|_| invalid("Cannot serialize graphic frame plan"))
    }
    pub fn callback(
        &mut self,
        request: GraphicFrameCallback,
        resources: &mut (impl ClassicTextFrameResources + ?Sized),
    ) -> Result<Value, EditorError> {
        resources.checkpoint()?;
        match request {
            GraphicFrameCallback::Original(
                request @ CaptionFrameCallback::Original(TextFrameCallback::Media(
                    VisualCallback::Admit(_),
                )),
            ) => {
                // Keep the delegated authorization/once-only admission fence,
                // but grant only media space left after generated graphics.
                let element_id = match &request {
                    CaptionFrameCallback::Original(TextFrameCallback::Media(
                        VisualCallback::Admit(v),
                    )) => &v.element_id,
                    _ => unreachable!("Matched decode admission"),
                };
                let admission = ClassicVisualSession::new(
                    self.prepared.execution().execution().media(),
                    &graphic_media_policy(self.prepared, self.original.policy())?,
                )?
                .decode_admission_for(element_id)?;
                self.original.callback(request, resources)?;
                serde_json::to_value(admission)
                    .map_err(|_| invalid("Cannot serialize graphic media admission"))
            }
            GraphicFrameCallback::Graphic(GraphicCallback::Bind { graphics }) => {
                if self.bound.is_some() {
                    return Err(invalid("Graphic binding repeated"));
                }
                let child = self.child.as_ref().ok_or_else(|| {
                    invalid("Graphic binding requires successful caption binding first")
                })?;
                // The complete peak remains under the full policy; actual media
                // decode dimensions must still leave room for generated rasters.
                ClassicVisualSession::new(
                    self.prepared.execution().execution().media(),
                    &graphic_media_policy(self.prepared, self.original.policy())?,
                )?
                .project_decoded_sources(&child.decoded)?;
                let bound = bind_graphic_frame(self.prepared, child, &self.captions, &graphics)?;
                let value = serde_json::to_value(bound)
                    .map_err(|_| invalid("Cannot serialize graphic binding"))?;
                self.bound = Some(graphics);
                Ok(value)
            }
            GraphicFrameCallback::Original(CaptionFrameCallback::Caption(
                CaptionCallback::Bind { rasters, captions },
            )) => {
                let value = self.original.callback(
                    CaptionFrameCallback::Caption(CaptionCallback::Bind {
                        rasters,
                        captions: captions.clone(),
                    }),
                    resources,
                )?;
                self.child = Some(
                    serde_json::from_value(value.clone())
                        .map_err(|_| invalid("Delegated caption binding malformed"))?,
                );
                self.captions = captions;
                Ok(value)
            }
            GraphicFrameCallback::Original(request) => self.original.callback(request, resources),
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
            return Err(invalid(
                "Graphic pixels require completed aggregate binding",
            ));
        }
        self.original.rgba_chunk(offset, bytes, sink)
    }
    pub fn validate_receipt(
        &self,
        receipt: &ClassicGraphicFrameReceipt,
    ) -> Result<(), EditorError> {
        if receipt.identity != self.plan.identity
            || receipt.engine_identity != CLASSIC_GRAPHIC_FRAME_PROFILE
            || self.bound.as_ref() != Some(&receipt.graphics)
        {
            return Err(invalid(
                "Graphic receipt differs from original identity/binding",
            ));
        }
        validate_graphic_receipts(self.prepared, &receipt.graphics).map_err(error)?;
        self.original.validate_receipt(&receipt.execution)
    }
}
