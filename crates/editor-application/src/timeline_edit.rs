//! Editorial orchestration lowers to the same persisted, reversible transaction
//! used by every transport. No I/O, private storage layout or rendering lives here.
use opencut_contracts::{
    Clip, EditorError, Operation, Sequence, Time, TimeRange,
    historical_visual::{HistoricalVisualDocument, HistoricalVisualSnapshot, TransitionNode},
    historical_visual_ops::{ApplyHistoricalVisualProject, VisualOperation},
    timeline_edit::{TimelineCommand, TimelineEditPlan, TimelineEditRequest},
};
use opencut_time::RationalTime as R;
use std::collections::{BTreeMap, BTreeSet};

fn invalid(message: impl Into<String>) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn time(t: &Time) -> Result<R, EditorError> {
    R::try_from(t)
}
fn end(r: &TimeRange) -> Result<R, EditorError> {
    time(&r.start)?.checked_add(time(&r.duration)?)
}
fn range(a: R, b: R) -> Result<TimeRange, EditorError> {
    if a.ticks() < 0 || !b.cmp_exact(a).is_gt() {
        return Err(invalid(
            "Editorial range must have a nonnegative start and positive duration",
        ));
    }
    Ok(TimeRange {
        start: a.into(),
        duration: b.checked_sub(a)?.into(),
    })
}
fn nonzero(t: &Time) -> Result<R, EditorError> {
    let value = time(t)?;
    if value.ticks() == 0 {
        return Err(EditorError::new("NO_EFFECT", "Edit delta is zero"));
    }
    Ok(value)
}
fn selection(ids: &[String]) -> Result<BTreeSet<String>, EditorError> {
    let values: BTreeSet<_> = ids.iter().cloned().collect();
    if values.len() != ids.len() || values.iter().any(|id| id.trim().is_empty()) {
        return Err(invalid("Selection IDs must be nonempty and unique"));
    }
    Ok(values)
}

struct Planner<'a> {
    sequence_id: &'a str,
    working: HistoricalVisualDocument,
    transaction: ApplyHistoricalVisualProject,
    transitions: Vec<TransitionNode>,
    lineage: BTreeMap<String, Vec<String>>,
    reserved: BTreeSet<String>,
    notes: Vec<String>,
}
impl<'a> Planner<'a> {
    fn new(
        snapshot: &HistoricalVisualSnapshot,
        sequence_id: &'a str,
        transaction: ApplyHistoricalVisualProject,
    ) -> Result<Self, EditorError> {
        if snapshot.project.base.id != transaction.project_id
            || snapshot.revision != transaction.expected_revision
        {
            return Err(EditorError::new(
                "REVISION_CONFLICT",
                "Editorial plan requires the exact project revision",
            ));
        }
        opencut_editor_core::historical_visual::validate_project_historical_visual(
            &snapshot.project,
        )?;
        let sequence = snapshot
            .project
            .base
            .sequences
            .iter()
            .find(|s| s.id == sequence_id)
            .ok_or_else(|| EditorError::new("NOT_FOUND", "Sequence does not exist"))?;
        if transaction.idempotency_key.trim().is_empty() {
            return Err(invalid("Idempotency key is required"));
        }
        let lineage = sequence
            .tracks
            .iter()
            .flat_map(|t| &t.clips)
            .map(|c| (c.id.clone(), vec![c.id.clone()]))
            .collect();
        // Reserve all strings conservatively: generated IDs cannot collide with an
        // existing clip, keyframe, effect, asset, curve, sequence or future extension.
        fn strings(value: &serde_json::Value, out: &mut BTreeSet<String>) {
            match value {
                serde_json::Value::String(v) => {
                    out.insert(v.clone());
                }
                serde_json::Value::Array(v) => {
                    for x in v {
                        strings(x, out)
                    }
                }
                serde_json::Value::Object(v) => {
                    for x in v.values() {
                        strings(x, out)
                    }
                }
                _ => {}
            }
        }
        let mut reserved = BTreeSet::new();
        strings(
            &serde_json::to_value(&snapshot.project).map_err(|e| invalid(e.to_string()))?,
            &mut reserved,
        );
        Ok(Self {
            sequence_id,
            working: snapshot.project.clone(),
            transaction,
            transitions: vec![],
            lineage,
            reserved,
            notes: vec![],
        })
    }

    fn sequence(&self) -> &Sequence {
        self.working
            .base
            .sequences
            .iter()
            .find(|s| s.id == self.sequence_id)
            .expect("validated sequence")
    }
    fn clip(&self, id: &str) -> Result<(String, Clip), EditorError> {
        for track in &self.sequence().tracks {
            if let Some(clip) = track.clips.iter().find(|c| c.id == id) {
                if track.locked {
                    return Err(EditorError::new(
                        "CONSTRAINT_VIOLATION",
                        format!("Track {} is locked", track.id),
                    ));
                }
                return Ok((track.id.clone(), clip.clone()));
            }
        }
        Err(EditorError::new(
            "NOT_FOUND",
            format!("Clip {id} does not exist in this sequence"),
        ))
    }
    fn fresh(&mut self) -> String {
        let mut n = self.reserved.len();
        loop {
            let id = format!("editorial_{n}");
            if self.reserved.insert(id.clone()) {
                return id;
            }
            n += 1;
        }
    }
    fn step(&mut self, operation: VisualOperation) -> Result<(), EditorError> {
        let mut request = self.transaction.clone();
        request.operations = vec![operation.clone()];
        self.working =
            crate::historical_visual::plan_apply(&self.working, &request)?.proposed_project;
        self.transaction.operations.push(operation);
        if self.transaction.operations.len() > 128 {
            return Err(invalid(
                "Editorial plan exceeds the atomic 128-operation limit",
            ));
        }
        Ok(())
    }
    fn base(&mut self, operation: Operation) -> Result<(), EditorError> {
        self.step(VisualOperation::Base { operation })
    }
    fn move_clip(&mut self, id: &str, start: R) -> Result<(), EditorError> {
        let (track, clip) = self.clip(id)?;
        if start.ticks() < 0 {
            return Err(invalid("Move would put a clip before sequence start"));
        }
        if time(&clip.timeline_range.start)?.cmp_exact(start).is_eq() {
            return Ok(());
        }
        self.base(Operation::MoveClip {
            sequence_id: self.sequence_id.to_owned(),
            track_id: track.clone(),
            clip_id: id.into(),
            destination_track_id: track,
            start: start.into(),
        })
    }
    fn split(&mut self, id: &str, at: R) -> Result<String, EditorError> {
        let (track, _) = self.clip(id)?;
        let right = self.fresh();
        self.base(Operation::SplitClip {
            sequence_id: self.sequence_id.to_owned(),
            track_id: track,
            clip_id: id.into(),
            at: at.into(),
            right_clip_id: right.clone(),
        })?;
        for children in self.lineage.values_mut() {
            if let Some(i) = children.iter().position(|value| value == id) {
                children.insert(i + 1, right.clone());
            }
        }
        self.copy_visuals(id, &right)?;
        Ok(right)
    }
    fn copy_visuals(&mut self, id: &str, right: &str) -> Result<(), EditorError> {
        if let Some(visual) = self
            .working
            .visual
            .iter()
            .find(|s| s.sequence_id == self.sequence_id)
            .cloned()
        {
            for mut effect in visual.effects.into_iter().filter(|v| v.target_id == id) {
                effect.id = self.fresh();
                effect.target_id = right.into();
                self.step(VisualOperation::EffectPut {
                    sequence_id: self.sequence_id.to_owned(),
                    effect,
                })?;
            }
            for mut mask in visual.masks.into_iter().filter(|v| v.target_id == id) {
                mask.id = self.fresh();
                mask.target_id = right.into();
                self.step(VisualOperation::MaskPut {
                    sequence_id: self.sequence_id.to_owned(),
                    mask,
                })?;
            }
        }
        Ok(())
    }
    fn remove_target_visuals(&mut self, id: &str) -> Result<(), EditorError> {
        if let Some(visual) = self
            .working
            .visual
            .iter()
            .find(|s| s.sequence_id == self.sequence_id)
            .cloned()
        {
            for effect in visual.effects.iter().filter(|v| v.target_id == id) {
                self.step(VisualOperation::EffectDelete {
                    sequence_id: self.sequence_id.to_owned(),
                    id: effect.id.clone(),
                })?;
            }
            for mask in visual.masks.iter().filter(|v| v.target_id == id) {
                self.step(VisualOperation::MaskDelete {
                    sequence_id: self.sequence_id.to_owned(),
                    id: mask.id.clone(),
                })?;
            }
        }
        Ok(())
    }
    fn delete(&mut self, id: &str) -> Result<(), EditorError> {
        let (track, _) = self.clip(id)?;
        self.remove_target_visuals(id)?;
        self.base(Operation::DeleteClip {
            sequence_id: self.sequence_id.to_owned(),
            track_id: track,
            clip_id: id.into(),
        })?;
        for children in self.lineage.values_mut() {
            children.retain(|v| v != id);
        }
        Ok(())
    }
    /// Crop with existing split semantics, including exact Bezier subdivision.
    /// Extending holds the first/last animation value while source time advances.
    fn resize(&mut self, id: &str, start: R, finish: R) -> Result<String, EditorError> {
        range(start, finish)?;
        let (_, original) = self.clip(id)?;
        let old_start = time(&original.timeline_range.start)?;
        let old_end = end(&original.timeline_range)?;
        if !start.cmp_exact(old_end).is_lt() || !finish.cmp_exact(old_start).is_gt() {
            return Err(invalid(
                "Roll or slide cannot consume a whole neighboring clip",
            ));
        }
        let mut retained = id.to_owned();
        if start.cmp_exact(old_start).is_gt() {
            retained = self.split(&retained, start)?;
            self.delete(id)?;
        }
        let (_, current) = self.clip(&retained)?;
        if finish.cmp_exact(end(&current.timeline_range)?).is_lt() {
            let discarded = self.split(&retained, finish)?;
            self.delete(&discarded)?;
        }
        let (track, mut clip) = self.clip(&retained)?;
        let current_start = time(&clip.timeline_range.start)?;
        let current_end = end(&clip.timeline_range)?;
        if start.cmp_exact(current_start).is_eq() && finish.cmp_exact(current_end).is_eq() {
            return Ok(retained);
        }
        let leading = current_start.checked_sub(start)?;
        let source = clip
            .source_range
            .as_ref()
            .ok_or_else(|| invalid("Roll/slide requires a media source range"))?;
        let rate_top = time(&source.duration)?;
        let rate_bottom = time(&clip.timeline_range.duration)?;
        let source_start =
            time(&source.start)?.checked_sub(leading.checked_scale(rate_top, rate_bottom)?)?;
        let source_finish = end(source)?.checked_add(
            finish
                .checked_sub(current_end)?
                .checked_scale(rate_top, rate_bottom)?,
        )?;
        // Grow first, so all shifted animation keys remain in bounds.
        self.base(Operation::TrimClip {
            sequence_id: self.sequence_id.to_owned(),
            track_id: track.clone(),
            clip_id: retained.clone(),
            timeline_range: range(start, finish)?,
            source_range: Some(range(source_start, source_finish)?),
        })?;
        if leading.ticks() != 0 {
            for channel in &mut clip.channels {
                for key in &mut channel.keyframes {
                    key.time = time(&key.time)?.checked_add(leading)?.into();
                }
                self.base(Operation::SetKeyframeChannel {
                    sequence_id: self.sequence_id.to_owned(),
                    track_id: track.clone(),
                    clip_id: retained.clone(),
                    channel: channel.clone(),
                })?;
            }
        }
        Ok(retained)
    }
    fn ripple(&mut self, start: R, finish: R, insert: bool) -> Result<(), EditorError> {
        let duration = finish.checked_sub(start)?;
        let mut clips: Vec<_> = self
            .sequence()
            .tracks
            .iter()
            .flat_map(|t| t.clips.iter())
            .collect();
        if insert {
            clips.sort_by(|a, b| {
                time(&b.timeline_range.start)
                    .unwrap()
                    .cmp_exact(time(&a.timeline_range.start).unwrap())
            });
        }
        let ids: Vec<_> = clips.iter().map(|c| c.id.clone()).collect();
        for id in ids {
            // Only inspect locks on objects that the operation would affect.
            let original = self
                .sequence()
                .tracks
                .iter()
                .flat_map(|t| t.clips.iter())
                .find(|c| c.id == id)
                .expect("original clip")
                .clone();
            if !end(&original.timeline_range)?.cmp_exact(start).is_gt() {
                continue;
            }
            let _ = self.clip(&id)?;
            if insert {
                let moving = if time(&original.timeline_range.start)?
                    .cmp_exact(start)
                    .is_lt()
                {
                    self.split(&id, start)?
                } else {
                    id
                };
                let (_, clip) = self.clip(&moving)?;
                self.move_clip(
                    &moving,
                    time(&clip.timeline_range.start)?.checked_add(duration)?,
                )?;
            } else {
                if time(&original.timeline_range.start)?
                    .cmp_exact(finish)
                    .is_lt()
                    && end(&original.timeline_range)?.cmp_exact(finish).is_gt()
                {
                    self.split(&id, finish)?;
                }
                let (_, left) = self.clip(&id)?;
                if time(&left.timeline_range.start)?.cmp_exact(start).is_lt()
                    && end(&left.timeline_range)?.cmp_exact(start).is_gt()
                {
                    self.split(&id, start)?;
                }
                let children = self.lineage[&id].clone();
                for child in children {
                    let (_, clip) = self.clip(&child)?;
                    let at = time(&clip.timeline_range.start)?;
                    if !at.cmp_exact(start).is_lt() && at.cmp_exact(finish).is_lt() {
                        self.delete(&child)?;
                    } else if !at.cmp_exact(finish).is_lt() {
                        self.move_clip(&child, at.checked_sub(duration)?)?;
                    }
                }
            }
        }
        let adjust = |value: R| -> Result<R, EditorError> {
            if value.cmp_exact(start).is_lt() {
                Ok(value)
            } else if insert {
                value.checked_add(duration)
            } else if value.cmp_exact(finish).is_lt() {
                Ok(start)
            } else {
                value.checked_sub(duration)
            }
        };
        for mut cue in self.sequence().caption_cues.clone() {
            let old = cue.timeline_range.clone();
            if insert
                && time(&old.start)?.cmp_exact(start).is_lt()
                && end(&old)?.cmp_exact(start).is_gt()
            {
                let mut right = cue.clone();
                right.id = self.fresh();
                right.timeline_range = range(finish, end(&old)?.checked_add(duration)?)?;
                cue.timeline_range = range(time(&old.start)?, start)?;
                self.base(Operation::UpdateCaption {
                    sequence_id: self.sequence_id.to_owned(),
                    cue,
                })?;
                self.base(Operation::InsertCaption {
                    sequence_id: self.sequence_id.to_owned(),
                    cue: right,
                })?;
                continue;
            }
            let a = adjust(time(&old.start)?)?;
            // An interval ending exactly at the insertion stays before the gap.
            let b = if insert && end(&old)?.cmp_exact(start).is_eq() {
                start
            } else {
                adjust(end(&old)?)?
            };
            if !b.cmp_exact(a).is_gt() {
                self.base(Operation::DeleteCaption {
                    sequence_id: self.sequence_id.to_owned(),
                    cue_id: cue.id,
                })?;
            } else {
                cue.timeline_range = range(a, b)?;
                if cue.timeline_range != old {
                    self.base(Operation::UpdateCaption {
                        sequence_id: self.sequence_id.to_owned(),
                        cue,
                    })?;
                }
            }
        }
        if let Some(visual) = self
            .working
            .visual
            .iter()
            .find(|s| s.sequence_id == self.sequence_id)
            .cloned()
        {
            for mut shape in visual.shapes {
                let old = shape.timeline_range.clone();
                if insert
                    && time(&old.start)?.cmp_exact(start).is_lt()
                    && end(&old)?.cmp_exact(start).is_gt()
                {
                    let mut right = shape.clone();
                    right.id = self.fresh();
                    right.timeline_range = range(finish, end(&old)?.checked_add(duration)?)?;
                    shape.timeline_range = range(time(&old.start)?, start)?;
                    let old_id = shape.id.clone();
                    let right_id = right.id.clone();
                    self.step(VisualOperation::ShapePut {
                        sequence_id: self.sequence_id.to_owned(),
                        shape,
                    })?;
                    self.step(VisualOperation::ShapePut {
                        sequence_id: self.sequence_id.to_owned(),
                        shape: right,
                    })?;
                    self.copy_visuals(&old_id, &right_id)?;
                    continue;
                }
                let a = adjust(time(&old.start)?)?;
                let b = if insert && end(&old)?.cmp_exact(start).is_eq() {
                    start
                } else {
                    adjust(end(&old)?)?
                };
                if !b.cmp_exact(a).is_gt() {
                    self.remove_target_visuals(&shape.id)?;
                    self.step(VisualOperation::ShapeDelete {
                        sequence_id: self.sequence_id.to_owned(),
                        id: shape.id,
                    })?;
                } else {
                    shape.timeline_range = range(a, b)?;
                    if shape.timeline_range != old {
                        self.step(VisualOperation::ShapePut {
                            sequence_id: self.sequence_id.to_owned(),
                            shape,
                        })?;
                    }
                }
            }
        }
        Ok(())
    }
    fn restore_transitions(&mut self) -> Result<(), EditorError> {
        for mut transition in self.transitions.clone() {
            let outgoing = self
                .lineage
                .get(&transition.outgoing_clip_id)
                .and_then(|ids| ids.last())
                .cloned();
            let incoming = self
                .lineage
                .get(&transition.incoming_clip_id)
                .and_then(|ids| ids.first())
                .cloned();
            if let (Some(outgoing), Some(incoming)) = (outgoing, incoming) {
                let (_, left) = self.clip(&outgoing)?;
                let (_, right) = self.clip(&incoming)?;
                if end(&left.timeline_range)?
                    .cmp_exact(time(&right.timeline_range.start)?)
                    .is_eq()
                {
                    transition.outgoing_clip_id = outgoing;
                    transition.incoming_clip_id = incoming;
                    self.step(VisualOperation::TransitionPut {
                        sequence_id: self.sequence_id.to_owned(),
                        transition,
                    })?;
                    continue;
                }
            }
            self.notes.push(format!(
                "Transition {} removed because an endpoint was deleted or the clips no longer meet",
                transition.id
            ));
        }
        Ok(())
    }
}

pub fn plan(
    snapshot: &HistoricalVisualSnapshot,
    input: &TimelineEditRequest,
) -> Result<TimelineEditPlan, EditorError> {
    if snapshot.project.base.id != input.project_id || snapshot.revision != input.expected_revision
    {
        return Err(EditorError::new(
            "REVISION_CONFLICT",
            "Editorial plan requires the exact project revision",
        ));
    }
    opencut_editor_core::historical_visual::validate_project_historical_visual(&snapshot.project)?;
    let sequence = snapshot
        .project
        .base
        .sequences
        .iter()
        .find(|s| s.id == input.sequence_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Sequence does not exist"))?;
    if input.idempotency_key.trim().is_empty() {
        return Err(invalid("Idempotency key is required"));
    }
    match &input.command {
        TimelineCommand::RippleDelete { range: r } => {
            range(time(&r.start)?, end(r)?)?;
        }
        TimelineCommand::InsertGap { at, duration } => {
            let a = time(at)?;
            range(a, a.checked_add(time(duration)?)?)?;
        }
        _ => {}
    }
    let mut planner = Planner::new(
        snapshot,
        &input.sequence_id,
        ApplyHistoricalVisualProject {
            project_id: input.project_id.clone(),
            expected_revision: input.expected_revision.clone(),
            idempotency_key: input.idempotency_key.clone(),
            label: input.label.clone(),
            operations: vec![],
        },
    )?;
    if let Some(visual) = planner
        .working
        .visual
        .iter_mut()
        .find(|s| s.sequence_id == input.sequence_id)
    {
        let affects = |id: &str| -> bool {
            match &input.command {
                TimelineCommand::Slip { clip_id, .. } => id == clip_id,
                TimelineCommand::Roll {
                    outgoing_clip_id,
                    incoming_clip_id,
                    ..
                } => id == outgoing_clip_id || id == incoming_clip_id,
                TimelineCommand::MoveGroup { clip_ids, .. } => clip_ids.iter().any(|v| v == id),
                TimelineCommand::Slide { clip_id, .. } => sequence.tracks.iter().any(|track| {
                    track
                        .clips
                        .iter()
                        .find(|c| &c.id == clip_id)
                        .is_some_and(|target| {
                            track
                                .clips
                                .iter()
                                .find(|c| c.id == id)
                                .is_some_and(|candidate| {
                                    id == clip_id
                                        || end(&candidate.timeline_range)
                                            .unwrap()
                                            .cmp_exact(time(&target.timeline_range.start).unwrap())
                                            .is_eq()
                                        || time(&candidate.timeline_range.start)
                                            .unwrap()
                                            .cmp_exact(end(&target.timeline_range).unwrap())
                                            .is_eq()
                                })
                        })
                }),
                TimelineCommand::RippleDelete { range, .. } => sequence
                    .tracks
                    .iter()
                    .flat_map(|t| &t.clips)
                    .find(|c| c.id == id)
                    .is_some_and(|c| {
                        end(&c.timeline_range)
                            .unwrap()
                            .cmp_exact(time(&range.start).unwrap())
                            .is_gt()
                    }),
                TimelineCommand::InsertGap { at, .. } => sequence
                    .tracks
                    .iter()
                    .flat_map(|t| &t.clips)
                    .find(|c| c.id == id)
                    .is_some_and(|c| {
                        end(&c.timeline_range)
                            .unwrap()
                            .cmp_exact(time(at).unwrap())
                            .is_gt()
                    }),
                _ => true,
            }
        };
        let (affected, unaffected) = std::mem::take(&mut visual.transitions)
            .into_iter()
            .partition(|v: &TransitionNode| {
                affects(&v.outgoing_clip_id) || affects(&v.incoming_clip_id)
            });
        planner.transitions = affected;
        visual.transitions = unaffected;
        planner
            .transaction
            .operations
            .extend(
                planner
                    .transitions
                    .iter()
                    .map(|v| VisualOperation::TransitionDelete {
                        sequence_id: input.sequence_id.clone(),
                        id: v.id.clone(),
                    }),
            );
    }
    match &input.command {
        TimelineCommand::RippleDelete { range: removed } => {
            let a = time(&removed.start)?;
            let b = end(removed)?;
            range(a, b)?;
            planner.ripple(a, b, false)?;
        }
        TimelineCommand::InsertGap { at, duration } => {
            let a = time(at)?;
            let b = a.checked_add(time(duration)?)?;
            range(a, b)?;
            planner.ripple(a, b, true)?;
        }
        TimelineCommand::Slip {
            clip_id,
            source_delta,
        } => {
            let delta = nonzero(source_delta)?;
            let (track, mut clip) = planner.clip(clip_id)?;
            let source = clip
                .source_range
                .as_mut()
                .ok_or_else(|| invalid("Slip requires a media source range"))?;
            *source = range(
                time(&source.start)?.checked_add(delta)?,
                end(source)?.checked_add(delta)?,
            )?;
            planner.base(Operation::TrimClip {
                sequence_id: input.sequence_id.clone(),
                track_id: track,
                clip_id: clip_id.clone(),
                timeline_range: clip.timeline_range,
                source_range: clip.source_range,
            })?;
        }
        TimelineCommand::Roll {
            outgoing_clip_id,
            incoming_clip_id,
            delta,
        } => {
            let delta = nonzero(delta)?;
            let (lt, left) = planner.clip(outgoing_clip_id)?;
            let (rt, right) = planner.clip(incoming_clip_id)?;
            if outgoing_clip_id == incoming_clip_id
                || lt != rt
                || !end(&left.timeline_range)?
                    .cmp_exact(time(&right.timeline_range.start)?)
                    .is_eq()
            {
                return Err(invalid(
                    "Roll requires distinct adjacent clips on one track",
                ));
            }
            let cut = end(&left.timeline_range)?.checked_add(delta)?;
            if delta.ticks() > 0 {
                planner.resize(incoming_clip_id, cut, end(&right.timeline_range)?)?;
                planner.resize(outgoing_clip_id, time(&left.timeline_range.start)?, cut)?;
            } else {
                planner.resize(outgoing_clip_id, time(&left.timeline_range.start)?, cut)?;
                planner.resize(incoming_clip_id, cut, end(&right.timeline_range)?)?;
            }
        }
        TimelineCommand::Slide { clip_id, delta } => {
            let delta = nonzero(delta)?;
            let (track, clip) = planner.clip(clip_id)?;
            let clips = &planner
                .sequence()
                .tracks
                .iter()
                .find(|t| t.id == track)
                .unwrap()
                .clips;
            let left: Vec<_> = clips
                .iter()
                .filter(|c| {
                    c.id != *clip_id
                        && end(&c.timeline_range).is_ok_and(|e| {
                            e.cmp_exact(time(&clip.timeline_range.start).unwrap())
                                .is_eq()
                        })
                })
                .cloned()
                .collect();
            let right: Vec<_> = clips
                .iter()
                .filter(|c| {
                    c.id != *clip_id
                        && time(&c.timeline_range.start)
                            .is_ok_and(|s| s.cmp_exact(end(&clip.timeline_range).unwrap()).is_eq())
                })
                .cloned()
                .collect();
            if left.len() != 1 || right.len() != 1 {
                return Err(invalid("Slide requires one adjacent neighbor on each side"));
            }
            let a = time(&clip.timeline_range.start)?.checked_add(delta)?;
            let b = end(&clip.timeline_range)?.checked_add(delta)?;
            if delta.ticks() > 0 {
                planner.resize(&right[0].id, b, end(&right[0].timeline_range)?)?;
                planner.move_clip(clip_id, a)?;
                planner.resize(&left[0].id, time(&left[0].timeline_range.start)?, a)?;
            } else {
                planner.resize(&left[0].id, time(&left[0].timeline_range.start)?, a)?;
                planner.move_clip(clip_id, a)?;
                planner.resize(&right[0].id, b, end(&right[0].timeline_range)?)?;
            }
        }
        TimelineCommand::MoveGroup {
            clip_ids,
            caption_ids,
            shape_ids,
            delta,
        } => {
            let delta = nonzero(delta)?;
            let clips = selection(clip_ids)?;
            let captions = selection(caption_ids)?;
            let shapes = selection(shape_ids)?;
            if clips.is_empty() && captions.is_empty() && shapes.is_empty() {
                return Err(invalid("Sync selection is empty"));
            }
            let mut moving = clips
                .iter()
                .map(|id| planner.clip(id).map(|(_, clip)| clip))
                .collect::<Result<Vec<_>, _>>()?;
            moving.sort_by(|a, b| {
                let order = time(&a.timeline_range.start)
                    .unwrap()
                    .cmp_exact(time(&b.timeline_range.start).unwrap());
                if delta.ticks() > 0 {
                    order.reverse()
                } else {
                    order
                }
            });
            for clip in moving {
                planner.move_clip(
                    &clip.id,
                    time(&clip.timeline_range.start)?.checked_add(delta)?,
                )?;
            }
            for id in captions {
                let mut cue = planner
                    .sequence()
                    .caption_cues
                    .iter()
                    .find(|v| v.id == id)
                    .cloned()
                    .ok_or_else(|| {
                        EditorError::new("NOT_FOUND", "Selected caption does not exist")
                    })?;
                cue.timeline_range = range(
                    time(&cue.timeline_range.start)?.checked_add(delta)?,
                    end(&cue.timeline_range)?.checked_add(delta)?,
                )?;
                planner.base(Operation::UpdateCaption {
                    sequence_id: input.sequence_id.clone(),
                    cue,
                })?;
            }
            for id in shapes {
                let mut shape = planner
                    .working
                    .visual
                    .iter()
                    .filter(|s| s.sequence_id == input.sequence_id)
                    .flat_map(|s| s.shapes.iter())
                    .find(|v| v.id == id)
                    .cloned()
                    .ok_or_else(|| {
                        EditorError::new("NOT_FOUND", "Selected shape does not exist")
                    })?;
                shape.timeline_range = range(
                    time(&shape.timeline_range.start)?.checked_add(delta)?,
                    end(&shape.timeline_range)?.checked_add(delta)?,
                )?;
                planner.step(VisualOperation::ShapePut {
                    sequence_id: input.sequence_id.clone(),
                    shape,
                })?;
            }
        }
        TimelineCommand::ReorderTracks { track_ids } => {
            let selected = selection(track_ids)?;
            let tracks = planner.sequence().tracks.clone();
            if selected != tracks.iter().map(|t| t.id.clone()).collect() {
                return Err(invalid("Track order must be a complete permutation"));
            }
            if tracks.iter().map(|t| &t.id).eq(track_ids.iter()) {
                return Err(EditorError::new("NO_EFFECT", "Track order is unchanged"));
            }
            if tracks.iter().any(|t| t.locked) {
                return Err(EditorError::new(
                    "CONSTRAINT_VIOLATION",
                    "Cannot reorder locked tracks",
                ));
            }
            // The persisted operation set deliberately remains unchanged. Empty
            // then recreate the tracks in one transaction, retaining stable IDs.
            for track in &tracks {
                for clip in &track.clips {
                    planner.transaction.operations.push(VisualOperation::Base {
                        operation: Operation::DeleteClip {
                            sequence_id: input.sequence_id.clone(),
                            track_id: track.id.clone(),
                            clip_id: clip.id.clone(),
                        },
                    });
                }
                planner.transaction.operations.push(VisualOperation::Base {
                    operation: Operation::RemoveTrack {
                        sequence_id: input.sequence_id.clone(),
                        track_id: track.id.clone(),
                    },
                });
            }
            for id in track_ids {
                let mut track = tracks.iter().find(|t| &t.id == id).unwrap().clone();
                let clips = std::mem::take(&mut track.clips);
                planner.transaction.operations.push(VisualOperation::Base {
                    operation: Operation::AddTrack {
                        sequence_id: input.sequence_id.clone(),
                        track,
                    },
                });
                for clip in clips {
                    planner.transaction.operations.push(VisualOperation::Base {
                        operation: Operation::InsertClip {
                            sequence_id: input.sequence_id.clone(),
                            track_id: id.clone(),
                            clip,
                        },
                    });
                }
            }
            for curve in snapshot
                .project
                .keyframe_curves
                .iter()
                .filter(|v| v.sequence_id == input.sequence_id)
            {
                planner
                    .transaction
                    .operations
                    .push(VisualOperation::KeyframeCurveSet {
                        sequence_id: input.sequence_id.clone(),
                        curve: curve.clone(),
                    });
            }
        }
    }
    planner.restore_transitions()?;
    let verified = crate::historical_visual::plan_apply(&snapshot.project, &planner.transaction)?;
    Ok(TimelineEditPlan {
        transaction: planner.transaction,
        created_ids: verified.created_ids,
        changed_ids: verified.changed_ids,
        deleted_ids: verified.deleted_ids,
        notes: planner.notes,
    })
}

pub mod classic;

#[cfg(test)]
mod tests;
