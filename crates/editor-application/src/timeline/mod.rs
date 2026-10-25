//! Pure media timeline plans over one authorized V4 head. Resources are neither
//! rebound nor decoded; clients submit the public transaction unchanged.
use crate::scene::{SceneCommitIntent, StoredProjectDocument, public::ProjectPublicResources};
use opencut_contracts::{
    EditorError, FrameRate, Time, project_public::*, scene::*, scene_ops::*, timeline::*,
};
use opencut_time::RationalTime as R;
use std::collections::{BTreeMap, BTreeSet};
mod animation;
mod edits;
mod identities;
mod ripple;
#[cfg(test)]
mod tests;
mod timing;
mod transitions;

/// Property edits share exact curve splitting and stable identity allocation.
pub(crate) fn clamp_animation_duration_classic(
    project: &EditorDocument,
    channels: &[AnimationChannel],
    duration: &Time,
    revision: &str,
    key: &str,
    owner: &str,
) -> Result<Vec<AnimationChannel>, EditorError> {
    let mut ids = identities::Ids::new(project, revision, key)?;
    animation::clamp_duration_classic(channels, time(duration)?, &mut ids, owner)
}

fn invalid(message: impl Into<String>) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn missing(message: impl Into<String>) -> EditorError {
    EditorError::new("NOT_FOUND", message)
}
fn unsupported(message: impl Into<String>) -> EditorError {
    EditorError::new("UNSUPPORTED_TIMELINE_PROFILE", message)
}
fn time(t: &Time) -> Result<R, EditorError> {
    R::try_from(t)
}
fn zero() -> R {
    R::new(0, 1).expect("zero")
}
fn end(e: &TimelineElement) -> Result<R, EditorError> {
    time(&e.timeline_range.start)?.checked_add(time(&e.timeline_range.duration)?)
}
fn id(value: &str) -> Result<(), EditorError> {
    if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(invalid("Timeline identity must contain 1..128 bytes"));
    }
    Ok(())
}
fn unlocked(track: &SceneTrack) -> Result<(), EditorError> {
    if track.locked {
        return Err(EditorError::new(
            "CONSTRAINT_VIOLATION",
            format!("Track {} is locked", track.id),
        ));
    }
    Ok(())
}
fn track(scene: &Scene, id: &str) -> Result<usize, EditorError> {
    scene
        .tracks
        .iter()
        .position(|t| t.id == id)
        .ok_or_else(|| missing("Timeline track does not exist"))
}
fn target(scene: &Scene, value: &TimelineTarget) -> Result<(usize, usize), EditorError> {
    id(&value.track_id)?;
    id(&value.element_id)?;
    let t = track(scene, &value.track_id)?;
    unlocked(&scene.tracks[t])?;
    let e = scene.tracks[t]
        .elements
        .iter()
        .position(|e| e.id == value.element_id)
        .ok_or_else(|| missing("Element does not belong to its explicit source track"))?;
    Ok((t, e))
}
fn validate_request(input: &TimelineEditRequest) -> Result<(), EditorError> {
    crate::require_idempotency_key(&input.idempotency_key)?;
    id(&input.project_id)?;
    id(&input.sequence_id)?;
    if input.expected_revision.trim().is_empty()
        || input.label.trim().is_empty()
        || input.label.len() > 4096
    {
        return Err(invalid(
            "Timeline edit needs a fixed revision and 1..4096 byte label",
        ));
    }
    Ok(())
}
fn transaction(input: &TimelineEditRequest) -> ApplyProject {
    ApplyProject {
        branch_name: None,
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: input.label.clone(),
        operations: vec![],
    }
}
pub fn plan(
    store: &impl ProjectPublicResources,
    input: &TimelineEditRequest,
) -> Result<TimelineEditPlan, EditorError> {
    validate_request(input)?;
    let tx = transaction(input);
    store.authorize_project(SceneCommitIntent::PublicApply(&tx))?;
    let source = store.head_scene_source(&input.project_id)?;
    if source.document.project_id() != input.project_id {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Timeline head scope differs",
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
            "Timeline planning requires a V4 head",
        ));
    };
    let result = plan_snapshot(&project, &source.revision, input)?;
    // Authorization belongs to the real store, including a no-change proposal.
    store.authorize_project(SceneCommitIntent::PublicApply(
        result.transaction.as_ref().unwrap_or(&tx),
    ))?;
    Ok(result)
}
/// Internal composition over an already authorized working snapshot. The caller
/// must retain the original fixed head and validate the combined public proposal.
pub(crate) fn plan_working_snapshot(
    snapshot: &EditorSnapshot,
    input: &TimelineEditRequest,
) -> Result<TimelineEditPlan, EditorError> {
    validate_request(input)?;
    if snapshot.project.id != input.project_id || snapshot.revision != input.expected_revision {
        return Err(invalid(
            "Timeline working snapshot differs from the captured request",
        ));
    }
    plan_snapshot(&snapshot.project, &snapshot.revision, input)
}
fn plan_snapshot(
    project: &EditorDocument,
    revision: &str,
    input: &TimelineEditRequest,
) -> Result<TimelineEditPlan, EditorError> {
    let mut tx = transaction(input);
    opencut_editor_core::scene::validate_project_scene(&project)?;
    let before = project
        .sequences
        .iter()
        .find(|s| s.id == input.sequence_id)
        .ok_or_else(|| missing("Timeline sequence does not exist"))?;
    let mut after = before.clone();
    let mut decision = TimelineDecision {
        created_ids: vec![],
        changed_ids: vec![],
        deleted_ids: vec![],
        selection: vec![],
        ripple_shifted_ids: vec![],
        removed_transition_ids: vec![],
    };
    let mut ids = identities::Ids::new(&project, &input.expected_revision, &input.idempotency_key)?;
    let ripple = edits::apply(&project, before, &mut after, input, &mut decision, &mut ids)?;
    if ripple {
        ripple::apply(before, &mut after, &mut decision)?;
    }
    // Preserve transitions whose actual seam and source handles remain valid.
    // Caption asset anchors and unrelated scene-owned data are unchanged.
    transitions::retain(&project, &mut after, &mut decision)?;
    let internal = edits::diff(before, &after, &mut decision)?;
    for op in &internal {
        tx.operations.push(edits::public(op));
    }
    if !internal.is_empty() {
        let proposed = crate::scene::plan(
            &EditorSnapshot {
                project: project.clone(),
                revision: revision.to_owned(),
            },
            &ApplySceneProject {
                project_id: tx.project_id.clone(),
                expected_revision: tx.expected_revision.clone(),
                idempotency_key: tx.idempotency_key.clone(),
                label: tx.label.clone(),
                operations: internal,
            },
        )?;
        if proposed.sequences.iter().find(|s| s.id == after.id) != Some(&after) {
            return Err(EditorError::new(
                "INVALID_TIMELINE_PLAN",
                "Atomic operations changed the planned scene order",
            ));
        }
    }
    Ok(TimelineEditPlan {
        original_request_digest: crate::identity::request_identity(
            "project_timeline",
            &input.project_id,
            input,
        )?
        .current_digest()
        .into(),
        decision,
        transaction: (!tx.operations.is_empty()).then_some(tx),
    })
}
