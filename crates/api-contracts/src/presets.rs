//! Data-only presets are distinct capabilities from executable WASM components.
use opencut_contracts::{
    CaptionStyle, Operation,
    project_public::{ApplyProject, PublicProjectDocument},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetManifest {
    pub manifest_version: u32,
    pub id: String,
    pub version: String,
    pub display_name: String,
    pub presets: Vec<PresetDeclaration>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetDeclaration {
    pub name: String,
    pub spec_path: String,
    pub spec_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PresetSpec {
    CaptionStyle {
        style: CaptionStyle,
    },
    EditTemplate {
        operations: Vec<Operation>,
        /// Clip id -> caller binding key. The referenced clip must have no asset id in the spec.
        clip_bindings: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InstallInput {
    pub package_root: String,
}

/// Project-package install action pinned to the package bytes validated at import.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InstallPinnedInput {
    pub package_root: String,
    pub expected_package_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageInput {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InstalledPresetPackage {
    pub manifest: PresetManifest,
    pub package_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GrantTarget {
    pub project_id: String,
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetGrant {
    pub target: GrantTarget,
    pub package_sha256: String,
    pub preset_names: BTreeSet<String>,
    pub enabled: bool,
    pub expected_revision: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub target: GrantTarget,
    pub package_sha256: String,
    pub preset_names: BTreeSet<String>,
    pub enabled: bool,
    pub revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetInput {
    pub target: GrantTarget,
    pub preset_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetPlanInput {
    pub target: GrantTarget,
    pub preset_name: String,
    pub revision: String,
    pub idempotency_key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_name: Option<String>,
    /// New caption ID -> explicit owning caption track in the target scene.
    #[serde(default)]
    pub caption_track_bindings: BTreeMap<String, String>,
    /// Declared family -> already imported font ID in the fixed revision.
    /// This is an explicit choice,never host-font discovery or implicit fallback.
    #[serde(default)]
    pub font_bindings: BTreeMap<String, String>,
    pub parameters: PresetParameters,
    /// Binding name -> asset id already pinned in the requested project revision.
    pub asset_bindings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PresetParameters {
    CaptionStyle {
        sequence_id: String,
        cue_ids: Vec<String>,
    },
    EditTemplate {},
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetView {
    pub package: InstalledPresetPackage,
    pub grant: Grant,
    pub preset: PresetDeclaration,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetPlan {
    pub package_sha256: String,
    pub spec_sha256: String,
    pub grant_revision: String,
    pub transaction: ApplyProject,
    pub preview: PublicProjectDocument,
    /// Reviewable adaptation facts,not renderer or pixel-identity evidence.
    pub normalizations: Vec<String>,
}
