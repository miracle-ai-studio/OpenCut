//! Evidence joins use authorized immutable revisions and existing analysis results.
use crate::{
    queries::JobRead,
    scene::{self, SceneRead},
};
use opencut_api_contracts::{agent_evidence::*, analysis_evidence::AnalysisEvidenceInput};
use opencut_contracts::{EditorError, project_inspection::InspectProject, scene::*};
use opencut_time::{RationalTime, range_end};
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
pub fn search(
    store: &(impl SceneRead + JobRead),
    input: ContentSearch,
) -> Result<ContentSearchResult, EditorError> {
    if input.revision.trim().is_empty()
        || input.analysis_job_ids.is_empty()
        || input.analysis_job_ids.len() > 32
        || !(1..=100).contains(&input.limit_per_analysis)
    {
        return Err(invalid(
            "Pin a revision and 1..32 analyses with 1..100 items per analysis",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    if input
        .analysis_job_ids
        .iter()
        .any(|id| !seen.insert(id.clone()))
        || input.cursors.keys().any(|id| !seen.contains(id))
    {
        return Err(invalid(
            "Analysis identities must be unique and cursor keys must belong to this search",
        ));
    }
    let fixed = scene::inspect(
        store,
        &InspectProject {
            project_id: input.project_id.clone(),
            revision: Some(input.revision.clone()),
        },
    )?;
    let mut pages = Vec::new();
    for job_id in &input.analysis_job_ids {
        let page = crate::analysis_evidence::query(
            store,
            AnalysisEvidenceInput {
                job_id: job_id.clone(),
                text: input.text.clone(),
                source_range: input.source_range.clone(),
                cursor: input.cursors.get(job_id).cloned(),
                limit: usize::from(input.limit_per_analysis),
            },
        )?;
        if page.project_id != input.project_id
            || !fixed.project.assets.iter().any(|a| {
                a.id == page.asset_id
                    && a.version.content_hash == page.asset_version.content_hash
                    && a.version.size == page.asset_version.size
            })
        {
            return Err(EditorError::new(
                "EVIDENCE_VERSION_MISMATCH",
                "Analysis does not describe the requested project's fixed asset version",
            ));
        }
        pages.push(page);
    }
    let result = ContentSearchResult {
        project_id: input.project_id,
        revision: input.revision,
        pages,
    };
    if serde_json::to_vec(&result)
        .map_err(|_| invalid("Cannot encode evidence"))?
        .len()
        > 2 * 1024 * 1024
    {
        return Err(EditorError::new(
            "RESOURCE_TOO_LARGE",
            "Reduce analysis page sizes",
        ));
    }
    Ok(result)
}
pub fn map(store: &impl SceneRead, input: MapMediaTime) -> Result<MappedMediaTimes, EditorError> {
    if input.revision.trim().is_empty() || input.times.is_empty() || input.times.len() > 256 {
        return Err(invalid("Pin a revision and provide 1..256 points"));
    }
    let fixed = scene::inspect(
        store,
        &InspectProject {
            project_id: input.project_id.clone(),
            revision: Some(input.revision.clone()),
        },
    )?;
    let element = fixed
        .project
        .sequences
        .iter()
        .find(|s| s.id == input.sequence_id)
        .and_then(|s| s.tracks.iter().find(|t| t.id == input.target.track_id))
        .and_then(|t| t.elements.iter().find(|e| e.id == input.target.element_id))
        .ok_or_else(|| invalid("Media placement does not exist"))?;
    let (asset_id, retime) = match &element.content {
        ElementContent::Video {
            asset_id, retime, ..
        }
        | ElementContent::Audio {
            asset_id, retime, ..
        } => (asset_id, retime),
        _ => return Err(invalid("Clock mapping requires timed media")),
    };
    let asset = fixed
        .project
        .assets
        .iter()
        .find(|a| &a.id == asset_id)
        .ok_or_else(|| invalid("Media resource missing"))?;
    let source_range =
        opencut_editor_core::scene::resolve_element_source_request_scene(element, Some(asset))?;
    let source_start = RationalTime::try_from(&source_range.start)?;
    let sequence_start = RationalTime::try_from(&element.timeline_range.start)?;
    let (from, to, inverse) = match input.direction {
        MapDirection::SourceToSequence => (&source_range, &element.timeline_range, true),
        MapDirection::SequenceToSource => (&element.timeline_range, &source_range, false),
    };
    let from_start = RationalTime::try_from(&from.start)?;
    let from_end = RationalTime::try_from(&range_end(from)?)?;
    let to_start = if inverse {
        sequence_start
    } else {
        source_start
    };
    let to_end = RationalTime::try_from(&range_end(to)?)?;
    let mut points = Vec::new();
    for time in &input.times {
        let at = RationalTime::try_from(time)?;
        if at.cmp_exact(from_start).is_lt() || at.cmp_exact(from_end).is_gt() {
            return Err(invalid(
                "Point is outside the placement's closed mapping extent",
            ));
        }
        let local = at.checked_sub(from_start)?;
        let offset = opencut_editor_core::scene::map_retime_duration_scene(local, retime, inverse)?;
        let mapped = to_start.checked_add(offset)?;
        // Quantized inverse may request an extra source tick. Preserve that
        // observation rather than silently clamping or fabricating a round trip.
        if mapped.cmp_exact(to_end).is_gt() {
            return Err(EditorError::new(
                "TIME_MAPPING_OUTSIDE_SPAN",
                "Quantized point exceeds the opposite span; select a supported interior point",
            ));
        }
        let roundtrip = from_start.checked_add(
            opencut_editor_core::scene::map_retime_duration_scene(offset, retime, !inverse)?,
        )?;
        points.push(MappedMediaPoint {
            input: time.clone(),
            mapped: mapped.into(),
            roundtrip: roundtrip.into(),
        });
    }
    Ok(MappedMediaTimes {
        project_id: input.project_id,
        revision: input.revision,
        sequence_id: input.sequence_id,
        target: input.target,
        asset_id: asset_id.clone(),
        asset_version: asset.version.clone(),
        mapping: retime.mapping,
        source_range,
        sequence_range: element.timeline_range.clone(),
        points,
    })
}
