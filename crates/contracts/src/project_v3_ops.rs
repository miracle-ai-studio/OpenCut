//! Versioned visual-edit requests. Not part of the legacy schema 2 tool set.
use crate::{
    CreateProject, Operation,
    project_v3::{EffectNode, MaskNode, ProjectDocumentV3, ShapeClip, TransitionNode},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateV3Project {
    pub project: CreateProject,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PromoteV3Project {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectV3Project {
    pub project_id: String,
    /// None selects the current main head. Some selects one immutable revision.
    pub revision: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListV3Projects {
    pub cursor: Option<String>,
    pub limit: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V3ProjectSummary {
    pub project_id: String,
    pub name: String,
    pub revision: String,
    pub schema_version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct V3ProjectPage {
    pub items: Vec<V3ProjectSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyV3Project {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub label: String,
    pub operations: Vec<VisualOperation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualPlan {
    pub proposed_project: ProjectDocumentV3,
    pub created_ids: Vec<String>,
    pub changed_ids: Vec<String>,
    pub deleted_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum VisualOperation {
    /// Apply an existing typed base edit inside this schema 3 revision batch.
    #[serde(rename = "base.apply")]
    Base { operation: Operation },
    #[serde(rename = "shape.put")]
    ShapePut {
        sequence_id: String,
        shape: ShapeClip,
    },
    #[serde(rename = "shape.delete")]
    ShapeDelete { sequence_id: String, id: String },
    #[serde(rename = "effect.put")]
    EffectPut {
        sequence_id: String,
        effect: EffectNode,
    },
    #[serde(rename = "effect.delete")]
    EffectDelete { sequence_id: String, id: String },
    #[serde(rename = "mask.put")]
    MaskPut { sequence_id: String, mask: MaskNode },
    #[serde(rename = "mask.delete")]
    MaskDelete { sequence_id: String, id: String },
    #[serde(rename = "transition.put")]
    TransitionPut {
        sequence_id: String,
        transition: TransitionNode,
    },
    #[serde(rename = "transition.delete")]
    TransitionDelete { sequence_id: String, id: String },
}

impl VisualOperation {
    pub fn sequence_id(&self) -> Option<&str> {
        match self {
            Self::Base { .. } => None,
            Self::ShapePut { sequence_id, .. }
            | Self::ShapeDelete { sequence_id, .. }
            | Self::EffectPut { sequence_id, .. }
            | Self::EffectDelete { sequence_id, .. }
            | Self::MaskPut { sequence_id, .. }
            | Self::MaskDelete { sequence_id, .. }
            | Self::TransitionPut { sequence_id, .. }
            | Self::TransitionDelete { sequence_id, .. } => Some(sequence_id),
        }
    }
}
