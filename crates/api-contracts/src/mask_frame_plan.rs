//! Additive box-mask plan over the unchanged blend execution declaration.
use crate::blend_frame_plan::BlendFramePlan;
use opencut_contracts::{mask_frame::LayerMask, text_frame::TextFrameIdentity};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaskFramePlan {
    pub identity: TextFrameIdentity,
    pub execution: BlendFramePlan,
    pub masks: Vec<LayerMask>,
}
