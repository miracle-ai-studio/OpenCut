//! Complete non-main track removal over the current authorized V4 head. This is
//! a pure proposal: resources remain bound and the public edit port owns commit,
//! idempotency, history, recovery and resource/lease fences.
use crate::scene::{
    SceneCommitIntent, StoredProjectDocument,
    public::{ProjectPublicResources, plan_public},
};
use opencut_contracts::{
    EditorError,
    project_public::{ApplyProject, ProjectEditOperation as Op},
    track_removal::PlanTrackRemoval,
};
use std::collections::BTreeSet;

pub fn plan(
    store: &impl ProjectPublicResources,
    input: &PlanTrackRemoval,
) -> Result<ApplyProject, EditorError> {
    crate::require_idempotency_key(&input.idempotency_key)?;
    if [
        &input.project_id,
        &input.expected_revision,
        &input.sequence_id,
        &input.track_id,
    ]
    .iter()
    .any(|id| id.trim().is_empty() || id.len() > 128 || id.chars().any(char::is_control))
        || input.label.trim().is_empty()
        || input.label.len() > 4096
        || input.label.chars().any(char::is_control)
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Track removal requires bounded fixed identities and a label",
        ));
    }
    let mut tx = ApplyProject {
        branch_name: None,
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: input.label.clone(),
        operations: vec![],
    };
    store.authorize_project(SceneCommitIntent::PublicApply(&tx))?;
    let source = store.head_scene_source(&input.project_id)?;
    if source.document.project_id() != input.project_id {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Track removal head scope differs",
        ));
    }
    if source.revision != input.expected_revision {
        return Err(crate::revision_conflict(
            &input.expected_revision,
            &source.revision,
        ));
    }
    let StoredProjectDocument::Editable(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Track removal requires a V4 head",
        ));
    };
    opencut_editor_core::scene::validate_project_scene(&project)?;
    let sequence = project
        .sequences
        .iter()
        .find(|s| s.id == input.sequence_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Track removal scene does not exist"))?;
    let track = sequence
        .tracks
        .iter()
        .find(|t| t.id == input.track_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Track removal target does not exist"))?;
    if track.id == sequence.main_track_id || track.locked {
        return Err(EditorError::new(
            "CONSTRAINT_VIOLATION",
            "Only an unlocked non-main track can be removed",
        ));
    }
    let elements: BTreeSet<_> = track.elements.iter().map(|e| e.id.as_str()).collect();
    for transition in &sequence.transitions {
        if elements.contains(transition.outgoing_element_id.as_str())
            || elements.contains(transition.incoming_element_id.as_str())
        {
            tx.operations.push(Op::TransitionDelete {
                sequence_id: sequence.id.clone(),
                transition_id: transition.id.clone(),
            });
        }
    }
    for caption in &sequence.captions {
        if caption.track_id == track.id {
            tx.operations.push(Op::CaptionDelete {
                sequence_id: sequence.id.clone(),
                caption_id: caption.id.clone(),
            });
        }
    }
    tx.operations.push(Op::TrackDelete {
        sequence_id: sequence.id.clone(),
        track_id: track.id.clone(),
    });
    // Exercise the exact public-operation/lock/aggregate validator, including a
    // fresh fixed-head read. Never return a proposal whose cleanup is incomplete.
    let proposed = plan_public(store, &tx)?;
    let mut expected = (*project).clone();
    let after = expected
        .sequences
        .iter_mut()
        .find(|s| s.id == sequence.id)
        .expect("captured scene");
    after.transitions.retain(|t| {
        !elements.contains(t.outgoing_element_id.as_str())
            && !elements.contains(t.incoming_element_id.as_str())
    });
    after.captions.retain(|c| c.track_id != track.id);
    after.tracks.retain(|t| t.id != track.id);
    if proposed
        != crate::scene::public::project(&opencut_contracts::scene::EditorSnapshot {
            project: expected,
            revision: source.revision,
        })
        .project
    {
        return Err(EditorError::new(
            "INVALID_TRACK_REMOVAL_PLAN",
            "Track removal changed unrelated project content",
        ));
    }
    store.authorize_project(SceneCommitIntent::PublicApply(&tx))?;
    Ok(tx)
}

#[cfg(test)]
mod tests;
