//! Scoped immutable resource descriptors. Hashes identify bytes, never authority.
use crate::AssetVersion;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Asset,
    Font,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectResource {
    pub project_id: String,
    pub revision: String,
    pub kind: ResourceKind,
    pub resource_id: String,
}

/// A descriptor does not prove the CAS object is presently available. Content
/// reads recheck current project authority and verify the opened exact bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceView {
    pub project_id: String,
    pub revision: String,
    pub kind: ResourceKind,
    pub resource_id: String,
    pub version: AssetVersion,
    pub mime_type: Option<String>,
    pub content_path: String,
}
