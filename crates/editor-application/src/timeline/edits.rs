//! Media commands mutate a private scene clone; one final aggregate check sees
//! all simultaneous moves/splits. Diff emits only the existing public operations.
use super::*;
fn selection(targets: &[TimelineTarget]) -> Result<(), EditorError> {
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
fn source_start(before: &Scene, value: &TimelineTarget) -> Result<R, EditorError> {
    if before.captions.iter().any(|c| c.id == value.element_id) {
        let index = caption_target(before, value)?;
        time(&before.captions[index].timeline_range.start)
    } else {
        let (track, element) = target(before, value)?;
        time(&before.tracks[track].elements[element].timeline_range.start)
    }
}

/// Snap a rigid group once, preserve every exact member offset, and apply
/// Classic's earliest-main placement rule. Independent moves retain their offsets.
fn group_delta(
    before: &Scene,
    moves: &[TimelineMove],

    fps: &FrameRate,
    classic: bool,
) -> Result<Option<R>, EditorError> {
    let first = &moves[0];
    let first_start = source_start(before, &first.target)?;
    let raw_delta = time(&first.new_start)?.checked_sub(first_start)?;
    let mut starts = Vec::with_capacity(moves.len());
    for movement in moves {
        let start = source_start(before, &movement.target)?;
        if time(&movement.new_start)?
            .checked_sub(start)?
            .cmp_exact(raw_delta)
            .is_ne()
        {
            return Ok(None);
        }
        starts.push(start);
    }
    // A zero time offset must not realign authored data, but changing tracks
    // still needs the same main-track and nonnegative placement constraints.
    let mut delta = if raw_delta.ticks() == 0 {
        zero()
    } else {
        timing::align(&first.new_start, fps, classic)?.checked_sub(first_start)?
    };
    let earliest = starts
        .iter()
        .copied()
        .min_by(|a, b| a.cmp_exact(*b))
        .expect("nonempty selection");
    let minimum_delta = zero().checked_sub(earliest)?;
    if delta.cmp_exact(minimum_delta).is_lt() {
        delta = minimum_delta;
    }
    if let Some(index) = moves
        .iter()
        .position(|m| m.target_track_id == before.main_track_id)
    {
        let moving: BTreeSet<_> = moves.iter().map(|m| m.target.element_id.as_str()).collect();
        let stationary = before
            .tracks
            .iter()
            .find(|t| t.id == before.main_track_id)
            .into_iter()
            .flat_map(|t| &t.elements)
            .filter(|e| !moving.contains(e.id.as_str()))
            .map(|e| time(&e.timeline_range.start))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .min_by(|a, b| a.cmp_exact(*b));
        let requested = starts[index].checked_add(delta)?;
        if stationary.is_none_or(|earliest| requested.cmp_exact(earliest).is_le()) {
            delta = zero().checked_sub(starts[index])?;
        }
    }
    if delta.cmp_exact(minimum_delta).is_lt() {
        delta = minimum_delta;
    }
    Ok(Some(delta))
}
pub(super) fn apply(
    project: &EditorDocument,
    before: &Scene,
    after: &mut Scene,
    input: &TimelineEditRequest,
    decision: &mut TimelineDecision,
    ids: &mut identities::Ids<'_>,
) -> Result<bool, EditorError> {
    use TimelineCommand::*;
    match &input.command {
        MoveSelection { moves, ripple } => {
            let targets: Vec<_> = moves.iter().map(|m| m.target.clone()).collect();
            selection(&targets)?;
            let (fps, classic) = timing::fps(project, &before.id)?;
            let delta = group_delta(before, moves, fps, classic)?;
            let mut moved = vec![];
            for movement in moves {
                if before
                    .captions
                    .iter()
                    .any(|c| c.id == movement.target.element_id)
                {
                    let index = caption_target(before, &movement.target)?;
                    let destination = track(before, &movement.target_track_id)?;
                    unlocked(&before.tracks[destination])?;
                    if before.tracks[destination].kind != SceneTrackKind::Caption {
                        return Err(invalid("Caption move requires a caption track"));
                    }
                    let (fps, classic) = timing::fps(project, &before.id)?;
                    let mut cue = before.captions[index].clone();
                    cue.track_id = movement.target_track_id.clone();
                    cue.timeline_range.start = match delta {
                        Some(delta) => time(&cue.timeline_range.start)?.checked_add(delta)?,
                        None => timing::align(&movement.new_start, fps, classic)?,
                    }
                    .into();
                    after.captions[index] = cue;
                    decision.selection.push(TimelineTarget {
                        track_id: movement.target_track_id.clone(),
                        element_id: movement.target.element_id.clone(),
                    });
                    continue;
                }
                let (source, index) = target(before, &movement.target)?;
                id(&movement.target_track_id)?;
                let destination = track(before, &movement.target_track_id)?;
                unlocked(&before.tracks[destination])?;

                let mut e = before.tracks[source].elements[index].clone();
                let start = match delta {
                    Some(delta) => time(&e.timeline_range.start)?.checked_add(delta)?,
                    None => timing::align(&movement.new_start, fps, classic)?,
                };
                let same_start =
                    start.cmp_exact(time(&e.timeline_range.start)?) == std::cmp::Ordering::Equal;
                if !same_start {
                    e.timeline_range.start = start.into();
                }
                if destination != source || !same_start {
                    moved.push((destination, e));
                }
                decision.selection.push(TimelineTarget {
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
            if before.captions.iter().any(|c| c.id == value.element_id) {
                let index = caption_target(before, value)?;
                let mut cue = before.captions[index].clone();
                let (fps, classic) = timing::fps(project, &before.id)?;
                let boundary = timing::align(boundary, fps, classic)?;
                let minimum = R::new(i64::from(fps.denominator), fps.numerator)?;
                let start = time(&cue.timeline_range.start)?;
                let finish = start.checked_add(time(&cue.timeline_range.duration)?)?;
                let (next_start, next_end) = match edge {
                    TimelineTrimEdge::Left => (boundary, finish),
                    TimelineTrimEdge::Right => (start, boundary),
                };
                if next_start.ticks() < 0
                    || next_end.checked_sub(next_start)?.cmp_exact(minimum).is_lt()
                {
                    return Err(invalid("Caption trim must retain one frame"));
                }
                cue.timeline_range.start = next_start.into();
                cue.timeline_range.duration = next_end.checked_sub(next_start)?.into();
                after.captions[index] = cue;
                decision.selection.push(value.clone());
                return Ok(*ripple);
            }
            let (t, e) = target(before, value)?;
            after.tracks[t].elements[e] = timing::trim(
                project,
                before,
                t,
                &before.tracks[t].elements[e],
                *edge,
                boundary,
                ids,
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
            if *retain != TimelineSplitRetain::Left {
                for value in targets {
                    ids.claim(&value.right_element_id)?;
                }
            } else {
                for value in targets {
                    id(&value.right_element_id)?;
                }
            }
            for value in targets {
                if before
                    .captions
                    .iter()
                    .any(|c| c.id == value.target.element_id)
                {
                    let i = caption_target(before, &value.target)?;
                    let cue = &before.captions[i];
                    let start = time(&cue.timeline_range.start)?;
                    let finish = start.checked_add(time(&cue.timeline_range.duration)?)?;
                    if !at.cmp_exact(start).is_gt() || !at.cmp_exact(finish).is_lt() {
                        decision.selection.push(value.target.clone());
                        continue;
                    }
                    let mut left = cue.clone();
                    let mut right = cue.clone();
                    left.timeline_range.duration = at.checked_sub(start)?.into();
                    right.id = value.right_element_id.clone();
                    right.timeline_range.start = at.into();
                    right.timeline_range.duration = finish.checked_sub(at)?.into();
                    let index = after
                        .captions
                        .iter()
                        .position(|c| c.id == cue.id)
                        .ok_or_else(|| missing("Caption split target disappeared"))?;
                    match retain {
                        TimelineSplitRetain::Both => {
                            after.captions.splice(index..=index, [left, right]);
                        }
                        TimelineSplitRetain::Left => after.captions[index] = left,
                        TimelineSplitRetain::Right => after.captions[index] = right,
                    }
                    decision.selection.push(TimelineTarget {
                        track_id: cue.track_id.clone(),
                        element_id: if *retain == TimelineSplitRetain::Left {
                            cue.id.clone()
                        } else {
                            value.right_element_id.clone()
                        },
                    });
                    continue;
                }
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
                let (left, mut right) = timing::split(project, original, at, ids)?;
                if *retain != TimelineSplitRetain::Left {
                    ids.rebind(&mut right, &value.right_element_id)?;
                }
                match retain {
                    TimelineSplitRetain::Both => {
                        after.tracks[t]
                            .elements
                            .splice(index..=index, [left, right]);
                        decision.selection.push(TimelineTarget {
                            track_id: value.target.track_id.clone(),
                            element_id: value.right_element_id.clone(),
                        });
                    }
                    TimelineSplitRetain::Left => {
                        after.tracks[t].elements[index] = left;
                        decision.selection.push(value.target.clone());
                    }
                    TimelineSplitRetain::Right => {
                        after.tracks[t].elements[index] = right;
                        decision.selection.push(TimelineTarget {
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
                if before.captions.iter().any(|c| c.id == value.element_id) {
                    caption_target(before, value)?;
                } else {
                    target(before, value)?;
                }
            }
            let selected: BTreeSet<_> = targets.iter().map(|t| t.element_id.as_str()).collect();
            for t in &mut after.tracks {
                t.elements.retain(|e| !selected.contains(e.id.as_str()));
            }
            {
                after.captions.retain(|c| !selected.contains(c.id.as_str()));
            }
            Ok(*ripple)
        }
        AddTrack {
            id: track_id,
            name,
            track_kind: kind,
        } => {
            ids.claim(track_id)?;
            if name.trim().is_empty() || name.len() > 4096 {
                return Err(invalid(
                    "Empty media track requires a name and supported kind",
                ));
            }
            after.tracks.push(SceneTrack {
                id: track_id.clone(),
                name: name.clone(),
                kind: *kind,
                enabled: true,
                muted: false,
                locked: false,
                opacity: 1.0,
                gain: Gain::Decibels { value: 0.0 },
                elements: vec![],
                channels: vec![],
            });
            Ok(false)
        }
        DeleteEmptyTrack { track_id } => {
            id(track_id)?;
            let t = track(before, track_id)?;
            unlocked(&before.tracks[t])?;

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

            after.tracks[t].locked = *locked;
            Ok(false)
        }
        SetTrackEnabled { track_id, enabled } => {
            id(track_id)?;
            let t = track(before, track_id)?;
            unlocked(&before.tracks[t])?;

            after.tracks[t].enabled = *enabled;
            Ok(false)
        }
        SetTrackMuted { track_id, muted } => {
            id(track_id)?;
            let t = track(before, track_id)?;
            unlocked(&before.tracks[t])?;

            after.tracks[t].muted = *muted;
            Ok(false)
        }
    }
}

pub(super) fn diff(
    before: &Scene,
    after: &Scene,
    decision: &mut TimelineDecision,
) -> Result<Vec<ProjectOperation>, EditorError> {
    use ProjectOperation::*;
    let mut ops = vec![];
    for cut in &before.transitions {
        if !after.transitions.iter().any(|c| c.id == cut.id) {
            ops.push(TransitionDelete {
                sequence_id: before.id.clone(),
                transition_id: cut.id.clone(),
            });
        }
    }
    for cue in &before.captions {
        if !after.captions.iter().any(|c| c.id == cue.id) {
            ops.push(CaptionDelete {
                sequence_id: before.id.clone(),
                caption_id: cue.id.clone(),
            });
            decision.deleted_ids.push(cue.id.clone());
        }
    }
    for cue in &after.captions {
        let old = before.captions.iter().find(|c| c.id == cue.id);
        if old != Some(cue) {
            ops.push(CaptionPut {
                sequence_id: before.id.clone(),
                caption: cue.clone(),
            });
            if old.is_some() {
                decision.changed_ids.push(cue.id.clone());
            } else {
                decision.created_ids.push(cue.id.clone());
            }
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
    // CaptionPut updates in place or appends, so replacing/splitting a cue in
    // the middle cannot reproduce the authored vector order. Preserve it with
    // the existing aggregate operation, as TrackPut does for element order.
    // The public planner still checks unchanged bookmarks and all locked tracks
    // (including their captions); no unselected cue is deleted and reinserted.
    let append_order: Vec<_> = before
        .captions
        .iter()
        .filter(|cue| after.captions.iter().any(|next| next.id == cue.id))
        .map(|cue| cue.id.as_str())
        .chain(
            after
                .captions
                .iter()
                .filter(|cue| !before.captions.iter().any(|old| old.id == cue.id))
                .map(|cue| cue.id.as_str()),
        )
        .collect();
    if append_order
        != after
            .captions
            .iter()
            .map(|cue| cue.id.as_str())
            .collect::<Vec<_>>()
    {
        return Ok(vec![SequencePut {
            sequence: after.clone(),
        }]);
    }
    Ok(ops)
}
pub(super) fn public(op: &ProjectOperation) -> ProjectEditOperation {
    match op {
        ProjectOperation::SequencePut { sequence } => ProjectEditOperation::SequencePut {
            sequence: sequence.clone(),
        },
        ProjectOperation::CaptionPut {
            sequence_id,
            caption,
        } => ProjectEditOperation::CaptionPut {
            sequence_id: sequence_id.clone(),
            caption: Box::new(caption.clone()),
        },
        ProjectOperation::CaptionDelete {
            sequence_id,
            caption_id,
        } => ProjectEditOperation::CaptionDelete {
            sequence_id: sequence_id.clone(),
            caption_id: caption_id.clone(),
        },
        ProjectOperation::ElementPut {
            sequence_id,
            track_id,
            element,
        } => ProjectEditOperation::ElementPut {
            sequence_id: sequence_id.clone(),
            track_id: track_id.clone(),
            element: element.clone(),
        },
        ProjectOperation::ElementDelete {
            sequence_id,
            element_id,
        } => ProjectEditOperation::ElementDelete {
            sequence_id: sequence_id.clone(),
            element_id: element_id.clone(),
        },
        ProjectOperation::TrackPut { sequence_id, track } => ProjectEditOperation::TrackPut {
            sequence_id: sequence_id.clone(),
            track: track.clone(),
        },
        ProjectOperation::TrackDelete {
            sequence_id,
            track_id,
        } => ProjectEditOperation::TrackDelete {
            sequence_id: sequence_id.clone(),
            track_id: track_id.clone(),
        },
        ProjectOperation::TrackLockSet {
            sequence_id,
            track_id,
            locked,
        } => ProjectEditOperation::TrackLockSet {
            sequence_id: sequence_id.clone(),
            track_id: track_id.clone(),
            locked: *locked,
        },
        ProjectOperation::TransitionDelete {
            sequence_id,
            transition_id,
        } => ProjectEditOperation::TransitionDelete {
            sequence_id: sequence_id.clone(),
            transition_id: transition_id.clone(),
        },
        _ => unreachable!("Timeline diff cannot acquire private resources"),
    }
}

fn caption_target(scene: &Scene, value: &TimelineTarget) -> Result<usize, EditorError> {
    id(&value.track_id)?;
    id(&value.element_id)?;
    let t = track(scene, &value.track_id)?;
    unlocked(&scene.tracks[t])?;
    if scene.tracks[t].kind != SceneTrackKind::Caption {
        return Err(invalid("Caption belongs to a caption track"));
    }
    scene
        .captions
        .iter()
        .position(|c| c.id == value.element_id && c.track_id == value.track_id)
        .ok_or_else(|| missing("Caption does not belong to its explicit track"))
}
