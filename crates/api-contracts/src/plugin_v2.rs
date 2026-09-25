//! Declarative presets live outside the planner-v1 manifest and WIT contract.
use opencut_contracts::{CaptionStyle, EditPlan, Operation};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManifestV2 {
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageInput {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InstalledPackage {
    pub manifest: ManifestV2,
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
    pub package: InstalledPackage,
    pub grant: Grant,
    pub preset: PresetDeclaration,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetPlan {
    pub package_sha256: String,
    pub spec_sha256: String,
    pub grant_revision: String,
    pub operations: Vec<Operation>,
    pub plan: EditPlan,
}
