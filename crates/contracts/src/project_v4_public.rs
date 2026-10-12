//! Complete public editor state. Private resource provenance stays in the
//! revision aggregate; these views are not storage or resource-write DTOs.
mod operations;
use crate::{AssetKind, AssetVersion, Time, project_v4::*};
pub use operations::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicProjectSnapshotV4 {
    pub project: PublicProjectDocumentV4,
    pub revision: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicProjectDocumentV4 {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub default_sequence_id: String,
    pub settings: SettingsProfileV4,
    pub assets: Vec<PublicAssetV4>,
    pub fonts: Vec<PublicFontResourceV4>,
    /// Typed complete scenes, including bookmarks, source spans and channels.
    /// Their presence does not imply compiler or renderer support.
    pub sequences: Vec<SequenceV4>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicAssetV4 {
    pub id: String,
    pub name: String,
    pub kind: AssetKind,
    pub version: AssetVersion,
    pub duration: Option<Time>,
    pub origin: PublicResourceOriginV4,
    pub metadata: MediaMetadataV4,
    pub license: Option<ResourceLicenseV4>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicFontResourceV4 {
    pub id: String,
    pub family: String,
    pub weight: u16,
    pub style: FontStyleV4,
    pub face_index: u32,
    pub content: AssetVersion,
    pub origin: PublicResourceOriginV4,
    pub license: Option<ResourceLicenseV4>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PublicResourceOriginV4 {
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
