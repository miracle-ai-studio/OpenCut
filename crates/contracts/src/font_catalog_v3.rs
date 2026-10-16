//! Unified pinned font discovery. Display labels never replace native face identity.
use crate::{
    font_derived_v1::FontDerivedSourceV1, font_import_v1::FontCatalogEntryV1,
    font_synthesis_v1::FontSynthesisSourceV1, project_v4::FontStyleV4,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontRenderProfileV3 {
    StaticOutline,
    ColrV0,
    ColrV1,
    MonochromeBitmap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontCatalogKindV3 {
    StaticV1,
    DerivedV1,
    DerivedV2,
    StaticV3,
    SynthesisV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontCatalogFaceV3 {
    pub id: String,
    pub display_family: String,
    pub family: String,
    pub weight: u16,
    pub style: FontStyleV4,
    /// A rendering request, separate from the immutable native face style.
    /// Present only for an explicit synthesis route; never relabels the bytes.
    pub requested_style: Option<FontStyleV4>,
    pub kind: FontCatalogKindV3,
    pub content_hash: String,
    pub size_bytes: String,
    pub render_profile: FontRenderProfileV3,
    pub alias_reason: Option<String>,
    /// Host runtime/cache admission; actual acquisition verifies all pinned bytes.
    pub import_available: bool,
    pub unavailable_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontCatalogV3 {
    pub schema_version: u32,
    pub download_available: bool,
    pub unavailable_reason: Option<String>,
    /// Original inventory baseline; individual sources also pin historical or
    /// separately maintained official repositories in their source details.
    pub source_commit: String,
    pub classic_atlas_sha256: String,
    pub classic_family_count: u32,
    pub fonts: Vec<FontCatalogFaceV3>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontSourceRequestV3 {
    pub catalog_id: String,
    pub kind: FontCatalogKindV3,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "source", rename_all = "snake_case")]
pub enum FontSourceDetailsV3 {
    StaticV1(FontCatalogEntryV1),
    DerivedV1(FontDerivedSourceV1),
    DerivedV2(FontDerivedSourceV1),
    StaticV3(FontCatalogEntryV1),
    SynthesisV1(FontSynthesisSourceV1),
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontSourceV3 {
    pub face: FontCatalogFaceV3,
    pub details: FontSourceDetailsV3,
    pub license_url: String,
    pub license_sha256: String,
}
