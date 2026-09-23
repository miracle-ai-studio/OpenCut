//! Backend-independent sequence dependency selection used by acceptance and rendering.
use opencut_contracts::{EditorError, ProjectSnapshot, TrackKind};

#[derive(Clone, Copy, Debug)]
pub enum AssetScope {
    All,
    Visual,
}

/// IDs are unique and sorted. Disabled tracks never contribute; muted audio
/// tracks do not contribute. Muting a video track suppresses its audio only,
/// so its visual source remains required. Captions use separate font dependencies.
pub fn required_asset_ids(
    snapshot: &ProjectSnapshot,
    sequence_id: &str,
    scope: AssetScope,
) -> Result<Vec<String>, EditorError> {
    let sequence = snapshot
        .project
        .sequences
        .iter()
        .find(|s| s.id == sequence_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "sequence does not exist"))?;
    let mut ids = std::collections::BTreeSet::new();
    for track in sequence.tracks.iter().filter(|track| track.enabled) {
        let uses_track = match track.kind {
            TrackKind::Video | TrackKind::Graphic => true,
            TrackKind::Audio => matches!(scope, AssetScope::All) && !track.muted,
            _ => false,
        };
        if uses_track {
            ids.extend(track.clips.iter().filter_map(|clip| clip.asset_id.clone()));
        }
    }
    Ok(ids.into_iter().collect())
}
