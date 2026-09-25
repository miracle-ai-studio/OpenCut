//! Typed, bounded evidence over one authorized successful analysis task.
use opencut_contracts::{LockedAsset, Time, TimeRange};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub type AnalysisEvidenceInput = crate::analysis_query::AnalysisQueryInput;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AnalysisParameters {
    Transcription {
        language: Option<String>,
        word_timing_status: String,
    },
    Silence {
        noise_db: i16,
        min_duration_ms: u32,
    },
    Shots {
        threshold: f32,
    },
    Waveform {
        buckets: u16,
    },
    SpeakerTurns {
        language: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnalysisProvenance {
    pub provider: String,
    pub method: Option<String>,
    pub model: Option<String>,
    pub model_sha256: Option<String>,
    pub confidence_known: Option<bool>,
    pub speaker_identity_status: Option<String>,
    pub parameters: AnalysisParameters,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AnalysisEvidenceItem {
    TranscriptSegment {
        evidence_id: String,
        index: u32,
        source_range: TimeRange,
        text: String,
    },
    SilenceInterval {
        evidence_id: String,
        index: u32,
        source_range: TimeRange,
    },
    ShotBoundary {
        evidence_id: String,
        index: u32,
        source_time: Time,
        scene_score: f32,
    },
    WaveformBucket {
        evidence_id: String,
        index: u32,
        source_range: TimeRange,
        rms: f32,
        peak: f32,
    },
    SpeakerTurn {
        evidence_id: String,
        index: u32,
        source_time: Time,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnalysisEvidencePage {
    pub analysis_job_id: String,
    pub analysis_kind: String,
    pub project_id: String,
    pub revision: String,
    pub asset_id: String,
    pub asset_version: LockedAsset,
    pub provenance: AnalysisProvenance,
    pub evidence: Vec<AnalysisEvidenceItem>,
    pub next_cursor: Option<String>,
}
