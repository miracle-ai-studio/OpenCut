//! Layer blend facts over the existing graphic/media/text execution partition.
//! The ordered table covers every sampled layer, including normal and no-quad
//! layers. No blend mode is inferred from a decoder or browser observation.
use crate::{graphic_frame::BoundGraphicFrame, scene::BlendMode, text_frame::TextFrameIdentity};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayerBlend {
    pub element_id: String,
    pub blend_mode: BlendMode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundBlendFrame {
    pub identity: TextFrameIdentity,
    /// Binding evidence for decoded and generated sources, not another render.
    pub execution: BoundGraphicFrame,
    pub blends: Vec<LayerBlend>,
}
