use crate::classic_visual::{ClassicVisualRuntimeReceipt, VisualCallback, VisualQuotaReceipt};
pub use opencut_api_contracts::visual_text_frame_v2::ClassicTextFramePlanV2;
pub use opencut_contracts::text_frame_v2::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicTextFontHandleV2 {
    pub font_id: String,
    pub handle: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum TextFrameCallbackV2 {
    Text(TextCallbackV2),
    Media(VisualCallback),
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", content = "args", deny_unknown_fields)]
pub enum TextCallbackV2 {
    #[serde(rename = "resolveVerifiedFontBytes")]
    Resolve(TextFontResolveV2),
    #[serde(rename = "bindTextFrame")]
    Bind(TextFrameBindV2),
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextFontResolveV2 {
    pub font: TextFontResourceV2,
    pub handle: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextFrameBindV2 {
    pub rasters: Vec<TextRasterReceiptV2>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicTextFrameReceiptV2 {
    pub identity: TextFrameIdentityV2,
    pub engine_identity: String,
    pub bound: BoundTextFrameV2,
    pub rgba_bytes: String,
    pub peak_accounted_bytes: String,
}
#[derive(Debug)]
pub struct ClassicTextFrameExecutionV2 {
    pub receipt: ClassicTextFrameReceiptV2,
    pub quota: VisualQuotaReceipt,
    pub runtime_identity: ClassicVisualRuntimeReceipt,
}
