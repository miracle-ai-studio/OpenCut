//! Additive V4 whole-scene transcription with public mix evidence and no paths.
use crate::project_v4_audio::V4AudioWavResultV1;
use opencut_contracts::Time;
use opencut_media_contracts::TranscriptionResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const SEQUENCE_TRANSCRIBE_V4_KIND: &str = "analysis.sequence_transcribe.v4";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitSequenceTranscribeV4 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub idempotency_key: String,
    pub language: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SequenceTranscribeResultV4 {
    pub schema_version: u32,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub duration: Time,
    pub audio: V4AudioWavResultV1,
    pub transcription: TranscriptionResult,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SequenceTranscribeCapabilityV4 {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub supported_languages: Vec<String>,
    pub timing: String,
    pub unavailable_reason: Option<String>,
}
