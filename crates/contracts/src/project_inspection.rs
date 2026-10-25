//! One project read contract. Historical file metadata is read-only data,not a
//! choice of business protocol or an authority to execute native resource paths.
use crate::{
    AssetKind, AssetVersion, Sequence, Time,
    historical_visual::{HistoricalVisualKeyframeCurve, HistoricalVisualSequenceVisual},
    project_public::PublicProjectSnapshot,
    project_readonly::ReadOnlyProjectView,
    project_resources::ResourceView,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectInspection {
    Editor { snapshot: PublicProjectSnapshot },
    Historical { snapshot: HistoricalProjectSnapshot },
    Preserved { view: ReadOnlyProjectView },
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HistoricalContentKind {
    Base,
    Visual,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalProjectSnapshot {
    pub project: HistoricalProjectDocument,
    pub revision: String,
    pub read_only: bool,
    pub format: HistoricalContentKind,
    #[serde(default)]
    pub visual: Vec<HistoricalVisualSequenceVisual>,
    #[serde(default)]
    pub curves: Vec<HistoricalVisualKeyframeCurve>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalProjectDocument {
    pub id: String,
    pub name: String,
    pub default_sequence_id: String,
    pub sequences: Vec<Sequence>,
    pub assets: Vec<HistoricalAssetView>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalAssetView {
    pub id: String,
    pub name: String,
    pub kind: AssetKind,
    /// A scoped content path when an immutable resource is known,otherwise empty.
    /// This field never contains the original native source locator.
    pub uri: String,
    pub duration: Option<Time>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<AssetVersion>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<ResourceView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectProject {
    pub project_id: String,
    pub revision: Option<String>,
}
