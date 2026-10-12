//! Classic clipboard group placement, expressed in V4's visual layer order.
use super::*;

fn blocked(message: &str) -> EditorError {
    EditorError::new("CONSTRAINT_VIOLATION", message)
}
fn overlaps(a: &TimelineElementV4, b: &TimelineElementV4) -> Result<bool, EditorError> {
    let a_start = time(&a.timeline_range.start)?;
    let b_start = time(&b.timeline_range.start)?;
    let a_end = a_start.checked_add(time(&a.timeline_range.duration)?)?;
    let b_end = b_start.checked_add(time(&b.timeline_range.duration)?)?;
    Ok(a_start.cmp_exact(b_end).is_lt() && a_end.cmp_exact(b_start).is_gt())
}
fn available(
    track: &TrackV4,
    kind: TrackKindV4,
    elements: &[TimelineElementV4],
) -> Result<bool, EditorError> {
    if track.locked || track.kind != kind {
        return Ok(false);
    }
    for element in elements {
        for existing in &track.elements {
            if overlaps(element, existing)? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
fn display_order(scene: &SequenceV4) -> Vec<usize> {
    // Classic overlay is top-to-bottom; V4 visuals are bottom-to-top.
    let mut order: Vec<_> = scene
        .tracks
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, t)| t.kind != TrackKindV4::Audio && t.id != scene.main_track_id)
        .map(|(i, _)| i)
        .collect();
    if let Some(main) = scene
        .tracks
        .iter()
        .position(|t| t.id == scene.main_track_id)
    {
        order.push(main);
    }
    order.extend(
        scene
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.kind == TrackKindV4::Audio)
            .map(|(i, _)| i),
    );
    order
}
fn insert_index(
    scene: &SequenceV4,
    kind: TrackKindV4,
    anchor: &ClipboardTrackInsertionV4,
) -> Result<usize, EditorError> {
    match anchor {
        ClipboardTrackInsertionV4::Highest => Ok(if kind == TrackKindV4::Audio {
            scene
                .tracks
                .iter()
                .position(|t| t.kind == TrackKindV4::Audio)
                .unwrap_or(scene.tracks.len())
        } else {
            scene
                .tracks
                .iter()
                .rposition(|t| t.kind != TrackKindV4::Audio)
                .map_or(0, |i| i + 1)
        }),
        ClipboardTrackInsertionV4::Above { track_id }
        | ClipboardTrackInsertionV4::Below { track_id } => {
            id(track_id)?;
            let index = scene
                .tracks
                .iter()
                .position(|t| &t.id == track_id)
                .ok_or_else(|| missing("Clipboard insertion anchor is missing"))?;
            let audio = kind == TrackKindV4::Audio;
            if audio != (scene.tracks[index].kind == TrackKindV4::Audio) {
                return Err(invalid(
                    "Insertion anchor must belong to the same visual/audio region",
                ));
            }
            let above = matches!(anchor, ClipboardTrackInsertionV4::Above { .. });
            Ok(index + usize::from(above != audio))
        }
    }
}
fn pin_main(
    scene: &SequenceV4,
    index: usize,
    elements: &mut [TimelineElementV4],
) -> Result<(), EditorError> {
    if scene.tracks[index].id != scene.main_track_id {
        return Ok(());
    }
    let requested = elements
        .iter()
        .map(|e| time(&e.timeline_range.start))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min_by(|a, b| a.cmp_exact(*b))
        .ok_or_else(|| invalid("Empty clipboard group"))?;
    let existing = scene.tracks[index]
        .elements
        .iter()
        .map(|e| time(&e.timeline_range.start))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min_by(|a, b| a.cmp_exact(*b));
    if existing.is_none_or(|earliest| requested.cmp_exact(earliest).is_le()) {
        for element in elements {
            element.timeline_range.start =
                Time::from(time(&element.timeline_range.start)?.checked_sub(requested)?);
        }
    }
    Ok(())
}
pub(super) fn apply(
    scene: &mut SequenceV4,
    source_track_id: &str,
    kind: TrackKindV4,
    target: &ClipboardTrackTargetV4,
    elements: &mut [TimelineElementV4],
    decision: &mut PasteTimelineDecisionV4,
) -> Result<String, EditorError> {
    if kind == TrackKindV4::Caption {
        return Err(invalid(
            "Captions are scene-owned, not element clipboard content",
        ));
    }
    let mut create = None;
    let existing = match target {
        ClipboardTrackTargetV4::Existing { track_id } => {
            let index = scene
                .tracks
                .iter()
                .position(|t| &t.id == track_id)
                .ok_or_else(|| missing("Clipboard target track is missing"))?;
            if scene.tracks[index].locked {
                return Err(blocked("Clipboard target track is locked"));
            }
            if scene.tracks[index].kind != kind {
                return Err(invalid(
                    "Clipboard target track kind differs from source group",
                ));
            }
            Some(index)
        }
        ClipboardTrackTargetV4::New {
            track_id,
            name,
            track_kind,
            insertion_anchor,
        } => {
            if *track_kind != kind {
                return Err(invalid(
                    "New clipboard track kind differs from source group",
                ));
            }
            create = Some((
                track_id.clone(),
                name.clone(),
                insert_index(scene, kind, insertion_anchor)?,
            ));
            None
        }
        ClipboardTrackTargetV4::ClassicAboveSource { new_track_id, name } => {
            let order = display_order(scene);
            let above = order
                .iter()
                .position(|&i| scene.tracks[i].id == source_track_id)
                .and_then(|i| i.checked_sub(1))
                .map(|i| order[i]);
            let mut chosen = None;
            if let Some(index) = above
                && available(&scene.tracks[index], kind, elements)?
            {
                chosen = Some(index);
            }
            if chosen.is_none() {
                for index in order {
                    if available(&scene.tracks[index], kind, elements)? {
                        chosen = Some(index);
                        break;
                    }
                }
            }
            if chosen.is_none() {
                create = Some((
                    new_track_id.clone(),
                    name.clone(),
                    insert_index(scene, kind, &ClipboardTrackInsertionV4::Highest)?,
                ));
            }
            chosen
        }
    };
    if let Some(index) = existing {
        pin_main(scene, index, elements)?;
        // Explicit placement does not fall back when its final pinned group
        // conflicts. Disabled elements occupy their authored timeline too.
        if !available(&scene.tracks[index], kind, elements)? {
            return Err(blocked("Clipboard group overlaps its target track"));
        }
        let track = &mut scene.tracks[index];
        track.elements.extend_from_slice(elements);
        return Ok(track.id.clone());
    }
    let (track_id, name, index) = create.ok_or_else(|| invalid("No clipboard placement"))?;
    scene.tracks.insert(
        index,
        TrackV4 {
            id: track_id.clone(),
            name,
            kind,
            enabled: true,
            muted: false,
            locked: false,
            opacity: 1.0,
            gain: GainV4::Decibels { value: 0.0 },
            elements: elements.to_vec(),
            channels: vec![],
        },
    );
    decision.new_track_ids.push(track_id.clone());
    Ok(track_id)
}
