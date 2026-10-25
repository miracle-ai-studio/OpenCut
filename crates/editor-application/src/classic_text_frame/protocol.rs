use crate::classic_visual::{ClassicVisualRuntimeReceipt, VisualCallback, VisualQuotaReceipt};
pub use opencut_api_contracts::text_frame_plan::TextFramePlan;
pub use opencut_contracts::text_frame::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicTextFontHandle {
    pub font_id: String,
    pub handle: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum TextFrameCallback {
    Text(TextCallback),
    Media(VisualCallback),
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", content = "args", deny_unknown_fields)]
pub enum TextCallback {
    #[serde(rename = "resolveVerifiedFontBytes")]
    Resolve(TextFontResolve),
    #[serde(rename = "bindTextFrame")]
    Bind(TextFrameBind),
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextFontResolve {
    pub font: TextFontResource,
    pub handle: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextFrameBind {
    pub rasters: Vec<TextRasterReceipt>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicTextFrameReceipt {
    pub identity: TextFrameIdentity,
    pub engine_identity: String,
    pub bound: BoundTextFrame,
    pub rgba_bytes: String,
    pub peak_accounted_bytes: String,
}
#[derive(Debug)]
pub struct ClassicTextFrameExecution {
    pub receipt: ClassicTextFrameReceipt,
    pub quota: VisualQuotaReceipt,
    pub runtime_identity: ClassicVisualRuntimeReceipt,
}
