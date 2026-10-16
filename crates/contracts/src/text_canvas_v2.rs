//! Versioned canvas facts with an explicit requested synthesis descriptor.
use crate::{
    font_synthesis_v1::FontSynthesisBindingV1,
    text_frame_v2::{TextFrameIdentityV2, TextLayerPlanV2},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const CLASSIC_TEXT_CANVAS_PROFILE_V2: &str = "classic-v31-canvas-text-v2";
pub use crate::text_canvas_v1::TextCanvasPlanRequestV1 as TextCanvasPlanRequestV2;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextCanvasPlanResultV2 {
    pub schema_version: u32,
    pub identity: TextFrameIdentityV2,
    /// Font metadata and coverage always describe the actual source bytes.
    pub layer: TextLayerPlanV2,
    /// Canvas measure/draw uses this requested style; FontFace still uses the
    /// actual descriptor above. None retains ordinary exact-face behavior.
    pub font_synthesis: Option<FontSynthesisBindingV1>,
}
