//! Compose existing planners without publishing intermediate state or receipts.
use crate::scene::{self, SceneCommitIntent, public::ProjectPublicResources};
use opencut_contracts::{
    EditorError, agent_edit::*, project_inspection::InspectProject, project_public::ApplyProject,
    timeline::TimelineEditRequest,
};

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
pub fn plan(
    store: &impl ProjectPublicResources,
    request: &AgentEditRequest,
) -> Result<AgentEditPlan, EditorError> {
    crate::require_idempotency_key(&request.idempotency_key)?;
    if request.steps.is_empty()
        || request.steps.len() > 32
        || request.expected_revision.trim().is_empty()
        || request.label.trim().is_empty()
        || request.label.len() > 4096
    {
        return Err(invalid(
            "Agent edit needs 1..32 steps, a fixed revision and a 1..4096 byte label",
        ));
    }
    let mut transaction = ApplyProject {
        branch_name: None,
        project_id: request.project_id.clone(),
        expected_revision: request.expected_revision.clone(),
        idempotency_key: request.idempotency_key.clone(),
        label: request.label.clone(),
        operations: vec![],
    };
    store.authorize_project(SceneCommitIntent::PublicApply(&transaction))?;
    let mut working = scene::inspect(
        store,
        &InspectProject {
            project_id: request.project_id.clone(),
            revision: None,
        },
    )?;
    if working.revision != request.expected_revision {
        return Err(crate::revision_conflict(
            &request.expected_revision,
            &working.revision,
        ));
    }
    let mut decisions = Vec::new();
    for (index, step) in request.steps.iter().enumerate() {
        let (operations, timeline) = match step {
            AgentEditStep::Timeline {
                sequence_id,
                command,
            } => {
                // Each step has deterministic identity allocation, but never an
                // independent mutation receipt. Apply binds the outer request key.
                let step_key = crate::identity::request_identity(
                    "agent_edit_v1_step",
                    &request.project_id,
                    &(request, index),
                )?
                .current_digest()
                .to_owned();
                let plan = crate::timeline::plan_working_snapshot(
                    &working,
                    &TimelineEditRequest {
                        project_id: request.project_id.clone(),
                        expected_revision: request.expected_revision.clone(),
                        sequence_id: sequence_id.clone(),
                        idempotency_key: step_key,
                        label: request.label.clone(),
                        command: command.clone(),
                    },
                )?;
                (
                    plan.transaction.map(|t| t.operations).unwrap_or_default(),
                    Some(plan.decision),
                )
            }
            AgentEditStep::Operations { operations } => {
                if operations.is_empty() {
                    return Err(invalid("An operations step must not be empty"));
                }
                (operations.clone(), None)
            }
        };
        let operation_count = operations.len();
        if transaction.operations.len().saturating_add(operation_count) > 128 {
            return Err(invalid(
                "Combined Agent transaction exceeds 128 public operations",
            ));
        }
        if operation_count > 0 {
            transaction.operations.extend(operations);
            // Resolve the complete accumulated proposal against the original
            // fixed head. This preserves resource authority and ordinary atomic
            // edit/lock validation; later steps observe its private working graph.
            working = scene::public::plan_public_snapshot(store, &transaction)?;
        }
        decisions.push(AgentEditDecision {
            step_index: index as u32,
            operation_count,
            timeline,
        });
    }
    store.authorize_project(SceneCommitIntent::PublicApply(&transaction))?;
    let result = AgentEditPlan {
        original_request_digest: crate::identity::request_identity(
            "agent_edit_v1_plan",
            &request.project_id,
            request,
        )?
        .current_digest()
        .to_owned(),
        expected_revision: request.expected_revision.clone(),
        steps: decisions,
        transaction: (!transaction.operations.is_empty()).then_some(transaction),
    };
    if serde_json::to_vec(&result)
        .map_err(|_| EditorError::new("INTERNAL_ERROR", "Could not serialize Agent plan"))?
        .len()
        > 2 * 1024 * 1024
    {
        return Err(EditorError::new(
            "RESOURCE_TOO_LARGE",
            "Agent plan exceeds the public control budget",
        ));
    }
    Ok(result)
}
