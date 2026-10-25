//! Read projections share V4 revision authorization and exact rational time.
use crate::scene::{self, SceneRead};
use opencut_contracts::{
    EditorError, TimeRange, agent_read::*, project_inspection::InspectProject, scene::*,
};
use opencut_time::RationalTime;
use serde::Serialize;
use std::collections::BTreeSet;

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

pub fn artifact(
    store: &impl crate::queries::JobRead,
    request: &opencut_api_contracts::agent_read::GetArtifact,
) -> Result<opencut_api_contracts::agent_read::ArtifactAccess, EditorError> {
    let (job_id, _) = crate::job_views::parse_artifact_id(&request.artifact_id)?;
    let view = crate::queries::job_view(store, job_id)?;
    let artifact = view
        .artifacts
        .into_iter()
        .find(|a| a.artifact_id == request.artifact_id)
        .ok_or_else(|| {
            EditorError::new(
                "NOT_FOUND",
                "Artifact is not declared by the authorized task result",
            )
        })?;
    let id = crate::resources::component(&artifact.artifact_id);
    bounded(opencut_api_contracts::agent_read::ArtifactAccess {
        artifact,
        resource_uri: format!("opencut://artifacts/{id}"),
        http_path: format!("/api/artifacts/{id}"),
    })
}

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn page(total: usize, offset: u32, limit: u16) -> Result<(usize, usize, Option<u32>), EditorError> {
    if !(1..=100).contains(&limit) || offset as usize > total {
        return Err(invalid(
            "Read limit must be 1..100 and offset must be within the result",
        ));
    }
    let start = offset as usize;
    let end = start.saturating_add(limit as usize).min(total);
    let next = if end < total {
        Some(u32::try_from(end).map_err(|_| invalid("Read offset exceeds the supported range"))?)
    } else {
        None
    };
    Ok((start, end, next))
}
pub(crate) fn bounded<T: Serialize>(value: T) -> Result<T, EditorError> {
    let bytes = serde_json::to_vec(&value).map_err(|_| {
        EditorError::new("INTERNAL_ERROR", "Could not serialize V4 read projection")
    })?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(EditorError::new(
            "RESOURCE_TOO_LARGE",
            "V4 read page exceeds 2 MiB; request fewer objects",
        ));
    }
    Ok(value)
}
fn end(range: &TimeRange) -> Result<RationalTime, EditorError> {
    RationalTime::try_from(&range.start)?.checked_add(RationalTime::try_from(&range.duration)?)
}
fn overlaps(range: &TimeRange, lo: RationalTime, hi: RationalTime) -> Result<bool, EditorError> {
    Ok(RationalTime::try_from(&range.start)?.cmp_exact(hi).is_lt()
        && end(range)?.cmp_exact(lo).is_gt())
}
pub fn summary(
    store: &impl SceneRead,
    request: &ProjectSummaryRequest,
) -> Result<ProjectSummaryPage, EditorError> {
    if !(1..=100).contains(&request.limit) {
        return Err(invalid("Read limit must be 1..100"));
    }
    if request.revision.is_none() && request.sequence_offset != 0 {
        return Err(invalid(
            "Continuation requires the fixed revision returned by the first summary page",
        ));
    }
    // The read port enforces current project authorization even for historical revisions.
    let snapshot = scene::inspect(
        store,
        &InspectProject {
            project_id: request.project_id.clone(),
            revision: request.revision.clone(),
        },
    )?;
    let project = snapshot.project;
    let (start, stop, next) = page(
        project.sequences.len(),
        request.sequence_offset,
        request.limit,
    )?;
    let mut sequences = Vec::new();
    for sequence in &project.sequences[start..stop] {
        let duration = crate::project_library::authored_duration(sequence)?;
        sequences.push(SequenceSummaryPage {
            sequence_id: sequence.id.clone(),
            name: sequence.name.clone(),
            main_track_id: sequence.main_track_id.clone(),
            authored_duration: duration,
            track_count: sequence.tracks.len(),
            element_count: sequence.tracks.iter().map(|t| t.elements.len()).sum(),
            caption_count: sequence.captions.len(),
            transition_count: sequence.transitions.len(),
            bookmark_count: sequence.bookmarks.len(),
        });
    }
    bounded(ProjectSummaryPage {
        project_id: project.id,
        revision: snapshot.revision,
        name: project.name,
        default_sequence_id: project.default_sequence_id,
        settings: project.settings,
        asset_count: project.assets.len(),
        font_count: project.fonts.len(),
        sequence_count: project.sequences.len(),
        sequences,
        next_sequence_offset: next,
    })
}

pub fn timeline(
    store: &impl SceneRead,
    request: &TimelineQueryRequest,
) -> Result<TimelineQuery, EditorError> {
    if !(1..=100).contains(&request.limit) {
        return Err(invalid("Read limit must be 1..100"));
    }
    if request.revision.trim().is_empty() {
        return Err(invalid("Timeline query requires an immutable revision"));
    }
    opencut_time::validate_range(&request.range, true)?;
    let lo = RationalTime::try_from(&request.range.start)?;
    let hi = end(&request.range)?;
    if lo.ticks() < 0 {
        return Err(invalid("Timeline query start must be nonnegative"));
    }
    let snapshot = scene::inspect(
        store,
        &InspectProject {
            project_id: request.project_id.clone(),
            revision: Some(request.revision.clone()),
        },
    )?;
    let sequence = snapshot
        .project
        .sequences
        .iter()
        .find(|s| s.id == request.sequence_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "V4 sequence does not exist"))?;
    let requested_tracks: BTreeSet<&str> = request.track_ids.iter().map(String::as_str).collect();
    if requested_tracks.len() != request.track_ids.len()
        || requested_tracks
            .iter()
            .any(|id| !sequence.tracks.iter().any(|t| t.id == *id))
    {
        return Err(invalid(
            "Track filters must contain unique existing sequence tracks",
        ));
    }
    let selected = |id: &str| requested_tracks.is_empty() || requested_tracks.contains(id);
    let mut matched = Vec::new();
    // Borrow matching objects first. Clone only the requested page.
    enum Item<'a> {
        Element(&'a str, &'a TimelineElement),
        Caption(&'a Caption),
        Bookmark(&'a opencut_contracts::scene_bookmarks::SceneBookmark),
    }
    for track in &sequence.tracks {
        if !selected(&track.id) {
            continue;
        }
        for element in &track.elements {
            if overlaps(&element.timeline_range, lo, hi)? {
                matched.push(Item::Element(&track.id, element));
            }
        }
    }
    for caption in &sequence.captions {
        if selected(&caption.track_id) && overlaps(&caption.timeline_range, lo, hi)? {
            matched.push(Item::Caption(caption));
        }
    }
    if request.include_annotations {
        for bookmark in &sequence.bookmarks {
            let at = RationalTime::try_from(&bookmark.time)?;
            let included = if let Some(duration) = &bookmark.duration {
                if RationalTime::try_from(duration)?.ticks() > 0 {
                    overlaps(
                        &TimeRange {
                            start: bookmark.time.clone(),
                            duration: duration.clone(),
                        },
                        lo,
                        hi,
                    )?
                } else {
                    !at.cmp_exact(lo).is_lt() && at.cmp_exact(hi).is_lt()
                }
            } else {
                !at.cmp_exact(lo).is_lt() && at.cmp_exact(hi).is_lt()
            };
            if included {
                matched.push(Item::Bookmark(bookmark));
            }
        }
    }
    let (start, stop, next) = page(matched.len(), request.offset, request.limit)?;
    let mut track_ids = BTreeSet::new();
    let mut element_ids = BTreeSet::new();
    let mut asset_ids = BTreeSet::new();
    let mut items = Vec::new();
    for item in &matched[start..stop] {
        items.push(match item {
            Item::Element(track_id, element) => {
                track_ids.insert(*track_id);
                element_ids.insert(element.id.as_str());
                match &element.content {
                    ElementContent::Video { asset_id, .. }
                    | ElementContent::Audio { asset_id, .. }
                    | ElementContent::Image { asset_id }
                    | ElementContent::Sticker { asset_id, .. } => {
                        asset_ids.insert(asset_id.as_str());
                    }
                    _ => {}
                }
                TimelineItem::Element {
                    track_id: (*track_id).to_owned(),
                    element: Box::new((*element).clone()),
                }
            }
            Item::Caption(caption) => {
                track_ids.insert(caption.track_id.as_str());
                TimelineItem::Caption {
                    caption: Box::new((*caption).clone()),
                }
            }
            Item::Bookmark(bookmark) => TimelineItem::Bookmark {
                bookmark: (*bookmark).clone(),
            },
        });
    }
    bounded(TimelineQuery {
        project_id: snapshot.project.id.clone(),
        revision: snapshot.revision,
        sequence_id: sequence.id.clone(),
        range: request.range.clone(),
        matching_items: matched.len(),
        items,
        next_offset: next,
        tracks: sequence
            .tracks
            .iter()
            .filter(|t| track_ids.contains(t.id.as_str()))
            .map(|t| TimelineTrack {
                id: t.id.clone(),
                name: t.name.clone(),
                kind: t.kind,
                enabled: t.enabled,
                muted: t.muted,
                locked: t.locked,
                opacity: t.opacity,
                gain: t.gain.clone(),
                channels: t.channels.clone(),
            })
            .collect(),
        related_transitions: sequence
            .transitions
            .iter()
            .filter(|t| {
                element_ids.contains(t.outgoing_element_id.as_str())
                    || element_ids.contains(t.incoming_element_id.as_str())
            })
            .cloned()
            .collect(),
        asset_versions: snapshot
            .project
            .assets
            .iter()
            .filter(|a| asset_ids.contains(a.id.as_str()))
            .map(|a| AgentAssetVersion {
                asset_id: a.id.clone(),
                kind: a.kind.clone(),
                version: a.version.clone(),
            })
            .collect(),
    })
}
