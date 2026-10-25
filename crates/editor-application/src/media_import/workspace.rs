//! Explicit track creation and reuse of revision-owned assets. Placement and
//! settings calculations remain shared with the frozen import planner.
use super::*;

pub(super) fn validate_tracks(input: &MediaImportRequest) -> Result<(), EditorError> {
    if input.new_tracks.len() > 60 {
        return Err(invalid("Workspace accepts at most 60 new tracks"));
    }
    let mut ids = BTreeSet::new();
    for track in &input.new_tracks {
        if track.id.trim().is_empty()
            || track.name.trim().is_empty()
            || !ids.insert(&track.id)
            || !matches!(
                track.kind,
                SceneTrackKind::Video | SceneTrackKind::Audio | SceneTrackKind::Graphic
            )
            || !input.placements.iter().any(|p| p.track_id == track.id)
        {
            return Err(invalid(
                "New tracks need unique IDs, names, a supported kind and a placement",
            ));
        }
    }
    Ok(())
}

pub(super) fn new_track(input: &MediaTrack) -> SceneTrack {
    SceneTrack {
        id: input.id.clone(),
        name: input.name.clone(),
        kind: input.kind,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1.0,
        gain: Gain::Decibels { value: 0.0 },
        elements: Vec::new(),
        channels: Vec::new(),
    }
}
