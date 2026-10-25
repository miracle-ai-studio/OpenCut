//! Complete public editor state. Private resource provenance stays in the
//! revision aggregate; these views are not storage or resource-write DTOs.
mod operations;
use crate::{AssetKind, AssetVersion, Time, scene::*};
pub use operations::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicProjectSnapshot {
    pub project: PublicProjectDocument,
    pub revision: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicProjectDocument {
    pub id: String,
    pub name: String,
    pub default_sequence_id: String,
    pub settings: SettingsProfile,
    pub assets: Vec<PublicAsset>,
    pub fonts: Vec<PublicFontResource>,
    /// Typed complete scenes, including bookmarks, source spans and channels.
    /// Their presence does not imply compiler or renderer support.
    pub sequences: Vec<Scene>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicAsset {
    pub id: String,
    pub name: String,
    pub kind: AssetKind,
    pub version: AssetVersion,
    pub duration: Option<Time>,
    pub origin: PublicResourceOrigin,
    pub metadata: ProjectMediaMetadata,
    pub license: Option<ResourceLicense>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicFontResource {
    pub id: String,
    pub family: String,
    pub weight: u16,
    pub style: FontStyle,
    pub face_index: u32,
    pub content: AssetVersion,
    pub origin: PublicResourceOrigin,
    pub license: Option<ResourceLicense>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PublicResourceOrigin {
    LocalImport {
        original_name: Option<String>,
    },
    Online {
        provider: String,
        source_url: Option<String>,
    },
    /// The private legacy locator is deliberately not part of this schema.
    Legacy {},
    Generated {
        provider: String,
        generation_id: Option<String>,
    },
}
