//! Agent-facing artifact access reuses the existing public task projection.
use crate::job_views::ArtifactReference;
pub use opencut_contracts::agent_read::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetArtifact {
    pub artifact_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactAccess {
    /// Describes the published result; availability stays unchecked until retrieval.
    pub artifact: ArtifactReference,
    pub resource_uri: String,
    /// Relative to the explicitly connected HTTP service, with authorization
    /// rechecked when bytes are retrieved. Never a native filesystem path.
    pub http_path: String,
}
