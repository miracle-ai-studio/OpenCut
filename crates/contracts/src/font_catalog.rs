//! Unified pinned font discovery. Display labels never replace native face identity.
use crate::{
    font_derived::FontDerivedSource, font_import::FontCatalogEntry,
    font_synthesis::FontSynthesisSource, scene::FontStyle,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontRenderProfile {
    StaticOutline,
    ColrV0,
    ColrV1,
    MonochromeBitmap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontKind {
    Static,
    Derived,
    Synthesis,
    Distribution,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontFace {
    pub id: String,
    pub display_family: String,
    pub family: String,
    pub weight: u16,
    pub style: FontStyle,
    /// A rendering request, separate from the immutable native face style.
    /// Present only for an explicit synthesis route; never relabels the bytes.
    pub requested_style: Option<FontStyle>,
    pub kind: FontKind,
    pub content_hash: String,
    pub size_bytes: String,
    pub render_profile: FontRenderProfile,
    pub alias_reason: Option<String>,
    /// Host runtime/cache admission; actual acquisition verifies all pinned bytes.
    pub import_available: bool,
    pub unavailable_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontCatalog {
    pub families: Vec<crate::font_inventory::ClassicFontFamily>,
    pub pinned_face_count: u32,
    pub download_available: bool,
    pub unavailable_reason: Option<String>,
    /// Original inventory baseline; individual sources also pin historical or
    /// separately maintained official repositories in their source details.
    pub source_commit: String,
    pub classic_atlas_sha256: String,
    pub classic_family_count: u32,
    pub fonts: Vec<FontFace>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontSourceRequest {
    pub catalog_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "source", rename_all = "snake_case")]
pub enum FontSourceDetails {
    Static(FontCatalogEntry),
    Derived(FontDerivedSource),
    Distribution(FontDistributionSource),
    Synthesis(FontSynthesisSource),
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontSource {
    pub face: FontFace,
    pub details: FontSourceDetails,
    pub license_url: String,
    pub license_sha256: String,
}

/// Official CSS distributions have immutable file/version identities, not Git
/// commits. Preserve that factual provenance without fabricating a repository identity.

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontDistributionProvenance {
    pub css_url: String,
    pub css_sha256: String,
    pub release: String,
    pub source_url: String,
    pub copyright: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontDistributionSource {
    pub face: FontFace,
    pub provenance: FontDistributionProvenance,
    pub license_id: String,
    pub license_notice: String,
    pub license_url: String,
    pub license_sha256: String,
}
