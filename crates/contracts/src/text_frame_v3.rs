//! Caption wrapping facts are versioned separately; free TextV4 and frame V2
//! retain explicit-newline semantics and never acquire automatic wrapping.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptionRasterPlanV3 {
    pub element_id: String,
    pub max_width_px: f64,
    pub bottom_margin_px: f64,
    pub wrap_profile: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptionWrapMeasurementV3 {
    pub content: String,
    pub width: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptionLayoutReceiptV3 {
    pub element_id: String,
    pub original_text_hash: String,
    pub wrapped_text: String,
    /// Every greedy candidate in order, including rejected overflow candidates.
    /// Widths are measured with the same fixed font as the actual raster.
    pub measurements: Vec<CaptionWrapMeasurementV3>,
}
