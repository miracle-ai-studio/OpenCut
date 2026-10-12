//! Classic selected splitting and per-track ripple, using exact rational time.
//! Input is a versioned intent; every resulting operation uses the existing
//! fixed-revision visual transaction. No transport, storage or renderer I/O.
use super::*;
use opencut_contracts::timeline_edit::{
    ClassicCommandV1, ClassicEditPlanV1, ClassicEditRequestV1, ClassicSelectionV1, SplitRetainV1,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Row {
    Track(String),
    Captions,
}
#[derive(Clone)]
enum Target {
    Clip(String),
    Caption(String),
    Shape(String),
}
#[derive(Clone)]
struct Span {
    id: String,
    start: R,
    finish: R,
    target: Target,
}
type Interval = (R, R);

fn spans(
    project: &ProjectDocumentV3,
    sequence_id: &str,
) -> Result<BTreeMap<Row, Vec<Span>>, EditorError> {
    let sequence = project
        .base
        .sequences
        .iter()
        .find(|s| s.id == sequence_id)
        .ok_or_else(|| invalid("The edited sequence was removed"))?;
    let mut rows = BTreeMap::new();
    for track in &sequence.tracks {
        let values = track
            .clips
            .iter()
            .map(|clip| {
                Ok(Span {
                    id: clip.id.clone(),
                    start: time(&clip.timeline_range.start)?,
                    finish: end(&clip.timeline_range)?,
                    target: Target::Clip(clip.id.clone()),
                })
            })
            .collect::<Result<Vec<_>, EditorError>>()?;
        rows.insert(Row::Track(track.id.clone()), values);
    }
    for shape in project
        .visual
        .iter()
        .filter(|s| s.sequence_id == sequence_id)
        .flat_map(|s| &s.shapes)
    {
        rows.entry(Row::Track(shape.track_id.clone()))
            .or_default()
            .push(Span {
                id: shape.id.clone(),
                start: time(&shape.timeline_range.start)?,
                finish: end(&shape.timeline_range)?,
                target: Target::Shape(shape.id.clone()),
            });
    }
    rows.insert(
        Row::Captions,
        sequence
            .caption_cues
            .iter()
            .map(|cue| {
                Ok(Span {
                    id: cue.id.clone(),
                    start: time(&cue.timeline_range.start)?,
                    finish: end(&cue.timeline_range)?,
                    target: Target::Caption(cue.id.clone()),
                })
            })
            .collect::<Result<Vec<_>, EditorError>>()?,
    );
    Ok(rows)
}

fn normalize(mut values: Vec<Interval>) -> Vec<Interval> {
    values.retain(|(a, b)| b.cmp_exact(*a).is_gt());
    values.sort_by(|a, b| a.0.cmp_exact(b.0));
    let mut merged: Vec<Interval> = vec![];
    for (a, b) in values {
        if let Some(last) = merged.last_mut()
            && !a.cmp_exact(last.1).is_gt()
        {
            if b.cmp_exact(last.1).is_gt() {
                last.1 = b;
            }
        } else {
            merged.push((a, b));
        }
    }
    merged
}

fn freed(before: &[Span], after: &[Span], all_after: &BTreeSet<String>) -> Vec<Interval> {
    let after_ids: BTreeMap<_, _> = after.iter().map(|s| (&s.id, s)).collect();
    let before_ids: BTreeSet<_> = before.iter().map(|s| &s.id).collect();
    let vacated = before
        .iter()
        .filter_map(|old| match after_ids.get(&old.id) {
            Some(new) if old.finish.cmp_exact(new.finish).is_gt() => Some((new.finish, old.finish)),
            None if !all_after.contains(&old.id) => Some((old.start, old.finish)),
            _ => None,
        })
        .collect();
    let joined = normalize(
        after
            .iter()
            .filter(|s| !before_ids.contains(&s.id))
            .map(|s| (s.start, s.finish))
            .collect(),
    );
    let mut remaining = normalize(vacated);
    for (a, b) in joined {
        remaining = remaining
            .into_iter()
            .flat_map(|(start, finish)| {
                if !b.cmp_exact(start).is_gt() || !a.cmp_exact(finish).is_lt() {
                    return vec![(start, finish)];
                }
                let mut pieces = vec![];
                if a.cmp_exact(start).is_gt() {
                    pieces.push((start, a));
                }
                if b.cmp_exact(finish).is_lt() {
                    pieces.push((b, finish));
                }
                pieces
            })
            .collect();
    }
    remaining
}

fn detach_transitions(planner: &mut Planner<'_>, affected: &BTreeSet<String>) {
    if let Some(graph) = planner
        .working
        .visual
        .iter_mut()
        .find(|s| s.sequence_id == planner.sequence_id)
    {
        let (transitions, other): (Vec<_>, Vec<_>) = std::mem::take(&mut graph.transitions)
            .into_iter()
            .partition(|t| {
                affected.contains(&t.outgoing_clip_id) || affected.contains(&t.incoming_clip_id)
            });
        graph.transitions = other;
        planner
            .transaction
            .operations
            .extend(
                transitions
                    .iter()
                    .map(|t| VisualOperation::TransitionDelete {
                        sequence_id: planner.sequence_id.to_owned(),
                        id: t.id.clone(),
                    }),
            );
        planner.transitions.extend(transitions);
    }
}

fn ripple(planner: &mut Planner<'_>, before: &ProjectDocumentV3) -> Result<(), EditorError> {
    let before_rows = spans(before, planner.sequence_id)?;
    let after_rows = spans(&planner.working, planner.sequence_id)?;
    let all_after = after_rows
        .values()
        .flatten()
        .map(|s| s.id.clone())
        .collect();
    let adjustments: Vec<_> = before_rows
        .iter()
        .flat_map(|(row, values)| {
            freed(
                values,
                after_rows.get(row).map(Vec::as_slice).unwrap_or(&[]),
                &all_after,
            )
            .into_iter()
            .rev()
            .map(|interval| (row.clone(), interval))
        })
        .collect();
    if adjustments.is_empty() {
        return Ok(());
    }
    let mut moved = BTreeSet::new();
    for (row, values) in &after_rows {
        for span in values {
            if let Target::Clip(id) = &span.target
                && adjustments
                    .iter()
                    .any(|(r, (_, b))| r == row && !span.start.cmp_exact(*b).is_lt())
            {
                moved.insert(id.clone());
            }
        }
    }
    detach_transitions(planner, &moved);
    for (row, (a, b)) in adjustments {
        let delta = b.checked_sub(a)?;
        let mut targets = spans(&planner.working, planner.sequence_id)?
            .remove(&row)
            .unwrap_or_default();
        targets.retain(|s| !s.start.cmp_exact(b).is_lt());
        targets.sort_by(|a, b| a.start.cmp_exact(b.start));
        for target in targets {
            let start = target.start.checked_sub(delta)?;
            match target.target {
                Target::Clip(id) => planner.move_clip(&id, start)?,
                Target::Caption(id) => {
                    let mut cue = planner
                        .sequence()
                        .caption_cues
                        .iter()
                        .find(|c| c.id == id)
                        .unwrap()
                        .clone();
                    cue.timeline_range.start = start.into();
                    planner.base(Operation::UpdateCaption {
                        sequence_id: planner.sequence_id.to_owned(),
                        cue,
                    })?;
                }
                Target::Shape(id) => {
                    let mut shape = planner
                        .working
                        .visual
                        .iter()
                        .filter(|s| s.sequence_id == planner.sequence_id)
                        .flat_map(|s| &s.shapes)
                        .find(|s| s.id == id)
                        .unwrap()
                        .clone();
                    shape.timeline_range.start = start.into();
                    planner.step(VisualOperation::ShapePut {
                        sequence_id: planner.sequence_id.to_owned(),
                        shape,
                    })?;
                }
            }
        }
    }
    Ok(())
}

// VisualOperation::sequence_id intentionally excludes wrapped base operations.
// Classic planning has a stricter captured-scene boundary for both representations.
fn operation_sequence(op: &VisualOperation) -> Option<&str> {
    if let VisualOperation::Base { operation } = op {
        match operation {
            Operation::BindAsset { .. }
            | Operation::ReplaceAsset { .. }
            | Operation::RemoveAsset { .. } => None,
            Operation::UpdateSequenceSettings { sequence_id, .. }
            | Operation::AddTrack { sequence_id, .. }
            | Operation::RemoveTrack { sequence_id, .. }
            | Operation::InsertClip { sequence_id, .. }
            | Operation::SplitClip { sequence_id, .. }
            | Operation::MoveClip { sequence_id, .. }
            | Operation::TrimClip { sequence_id, .. }
            | Operation::DeleteClip { sequence_id, .. }
            | Operation::UpdateClipParams { sequence_id, .. }
            | Operation::UpdateTrackParams { sequence_id, .. }
            | Operation::UpdateTrack { sequence_id, .. }
            | Operation::RetimeClip { sequence_id, .. }
            | Operation::SetKeyframeChannel { sequence_id, .. }
            | Operation::DeleteKeyframeChannel { sequence_id, .. }
            | Operation::InsertCaption { sequence_id, .. }
            | Operation::UpdateCaption { sequence_id, .. }
            | Operation::DeleteCaption { sequence_id, .. } => Some(sequence_id),
        }
    } else {
        op.sequence_id()
    }
}

pub fn plan(
    snapshot: &ProjectSnapshotV3,
    input: &ClassicEditRequestV1,
) -> Result<ClassicEditPlanV1, EditorError> {
    let mut planner = Planner::new(
        snapshot,
        &input.sequence_id,
        ApplyV3Project {
            project_id: input.project_id.clone(),
            expected_revision: input.expected_revision.clone(),
            idempotency_key: input.idempotency_key.clone(),
            label: input.label.clone(),
            operations: vec![],
        },
    )?;
    let mut selected = None;
    match &input.command {
        ClassicCommandV1::Apply { operations } => {
            if operations
                .iter()
                .filter_map(operation_sequence)
                .any(|s| s != input.sequence_id)
            {
                return Err(invalid(
                    "Classic edit operations must target the captured sequence",
                ));
            }
            planner.transaction.operations = operations.clone();
            planner.working =
                crate::project_v3::plan_apply(&snapshot.project, &planner.transaction)?
                    .proposed_project;
            for children in planner.lineage.values_mut() {
                children.retain(|id| {
                    planner
                        .working
                        .base
                        .sequences
                        .iter()
                        .flat_map(|s| &s.tracks)
                        .flat_map(|t| &t.clips)
                        .any(|c| &c.id == id)
                });
            }
        }
        ClassicCommandV1::Split {
            clip_ids,
            caption_ids,
            shape_ids,
            at,
            retain,
        } => {
            let clips = selection(clip_ids)?;
            let captions = selection(caption_ids)?;
            let shapes = selection(shape_ids)?;
            let count = clips.len() + captions.len() + shapes.len();
            if count == 0 || count > 128 {
                return Err(invalid("Select between 1 and 128 elements"));
            }
            let at = time(at)?;
            if at.ticks() < 0 {
                return Err(invalid("Split time must be nonnegative"));
            }
            // Detach only transitions affected by selected clips; preserve unrelated locks.
            if let Some(graph) = planner
                .working
                .visual
                .iter_mut()
                .find(|s| s.sequence_id == input.sequence_id)
            {
                let (affected, other): (Vec<_>, Vec<_>) = std::mem::take(&mut graph.transitions)
                    .into_iter()
                    .partition(|t| {
                        clips.contains(&t.outgoing_clip_id) || clips.contains(&t.incoming_clip_id)
                    });
                graph.transitions = other;
                planner
                    .transaction
                    .operations
                    .extend(affected.iter().map(|t| VisualOperation::TransitionDelete {
                        sequence_id: input.sequence_id.clone(),
                        id: t.id.clone(),
                    }));
                planner.transitions = affected;
            }
            let mut result = ClassicSelectionV1 {
                clip_ids: vec![],
                caption_ids: vec![],
                shape_ids: vec![],
                playhead: None,
            };
            for id in clip_ids {
                let (_, clip) = planner.clip(id)?;
                if !at.cmp_exact(time(&clip.timeline_range.start)?).is_gt()
                    || !at.cmp_exact(end(&clip.timeline_range)?).is_lt()
                {
                    continue;
                }
                let right = planner.split(id, at)?;
                if *retain == SplitRetainV1::Left {
                    planner.delete(&right)?;
                } else {
                    if *retain == SplitRetainV1::Right {
                        planner.delete(id)?;
                    }
                    result.clip_ids.push(right);
                }
            }
            for id in caption_ids {
                let cue = planner
                    .sequence()
                    .caption_cues
                    .iter()
                    .find(|c| &c.id == id)
                    .cloned()
                    .ok_or_else(|| {
                        EditorError::new("NOT_FOUND", "Selected caption does not exist")
                    })?;
                let start = time(&cue.timeline_range.start)?;
                let finish = end(&cue.timeline_range)?;
                if !at.cmp_exact(start).is_gt() || !at.cmp_exact(finish).is_lt() {
                    continue;
                }
                let mut left = cue.clone();
                left.timeline_range = range(start, at)?;
                if *retain != SplitRetainV1::Right {
                    planner.base(Operation::UpdateCaption {
                        sequence_id: input.sequence_id.clone(),
                        cue: left,
                    })?;
                } else {
                    planner.base(Operation::DeleteCaption {
                        sequence_id: input.sequence_id.clone(),
                        cue_id: id.clone(),
                    })?;
                }
                if *retain != SplitRetainV1::Left {
                    let mut right = cue;
                    right.id = planner.fresh();
                    right.timeline_range = range(at, finish)?;
                    result.caption_ids.push(right.id.clone());
                    planner.base(Operation::InsertCaption {
                        sequence_id: input.sequence_id.clone(),
                        cue: right,
                    })?;
                }
            }
            for id in shape_ids {
                let shape = planner
                    .working
                    .visual
                    .iter()
                    .filter(|s| s.sequence_id == input.sequence_id)
                    .flat_map(|s| &s.shapes)
                    .find(|s| &s.id == id)
                    .cloned()
                    .ok_or_else(|| {
                        EditorError::new("NOT_FOUND", "Selected shape does not exist")
                    })?;
                let start = time(&shape.timeline_range.start)?;
                let finish = end(&shape.timeline_range)?;
                if !at.cmp_exact(start).is_gt() || !at.cmp_exact(finish).is_lt() {
                    continue;
                }
                let mut left = shape.clone();
                left.timeline_range = range(start, at)?;
                planner.step(VisualOperation::ShapePut {
                    sequence_id: input.sequence_id.clone(),
                    shape: left,
                })?;
                if *retain != SplitRetainV1::Left {
                    let mut right = shape;
                    right.id = planner.fresh();
                    right.timeline_range = range(at, finish)?;
                    result.shape_ids.push(right.id.clone());
                    let right_id = right.id.clone();
                    planner.step(VisualOperation::ShapePut {
                        sequence_id: input.sequence_id.clone(),
                        shape: right,
                    })?;
                    planner.copy_visuals(id, &right_id)?;
                }
                if *retain == SplitRetainV1::Right {
                    planner.remove_target_visuals(id)?;
                    planner.step(VisualOperation::ShapeDelete {
                        sequence_id: input.sequence_id.clone(),
                        id: id.clone(),
                    })?;
                }
            }
            if !result.clip_ids.is_empty()
                || !result.caption_ids.is_empty()
                || !result.shape_ids.is_empty()
            {
                selected = Some(result);
            }
        }
    }
    if input.ripple {
        ripple(&mut planner, &snapshot.project)?;
    }
    planner.restore_transitions()?;
    if planner.transaction.operations.is_empty() {
        return Err(EditorError::new(
            "NO_EFFECT",
            "No selected element crosses the split boundary",
        ));
    }
    let verified = crate::project_v3::plan_apply(&snapshot.project, &planner.transaction)?;
    if verified.proposed_project == snapshot.project {
        return Err(EditorError::new(
            "NO_EFFECT",
            "No selected element crosses the split boundary or changes content",
        ));
    }
    if input.ripple
        && matches!(
            input.command,
            ClassicCommandV1::Split {
                retain: SplitRetainV1::Right,
                ..
            }
        )
        && let Some(result) = selected.as_mut()
    {
        let rows = spans(&planner.working, &input.sequence_id)?;
        result.playhead = result
            .clip_ids
            .first()
            .or(result.caption_ids.first())
            .or(result.shape_ids.first())
            .and_then(|id| rows.values().flatten().find(|s| &s.id == id))
            .map(|s| s.start.into());
    }
    Ok(ClassicEditPlanV1 {
        plan: TimelineEditPlan {
            transaction: planner.transaction,
            created_ids: verified.created_ids,
            changed_ids: verified.changed_ids,
            deleted_ids: verified.deleted_ids,
            notes: planner.notes,
        },
        selection: selected,
    })
}

#[cfg(test)]
mod tests;
