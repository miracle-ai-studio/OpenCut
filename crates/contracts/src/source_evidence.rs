//! Explicit source-time evidence, separate from the frozen Classic 1s thumbnail.
use crate::{
    AssetVersion, Time, TimeRange,
    source_art::{SourceArtData, SourceArtIdentity, SourceArtThumbnailObservation},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceFrameRequest {
    pub project_id: String,
    pub revision: String,
    pub asset_id: String,
    pub request_id: String,
    pub time: Time,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceFrameResponse {
    pub project_id: String,
    pub revision: String,
    pub asset_id: String,
    pub version: AssetVersion,
    pub requested_time: Time,
    pub identity: SourceArtIdentity,
    pub data: SourceArtData,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceEvidenceRequest {
    pub project_id: String,
    pub revision: String,
    pub request_id: String,
    /// Up to12 exact source points; images are static observations.
    pub frames: Vec<SourceEvidencePoint>,
    pub waveforms: Vec<SourceEvidenceWaveform>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceEvidencePoint {
    pub asset_id: String,
    pub time: Time,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceEvidenceWaveform {
    pub asset_id: String,
    pub range: TimeRange,
    pub bins: u32,
    pub sample_rate: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceEvidenceReceipt {
    pub asset_id: String,
    pub version: AssetVersion,
    pub identity: SourceArtIdentity,
    pub requested_time: Option<Time>,
    pub requested_range: Option<TimeRange>,
    pub frame_observation: Option<SourceArtThumbnailObservation>,
}
/// Inline immutable artifact. It has no server-side retention lease or opaque
/// artifact ID; clients may save these exact bytes and verify their hash.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InlineEvidenceArtifact {
    pub mime_type: String,
    pub content_hash: String,
    pub bytes_hex: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceEvidenceResult {
    pub project_id: String,
    pub revision: String,
    pub receipts: Vec<SourceEvidenceReceipt>,
    pub contact_sheet: InlineEvidenceArtifact,
    /// Waveforms are source peak envelopes, without mastering/gain or PCM claims.
    pub waveform_profile: String,
}
