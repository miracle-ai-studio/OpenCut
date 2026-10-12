//! Explicit track creation and reuse of revision-owned assets. Placement and
//! settings calculations remain shared with the frozen import planner.
use super::*;

pub fn plan_workspace(
    store: &impl ProjectV4PublicResources,
    input: &MediaWorkspaceRequestV4,
) -> Result<MediaImportPlanV4, EditorError> {
    let view = MediaImportRequestV4 {
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        sequence_id: input.sequence_id.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: input.label.clone(),
        bindings: input.bindings.clone(),
        placements: input.placements.clone(),
        ripple: input.ripple,
    };
    validate(&view, true)?;
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
                TrackKindV4::Video | TrackKindV4::Audio | TrackKindV4::Graphic
            )
            || !input.placements.iter().any(|p| p.track_id == track.id)
        {
            return Err(invalid(
                "New tracks need unique IDs, names, a supported kind and a placement",
            ));
        }
    }
    plan_shared(
        store,
        &view,
        &input.new_tracks,
        true,
        "project_v4_media_workspace",
        input,
    )
}

pub(super) fn new_track(input: &MediaTrackV4) -> TrackV4 {
    TrackV4 {
        id: input.id.clone(),
        name: input.name.clone(),
        kind: input.kind,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1.0,
        gain: GainV4::Decibels { value: 0.0 },
        elements: Vec::new(),
        channels: Vec::new(),
    }
}
