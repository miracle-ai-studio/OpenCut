//! Durable task submission; fixed project operations share the render and analysis use cases.
use crate::agent_render::RenderRequest;
pub use opencut_job_contracts::{
    ExportJobInput, FrameJobInput,
    asset_analysis::{AssetAnalysisOptions, SubmitAssetAnalysis},
    scene_transcription::SceneTranscriptionRequest,
};
use opencut_media_contracts::{ProbeRequest, VerifyRequest};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum SubmitJob {
    #[serde(rename = "render")]
    Render { request: RenderRequest },
    #[serde(rename = "analysis.asset")]
    AssetAnalysis { request: SubmitAssetAnalysis },
    #[serde(rename = "analysis.sequence_transcribe")]
    SceneTranscription { request: SceneTranscriptionRequest },
    /// Host-local source access is owner-only, checked before opening any path.
    #[serde(rename = "media.probe")]
    Probe {
        idempotency_key: String,
        input: ProbeRequest,
    },
    #[serde(rename = "preview.frame")]
    SourceFrame {
        idempotency_key: String,
        input: FrameJobInput,
    },
    #[serde(rename = "render.export")]
    MediaExport {
        idempotency_key: String,
        input: ExportJobInput,
    },
    #[serde(rename = "artifact.verify")]
    Verify {
        idempotency_key: String,
        input: VerifyRequest,
    },
}
