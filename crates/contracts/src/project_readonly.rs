//! Read-only opaque-effect preservation with one current public view.
use crate::{project_public::PublicProjectDocument, scene::EditorDocument};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EffectPluginRef {
    pub id: String,
    pub version: String,
    pub package_sha256: String,
    pub contribution: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpaqueEffectTarget {
    pub sequence_id: String,
    pub track_id: String,
    pub element_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpaqueEffectNode {
    pub id: String,
    pub target: OpaqueEffectTarget,
    /// Position in the combined stack. Remaining positions retain known-effect order.
    pub position: u32,
    pub enabled: bool,
    pub plugin: EffectPluginRef,
    /// Untrusted JSON, retained byte-for-byte as a UTF-8 string. Never evaluated.
    pub payload_json: String,
}
/// Private revision aggregate; base resource provenance is not a public DTO.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadOnlyProjectDocument {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub base: EditorDocument,
    pub opaque_effects: Vec<OpaqueEffectNode>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateReadOnlyProject {
    pub source_project_id: String,
    pub source_revision: String,
    pub project_id: String,
    pub name: String,
    pub opaque_effects: Vec<OpaqueEffectNode>,
    pub idempotency_key: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableEffectReason {
    NotInstalled,
    UnsupportedByHost,
    InvalidPackage,
    InspectionUnavailable,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadOnlyEffect {
    pub id: String,
    pub target: OpaqueEffectTarget,
    pub position: u32,
    pub enabled: bool,
    pub plugin: EffectPluginRef,
    pub payload_sha256: String,
    pub payload_size_bytes: u32,
    pub unavailable_reason: UnavailableEffectReason,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadOnlyProjectView {
    pub project_id: String,
    pub name: String,
    pub revision: String,

    pub read_only: bool,
    pub known_content: PublicProjectDocument,
    pub effects: Vec<ReadOnlyEffect>,
    pub export_failure_code: String,
}
/// Public preservation file: known resources use public CAS identities, not locators.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadOnlyProjectManifest {
    pub schema_version: u32,
    pub project_id: String,
    pub name: String,
    pub revision: String,
    pub document_schema_version: u32,
    pub known_content: PublicProjectDocument,
    pub opaque_effects: Vec<OpaqueEffectNode>,
}
