//! Media commands mutate a private scene clone; one final aggregate check sees
//! all simultaneous moves/splits. Diff emits only the existing public operations.
use super::*;
fn selection(targets: &[TimelineTargetV4]) -> Result<(), EditorError> {
    if targets.is_empty() || targets.len() > 60 {
        return Err(invalid("Choose 1..60 media targets"));
    }
    let mut ids = BTreeSet::new();
    for target in targets {
        id(&target.track_id)?;
        id(&target.element_id)?;
        if !ids.insert(&target.element_id) {
            return Err(invalid("Timeline selection repeats an element"));
        }
    }
    Ok(())
}
fn media_track(track: &TrackV4) -> Result<(), EditorError> {
    if !matches!(
        track.kind,
        TrackKindV4::Video | TrackKindV4::Audio | TrackKindV4::Graphic
    ) {
        return Err(unsupported("This version manages media tracks only"));
    }
    Ok(())
}
pub(super) fn apply(
    project: &ProjectDocumentV4,
    before: &SequenceV4,
    after: &mut SequenceV4,
    input: &TimelineEditRequestV4,
    decision: &mut TimelineDecisionV4,
    ids: &mut identities::Ids<'_>,
) -> Result<bool, EditorError> {
    use TimelineCommandV4::*;
    match &input.command {
        MoveSelection { moves, ripple } => {
            let targets: Vec<_> = moves.iter().map(|m| m.target.clone()).collect();
            selection(&targets)?;
            let (fps, classic) = timing::fps(project, &before.id)?;
            let mut moved = vec![];
            for movement in moves {
                let (source, index) = target(before, &movement.target)?;
                id(&movement.target_track_id)?;
                let destination = track(before, &movement.target_track_id)?;
                unlocked(&before.tracks[destination])?;
                media_track(&before.tracks[destination])?;
                let mut e = before.tracks[source].elements[index].clone();
                let start = timing::align(&movement.new_start, fps, classic)?;
                let same_start =
                    start.cmp_exact(time(&e.timeline_range.start)?) == std::cmp::Ordering::Equal;
                if !same_start {
                    e.timeline_range.start = start.into();
                }
                if destination != source || !same_start {
                    moved.push((destination, e));
                }
                decision.selection.push(TimelineTargetV4 {
                    track_id: movement.target_track_id.clone(),
                    element_id: movement.target.element_id.clone(),
                });
            }
            let moved_ids: BTreeSet<_> = moved.iter().map(|(_, e)| e.id.clone()).collect();
            for t in &mut after.tracks {
                t.elements.retain(|e| !moved_ids.contains(e.id.as_str()));
            }
            for (t, e) in moved {
                after.tracks[t].elements.push(e);
            }
            Ok(*ripple)
        }
        Trim {
            target: value,
            edge,
            boundary,
            ripple,
        } => {
            let (t, e) = target(before, value)?;
            after.tracks[t].elements[e] = timing::trim(
                project,
                before,
                t,
                &before.tracks[t].elements[e],
                *edge,
                boundary,
            )?;
            decision.selection.push(value.clone());
            Ok(*ripple)
        }
        Split {
            targets,
            at,
            retain,
            ripple,
        } => {
            let selected: Vec<_> = targets.iter().map(|t| t.target.clone()).collect();
            selection(&selected)?;
            let at = time(at)?;
            if at.ticks() < 0 {
                return Err(invalid("Split point must be nonnegative"));
            }
            // Reserve all requested right IDs before deriving any nested IDs;
            // even a no-op boundary cannot conceal duplicate/future collisions.
            if *retain != TimelineSplitRetainV4::Left {
                for value in targets {
                    ids.claim(&value.right_element_id)?;
                }
            } else {
                for value in targets {
                    id(&value.right_element_id)?;
                }
            }
            for value in targets {
                let (t, e) = target(before, &value.target)?;
                let original = &before.tracks[t].elements[e];
                let index = after.tracks[t]
                    .elements
                    .iter()
                    .position(|e| e.id == original.id)
                    .expect("existing selected target");
                if !at.cmp_exact(time(&original.timeline_range.start)?).is_gt()
                    || !at.cmp_exact(end(original)?).is_lt()
                {
                    decision.selection.push(value.target.clone());
                    continue;
                }
                let (left, mut right) = timing::split(project, original, at)?;
                if *retain != TimelineSplitRetainV4::Left {
                    ids.rebind(&mut right, &value.right_element_id)?;
                }
                match retain {
                    TimelineSplitRetainV4::Both => {
                        after.tracks[t]
                            .elements
                            .splice(index..=index, [left, right]);
                        decision.selection.push(TimelineTargetV4 {
                            track_id: value.target.track_id.clone(),
                            element_id: value.right_element_id.clone(),
                        });
                    }
                    TimelineSplitRetainV4::Left => {
                        after.tracks[t].elements[index] = left;
                        decision.selection.push(value.target.clone());
                    }
                    TimelineSplitRetainV4::Right => {
                        after.tracks[t].elements[index] = right;
                        decision.selection.push(TimelineTargetV4 {
                            track_id: value.target.track_id.clone(),
                            element_id: value.right_element_id.clone(),
                        });
                    }
                }
            }
            Ok(*ripple)
        }
        Delete { targets, ripple } => {
            selection(targets)?;
            for value in targets {
                target(before, value)?;
            }
            let selected: BTreeSet<_> = targets.iter().map(|t| t.element_id.as_str()).collect();
            for t in &mut after.tracks {
                t.elements.retain(|e| !selected.contains(e.id.as_str()));
            }
            Ok(*ripple)
        }
        AddTrack {
            id: track_id,
            name,
            track_kind: kind,
        } => {
            ids.claim(track_id)?;
            if name.trim().is_empty()
                || name.len() > 4096
                || !matches!(
                    kind,
                    TrackKindV4::Video | TrackKindV4::Audio | TrackKindV4::Graphic
                )
            {
                return Err(invalid(
                    "Empty media track requires a name and supported kind",
                ));
            }
            after.tracks.push(TrackV4 {
                id: track_id.clone(),
                name: name.clone(),
                kind: *kind,
                enabled: true,
                muted: false,
                locked: false,
                opacity: 1.0,
                gain: GainV4::Decibels { value: 0.0 },
                elements: vec![],
                channels: vec![],
            });
            Ok(false)
        }
        DeleteEmptyTrack { track_id } => {
            id(track_id)?;
            let t = track(before, track_id)?;
            unlocked(&before.tracks[t])?;
            media_track(&before.tracks[t])?;
            if *track_id == before.main_track_id
                || !before.tracks[t].elements.is_empty()
                || !before.tracks[t].channels.is_empty()
                || before.captions.iter().any(|c| c.track_id == *track_id)
            {
                return Err(invalid(
                    "Delete requires an empty non-main media track without channels or captions",
                ));
            }
            after.tracks.remove(t);
            Ok(false)
        }
        SetTrackLocked { track_id, locked } => {
            id(track_id)?;
            let t = track(before, track_id)?;
            media_track(&before.tracks[t])?;
            after.tracks[t].locked = *locked;
            Ok(false)
        }
        SetTrackEnabled { track_id, enabled } => {
            id(track_id)?;
            let t = track(before, track_id)?;
            unlocked(&before.tracks[t])?;
            media_track(&before.tracks[t])?;
            after.tracks[t].enabled = *enabled;
            Ok(false)
        }
        SetTrackMuted { track_id, muted } => {
            id(track_id)?;
            let t = track(before, track_id)?;
            unlocked(&before.tracks[t])?;
            media_track(&before.tracks[t])?;
            after.tracks[t].muted = *muted;
            Ok(false)
        }
    }
}

pub(super) fn diff(
    before: &SequenceV4,
    after: &SequenceV4,
    decision: &mut TimelineDecisionV4,
) -> Result<Vec<ProjectOperationV4>, EditorError> {
    use ProjectOperationV4::*;
    let mut ops = vec![];
    for cut in &before.transitions {
        if !after.transitions.iter().any(|c| c.id == cut.id) {
            ops.push(TransitionDelete {
                sequence_id: before.id.clone(),
                transition_id: cut.id.clone(),
            });
        }
    }
    for t in &before.tracks {
        if !after.tracks.iter().any(|n| n.id == t.id) {
            ops.push(TrackDelete {
                sequence_id: before.id.clone(),
                track_id: t.id.clone(),
            });
            decision.deleted_ids.push(t.id.clone());
        }
    }
    let old_elements: BTreeMap<_, _> = before
        .tracks
        .iter()
        .flat_map(|t| {
            t.elements
                .iter()
                .map(move |e| (e.id.as_str(), (t.id.as_str(), e)))
        })
        .collect();
    let new_ids: BTreeSet<_> = after
        .tracks
        .iter()
        .flat_map(|t| &t.elements)
        .map(|e| e.id.as_str())
        .collect();
    // Full track replacement is needed only when ElementPut's append/in-place
    // behavior cannot retain the planned vector order (notably middle splits).
    let replacements: BTreeSet<_> = after
        .tracks
        .iter()
        .filter(|track| {
            let Some(old) = before.tracks.iter().find(|old| old.id == track.id) else {
                return true;
            };
            if old.enabled != track.enabled || old.muted != track.muted {
                return true;
            }
            let expected: Vec<_> = old
                .elements
                .iter()
                .filter(|e| track.elements.iter().any(|n| n.id == e.id))
                .map(|e| e.id.as_str())
                .chain(
                    track
                        .elements
                        .iter()
                        .filter(|e| !old.elements.iter().any(|n| n.id == e.id))
                        .map(|e| e.id.as_str()),
                )
                .collect();
            expected
                != track
                    .elements
                    .iter()
                    .map(|e| e.id.as_str())
                    .collect::<Vec<_>>()
        })
        .map(|track| track.id.as_str())
        .collect();
    // TrackPut cannot transfer an element from another track. Remove those
    // origins first, without classifying a move as a deleted entity.
    for destination in &after.tracks {
        if replacements.contains(destination.id.as_str()) {
            for element in &destination.elements {
                if old_elements
                    .get(element.id.as_str())
                    .is_some_and(|(track, _)| *track != destination.id)
                {
                    ops.push(ElementDelete {
                        sequence_id: before.id.clone(),
                        element_id: element.id.clone(),
                    });
                }
            }
        }
    }
    for id in old_elements.keys() {
        if !new_ids.contains(id) {
            ops.push(ElementDelete {
                sequence_id: before.id.clone(),
                element_id: (*id).into(),
            });
            decision.deleted_ids.push((*id).into());
        }
    }
    for t in &after.tracks {
        let old = before.tracks.iter().find(|n| n.id == t.id);
        let mut handled = false;
        if let Some(old) = old {
            // Track-only controls preserve the full existing local/scene graph.
            if old.locked != t.locked {
                ops.push(TrackLockSet {
                    sequence_id: before.id.clone(),
                    track_id: t.id.clone(),
                    locked: t.locked,
                });
                decision.changed_ids.push(t.id.clone());
                handled = true;
            } else if old.enabled != t.enabled || old.muted != t.muted {
                ops.push(TrackPut {
                    sequence_id: before.id.clone(),
                    track: t.clone(),
                });
                decision.changed_ids.push(t.id.clone());
                handled = true;
            }
        } else {
            ops.push(TrackPut {
                sequence_id: before.id.clone(),
                track: t.clone(),
            });
            decision.created_ids.push(t.id.clone());
            handled = true;
        }
        if !handled && replacements.contains(t.id.as_str()) {
            ops.push(TrackPut {
                sequence_id: before.id.clone(),
                track: t.clone(),
            });
            decision.changed_ids.push(t.id.clone());
            handled = true;
        }
        for e in &t.elements {
            if old_elements
                .get(e.id.as_str())
                .is_some_and(|(id, old)| *id == t.id && *old == e)
            {
                continue;
            }
            if !handled {
                ops.push(ElementPut {
                    sequence_id: before.id.clone(),
                    track_id: t.id.clone(),
                    element: Box::new(e.clone()),
                });
            }
            if old_elements.contains_key(e.id.as_str()) {
                decision.changed_ids.push(e.id.clone());
            } else {
                decision.created_ids.push(e.id.clone());
            }
        }
    }
    if ops.len() > 128 {
        return Err(invalid(
            "Timeline edit exceeds the atomic 128-operation limit",
        ));
    }
    Ok(ops)
}
pub(super) fn public(op: &ProjectOperationV4) -> PublicProjectOperationV4 {
    match op {
        ProjectOperationV4::ElementPut {
            sequence_id,
            track_id,
            element,
        } => PublicProjectOperationV4::ElementPut {
            sequence_id: sequence_id.clone(),
            track_id: track_id.clone(),
            element: element.clone(),
        },
        ProjectOperationV4::ElementDelete {
            sequence_id,
            element_id,
        } => PublicProjectOperationV4::ElementDelete {
            sequence_id: sequence_id.clone(),
            element_id: element_id.clone(),
        },
        ProjectOperationV4::TrackPut { sequence_id, track } => PublicProjectOperationV4::TrackPut {
            sequence_id: sequence_id.clone(),
            track: track.clone(),
        },
        ProjectOperationV4::TrackDelete {
            sequence_id,
            track_id,
        } => PublicProjectOperationV4::TrackDelete {
            sequence_id: sequence_id.clone(),
            track_id: track_id.clone(),
        },
        ProjectOperationV4::TrackLockSet {
            sequence_id,
            track_id,
            locked,
        } => PublicProjectOperationV4::TrackLockSet {
            sequence_id: sequence_id.clone(),
            track_id: track_id.clone(),
            locked: *locked,
        },
        ProjectOperationV4::TransitionDelete {
            sequence_id,
            transition_id,
        } => PublicProjectOperationV4::TransitionDelete {
            sequence_id: sequence_id.clone(),
            transition_id: transition_id.clone(),
        },
        _ => unreachable!("Timeline diff cannot acquire private resources"),
    }
}
