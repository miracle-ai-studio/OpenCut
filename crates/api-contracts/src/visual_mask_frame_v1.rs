//! Additive box-mask plan over the unchanged blend execution declaration.
use crate::visual_blend_frame_v1::ClassicBlendFramePlanV1;
use opencut_contracts::{mask_frame_v1::LayerMaskV1, text_frame_v2::TextFrameIdentityV2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicMaskFramePlanV1 {
    pub identity: TextFrameIdentityV2,
    pub execution: ClassicBlendFramePlanV1,
    pub masks: Vec<LayerMaskV1>,
}
