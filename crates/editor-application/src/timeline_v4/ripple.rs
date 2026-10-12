//! Exact port of Classic's per-track vacated-minus-joined interval algorithm.
//! Adjustments apply from latest to earliest; other tracks and scene-time
//! channels/bookmarks/captions do not move with a media track's local gap.
use super::*;
type Interval = (R, R);
fn normalize(mut intervals: Vec<Interval>) -> Vec<Interval> {
    intervals.retain(|(a, b)| b.cmp_exact(*a).is_gt());
    intervals.sort_by(|a, b| a.0.cmp_exact(b.0));
    let mut joined: Vec<Interval> = vec![];
    for (a, b) in intervals {
        if let Some(last) = joined.last_mut()
            && !a.cmp_exact(last.1).is_gt()
        {
            if b.cmp_exact(last.1).is_gt() {
                last.1 = b;
            }
        } else {
            joined.push((a, b));
        }
    }
    joined
}
fn subtract(mut intervals: Vec<Interval>, joined: Vec<Interval>) -> Vec<Interval> {
    for (a, b) in joined {
        intervals = intervals
            .into_iter()
            .flat_map(|(start, end)| {
                if !b.cmp_exact(start).is_gt() || !a.cmp_exact(end).is_lt() {
                    return vec![(start, end)];
                }
                let mut out = vec![];
                if a.cmp_exact(start).is_gt() {
                    out.push((start, a));
                }
                if b.cmp_exact(end).is_lt() {
                    out.push((b, end));
                }
                out
            })
            .collect();
    }
    intervals
}
pub(super) fn apply(
    before: &SequenceV4,
    after: &mut SequenceV4,
    decision: &mut TimelineDecisionV4,
) -> Result<(), EditorError> {
    let all_after: BTreeSet<_> = after
        .tracks
        .iter()
        .flat_map(|t| &t.elements)
        .map(|e| e.id.clone())
        .collect();
    for original in &before.tracks {
        let Some(track) = after.tracks.iter_mut().find(|t| t.id == original.id) else {
            continue;
        };
        let old_ids: BTreeSet<_> = original.elements.iter().map(|e| e.id.as_str()).collect();
        let new_by_id: BTreeMap<_, _> = track.elements.iter().map(|e| (&e.id, e)).collect();
        let mut vacated = vec![];
        for old in &original.elements {
            if let Some(new) = new_by_id.get(&old.id) {
                if end(old)?.cmp_exact(end(new)?).is_gt() {
                    vacated.push((end(new)?, end(old)?));
                }
            } else if !all_after.contains(&old.id) {
                vacated.push((time(&old.timeline_range.start)?, end(old)?));
            }
        }
        let joined = track
            .elements
            .iter()
            .filter(|e| !old_ids.contains(e.id.as_str()))
            .map(|e| Ok((time(&e.timeline_range.start)?, end(e)?)))
            .collect::<Result<Vec<_>, EditorError>>()?;
        let freed = subtract(normalize(vacated), normalize(joined));
        for (a, b) in freed.into_iter().rev() {
            let delta = b.checked_sub(a)?;
            for e in &mut track.elements {
                let start = time(&e.timeline_range.start)?;
                if !start.cmp_exact(b).is_lt() {
                    if track.locked {
                        return Err(EditorError::new(
                            "CONSTRAINT_VIOLATION",
                            "Ripple would alter a locked track",
                        ));
                    }
                    // This batch never silently edits a later unsupported type.
                    media(e)?;
                    let next = start.checked_sub(delta)?;
                    if next.ticks() < 0 {
                        return Err(invalid("Ripple would move an element before scene zero"));
                    }
                    e.timeline_range.start = next.into();
                    if !decision.ripple_shifted_ids.contains(&e.id) {
                        decision.ripple_shifted_ids.push(e.id.clone());
                    }
                }
            }
        }
    }
    Ok(())
}
