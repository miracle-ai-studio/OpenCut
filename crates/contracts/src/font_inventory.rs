//! Lightweight discovery of pinned Classic faces. Source details and license
//! notices are fetched for one selected face; metadata never grants resource access.
use crate::scene::FontStyle;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicFontFamily {
    pub family: String,
    pub category: ClassicFontCategory,
    pub faces: Vec<ClassicFontFace>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClassicFontCategory {
    Google,
    System,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassicFontFace {
    pub weight: u16,
    pub style: FontStyle,
    pub catalog_id: Option<String>,
    pub unavailable_reason: Option<ClassicFontUnavailableReason>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClassicFontUnavailableReason {
    #[serde(alias = "weight_not_expressed_by_text_v4")]
    UnsupportedWeight,
    NoPinnedStaticSource,
    SystemFontNotRedistributable,
}
pub use crate::font_catalog::FontSourceRequest;
