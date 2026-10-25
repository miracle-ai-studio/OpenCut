//! Fixed-head authoring compiled to existing atomic public operations. No new
//! storage port, time sampler, resource reader or client-side domain authority.
use crate::scene::{
    SceneCommitIntent, StoredProjectDocument,
    public::{ProjectPublicResources, plan_public},
};
use opencut_contracts::{EditorError, Time, keyframe_edit::*, project_public::*, scene::*};
use opencut_time::RationalTime as R;
use std::collections::BTreeSet;
mod edits;
#[cfg(test)]
mod tests;
fn invalid(message: impl Into<String>) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn missing() -> EditorError {
    EditorError::new("NOT_FOUND", "Keyframe channel or key does not exist")
}
fn cid(c: &AnimationChannel) -> &str {
    match c {
        AnimationChannel::Scalar { id, .. } | AnimationChannel::Discrete { id, .. } => id,
    }
}
fn bounded(value: &str) -> Result<(), EditorError> {
    if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(invalid("Keyframe identities need 1..128 noncontrol bytes"));
    }
    Ok(())
}
fn channels_mut<'a>(
    sequence: &'a mut Scene,
    channel_id: &str,
) -> Result<&'a mut AnimationChannel, EditorError> {
    bounded(channel_id)?;
    for track in &mut sequence.tracks {
        if let Some(c) = track.channels.iter_mut().find(|c| cid(c) == channel_id) {
            if track.locked {
                return Err(EditorError::new(
                    "TRACK_LOCKED",
                    "Unlock the keyframe track",
                ));
            }
            return Ok(c);
        }
        for element in &mut track.elements {
            if let Some(c) = element.channels.iter_mut().find(|c| cid(c) == channel_id) {
                if track.locked {
                    return Err(EditorError::new(
                        "TRACK_LOCKED",
                        "Unlock the keyframe track",
                    ));
                }
                return Ok(c);
            }
        }
    }
    Err(missing())
}
fn validate_times(original: &Scene, sequence: &Scene) -> Result<(), EditorError> {
    let mut sequence_end = R::new(0, 1)?;
    for caption in &sequence.captions {
        let end = R::try_from(&caption.timeline_range.start)?
            .checked_add(R::try_from(&caption.timeline_range.duration)?)?;
        if end.cmp_exact(sequence_end).is_gt() {
            sequence_end = end;
        }
    }
    for t in &sequence.tracks {
        for e in &t.elements {
            let end = R::try_from(&e.timeline_range.start)?
                .checked_add(R::try_from(&e.timeline_range.duration)?)?;
            if end.cmp_exact(sequence_end).is_gt() {
                sequence_end = end;
            }
        }
    }
    for track in &sequence.tracks {
        for (channels, end) in
            std::iter::once((&track.channels, sequence_end)).chain(track.elements.iter().map(|e| {
                (
                    &e.channels,
                    R::try_from(&e.timeline_range.duration).expect("validated duration"),
                )
            }))
        {
            for channel in channels {
                if original
                    .tracks
                    .iter()
                    .flat_map(|t| {
                        t.channels
                            .iter()
                            .chain(t.elements.iter().flat_map(|e| e.channels.iter()))
                    })
                    .any(|old| cid(old) == cid(channel) && old == channel)
                {
                    continue;
                }
                let times: Vec<&Time> = match channel {
                    AnimationChannel::Scalar { keys, .. } => keys.iter().map(|k| &k.time).collect(),
                    AnimationChannel::Discrete { keys, .. } => {
                        keys.iter().map(|k| &k.time).collect()
                    }
                };
                let mut previous = None;
                for t in times {
                    let t = R::try_from(t)?;
                    if t.ticks() < 0 || t.cmp_exact(end).is_gt() {
                        return Err(invalid(
                            "Keyframe time is outside its owner duration; paste and drag never clamp",
                        ));
                    }
                    if previous.is_some_and(|p: R| p.cmp_exact(t).is_ge()) {
                        return Err(invalid("Duplicate key times reject the whole batch"));
                    }
                    previous = Some(t);
                }
            }
        }
    }
    Ok(())
}
pub fn plan(
    store: &impl ProjectPublicResources,
    input: &KeyframeEditRequest,
) -> Result<ApplyProject, EditorError> {
    bounded(&input.project_id)?;
    bounded(&input.sequence_id)?;
    if input.edits.is_empty() || input.edits.len() > 128 {
        return Err(invalid("Choose 1..128 keyframe edits"));
    }
    let mut touched = 0usize;
    for edit in &input.edits {
        touched = touched.saturating_add(match edit {
            KeyframeEdit::ChannelPut { channel, .. } => match channel {
                AnimationChannel::Scalar { keys, .. } => keys.len(),
                AnimationChannel::Discrete { keys, .. } => keys.len(),
            },
            KeyframeEdit::Move { keys, .. } | KeyframeEdit::Delete { keys } => keys.len(),
            KeyframeEdit::PasteScalar { keys, .. } => keys.len(),
            KeyframeEdit::PasteDiscrete { keys, .. } => keys.len(),
            _ => 1,
        });
        if touched > 4096 {
            return Err(invalid("A keyframe batch may touch at most 4096 keys"));
        }
    }
    let mut transaction = ApplyProject {
        branch_name: None,
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: input.label.clone(),
        operations: vec![],
    };
    store.authorize_project(SceneCommitIntent::PublicApply(&transaction))?;
    let source = store.head_scene_source(&input.project_id)?;
    if source.revision != input.expected_revision {
        return Err(crate::revision_conflict(
            &input.expected_revision,
            &source.revision,
        ));
    }
    let StoredProjectDocument::Editable(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Keyframe editing requires V4",
        ));
    };
    if project.id != input.project_id {
        return Err(invalid("Keyframe project scope differs"));
    }
    opencut_editor_core::scene::validate_project_scene(&project)?;
    let original = project
        .sequences
        .iter()
        .find(|s| s.id == input.sequence_id)
        .ok_or_else(missing)?;
    let mut next = original.clone();
    for edit in &input.edits {
        edits::apply(&mut next, edit)?;
    }
    // Only modified owners are constrained. Existing imported off-span channels
    // elsewhere must not prevent a local edit or be silently rewritten.
    for (old, new) in original.tracks.iter().zip(&next.tracks) {
        if old.channels != new.channels {
            transaction.operations.push(ProjectEditOperation::TrackPut {
                sequence_id: input.sequence_id.clone(),
                track: new.clone(),
            });
        } else {
            for (old_e, new_e) in old.elements.iter().zip(&new.elements) {
                if old_e.channels != new_e.channels {
                    transaction
                        .operations
                        .push(ProjectEditOperation::ElementPut {
                            sequence_id: input.sequence_id.clone(),
                            track_id: new.id.clone(),
                            element: Box::new(new_e.clone()),
                        });
                }
            }
        }
    }
    if transaction.operations.is_empty() {
        return Err(invalid("The keyframe edit makes no change"));
    }
    validate_times(original, &next)?;
    plan_public(store, &transaction)?;
    store.authorize_project(SceneCommitIntent::PublicApply(&transaction))?;
    Ok(transaction)
}
