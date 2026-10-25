//! Pure schema 3 aggregate edits. SQLite owns CAS, identity, audit and publication.
use crate::access::SourceAccess;
use opencut_contracts::{
    CreateProject, EditorError, Operation, ProjectDocument, ProjectSnapshot, TransactionRequest,
    historical_visual::{
        EffectNode, HistoricalVisualDocument, HistoricalVisualSequenceVisual, MaskNode, ShapeClip,
        TransitionNode,
    },
    historical_visual_ops::{ApplyHistoricalVisualProject, VisualOperation, VisualPlan},
};
use opencut_time::RationalTime;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

/// A fixed v3 revision paired with source-path authority derived from host identity.
#[derive(Clone, Debug)]
pub struct HistoricalVisualAuthorizedSnapshot {
    pub snapshot: opencut_contracts::historical_visual::HistoricalVisualSnapshot,
    pub sources: SourceAccess,
}

fn invalid(message: impl Into<String>) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}

pub fn create(input: &CreateProject) -> Result<HistoricalVisualDocument, EditorError> {
    let base = opencut_editor_core::create_project(input)?;
    promote(&base)
}

pub fn promote(base: &ProjectDocument) -> Result<HistoricalVisualDocument, EditorError> {
    if base.schema_version != 2 {
        return Err(invalid("Schema 3 promotion requires a schema 2 project"));
    }
    let project = HistoricalVisualDocument {
        schema_version: 3,
        base: base.clone(),
        visual: Vec::new(),
        keyframe_curves: Vec::new(),
    };
    opencut_editor_core::historical_visual::validate_project_historical_visual(&project)?;
    Ok(project)
}

trait Entity {
    fn id(&self) -> &str;
}
impl Entity for ShapeClip {
    fn id(&self) -> &str {
        &self.id
    }
}
impl Entity for EffectNode {
    fn id(&self) -> &str {
        &self.id
    }
}
impl Entity for MaskNode {
    fn id(&self) -> &str {
        &self.id
    }
}
impl Entity for TransitionNode {
    fn id(&self) -> &str {
        &self.id
    }
}

fn put<T: Entity + PartialEq>(
    items: &mut Vec<T>,
    value: T,
    created: &mut Vec<String>,
    changed: &mut Vec<String>,
) {
    if let Some(index) = items.iter().position(|item| item.id() == value.id()) {
        if items[index] != value {
            changed.push(value.id().into());
            items[index] = value;
        }
    } else {
        created.push(value.id().into());
        items.push(value);
    }
}
fn delete<T: Entity>(
    items: &mut Vec<T>,
    id: &str,
    deleted: &mut Vec<String>,
) -> Result<(), EditorError> {
    let index = items
        .iter()
        .position(|item| item.id() == id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", format!("Visual node {id} does not exist")))?;
    items.remove(index);
    deleted.push(id.into());
    Ok(())
}
fn sequence<'a>(
    project: &'a mut HistoricalVisualDocument,
    id: &str,
) -> Result<&'a mut HistoricalVisualSequenceVisual, EditorError> {
    if !project.base.sequences.iter().any(|s| s.id == id) {
        return Err(EditorError::new("NOT_FOUND", "Sequence does not exist"));
    }
    if let Some(index) = project.visual.iter().position(|s| s.sequence_id == id) {
        return Ok(&mut project.visual[index]);
    }
    project.visual.push(HistoricalVisualSequenceVisual {
        sequence_id: id.into(),
        shapes: Vec::new(),
        effects: Vec::new(),
        masks: Vec::new(),
        transitions: Vec::new(),
    });
    Ok(project.visual.last_mut().expect("inserted visual sequence"))
}

fn unlocked_track(
    project: &HistoricalVisualDocument,
    sequence_id: &str,
    track_id: &str,
) -> Result<(), EditorError> {
    let track = project
        .base
        .sequences
        .iter()
        .find(|s| s.id == sequence_id)
        .and_then(|s| s.tracks.iter().find(|t| t.id == track_id))
        .ok_or_else(|| EditorError::new("NOT_FOUND", format!("Track {track_id} does not exist")))?;
    if track.locked {
        return Err(EditorError::new(
            "CONSTRAINT_VIOLATION",
            format!("track {track_id} is locked"),
        ));
    }
    Ok(())
}

fn target_track<'a>(
    project: &'a HistoricalVisualDocument,
    sequence_id: &str,
    target_id: &str,
) -> Result<&'a str, EditorError> {
    let base = project
        .base
        .sequences
        .iter()
        .find(|s| s.id == sequence_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Sequence does not exist"))?;
    if let Some(track) = base
        .tracks
        .iter()
        .find(|t| t.clips.iter().any(|c| c.id == target_id))
    {
        return Ok(&track.id);
    }
    project
        .visual
        .iter()
        .find(|s| s.sequence_id == sequence_id)
        .and_then(|s| s.shapes.iter().find(|shape| shape.id == target_id))
        .map(|shape| shape.track_id.as_str())
        .ok_or_else(|| EditorError::new("NOT_FOUND", format!("Target {target_id} does not exist")))
}

fn unlocked_target(
    project: &HistoricalVisualDocument,
    sequence_id: &str,
    target_id: &str,
) -> Result<(), EditorError> {
    unlocked_track(
        project,
        sequence_id,
        target_track(project, sequence_id, target_id)?,
    )
}

fn check_visual_lock(
    project: &HistoricalVisualDocument,
    operation: &VisualOperation,
) -> Result<(), EditorError> {
    let Some(sequence_id) = operation.sequence_id() else {
        return Ok(());
    };
    let visual = project.visual.iter().find(|s| s.sequence_id == sequence_id);
    match operation {
        VisualOperation::ShapePut { shape, .. } => {
            if let Some(old) = visual.and_then(|s| s.shapes.iter().find(|v| v.id == shape.id)) {
                unlocked_track(project, sequence_id, &old.track_id)?;
            }
            unlocked_track(project, sequence_id, &shape.track_id)
        }
        VisualOperation::ShapeDelete { id, .. } => {
            let old = visual
                .and_then(|s| s.shapes.iter().find(|v| v.id == *id))
                .ok_or_else(|| {
                    EditorError::new("NOT_FOUND", format!("Visual node {id} does not exist"))
                })?;
            unlocked_track(project, sequence_id, &old.track_id)
        }
        VisualOperation::EffectPut { effect, .. } => {
            if let Some(old) = visual.and_then(|s| s.effects.iter().find(|v| v.id == effect.id)) {
                unlocked_target(project, sequence_id, &old.target_id)?;
            }
            unlocked_target(project, sequence_id, &effect.target_id)
        }
        VisualOperation::EffectDelete { id, .. } => {
            let old = visual
                .and_then(|s| s.effects.iter().find(|v| v.id == *id))
                .ok_or_else(|| {
                    EditorError::new("NOT_FOUND", format!("Visual node {id} does not exist"))
                })?;
            unlocked_target(project, sequence_id, &old.target_id)
        }
        VisualOperation::MaskPut { mask, .. } => {
            if let Some(old) = visual.and_then(|s| s.masks.iter().find(|v| v.id == mask.id)) {
                unlocked_target(project, sequence_id, &old.target_id)?;
            }
            unlocked_target(project, sequence_id, &mask.target_id)
        }
        VisualOperation::MaskDelete { id, .. } => {
            let old = visual
                .and_then(|s| s.masks.iter().find(|v| v.id == *id))
                .ok_or_else(|| {
                    EditorError::new("NOT_FOUND", format!("Visual node {id} does not exist"))
                })?;
            unlocked_target(project, sequence_id, &old.target_id)
        }
        VisualOperation::TransitionPut { transition, .. } => {
            if let Some(old) =
                visual.and_then(|s| s.transitions.iter().find(|v| v.id == transition.id))
            {
                unlocked_target(project, sequence_id, &old.outgoing_clip_id)?;
                unlocked_target(project, sequence_id, &old.incoming_clip_id)?;
            }
            unlocked_target(project, sequence_id, &transition.outgoing_clip_id)?;
            unlocked_target(project, sequence_id, &transition.incoming_clip_id)
        }
        VisualOperation::TransitionDelete { id, .. } => {
            let old = visual
                .and_then(|s| s.transitions.iter().find(|v| v.id == *id))
                .ok_or_else(|| {
                    EditorError::new("NOT_FOUND", format!("Visual node {id} does not exist"))
                })?;
            unlocked_target(project, sequence_id, &old.outgoing_clip_id)?;
            unlocked_target(project, sequence_id, &old.incoming_clip_id)
        }
        VisualOperation::KeyframeCurveSet { curve, .. } => {
            if let Some(old) = project
                .keyframe_curves
                .iter()
                .find(|existing| existing.id == curve.id)
            {
                unlocked_target(project, sequence_id, &old.clip_id)?;
            }
            unlocked_target(project, sequence_id, &curve.clip_id)
        }
        VisualOperation::KeyframeCurveDelete { id, .. } => {
            let curve = project
                .keyframe_curves
                .iter()
                .find(|curve| curve.id == *id)
                .ok_or_else(|| EditorError::new("NOT_FOUND", "Keyframe curve does not exist"))?;
            unlocked_target(project, sequence_id, &curve.clip_id)
        }
        VisualOperation::Base { .. } => Ok(()),
    }
}

fn entity_value(project: &HistoricalVisualDocument, id: &str) -> Option<serde_json::Value> {
    if project.base.id == id {
        return serde_json::to_value(&project.base).ok();
    }
    for asset in &project.base.assets {
        if asset.id == id {
            return serde_json::to_value(asset).ok();
        }
    }
    for sequence in &project.base.sequences {
        if sequence.id == id {
            return serde_json::to_value(sequence).ok();
        }
        for cue in &sequence.caption_cues {
            if cue.id == id {
                return serde_json::to_value(cue).ok();
            }
        }
        for track in &sequence.tracks {
            if track.id == id {
                return serde_json::to_value(track).ok();
            }
            for clip in &track.clips {
                if clip.id == id {
                    return serde_json::to_value(clip).ok();
                }
            }
        }
    }
    if let Some(curve) = project.keyframe_curves.iter().find(|curve| curve.id == id) {
        return serde_json::to_value(curve).ok();
    }
    for visual in &project.visual {
        for shape in &visual.shapes {
            if shape.id == id {
                return serde_json::to_value(shape).ok();
            }
        }
        for effect in &visual.effects {
            if effect.id == id {
                return serde_json::to_value(effect).ok();
            }
        }
        for mask in &visual.masks {
            if mask.id == id {
                return serde_json::to_value(mask).ok();
            }
        }
        for transition in &visual.transitions {
            if transition.id == id {
                return serde_json::to_value(transition).ok();
            }
        }
    }
    None
}

fn split_curve_id(source_curve_id: &str, right_clip_id: &str) -> String {
    let identity =
        serde_json::to_vec(&(source_curve_id, right_clip_id)).expect("curve identity serializes");
    format!("curve_{:x}", Sha256::digest(identity))
}

fn bezier_component(t: f64, first: f64, second: f64) -> f64 {
    let inverse = 1.0 - t;
    3.0 * inverse * inverse * t * first + 3.0 * inverse * t * t * second + t * t * t
}

fn split_bezier(
    curve: opencut_contracts::historical_visual::HistoricalVisualCubicBezier,
    progress: f64,
) -> Result<
    (
        opencut_contracts::historical_visual::HistoricalVisualCubicBezier,
        opencut_contracts::historical_visual::HistoricalVisualCubicBezier,
        f64,
    ),
    EditorError,
> {
    if !progress.is_finite() || !(0.0..1.0).contains(&progress) {
        return Err(invalid(
            "keyframe curve split point must lie inside its segment",
        ));
    }
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..48 {
        let middle = (low + high) * 0.5;
        if bezier_component(middle, curve.x1, curve.x2) < progress {
            low = middle;
        } else {
            high = middle;
        }
    }
    let t = (low + high) * 0.5;
    let p0 = (0.0, 0.0);
    let p1 = (curve.x1, curve.y1);
    let p2 = (curve.x2, curve.y2);
    let p3 = (1.0, 1.0);
    let lerp = |a: (f64, f64), b: (f64, f64)| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    let q0 = lerp(p0, p1);
    let q1 = lerp(p1, p2);
    let q2 = lerp(p2, p3);
    let r0 = lerp(q0, q1);
    let r1 = lerp(q1, q2);
    let point = lerp(r0, r1);
    if point.0 <= 0.0 || point.0 >= 1.0 || point.1 <= 0.0 || point.1 >= 1.0 {
        return Err(invalid("keyframe curve split could not be normalized"));
    }
    let unit = |value: f64| value.clamp(0.0, 1.0);
    let left = opencut_contracts::historical_visual::HistoricalVisualCubicBezier {
        x1: unit(q0.0 / point.0),
        y1: unit(q0.1 / point.1),
        x2: unit(r0.0 / point.0),
        y2: unit(r0.1 / point.1),
    };
    let right = opencut_contracts::historical_visual::HistoricalVisualCubicBezier {
        x1: unit((r1.0 - point.0) / (1.0 - point.0)),
        y1: unit((r1.1 - point.1) / (1.0 - point.1)),
        x2: unit((q2.0 - point.0) / (1.0 - point.0)),
        y2: unit((q2.1 - point.1) / (1.0 - point.1)),
    };
    Ok((left, right, point.1))
}

fn set_keyframe_value(
    project: &mut ProjectDocument,
    sequence_id: &str,
    clip_id: &str,
    parameter: &str,
    keyframe_id: &str,
    value: f64,
) -> Result<(), EditorError> {
    let keyframe = project
        .sequences
        .iter_mut()
        .find(|sequence| sequence.id == sequence_id)
        .and_then(|sequence| {
            sequence
                .tracks
                .iter_mut()
                .flat_map(|track| track.clips.iter_mut())
                .find(|clip| clip.id == clip_id)
        })
        .and_then(|clip| {
            clip.channels
                .iter_mut()
                .find(|channel| channel.parameter == parameter)
        })
        .and_then(|channel| {
            channel
                .keyframes
                .iter_mut()
                .find(|keyframe| keyframe.id == keyframe_id)
        })
        .ok_or_else(|| invalid("split keyframe boundary does not exist"))?;
    keyframe.value = serde_json::json!(value);
    Ok(())
}

fn boundary_keyframe_id(
    project: &ProjectDocument,
    sequence_id: &str,
    clip_id: &str,
    parameter: &str,
    at: &RationalTime,
    original_key_ids: &BTreeSet<String>,
) -> Result<String, EditorError> {
    let clip = project
        .sequences
        .iter()
        .find(|sequence| sequence.id == sequence_id)
        .and_then(|sequence| {
            sequence
                .tracks
                .iter()
                .flat_map(|track| track.clips.iter())
                .find(|clip| clip.id == clip_id)
        })
        .ok_or_else(|| invalid("split clip is missing"))?;
    let channel = clip
        .channels
        .iter()
        .find(|channel| channel.parameter == parameter)
        .ok_or_else(|| invalid("split keyframe channel is missing"))?;
    channel
        .keyframes
        .iter()
        .find(|keyframe| {
            !original_key_ids.contains(&keyframe.id)
                && RationalTime::try_from(&keyframe.time)
                    .is_ok_and(|time| time.cmp_exact(*at).is_eq())
        })
        .map(|keyframe| keyframe.id.clone())
        .ok_or_else(|| invalid("split keyframe boundary was not created"))
}

fn preserve_curves_across_split(
    project: &mut HistoricalVisualDocument,
    original: &ProjectDocument,
    split_point: &opencut_contracts::Time,
    sequence_id: &str,
    clip_id: &str,
    right_clip_id: &str,
) -> Result<(Vec<String>, Vec<String>), EditorError> {
    let original_clip = original
        .sequences
        .iter()
        .find(|sequence| sequence.id == sequence_id)
        .and_then(|sequence| {
            sequence
                .tracks
                .iter()
                .flat_map(|track| track.clips.iter())
                .find(|clip| clip.id == clip_id)
        })
        .ok_or_else(|| invalid("split source clip is missing"))?;
    let point = RationalTime::try_from(split_point)?
        .checked_sub(RationalTime::try_from(&original_clip.timeline_range.start)?)?;
    let mut retained = Vec::with_capacity(project.keyframe_curves.len());
    let mut added = Vec::new();
    let mut changed = Vec::new();
    for mut curve in std::mem::take(&mut project.keyframe_curves) {
        if curve.sequence_id != sequence_id || curve.clip_id != clip_id {
            retained.push(curve);
            continue;
        }
        let channel = original_clip
            .channels
            .iter()
            .find(|channel| channel.parameter == curve.parameter)
            .ok_or_else(|| invalid("split source keyframe channel is missing"))?;
        let index = channel
            .keyframes
            .iter()
            .position(|keyframe| keyframe.id == curve.keyframe_id)
            .ok_or_else(|| invalid("split source curve keyframe is missing"))?;
        let start = RationalTime::try_from(&channel.keyframes[index].time)?;
        let end = RationalTime::try_from(&channel.keyframes[index + 1].time)?;
        if point.cmp_exact(start).is_le() {
            curve.clip_id = right_clip_id.into();
            changed.push(curve.id.clone());
            retained.push(curve);
            continue;
        }
        if point.cmp_exact(end).is_ge() {
            retained.push(curve);
            continue;
        }

        let offset = point.checked_sub(start)?;
        let duration = end.checked_sub(start)?;
        let progress = (offset.ticks() as f64 * duration.timebase() as f64)
            / (offset.timebase() as f64 * duration.ticks() as f64);
        let (left_curve, right_curve, eased_progress) = split_bezier(curve.curve, progress)?;
        let from = channel.keyframes[index]
            .value
            .as_f64()
            .ok_or_else(|| invalid("split keyframe value is not numeric"))?;
        let to = channel.keyframes[index + 1]
            .value
            .as_f64()
            .ok_or_else(|| invalid("split keyframe value is not numeric"))?;
        let boundary_value = from * (1.0 - eased_progress) + to * eased_progress;
        let original_key_ids: BTreeSet<_> = channel
            .keyframes
            .iter()
            .map(|keyframe| keyframe.id.clone())
            .collect();
        let left_boundary = boundary_keyframe_id(
            &project.base,
            sequence_id,
            clip_id,
            &curve.parameter,
            &point,
            &original_key_ids,
        )?;
        let zero = RationalTime::new(0, 1)?;
        let right_boundary = boundary_keyframe_id(
            &project.base,
            sequence_id,
            right_clip_id,
            &curve.parameter,
            &zero,
            &original_key_ids,
        )?;
        set_keyframe_value(
            &mut project.base,
            sequence_id,
            clip_id,
            &curve.parameter,
            &left_boundary,
            boundary_value,
        )?;
        set_keyframe_value(
            &mut project.base,
            sequence_id,
            right_clip_id,
            &curve.parameter,
            &right_boundary,
            boundary_value,
        )?;
        curve.curve = left_curve;
        changed.push(curve.id.clone());
        retained.push(curve.clone());
        let mut right = curve;
        right.id = split_curve_id(&right.id, right_clip_id);
        right.clip_id = right_clip_id.into();
        right.keyframe_id = right_boundary;
        right.curve = right_curve;
        added.push(right.id.clone());
        retained.push(right);
    }
    project.keyframe_curves = retained;
    project.keyframe_curves.sort_by(|a, b| a.id.cmp(&b.id));
    Ok((added, changed))
}

pub fn plan_apply(
    current: &HistoricalVisualDocument,
    request: &ApplyHistoricalVisualProject,
) -> Result<VisualPlan, EditorError> {
    if current.base.id != request.project_id
        || request.operations.is_empty()
        || request.operations.len() > 128
        || request.label.trim().is_empty()
        || request.label.len() > 256
    {
        return Err(invalid("Invalid schema 3 edit request"));
    }
    opencut_editor_core::historical_visual::validate_project_historical_visual(current)?;
    let mut proposed = current.clone();
    let (mut created, mut changed, mut deleted) = (Vec::new(), Vec::new(), Vec::new());
    for operation in &request.operations {
        if let VisualOperation::Base {
            operation: base_operation,
        } = operation
        {
            let before_base = proposed.base.clone();
            let base_request = TransactionRequest {
                expected_revision: request.expected_revision.clone(),
                idempotency_key: request.idempotency_key.clone(),
                label: request.label.clone(),
                operations: vec![base_operation.clone()],
            };
            let planned = opencut_editor_core::plan_transaction(
                &ProjectSnapshot {
                    project: proposed.base.clone(),
                    revision: request.expected_revision.clone(),
                },
                &base_request,
            )?;
            proposed.base = planned.proposed_project;
            created.extend(planned.created_ids);
            changed.extend(planned.changed_ids);
            deleted.extend(planned.deleted_ids);
            match base_operation {
                Operation::SplitClip {
                    sequence_id,
                    clip_id,
                    at,
                    right_clip_id,
                    ..
                } => {
                    let (new_curves, changed_curves) = preserve_curves_across_split(
                        &mut proposed,
                        &before_base,
                        at,
                        sequence_id,
                        clip_id,
                        right_clip_id,
                    )?;
                    created.extend(new_curves);
                    changed.extend(changed_curves);
                }
                Operation::DeleteClip {
                    sequence_id,
                    clip_id,
                    ..
                } => {
                    let removed: Vec<_> = proposed
                        .keyframe_curves
                        .iter()
                        .filter(|curve| {
                            curve.sequence_id == *sequence_id && curve.clip_id == *clip_id
                        })
                        .map(|curve| curve.id.clone())
                        .collect();
                    proposed.keyframe_curves.retain(|curve| {
                        curve.sequence_id != *sequence_id || curve.clip_id != *clip_id
                    });
                    deleted.extend(removed);
                }
                Operation::DeleteKeyframeChannel {
                    sequence_id,
                    clip_id,
                    parameter,
                    ..
                } => {
                    let removed: Vec<_> = proposed
                        .keyframe_curves
                        .iter()
                        .filter(|curve| {
                            curve.sequence_id == *sequence_id
                                && curve.clip_id == *clip_id
                                && curve.parameter == *parameter
                        })
                        .map(|curve| curve.id.clone())
                        .collect();
                    proposed.keyframe_curves.retain(|curve| {
                        curve.sequence_id != *sequence_id
                            || curve.clip_id != *clip_id
                            || curve.parameter != *parameter
                    });
                    deleted.extend(removed);
                }
                _ => {}
            }
            continue;
        }
        match operation {
            VisualOperation::KeyframeCurveSet { sequence_id, curve } => {
                check_visual_lock(&proposed, operation)?;
                if curve.sequence_id != *sequence_id {
                    return Err(invalid("Keyframe curve sequence does not match operation"));
                }
                if let Some(index) = proposed
                    .keyframe_curves
                    .iter()
                    .position(|value| value.id == curve.id)
                {
                    if proposed.keyframe_curves[index] != *curve {
                        proposed.keyframe_curves[index] = curve.clone();
                        changed.push(curve.id.clone());
                    }
                } else {
                    proposed.keyframe_curves.push(curve.clone());
                    created.push(curve.id.clone());
                }
                continue;
            }
            VisualOperation::KeyframeCurveDelete { id, .. } => {
                check_visual_lock(&proposed, operation)?;
                let index = proposed
                    .keyframe_curves
                    .iter()
                    .position(|curve| curve.id == *id)
                    .ok_or_else(|| {
                        EditorError::new("NOT_FOUND", "Keyframe curve does not exist")
                    })?;
                proposed.keyframe_curves.remove(index);
                deleted.push(id.clone());
                continue;
            }
            _ => {}
        }
        check_visual_lock(&proposed, operation)?;
        let visual = sequence(
            &mut proposed,
            operation.sequence_id().expect("non-base visual operation"),
        )?;
        match operation {
            VisualOperation::Base { .. } => unreachable!(),
            VisualOperation::ShapePut { shape, .. } => put(
                &mut visual.shapes,
                shape.clone(),
                &mut created,
                &mut changed,
            ),
            VisualOperation::ShapeDelete { id, .. } => {
                delete(&mut visual.shapes, id, &mut deleted)?
            }
            VisualOperation::EffectPut { effect, .. } => put(
                &mut visual.effects,
                effect.clone(),
                &mut created,
                &mut changed,
            ),
            VisualOperation::EffectDelete { id, .. } => {
                delete(&mut visual.effects, id, &mut deleted)?
            }
            VisualOperation::MaskPut { mask, .. } => {
                put(&mut visual.masks, mask.clone(), &mut created, &mut changed)
            }
            VisualOperation::MaskDelete { id, .. } => delete(&mut visual.masks, id, &mut deleted)?,
            VisualOperation::TransitionPut { transition, .. } => put(
                &mut visual.transitions,
                transition.clone(),
                &mut created,
                &mut changed,
            ),
            VisualOperation::TransitionDelete { id, .. } => {
                delete(&mut visual.transitions, id, &mut deleted)?
            }
            VisualOperation::KeyframeCurveSet { .. }
            | VisualOperation::KeyframeCurveDelete { .. } => unreachable!(),
        }
    }
    proposed.keyframe_curves.sort_by(|a, b| a.id.cmp(&b.id));
    if proposed == *current || (created.is_empty() && changed.is_empty() && deleted.is_empty()) {
        return Err(EditorError::new("NO_EFFECT", "Visual edit has no effect"));
    }
    opencut_editor_core::historical_visual::validate_project_historical_visual(&proposed)?;
    let ids: BTreeSet<_> = created.into_iter().chain(changed).chain(deleted).collect();
    let (mut created, mut changed, mut deleted) = (Vec::new(), Vec::new(), Vec::new());
    for id in ids {
        match (entity_value(current, &id), entity_value(&proposed, &id)) {
            (None, Some(_)) => created.push(id),
            (Some(_), None) => deleted.push(id),
            (Some(before), Some(after)) if before != after => changed.push(id),
            _ => {}
        }
    }
    if created.is_empty() && changed.is_empty() && deleted.is_empty() {
        changed.push(current.base.id.clone());
    }
    Ok(VisualPlan {
        proposed_project: proposed,
        created_ids: created,
        changed_ids: changed,
        deleted_ids: deleted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{
        Clip, FrameRate, Keyframe, KeyframeChannel, KeyframeInterpolation,
        historical_visual::{HistoricalVisualCubicBezier, HistoricalVisualKeyframeCurve},
    };
    use serde_json::json;

    fn project(locked: bool) -> HistoricalVisualDocument {
        let mut project = create(&CreateProject {
            id: "p".into(),
            name: "P".into(),
            sequence_id: "s".into(),
            width: 640,
            height: 360,
            fps: FrameRate {
                numerator: 25,
                denominator: 1,
            },
        })
        .unwrap();
        project.base.sequences[0].tracks.push(
            serde_json::from_value(json!({
                "id":"graphics","name":"Graphics","kind":"graphic","enabled":true,
                "muted":false,"locked":locked,"clips":[],"params":{}
            }))
            .unwrap(),
        );
        project
    }
    fn project_with_keyframes(locked: bool) -> HistoricalVisualDocument {
        let mut project = project(locked);
        let graphic_track = project.base.sequences[0]
            .tracks
            .iter_mut()
            .find(|track| track.id == "graphics")
            .unwrap();
        graphic_track.clips.push(Clip {
            id: "animated-clip".into(),
            asset_id: None,
            timeline_range: serde_json::from_value(json!({
                "start":{"ticks":"0","timebase":1000},
                "duration":{"ticks":"1000","timebase":1000}
            }))
            .unwrap(),
            source_range: None,
            params: Default::default(),
            channels: vec![KeyframeChannel {
                parameter: "opacity".into(),
                keyframes: vec![
                    Keyframe {
                        id: "opacity-start".into(),
                        time: serde_json::from_value(json!({"ticks":"0","timebase":1000})).unwrap(),
                        value: json!(0.0),
                        interpolation: KeyframeInterpolation::Linear,
                    },
                    Keyframe {
                        id: "opacity-end".into(),
                        time: serde_json::from_value(json!({"ticks":"1000","timebase":1000}))
                            .unwrap(),
                        value: json!(1.0),
                        interpolation: KeyframeInterpolation::Linear,
                    },
                ],
            }],
        });
        project
    }
    fn shape() -> ShapeClip {
        serde_json::from_value(json!({"id":"card","track_id":"graphics",
            "timeline_range":{"start":{"ticks":"0","timebase":1000},
            "duration":{"ticks":"1000","timebase":1000}},
            "primitive":{"kind":"rectangle","width_px":200.0,"height_px":100.0},
            "fill":{"red":255,"green":0,"blue":0,"alpha":255},
            "position_x":0.0,"position_y":0.0,"rotation_degrees":0.0,"opacity":1.0}))
        .unwrap()
    }
    fn request(operations: Vec<VisualOperation>) -> ApplyHistoricalVisualProject {
        ApplyHistoricalVisualProject {
            project_id: "p".into(),
            expected_revision: "r".into(),
            idempotency_key: "k".into(),
            label: "Edit".into(),
            operations,
        }
    }
    #[test]
    fn locked_track_blocks_visual_create_and_existing_target_edits() {
        let mut project = project(true);
        let put = VisualOperation::ShapePut {
            sequence_id: "s".into(),
            shape: shape(),
        };
        assert_eq!(
            plan_apply(&project, &request(vec![put.clone()]))
                .unwrap_err()
                .code,
            "CONSTRAINT_VIOLATION"
        );
        project.base.sequences[0].tracks[0].locked = false;
        project = plan_apply(&project, &request(vec![put]))
            .unwrap()
            .proposed_project;
        project.base.sequences[0].tracks[0].locked = true;
        let effect: EffectNode = serde_json::from_value(json!({"id":"grade","target_id":"card",
            "effect":{"kind":"color_adjust","exposure_ev":0.0,"contrast":1.0,"saturation":1.0}}))
        .unwrap();
        assert_eq!(
            plan_apply(
                &project,
                &request(vec![VisualOperation::EffectPut {
                    sequence_id: "s".into(),
                    effect
                }])
            )
            .unwrap_err()
            .code,
            "CONSTRAINT_VIOLATION"
        );
        assert_eq!(
            plan_apply(
                &project,
                &request(vec![VisualOperation::ShapeDelete {
                    sequence_id: "s".into(),
                    id: "card".into()
                }])
            )
            .unwrap_err()
            .code,
            "CONSTRAINT_VIOLATION"
        );
    }

    #[test]
    fn versioned_keyframe_curve_edits_plan_and_respect_track_locks() {
        let curve = HistoricalVisualKeyframeCurve {
            id: "opacity-curve".into(),
            schema_version: 1,
            sequence_id: "s".into(),
            clip_id: "animated-clip".into(),
            parameter: "opacity".into(),
            keyframe_id: "opacity-start".into(),
            curve: HistoricalVisualCubicBezier {
                x1: 0.0,
                y1: 0.0,
                x2: 1.0,
                y2: 0.0,
            },
        };
        let operation = VisualOperation::KeyframeCurveSet {
            sequence_id: "s".into(),
            curve: curve.clone(),
        };
        let planned = plan_apply(
            &project_with_keyframes(false),
            &request(vec![operation.clone()]),
        )
        .unwrap();
        assert_eq!(planned.proposed_project.keyframe_curves, [curve]);
        assert_eq!(planned.created_ids, ["opacity-curve"]);

        let deleted = plan_apply(
            &planned.proposed_project,
            &request(vec![VisualOperation::KeyframeCurveDelete {
                sequence_id: "s".into(),
                id: "opacity-curve".into(),
            }]),
        )
        .unwrap();
        assert!(deleted.proposed_project.keyframe_curves.is_empty());
        assert_eq!(deleted.deleted_ids, ["opacity-curve"]);

        assert_eq!(
            plan_apply(&project_with_keyframes(true), &request(vec![operation]),)
                .unwrap_err()
                .code,
            "CONSTRAINT_VIOLATION"
        );
    }

    #[test]
    fn clip_split_subdivides_curve_and_samples_the_same_boundary_value() {
        let set_curve = plan_apply(
            &project_with_keyframes(false),
            &request(vec![VisualOperation::KeyframeCurveSet {
                sequence_id: "s".into(),
                curve: HistoricalVisualKeyframeCurve {
                    id: "opacity-curve".into(),
                    schema_version: 1,
                    sequence_id: "s".into(),
                    clip_id: "animated-clip".into(),
                    parameter: "opacity".into(),
                    keyframe_id: "opacity-start".into(),
                    curve: HistoricalVisualCubicBezier {
                        x1: 0.0,
                        y1: 0.0,
                        x2: 1.0,
                        y2: 0.0,
                    },
                },
            }]),
        )
        .unwrap()
        .proposed_project;
        let split = plan_apply(
            &set_curve,
            &request(vec![VisualOperation::Base {
                operation: Operation::SplitClip {
                    sequence_id: "s".into(),
                    track_id: "graphics".into(),
                    clip_id: "animated-clip".into(),
                    at: serde_json::from_value(json!({"ticks":"500","timebase":1000})).unwrap(),
                    right_clip_id: "right-clip".into(),
                },
            }]),
        )
        .unwrap();
        assert_eq!(split.proposed_project.keyframe_curves.len(), 2);
        let left_curve = split
            .proposed_project
            .keyframe_curves
            .iter()
            .find(|curve| curve.id == "opacity-curve")
            .unwrap();
        let right_curve_id = split_curve_id("opacity-curve", "right-clip");
        let right_curve = split
            .proposed_project
            .keyframe_curves
            .iter()
            .find(|curve| curve.id == right_curve_id)
            .unwrap();
        assert_eq!(left_curve.clip_id, "animated-clip");
        assert_eq!(right_curve.clip_id, "right-clip");
        assert_eq!(left_curve.curve.x1, 0.0);
        assert!(right_curve.curve.x1.is_finite());
        assert!(split.created_ids.contains(&right_curve_id));
        assert!(split.changed_ids.contains(&"opacity-curve".into()));

        let boundary = |clip_id: &str, boundary_suffix: &str| {
            split.proposed_project.base.sequences[0].tracks[0]
                .clips
                .iter()
                .find(|clip| clip.id == clip_id)
                .unwrap()
                .channels[0]
                .keyframes
                .iter()
                .find(|keyframe| keyframe.id.contains(boundary_suffix))
                .unwrap()
                .value
                .as_f64()
                .unwrap()
        };
        let left_value = boundary("animated-clip", "left-boundary");
        let right_value = boundary("right-clip", "right-boundary");
        assert!((left_value - right_value).abs() < 1e-12);
        assert!(
            left_value > 0.0 && left_value < 0.2,
            "eased split value: {left_value}"
        );
    }
    #[test]
    fn delete_then_put_same_id_reports_final_change() {
        let original = plan_apply(
            &project(false),
            &request(vec![VisualOperation::ShapePut {
                sequence_id: "s".into(),
                shape: shape(),
            }]),
        )
        .unwrap()
        .proposed_project;
        let mut changed_shape = shape();
        changed_shape.opacity = 0.5;
        let plan = plan_apply(
            &original,
            &request(vec![
                VisualOperation::ShapeDelete {
                    sequence_id: "s".into(),
                    id: "card".into(),
                },
                VisualOperation::ShapePut {
                    sequence_id: "s".into(),
                    shape: changed_shape,
                },
            ]),
        )
        .unwrap();
        assert_eq!(plan.changed_ids, ["card"]);
        assert!(plan.created_ids.is_empty() && plan.deleted_ids.is_empty());
    }
}
