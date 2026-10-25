//! Fixed resource selection. A catalog ID is never a network locator or a CAS grant.
use crate::{AssetVersion, scene::ResourceLicense};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CatalogResourceCategory {
    Shapes,
    Flags,
    Sounds,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogResourceEntry {
    pub id: String,
    pub name: String,
    pub category: CatalogResourceCategory,
    pub keywords: Vec<String>,
    pub source_url: String,
    pub source_sha256: Option<String>,
    pub license: ResourceLicense,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Offline source analysis for this exact pinned public sound. This is not
    /// a download grant, an audition-ready flag, or a project gain envelope.
    pub audio: Option<CatalogSoundAnalysis>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogSoundAnalysis {
    pub source_sha256: String,
    pub source_size: u64,
    /// Encoded media duration observed from the producer's HTMLAudio instance.
    /// The decoded PCM extent below is a separate clock. Neither authors a
    /// project placement; an actual audition uses its own media clock.
    pub audition_duration_seconds: f64,
    pub producer_sha256: String,
    /// Producer identity retained with the fixed analysis; not this host's
    /// current decoder identity.
    pub identity: crate::source_art::SourceArtIdentity,
    pub sample_rate: u32,
    pub channels: u32,
    pub total_samples: String,
    pub bucket_size: u32,
    /// Unmodified whole-source 128-sample peak buckets across all channels.
    #[schemars(length(min = 1, max = 22500))]
    pub amplitudes: Vec<f32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceCatalog {
    pub sound_download_available: bool,
    pub sound_unavailable_reason: Option<String>,
    pub entries: Vec<CatalogResourceEntry>,
    /// Classic's provider is empty; no unavailable brand is advertised as selectable.
    pub brands_available: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogUploadedMedia {
    pub media_id: String,
    pub version: AssetVersion,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportCatalogResource {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub catalog_id: String,
    pub asset_id: String,
    /// Flags are browser-rasterized PNGs through ordinary authorized uploads.
    /// Sounds have no uploaded selector: the native fixed-source port supplies bytes.
    pub uploaded: Option<CatalogUploadedMedia>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocalSoundKind {
    Whoosh,
    Click,
    Impact,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalSoundRecipe {
    pub kind: LocalSoundKind,
    pub duration_ms: u32,
    pub seed: u32,
    pub level_db: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenerateCatalogSound {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub asset_id: String,
    pub recipe: LocalSoundRecipe,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectCatalogSoundRecipe {
    pub project_id: String,
    pub revision: String,
    pub asset_id: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocalSoundSampleFormat {
    PcmS16le,
}
/// Only the exact bounded generator metadata is public; never the opaque
/// origin string or arbitrary provider configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordedLocalSound {
    pub recipe: LocalSoundRecipe,
    pub sample_rate: u32,
    pub channels: u32,
    pub sample_format: LocalSoundSampleFormat,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogSoundRecipeView {
    pub project_id: String,
    pub revision: String,
    pub asset_id: String,
    pub version: AssetVersion,
    pub recording: RecordedLocalSound,
}

/// Bounded audition of a pinned public sound, without importing it into a project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviewCatalogSound {
    pub catalog_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CatalogSoundMimeType {
    #[serde(rename = "audio/ogg")]
    AudioOgg,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogSoundPreview {
    pub catalog_id: String,
    pub version: AssetVersion,
    pub mime_type: CatalogSoundMimeType,
    /// Lowercase hex of at most 262144 verified encoded bytes. No native path or grant.
    #[schemars(length(max = 524288))]
    pub bytes_hex: String,
}
