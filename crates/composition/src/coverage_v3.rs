//! Independent source-to-IR coverage checks for schema-3 visual nodes.
//! This is not a pixel oracle or coverage of audio/caption/keyframe semantics.
use crate::project_v3::{CompositionV3, VisualTargetKind};
use crate::{CompositionError, CompositionScope};
use opencut_contracts::{TrackKind, project_v3::ProjectSnapshotV3};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VisualDisposition {
    Compiled { index: usize },
    DisabledTrack,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisualCoverageEntry {
    pub kind: &'static str,
    pub source_id: String,
    pub disposition: VisualDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisualCompileReceipt {
    pub revision: String,
    pub sequence_id: String,
    pub scope: CompositionScope,
    pub entries: Vec<VisualCoverageEntry>,
}

fn mismatch(message: impl Into<String>) -> CompositionError {
    CompositionError {
        code: "COMPOSITION_COVERAGE_MISMATCH",
        message: message.into(),
    }
}

/// Verify an already compiled graph against its source, without compiling it again.
/// Missing, duplicated, injected, reordered or modified effect/mask/transition nodes
/// are errors. Disabled targets are accounted for explicitly, never silently lost.
pub fn audit_visual_coverage(
    snapshot: &ProjectSnapshotV3,
    compiled: &CompositionV3,
) -> Result<VisualCompileReceipt, CompositionError> {
    if snapshot.revision != compiled.base.revision {
        return Err(mismatch("Composition revision differs from source"));
    }
    let sequence = snapshot
        .project
        .base
        .sequences
        .iter()
        .find(|s| s.id == compiled.base.sequence_id)
        .ok_or_else(|| mismatch("Composition sequence differs from source"))?;
    let mut active = BTreeMap::new();
    let mut known = BTreeSet::new();
    let mut entries = Vec::new();
    let mut expected_targets = Vec::new();
    let mut videos = 0;
    let mut images = 0;
    for (z, track) in sequence.tracks.iter().enumerate() {
        if !matches!(track.kind, TrackKind::Video | TrackKind::Graphic) {
            continue;
        }
        for (order, clip) in track.clips.iter().enumerate() {
            let image_asset = snapshot.project.base.assets.iter().any(|asset| {
                Some(asset.id.as_str()) == clip.asset_id.as_deref()
                    && asset.kind == opencut_contracts::AssetKind::Image
            });
            let kind = if image_asset {
                VisualTargetKind::ImageClip
            } else {
                VisualTargetKind::VideoClip
            };
            known.insert(clip.id.as_str());
            if !track.enabled {
                entries.push(entry("clip", &clip.id, VisualDisposition::DisabledTrack));
                continue;
            }
            let index = if kind == VisualTargetKind::VideoClip {
                let index = videos;
                videos += 1;
                let media = compiled
                    .base
                    .timeline
                    .video_clips
                    .get(index)
                    .ok_or_else(|| mismatch(format!("Missing video clip {}", clip.id)))?;
                verify_source(clip.asset_id.as_deref(), &media.input.asset_id, &clip.id)?;
                index
            } else {
                let index = images;
                images += 1;
                let media = compiled
                    .base
                    .timeline
                    .image_clips
                    .get(index)
                    .ok_or_else(|| mismatch(format!("Missing image clip {}", clip.id)))?;
                verify_source(clip.asset_id.as_deref(), &media.input.asset_id, &clip.id)?;
                index
            };
            active.insert(clip.id.as_str(), true);
            expected_targets.push((clip.id.as_str(), kind, index, z, order));
            entries.push(entry(
                "clip",
                &clip.id,
                VisualDisposition::Compiled { index },
            ));
        }
    }
    let visual = snapshot
        .project
        .visual
        .iter()
        .find(|v| v.sequence_id == sequence.id);
    let mut shape_index = 0;
    if let Some(visual) = visual {
        for shape in &visual.shapes {
            known.insert(shape.id.as_str());
            let (z, track) = sequence
                .tracks
                .iter()
                .enumerate()
                .find(|(_, t)| t.id == shape.track_id)
                .ok_or_else(|| mismatch(format!("Unknown shape track {}", shape.track_id)))?;
            if !track.enabled {
                entries.push(entry("shape", &shape.id, VisualDisposition::DisabledTrack));
                continue;
            }
            let actual = compiled
                .visual
                .shapes
                .get(shape_index)
                .ok_or_else(|| mismatch(format!("Missing shape {}", shape.id)))?;
            if actual.id != shape.id
                || actual.track_id != shape.track_id
                || actual.start != crate::validation::time(&shape.timeline_range.start)?
                || actual.duration != crate::validation::time(&shape.timeline_range.duration)?
                || actual.primitive != shape.primitive
                || actual.fill != shape.fill
                || actual.position_x != shape.position_x
                || actual.position_y != shape.position_y
                || actual.rotation_degrees != shape.rotation_degrees
                || actual.opacity != shape.opacity
            {
                return Err(mismatch(format!(
                    "Shape {} changed during compilation",
                    shape.id
                )));
            }
            active.insert(shape.id.as_str(), true);
            expected_targets.push((
                shape.id.as_str(),
                VisualTargetKind::Shape,
                shape_index,
                z,
                track.clips.len() + shape_index,
            ));
            entries.push(entry(
                "shape",
                &shape.id,
                VisualDisposition::Compiled { index: shape_index },
            ));
            shape_index += 1;
        }
        macro_rules! audit_nodes {
            ($field:ident, $kind:literal, |$node:ident| $targets:expr) => {{
                let mut index = 0;
                for $node in &visual.$field {
                    let node = $node;
                    let targets: Vec<&str> = $targets;
                    if targets.iter().any(|id| !known.contains(id)) {
                        return Err(mismatch(format!("Unknown target on {} {}", $kind, node.id)));
                    }
                    let disposition = if targets.iter().all(|id| active.contains_key(id)) {
                        if compiled.visual.$field.get(index) != Some(node) {
                            return Err(mismatch(format!(
                                "Missing or changed {} {}",
                                $kind, node.id
                            )));
                        }
                        let result = VisualDisposition::Compiled { index };
                        index += 1;
                        result
                    } else {
                        VisualDisposition::DisabledTrack
                    };
                    entries.push(entry($kind, &node.id, disposition));
                }
                if compiled.visual.$field.len() != index {
                    return Err(mismatch(concat!(
                        "Unexpected or duplicated ",
                        $kind,
                        " nodes"
                    )));
                }
            }};
        }
        audit_nodes!(effects, "effect", |node| vec![node.target_id.as_str()]);
        audit_nodes!(masks, "mask", |node| vec![node.target_id.as_str()]);
        audit_nodes!(transitions, "transition", |node| vec![
            node.outgoing_clip_id.as_str(),
            node.incoming_clip_id.as_str()
        ]);
    } else if !compiled.visual.effects.is_empty()
        || !compiled.visual.masks.is_empty()
        || !compiled.visual.transitions.is_empty()
    {
        return Err(mismatch("Visual nodes injected into an empty source graph"));
    }
    if videos != compiled.base.timeline.video_clips.len()
        || images != compiled.base.timeline.image_clips.len()
        || shape_index != compiled.visual.shapes.len()
        || expected_targets.len() != compiled.visual.targets.len()
    {
        return Err(mismatch("Visual target inventory differs from source"));
    }
    for ((id, kind, index, z, order), actual) in
        expected_targets.iter().zip(&compiled.visual.targets)
    {
        if actual.target_id != *id
            || actual.kind != *kind
            || actual.index != *index
            || usize::try_from(actual.z_index).ok() != Some(*z)
            || usize::try_from(actual.layer_order).ok() != Some(*order)
        {
            return Err(mismatch(format!("Visual binding differs for {id}")));
        }
    }
    let mut extent = None;
    if let Some(visual) = visual {
        for shape in &visual.shapes {
            if sequence
                .tracks
                .iter()
                .any(|t| t.id == shape.track_id && t.enabled)
            {
                let end = opencut_time::RationalTime::try_from(
                    &opencut_time::range_end(&shape.timeline_range)
                        .map_err(|e| mismatch(e.message))?,
                )
                .map_err(|e| mismatch(e.message))?;
                if extent.is_none_or(|v| end.cmp_exact(v).is_gt()) {
                    extent = Some(end);
                }
            }
        }
    }
    crate::semantic_coverage::audit_duration(
        &opencut_contracts::ProjectSnapshot {
            project: snapshot.project.base.clone(),
            revision: snapshot.revision.clone(),
        },
        &compiled.base,
        extent,
    )?;
    Ok(VisualCompileReceipt {
        revision: snapshot.revision.clone(),
        sequence_id: sequence.id.clone(),
        scope: compiled.base.scope,
        entries,
    })
}

fn entry(kind: &'static str, id: &str, disposition: VisualDisposition) -> VisualCoverageEntry {
    VisualCoverageEntry {
        kind,
        source_id: id.into(),
        disposition,
    }
}

fn verify_source(expected: Option<&str>, actual: &str, clip: &str) -> Result<(), CompositionError> {
    if expected != Some(actual) {
        return Err(mismatch(format!("Source binding differs for clip {clip}")));
    }
    Ok(())
}
