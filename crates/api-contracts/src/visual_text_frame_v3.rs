//! Additive caption plan. The media/free-text V2 plan remains unchanged.
use crate::visual_text_frame_v2::ClassicTextFramePlanV2;
use opencut_contracts::text_frame_v2::TextFrameIdentityV2;
use opencut_contracts::text_frame_v3::CaptionRasterPlanV3;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicTextFramePlanV3 {
    pub identity: TextFrameIdentityV2,
    pub execution: ClassicTextFramePlanV2,
    pub captions: Vec<CaptionRasterPlanV3>,
}
