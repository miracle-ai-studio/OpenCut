//! Explicit browser synthesis requests retain the actual font's native identity.
//! No font bytes, family names or native metadata are modified by this profile.
use crate::{font_derived::FontDerivedSource, font_import::FontCatalogEntry, scene::FontStyle};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const FONT_SYNTHESIS_PROVIDER: &str = "opencut-chromium-font-synthesis-v1";
pub const FONT_SYNTHESIS_BINDING_ID_PREFIX: &str = "font-synthesis-v1_";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontSynthesisProfile {
    /// Load the actual normal face, then request italic at the same weight from
    /// the pinned Chromium Canvas consumer. Synthetic bold is not permitted.
    ClassicChromiumItalicV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontSynthesisRequest {
    pub weight: u16,
    pub style: FontStyle,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontSynthesisBinding {
    pub schema_version: u32,
    pub binding_id: String,
    pub profile: FontSynthesisProfile,
    /// Exact unmodified source bytes and their real normal-style metadata.
    pub actual: FontCatalogEntry,
    /// Authored intent; TextV4/TextMaskV4 must also retain this requested style.
    pub requested: FontSynthesisRequest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontSynthesisSource {
    /// Existing opaque proof envelope. Its face describes actual bytes; its
    /// catalog id is distinct from binding.actual.id and its proof is schema 3.
    pub source: FontDerivedSource,
    pub binding: FontSynthesisBinding,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontSynthesisProof {
    pub project_id: String,
    pub revision: String,
    pub font_id: String,
    pub source: FontSynthesisSource,
}
