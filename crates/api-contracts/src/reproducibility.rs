//! Verification view; legacy package booleans remain historical claims.
use crate::historical_visual_package::PackageToolLock;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectPackageReproducibility {
    /// Owner-local canonical package directory. No import or installation occurs.
    pub package_path: String,
    pub expected_manifest_sha256: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Verified,
    NotVerified,
    Mismatch,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionIdentity {
    /// Hash of this running host executable, including its statically linked engine.
    /// This is an observation of the current host, not the package's original engine.
    pub process_binary_sha256: String,
    pub platform: String,
    pub native_tools: Vec<PackageToolLock>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageReproducibilityReport {
    pub manifest_sha256: String,
    pub source_revision: String,
    pub package_schema_version: u32,
    pub legacy_fully_reproducible_claim: bool,
    pub media_self_contained_declared: bool,
    /// Inspection verifies manifest and project bytes, not all media or plugin bytes.
    pub media_contents_verified: VerificationStatus,
    pub native_tool_environment: VerificationStatus,
    pub execution_environment_locked: VerificationStatus,
    pub semantic_output_verified: VerificationStatus,
    pub bit_exact_verified: VerificationStatus,
    pub strictly_reproducible: bool,
    pub current_execution: ExecutionIdentity,
    pub reasons: Vec<String>,
}
