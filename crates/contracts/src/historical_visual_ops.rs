//! Versioned visual-edit requests. Not part of the legacy schema 2 tool set.
use crate::{
    Operation,
    historical_visual::{
        EffectNode, HistoricalVisualDocument, HistoricalVisualKeyframeCurve, MaskNode, ShapeClip,
        TransitionNode,
    },
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use crate::project_catalog::CreateProjectRequest;

pub use crate::project_inspection::InspectProject;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyHistoricalVisualProject {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub label: String,
    pub operations: Vec<VisualOperation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualPlan {
    pub proposed_project: HistoricalVisualDocument,
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
    #[serde(rename = "keyframe.curve_set")]
    KeyframeCurveSet {
        sequence_id: String,
        curve: HistoricalVisualKeyframeCurve,
    },
    #[serde(rename = "keyframe.curve_delete")]
    KeyframeCurveDelete { sequence_id: String, id: String },
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
            | Self::TransitionDelete { sequence_id, .. }
            | Self::KeyframeCurveSet { sequence_id, .. }
            | Self::KeyframeCurveDelete { sequence_id, .. } => Some(sequence_id),
        }
    }
}
