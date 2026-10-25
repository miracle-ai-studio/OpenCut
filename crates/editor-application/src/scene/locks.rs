//! Locks protect existing content even when addressed through a parent replace.
use super::*;
pub(super) fn sequence<'a>(
    project: &'a EditorDocument,
    id: &str,
) -> Result<&'a Scene, EditorError> {
    project
        .sequences
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| missing("sequence"))
}
pub(super) fn track<'a>(sequence: &'a Scene, id: &str) -> Result<&'a SceneTrack, EditorError> {
    sequence
        .tracks
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| missing("track"))
}
fn unlocked(track: &SceneTrack) -> Result<(), EditorError> {
    if track.locked {
        Err(EditorError::new(
            "CONSTRAINT_VIOLATION",
            format!("Track {} is locked", track.id),
        ))
    } else {
        Ok(())
    }
}
fn target(sequence: &Scene, element: &str) -> Result<(), EditorError> {
    unlocked(
        sequence
            .tracks
            .iter()
            .find(|t| t.elements.iter().any(|e| e.id == element))
            .ok_or_else(|| missing("element"))?,
    )
}
fn transition(sequence: &Scene, value: &SceneTransition) -> Result<(), EditorError> {
    target(sequence, &value.outgoing_element_id)?;
    target(sequence, &value.incoming_element_id)
}
fn font_ref(binding: &FontBinding, id: &str) -> bool {
    matches!(binding, FontBinding::Locked { font_id } if font_id == id)
}
fn locked_resource(project: &EditorDocument, id: &str, font: bool) -> Result<(), EditorError> {
    for sequence in &project.sequences {
        for track in &sequence.tracks {
            if !track.locked {
                continue;
            }
            for element in &track.elements {
                let refers = if font {
                    matches!(&element.content, ElementContent::Text { text } if font_ref(&text.font, id))
                        || element.visual.as_ref().is_some_and(|v| v.masks.iter().any(|m|
                            matches!(&m.shape, LayerMaskShape::Text { mask } if font_ref(&mask.font, id))))
                } else {
                    matches!(&element.content, ElementContent::Video { asset_id, .. }
                        | ElementContent::Audio { asset_id, .. } | ElementContent::Image { asset_id }
                        | ElementContent::Sticker { asset_id, .. } if asset_id == id)
                };
                if refers {
                    unlocked(track)?;
                }
            }
            if sequence.captions.iter().any(|c| {
                c.track_id == track.id
                    && if font {
                        font_ref(&c.style.font, id)
                    } else {
                        c.source_anchor.as_ref().is_some_and(|a| a.asset_id == id)
                    }
            }) {
                unlocked(track)?;
            }
        }
    }
    Ok(())
}
pub(super) fn check(
    project: &EditorDocument,
    operation: &ProjectOperation,
) -> Result<(), EditorError> {
    use ProjectOperation::*;
    match operation {
        Rename { .. } | SettingsSet { .. } | DefaultSequenceSet { .. } | Bookmark { .. } => Ok(()),
        AssetPut { asset } => {
            if project
                .assets
                .iter()
                .any(|a| a.id == asset.id && a != asset)
            {
                locked_resource(project, &asset.id, false)?;
            }
            Ok(())
        }
        AssetDelete { asset_id } => locked_resource(project, asset_id, false),
        FontPut { font } => {
            if project.fonts.iter().any(|f| f.id == font.id && f != font) {
                locked_resource(project, &font.id, true)?;
            }
            Ok(())
        }
        FontDelete { font_id } => locked_resource(project, font_id, true),
        SequencePut { sequence: proposed } => {
            if let Some(old) = project.sequences.iter().find(|s| s.id == proposed.id) {
                // Inserting or deleting unlocked tracks can change indices
                // without moving surviving content. Keep the original relative
                // order of surviving tracks; locked tracks must still exist
                // unchanged by ID. Otherwise retain positional protection.
                let preserves_existing_order = proposed
                    .tracks
                    .iter()
                    .filter(|t| old.tracks.iter().any(|before| before.id == t.id))
                    .map(|t| &t.id)
                    .eq(old
                        .tracks
                        .iter()
                        .filter(|t| proposed.tracks.iter().any(|after| after.id == t.id))
                        .map(|t| &t.id));
                for (index, t) in old.tracks.iter().enumerate().filter(|(_, t)| t.locked) {
                    let counterpart = if preserves_existing_order {
                        proposed.tracks.iter().find(|after| after.id == t.id)
                    } else {
                        proposed.tracks.get(index)
                    };
                    if counterpart != Some(t)
                        || old
                            .captions
                            .iter()
                            .filter(|c| c.track_id == t.id)
                            .collect::<Vec<_>>()
                            != proposed
                                .captions
                                .iter()
                                .filter(|c| c.track_id == t.id)
                                .collect::<Vec<_>>()
                        || old
                            .transitions
                            .iter()
                            .filter(|cut| {
                                t.elements.iter().any(|e| {
                                    e.id == cut.outgoing_element_id
                                        || e.id == cut.incoming_element_id
                                })
                            })
                            .collect::<Vec<_>>()
                            != proposed
                                .transitions
                                .iter()
                                .filter(|cut| {
                                    t.elements.iter().any(|e| {
                                        e.id == cut.outgoing_element_id
                                            || e.id == cut.incoming_element_id
                                    })
                                })
                                .collect::<Vec<_>>()
                    {
                        unlocked(t)?;
                    }
                }
            }
            Ok(())
        }
        SequenceDelete { sequence_id } => {
            for t in &sequence(project, sequence_id)?.tracks {
                unlocked(t)?;
            }
            Ok(())
        }
        TrackPut {
            sequence_id,
            track: proposed,
        } => {
            if let Some(old) = sequence(project, sequence_id)?
                .tracks
                .iter()
                .find(|t| t.id == proposed.id)
                && old != proposed
            {
                unlocked(old)?;
            }
            Ok(())
        }
        TrackDelete {
            sequence_id,
            track_id,
        } => unlocked(track(sequence(project, sequence_id)?, track_id)?),
        TrackLockSet {
            sequence_id,
            track_id,
            ..
        } => {
            track(sequence(project, sequence_id)?, track_id)?;
            Ok(())
        }
        ElementPut {
            sequence_id,
            track_id,
            element,
        } => {
            let s = sequence(project, sequence_id)?;
            unlocked(track(s, track_id)?)?;
            if s.tracks
                .iter()
                .any(|t| t.elements.iter().any(|e| e.id == element.id))
            {
                target(s, &element.id)?;
            }
            Ok(())
        }
        ElementDelete {
            sequence_id,
            element_id,
        } => target(sequence(project, sequence_id)?, element_id),
        CaptionPut {
            sequence_id,
            caption,
        } => {
            let s = sequence(project, sequence_id)?;
            unlocked(track(s, &caption.track_id)?)?;
            if let Some(old) = s.captions.iter().find(|c| c.id == caption.id) {
                unlocked(track(s, &old.track_id)?)?;
            }
            Ok(())
        }
        CaptionDelete {
            sequence_id,
            caption_id,
        } => {
            let s = sequence(project, sequence_id)?;
            let c = s
                .captions
                .iter()
                .find(|c| c.id == *caption_id)
                .ok_or_else(|| missing("caption"))?;
            unlocked(track(s, &c.track_id)?)
        }
        TransitionPut {
            sequence_id,
            transition: value,
        } => {
            let s = sequence(project, sequence_id)?;
            if let Some(old) = s.transitions.iter().find(|t| t.id == value.id) {
                transition(s, old)?;
            }
            transition(s, value)
        }
        TransitionDelete {
            sequence_id,
            transition_id,
        } => {
            let s = sequence(project, sequence_id)?;
            transition(
                s,
                s.transitions
                    .iter()
                    .find(|t| t.id == *transition_id)
                    .ok_or_else(|| missing("transition"))?,
            )
        }
    }
}
