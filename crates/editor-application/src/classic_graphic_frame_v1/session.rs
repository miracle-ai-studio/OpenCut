use super::*;
use crate::{classic_text_frame_v2::*, classic_text_frame_v3::*, classic_visual::*};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum GraphicFrameCallbackV1 {
    Graphic(GraphicCallbackV1),
    Original(CaptionFrameCallbackV3),
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", content = "args", deny_unknown_fields)]
pub enum GraphicCallbackV1 {
    #[serde(rename = "bindGraphicFrameV1")]
    Bind {
        graphics: Vec<GraphicRasterReceiptV1>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicGraphicFrameReceiptV1 {
    pub identity: TextFrameIdentityV2,
    pub engine_identity: String,
    pub execution: ClassicCaptionFrameReceiptV3,
    pub graphics: Vec<GraphicRasterReceiptV1>,
}
#[derive(Debug)]
pub struct ClassicGraphicFrameExecutionV1 {
    pub receipt: ClassicGraphicFrameReceiptV1,
    pub quota: VisualQuotaReceipt,
    pub runtime_identity: ClassicVisualRuntimeReceipt,
}
pub trait ClassicGraphicFrameExecutorV1 {
    fn execute_graphic_frame(
        &self,
        session: &mut ClassicGraphicFrameSessionV1<'_>,
        resources: &mut dyn ClassicTextFrameResources,
        handles: &[ClassicVisualResourceHandle],
        fonts: &[ClassicTextFontHandleV2],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<ClassicGraphicFrameExecutionV1, EditorError>;
}
pub struct ClassicGraphicFrameSessionV1<'a> {
    prepared: &'a PreparedGraphicFrameV1,
    original: ClassicCaptionFrameSessionV3<'a>,
    plan: ClassicGraphicFramePlanV1,
    child: Option<BoundTextFrameV2>,
    captions: Vec<opencut_contracts::text_frame_v3::CaptionLayoutReceiptV3>,
    bound: Option<Vec<GraphicRasterReceiptV1>>,
}
impl<'a> ClassicGraphicFrameSessionV1<'a> {
    pub fn new(
        prepared: &'a PreparedGraphicFrameV1,
        policy: &LocalVisualExecutionPolicyV1,
        coverage: &[NativeTextCoverageV1],
    ) -> Result<Self, EditorError> {
        let plan = plan_v1(prepared, policy, coverage)?;
        // Admission subtracts all generated raster allocation before delegated admission.
        // The one aggregate GPU peak belongs to the full policy, including its child
        // receipt; no second execution or reduced/fabricated peak is claimed.
        Ok(Self {
            prepared,
            original: ClassicCaptionFrameSessionV3::new(prepared.execution(), policy, coverage)?,
            plan,
            child: None,
            captions: Vec::new(),
            bound: None,
        })
    }
    pub fn policy(&self) -> &LocalVisualExecutionPolicyV1 {
        self.original.policy()
    }
    pub fn typed_plan(&self) -> &ClassicGraphicFramePlanV1 {
        &self.plan
    }
    pub fn plan(&self) -> Result<Value, EditorError> {
        serde_json::to_value(&self.plan).map_err(|_| invalid("Cannot serialize graphic frame plan"))
    }
    pub fn callback(
        &mut self,
        request: GraphicFrameCallbackV1,
        resources: &mut (impl ClassicTextFrameResources + ?Sized),
    ) -> Result<Value, EditorError> {
        resources.checkpoint()?;
        match request {
            GraphicFrameCallbackV1::Graphic(GraphicCallbackV1::Bind { graphics }) => {
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
                    &graphic_media_policy_v1(self.prepared, self.original.policy())?,
                )?
                .project_decoded_sources(&child.decoded)?;
                let bound = bind_graphic_frame_v1(self.prepared, child, &self.captions, &graphics)?;
                let value = serde_json::to_value(bound)
                    .map_err(|_| invalid("Cannot serialize graphic binding"))?;
                self.bound = Some(graphics);
                Ok(value)
            }
            GraphicFrameCallbackV1::Original(CaptionFrameCallbackV3::Caption(
                CaptionCallbackV3::Bind { rasters, captions },
            )) => {
                let value = self.original.callback(
                    CaptionFrameCallbackV3::Caption(CaptionCallbackV3::Bind {
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
            GraphicFrameCallbackV1::Original(request) => self.original.callback(request, resources),
        }
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
        receipt: &ClassicGraphicFrameReceiptV1,
    ) -> Result<(), EditorError> {
        if receipt.identity != self.plan.identity
            || receipt.engine_identity != CLASSIC_GRAPHIC_FRAME_PROFILE_V1
            || self.bound.as_ref() != Some(&receipt.graphics)
        {
            return Err(invalid(
                "Graphic receipt differs from original identity/binding",
            ));
        }
        validate_graphic_receipts_v1(self.prepared, &receipt.graphics).map_err(error)?;
        self.original.validate_receipt(&receipt.execution)
    }
}
