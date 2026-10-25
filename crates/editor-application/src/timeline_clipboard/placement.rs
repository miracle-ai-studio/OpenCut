//! Classic clipboard group placement, expressed in V4's visual layer order.
use super::*;

fn blocked(message: &str) -> EditorError {
    EditorError::new("CONSTRAINT_VIOLATION", message)
}
fn overlaps(a: &TimelineElement, b: &TimelineElement) -> Result<bool, EditorError> {
    let a_start = time(&a.timeline_range.start)?;
    let b_start = time(&b.timeline_range.start)?;
    let a_end = a_start.checked_add(time(&a.timeline_range.duration)?)?;
    let b_end = b_start.checked_add(time(&b.timeline_range.duration)?)?;
    Ok(a_start.cmp_exact(b_end).is_lt() && a_end.cmp_exact(b_start).is_gt())
}
fn available(
    track: &SceneTrack,
    kind: SceneTrackKind,
    elements: &[TimelineElement],
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
pub(super) fn display_order(scene: &Scene) -> Vec<usize> {
    // Classic overlay is top-to-bottom; V4 visuals are bottom-to-top.
    let mut order: Vec<_> = scene
        .tracks
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, t)| t.kind != SceneTrackKind::Audio && t.id != scene.main_track_id)
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
            .filter(|(_, t)| t.kind == SceneTrackKind::Audio)
            .map(|(i, _)| i),
    );
    order
}
fn insert_index(
    scene: &Scene,
    kind: SceneTrackKind,
    anchor: &ClipboardTrackInsertion,
) -> Result<usize, EditorError> {
    match anchor {
        ClipboardTrackInsertion::Highest => Ok(if kind == SceneTrackKind::Audio {
            scene
                .tracks
                .iter()
                .position(|t| t.kind == SceneTrackKind::Audio)
                .unwrap_or(scene.tracks.len())
        } else {
            scene
                .tracks
                .iter()
                .rposition(|t| t.kind != SceneTrackKind::Audio)
                .map_or(0, |i| i + 1)
        }),
        ClipboardTrackInsertion::Above { track_id }
        | ClipboardTrackInsertion::Below { track_id } => {
            id(track_id)?;
            let index = scene
                .tracks
                .iter()
                .position(|t| &t.id == track_id)
                .ok_or_else(|| missing("Clipboard insertion anchor is missing"))?;
            let audio = kind == SceneTrackKind::Audio;
            if audio != (scene.tracks[index].kind == SceneTrackKind::Audio) {
                return Err(invalid(
                    "Insertion anchor must belong to the same visual/audio region",
                ));
            }
            let above = matches!(anchor, ClipboardTrackInsertion::Above { .. });
            Ok(index + usize::from(above != audio))
        }
    }
}
fn pin_main(
    scene: &Scene,
    index: usize,
    elements: &mut [TimelineElement],
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
    scene: &mut Scene,
    source_track_id: &str,
    kind: SceneTrackKind,
    target: &ClipboardTrackTarget,
    elements: &mut [TimelineElement],
    decision: &mut PasteTimelineDecision,
) -> Result<String, EditorError> {
    if kind == SceneTrackKind::Caption {
        return Err(invalid(
            "Captions are scene-owned, not element clipboard content",
        ));
    }
    let mut create = None;
    let existing = match target {
        ClipboardTrackTarget::Existing { track_id } => {
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
        ClipboardTrackTarget::New {
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
        ClipboardTrackTarget::ClassicAboveSource { new_track_id, name } => {
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
                    insert_index(scene, kind, &ClipboardTrackInsertion::Highest)?,
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
        SceneTrack {
            id: track_id.clone(),
            name,
            kind,
            enabled: true,
            muted: false,
            locked: false,
            opacity: 1.0,
            gain: Gain::Decibels { value: 0.0 },
            elements: elements.to_vec(),
            channels: vec![],
        },
    );
    decision.new_track_ids.push(track_id.clone());
    Ok(track_id)
}
