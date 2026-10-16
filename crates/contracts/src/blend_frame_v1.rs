//! Layer blend facts over the existing graphic/media/text execution partition.
//! The ordered table covers every sampled layer, including normal and no-quad
//! layers. No blend mode is inferred from a decoder or browser observation.
use crate::{
    graphic_frame_v1::BoundGraphicFrameV1, project_v4::BlendModeV4,
    text_frame_v2::TextFrameIdentityV2,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayerBlendV1 {
    pub element_id: String,
    pub blend_mode: BlendModeV4,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BoundBlendFrameV1 {
    pub identity: TextFrameIdentityV2,
    /// Binding evidence for decoded and generated sources, not another render.
    pub execution: BoundGraphicFrameV1,
    pub blends: Vec<LayerBlendV1>,
}
