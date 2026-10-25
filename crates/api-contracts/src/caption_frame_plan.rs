//! Additive caption plan. The media/free-text V2 plan remains unchanged.
use crate::text_frame_plan::TextFramePlan;
use opencut_contracts::caption_frame::CaptionRasterPlan;
use opencut_contracts::text_frame::TextFrameIdentity;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptionFramePlan {
    pub identity: TextFrameIdentity,
    pub execution: TextFramePlan,
    pub captions: Vec<CaptionRasterPlan>,
}
