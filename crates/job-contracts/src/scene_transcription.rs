//! Whole-scene transcription with public mix evidence and no native paths.
use crate::scene_audio::AudioWavResult;
use opencut_contracts::Time;
use opencut_media_contracts::TranscriptionResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const SCENE_TRANSCRIPTION_KIND: &str = "analysis.sequence_transcribe.v4";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneTranscriptionRequest {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub idempotency_key: String,
    pub language: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneTranscriptionResult {
    pub schema_version: u32,
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub duration: Time,
    pub audio: AudioWavResult,
    pub transcription: TranscriptionResult,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneTranscriptionCapability {
    pub schema_version: u32,
    pub available: bool,
    pub submit_tool: String,
    pub supported_languages: Vec<String>,
    pub timing: String,
    pub unavailable_reason: Option<String>,
}
