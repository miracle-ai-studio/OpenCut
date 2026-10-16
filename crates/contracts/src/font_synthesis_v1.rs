//! Explicit browser synthesis requests retain the actual font's native identity.
//! No font bytes, family names or native metadata are modified by this profile.
use crate::{
    font_derived_v1::FontDerivedSourceV1, font_import_v1::FontCatalogEntryV1,
    project_v4::FontStyleV4,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const FONT_SYNTHESIS_PROVIDER_V1: &str = "opencut-chromium-font-synthesis-v1";
pub const FONT_SYNTHESIS_BINDING_ID_PREFIX_V1: &str = "font-synthesis-v1_";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontSynthesisProfileV1 {
    /// Load the actual normal face, then request italic at the same weight from
    /// the pinned Chromium Canvas consumer. Synthetic bold is not permitted.
    ClassicChromiumItalicV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontSynthesisRequestV1 {
    pub weight: u16,
    pub style: FontStyleV4,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontSynthesisBindingV1 {
    pub schema_version: u32,
    pub binding_id: String,
    pub profile: FontSynthesisProfileV1,
    /// Exact unmodified source bytes and their real normal-style metadata.
    pub actual: FontCatalogEntryV1,
    /// Authored intent; TextV4/TextMaskV4 must also retain this requested style.
    pub requested: FontSynthesisRequestV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontSynthesisSourceV1 {
    /// Existing opaque proof envelope. Its face describes actual bytes; its
    /// catalog id is distinct from binding.actual.id and its proof is schema 3.
    pub source: FontDerivedSourceV1,
    pub binding: FontSynthesisBindingV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontSynthesisProofV1 {
    pub schema_version: u32,
    pub project_id: String,
    pub revision: String,
    pub font_id: String,
    pub source: FontSynthesisSourceV1,
}
