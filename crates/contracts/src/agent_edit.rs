//! Sequential read-only planning; one ordinary V4 transaction is the only write.
use crate::{project_public::*, timeline::*};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentEditRequest {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub label: String,
    pub steps: Vec<AgentEditStep>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentEditStep {
    /// Run the existing complete planner on the preceding steps' working graph.
    Timeline {
        sequence_id: String,
        command: TimelineCommand,
    },
    /// Existing media/text/graphic/property planners already return these public
    /// operations. Resolve them again against authorized immutable resources.
    Operations {
        operations: Vec<ProjectEditOperation>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentEditPlan {
    pub original_request_digest: String,
    pub expected_revision: String,
    pub steps: Vec<AgentEditDecision>,
    /// Submit unchanged through V4 public apply. No child receipt or revision is
    /// committed during planning. None represents a completely unchanged plan.
    pub transaction: Option<ApplyProject>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentEditDecision {
    pub step_index: u32,
    pub operation_count: usize,
    pub timeline: Option<TimelineDecision>,
}
