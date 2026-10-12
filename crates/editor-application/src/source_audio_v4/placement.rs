use super::*;
use opencut_time::{RationalTime, range_end};

fn fits(track: &TrackV4, source: &TimelineElementV4) -> Result<bool, EditorError> {
    let start = RationalTime::try_from(&source.timeline_range.start)?;
    let end = RationalTime::try_from(&range_end(&source.timeline_range)?)?;
    for element in &track.elements {
        let other_start = RationalTime::try_from(&element.timeline_range.start)?;
        let other_end = RationalTime::try_from(&range_end(&element.timeline_range)?)?;
        if start.cmp_exact(other_end).is_lt() && end.cmp_exact(other_start).is_gt() {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn target(
    sequence: &SequenceV4,
    source: &TimelineElementV4,
    destination: &SourceAudioDestinationV4,
) -> Result<(String, Option<TrackV4>), EditorError> {
    match destination {
        SourceAudioDestinationV4::Existing { track_id } => {
            let track = sequence
                .tracks
                .iter()
                .find(|t| t.id == *track_id)
                .ok_or_else(|| missing("Requested audio track does not exist"))?;
            if track.kind != TrackKindV4::Audio {
                return Err(invalid("Separation target must be an audio track"));
            }
            if track.locked {
                return Err(EditorError::new(
                    "CONSTRAINT_VIOLATION",
                    "Requested audio track is locked",
                ));
            }
            if !fits(track, source)? {
                return Err(EditorError::new(
                    "CONSTRAINT_VIOLATION",
                    "Requested audio track has an overlapping element",
                ));
            }
            Ok((track.id.clone(), None))
        }
        SourceAudioDestinationV4::FirstAvailable { fallback_track_id } => {
            for track in sequence
                .tracks
                .iter()
                .filter(|t| t.kind == TrackKindV4::Audio && !t.locked)
            {
                if fits(track, source)? {
                    return Ok((track.id.clone(), None));
                }
            }
            Ok((
                fallback_track_id.clone(),
                Some(TrackV4 {
                    id: fallback_track_id.clone(),
                    name: "Audio track".into(),
                    kind: TrackKindV4::Audio,
                    enabled: true,
                    muted: false,
                    locked: false,
                    opacity: 1.0,
                    gain: GainV4::Decibels { value: 0.0 },
                    elements: vec![],
                    channels: vec![],
                }),
            ))
        }
    }
}
