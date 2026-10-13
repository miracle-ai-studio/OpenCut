//! Pure media timeline plans over one authorized V4 head. Resources are neither
//! rebound nor decoded; clients submit the public transaction unchanged.
use crate::project_v4::{StoredProjectDocumentV4, V4Intent, public::ProjectV4PublicResources};
use opencut_contracts::{
    EditorError, FrameRate, Time, project_v4::*, project_v4_ops::*, project_v4_public::*,
    timeline_v4::*,
};
use opencut_time::RationalTime as R;
use std::collections::{BTreeMap, BTreeSet};
mod edits;
mod identities;
mod ripple;
#[cfg(test)]
mod tests;
mod timing;
mod transitions;

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
fn end(e: &TimelineElementV4) -> Result<R, EditorError> {
    time(&e.timeline_range.start)?.checked_add(time(&e.timeline_range.duration)?)
}
fn id(value: &str) -> Result<(), EditorError> {
    if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(invalid("Timeline identity must contain 1..128 bytes"));
    }
    Ok(())
}
fn unlocked(track: &TrackV4) -> Result<(), EditorError> {
    if track.locked {
        return Err(EditorError::new(
            "CONSTRAINT_VIOLATION",
            format!("Track {} is locked", track.id),
        ));
    }
    Ok(())
}
#[derive(Clone, Copy)]
enum Profile {
    Media,
    Elements,
}
impl Profile {
    fn accepts_text(self) -> bool {
        matches!(self, Self::Elements)
    }
}
fn media(e: &TimelineElementV4, profile: Profile) -> Result<(), EditorError> {
    if !(profile.accepts_text() && matches!(e.content, ElementContentV4::Text { .. }))
        && !matches!(
            e.content,
            ElementContentV4::Video { .. }
                | ElementContentV4::Audio { .. }
                | ElementContentV4::Image { .. }
        )
    {
        return Err(unsupported(
            "This timeline version edits video, audio and static image elements",
        ));
    }
    Ok(())
}
fn track(scene: &SequenceV4, id: &str) -> Result<usize, EditorError> {
    scene
        .tracks
        .iter()
        .position(|t| t.id == id)
        .ok_or_else(|| missing("Timeline track does not exist"))
}
fn target(
    scene: &SequenceV4,
    value: &TimelineTargetV4,
    profile: Profile,
) -> Result<(usize, usize), EditorError> {
    id(&value.track_id)?;
    id(&value.element_id)?;
    let t = track(scene, &value.track_id)?;
    unlocked(&scene.tracks[t])?;
    let e = scene.tracks[t]
        .elements
        .iter()
        .position(|e| e.id == value.element_id)
        .ok_or_else(|| missing("Element does not belong to its explicit source track"))?;
    media(&scene.tracks[t].elements[e], profile)?;
    Ok((t, e))
}
fn no_animation(e: &TimelineElementV4) -> Result<(), EditorError> {
    if !e.channels.is_empty() {
        return Err(EditorError::new(
            "UNSUPPORTED_TIMELINE_ANIMATION",
            "Animated trim/split requires the complete typed curve-cutting profile",
        ));
    }
    Ok(())
}

/// Current Read/Edit/write-control is checked before loading and after the
/// completed proposal. No write, idempotency receipt or CAS read occurs here.
pub fn plan(
    store: &impl ProjectV4PublicResources,
    input: &TimelineEditRequestV4,
) -> Result<TimelineEditPlanV4, EditorError> {
    plan_with_profile(store, input, Profile::Media)
}

/// Versioned element timeline profile: media and authored free text share the
/// same fixed-head authorization and atomic public-operation kernel.
pub fn plan_elements(
    store: &impl ProjectV4PublicResources,
    input: &TimelineEditRequestV4,
) -> Result<TimelineEditPlanV4, EditorError> {
    plan_with_profile(store, input, Profile::Elements)
}
fn plan_with_profile(
    store: &impl ProjectV4PublicResources,
    input: &TimelineEditRequestV4,
    profile: Profile,
) -> Result<TimelineEditPlanV4, EditorError> {
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
    let mut tx = ApplyPublicV4Project {
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: input.label.clone(),
        operations: vec![],
    };
    store.authorize_v4(V4Intent::PublicApply(&tx))?;
    let source = store.head_v4_source(&input.project_id)?;
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
    let StoredProjectDocumentV4::V4(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Timeline planning requires a V4 head",
        ));
    };
    opencut_editor_core::project_v4::validate_project_v4(&project)?;
    let before = project
        .sequences
        .iter()
        .find(|s| s.id == input.sequence_id)
        .ok_or_else(|| missing("Timeline sequence does not exist"))?;
    let mut after = before.clone();
    let mut decision = TimelineDecisionV4 {
        created_ids: vec![],
        changed_ids: vec![],
        deleted_ids: vec![],
        selection: vec![],
        ripple_shifted_ids: vec![],
        removed_transition_ids: vec![],
    };
    let mut ids = identities::Ids::new(&project, &input.expected_revision, &input.idempotency_key)?;
    let ripple = edits::apply(
        &project,
        before,
        &mut after,
        input,
        &mut decision,
        &mut ids,
        profile,
    )?;
    if ripple {
        ripple::apply(before, &mut after, &mut decision, profile)?;
    }
    // Preserve transitions whose actual seam and source handles remain valid.
    // Caption asset anchors and unrelated scene-owned data are unchanged.
    transitions::retain(&project, &mut after, &mut decision)?;
    let internal = edits::diff(before, &after, &mut decision)?;
    for op in &internal {
        tx.operations.push(edits::public(op));
    }
    if !internal.is_empty() {
        let proposed = crate::project_v4::plan(
            &ProjectSnapshotV4 {
                project: (*project).clone(),
                revision: source.revision,
            },
            &ApplyV4Project {
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
    // Complete no-change proposals still validate the unchanged aggregate and
    // the command's explicit target locks; no empty transaction is published.
    store.authorize_v4(V4Intent::PublicApply(&tx))?;
    Ok(TimelineEditPlanV4 {
        original_request_digest: crate::identity::request_identity(
            match profile {
                Profile::Media => "project_v4_timeline",
                Profile::Elements => "project_v4_element_timeline",
            },
            &input.project_id,
            input,
        )?
        .current_digest()
        .into(),
        decision,
        transaction: (!tx.operations.is_empty()).then_some(tx),
    })
}
