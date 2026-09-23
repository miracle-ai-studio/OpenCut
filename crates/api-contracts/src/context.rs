//! Portable resource views. MCP transports serialize these same application results.
use crate::{
    capabilities::HostCapabilities,
    domains::DomainDescriptor,
    job_views::{ArtifactReference, JobView},
    tools::ToolSpec,
};
use opencut_contracts::{FrameRate, OperationDescriptor, Sequence};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceReadInput {
    pub uri: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ResourceView {
    pub uri: String,
    pub mime_type: String,
    pub payload: ResourcePayload,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ResourcePayload {
    Capabilities(Box<HostCapabilities>),
    Tools(Vec<ToolSpec>),
    Domains(Vec<DomainDescriptor>),
    EventSchema(Value),
    OperationSchema(OperationSchema),
    ProjectSummary(ProjectSummary),
    Timeline(RevisionTimeline),
    Job(Box<JobView>),
    Artifact(ArtifactReference),
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OperationSchema {
    pub descriptor: OperationDescriptor,
    pub schema: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectSummary {
    pub project_id: String,
    pub name: String,
    pub branch: String,
    pub revision: String,
    pub schema_version: u32,
    pub default_sequence_id: String,
    pub sequences: Vec<SequenceSummary>,
    pub asset_count: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SequenceSummary {
    pub sequence_id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: FrameRate,
    pub track_count: usize,
    pub clip_count: usize,
    pub caption_count: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RevisionTimeline {
    pub project_id: String,
    pub revision: String,
    pub sequence: Sequence,
}
