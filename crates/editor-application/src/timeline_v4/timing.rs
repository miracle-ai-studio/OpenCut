//! Classic frame delta snapping followed by one source delta mapping. A split
//! derives right source span from total minus left, never two independent rounds.
use super::*;
use opencut_editor_core::project_v4::{map_retime_duration_v4, resolve_element_source_request_v4};
use opencut_time::{FrameRounding, align_to_frame};

fn min(a: R, b: R) -> R {
    if a.cmp_exact(b).is_lt() { a } else { b }
}
fn max(a: R, b: R) -> R {
    if a.cmp_exact(b).is_gt() { a } else { b }
}
fn clamp(v: R, lo: R, hi: Option<R>) -> R {
    hi.map_or(max(v, lo), |hi| min(max(v, lo), hi))
}
// Signed Math.round ties select the later tick, including negative half-ticks.
fn rounded(top: i128, bottom: i128) -> Result<i64, EditorError> {
    let floor = top.div_euclid(bottom);
    let rem = top.rem_euclid(bottom);
    i64::try_from(floor + i128::from(rem * 2 >= bottom))
        .map_err(|_| invalid("Timeline rounding overflows"))
}
// Classic roundMediaTime rounds tick magnitudes half away from zero.
fn tick(v: R) -> Result<R, EditorError> {
    let top = i128::from(v.ticks()) * 120000;
    let magnitude = rounded(top.abs(), i128::from(v.timebase()))?;
    R::new(if top < 0 { -magnitude } else { magnitude }, 120000)
}
// Classic FrameRate::ticks_per_frame requires an integer 120000-tick
// frame. TS roundFrameTicks falls back to its original tick value otherwise.
fn classic_frame_grid(fps: &FrameRate) -> bool {
    fps.numerator != 0
        && (120000_u64 * u64::from(fps.denominator)).is_multiple_of(u64::from(fps.numerator))
}
pub(super) fn fps<'a>(
    project: &'a ProjectDocumentV4,
    scene: &str,
) -> Result<(&'a FrameRate, bool), EditorError> {
    Ok(match &project.settings {
        SettingsProfileV4::ClassicGlobal { settings } => (&settings.fps, true),
        SettingsProfileV4::LegacyPerScene { formats } => (
            &formats
                .iter()
                .find(|f| f.sequence_id == scene)
                .ok_or_else(|| invalid("Scene format is missing"))?
                .fps,
            false,
        ),
    })
}
pub(super) fn align(v: &Time, fps: &FrameRate, classic: bool) -> Result<R, EditorError> {
    if classic && !classic_frame_grid(fps) {
        let value = time(v)?;
        if value.ticks() < 0 {
            return Err(invalid("Timeline start must be nonnegative"));
        }
        return tick(value);
    }
    let result = time(&align_to_frame(v, fps, FrameRounding::Nearest)?)?;
    if classic { tick(result) } else { Ok(result) }
}
fn snap_delta(v: R, fps: &FrameRate, classic: bool) -> Result<R, EditorError> {
    if classic && !classic_frame_grid(fps) {
        return tick(v);
    }
    let count = rounded(
        i128::from(v.ticks()) * i128::from(fps.numerator),
        i128::from(v.timebase()) * i128::from(fps.denominator),
    )?;
    let result = R::new(count, 1)?.checked_scale(
        R::new(i64::from(fps.denominator), 1)?,
        R::new(i64::from(fps.numerator), 1)?,
    )?;
    if classic { tick(result) } else { Ok(result) }
}
fn retime(e: &TimelineElementV4) -> Option<&RetimeV4> {
    match &e.content {
        ElementContentV4::Audio { retime, .. } | ElementContentV4::Video { retime, .. } => {
            Some(retime)
        }
        _ => None,
    }
}
fn map(v: R, e: &TimelineElementV4, inverse: bool) -> Result<R, EditorError> {
    let negative = v.ticks() < 0;
    let positive = if negative { zero().checked_sub(v)? } else { v };
    let result = if let Some(rate) = retime(e) {
        map_retime_duration_v4(positive, rate, inverse)?
    } else {
        positive
    };
    if negative {
        zero().checked_sub(result)
    } else {
        Ok(result)
    }
}
fn asset<'a>(
    project: &'a ProjectDocumentV4,
    e: &TimelineElementV4,
) -> Result<Option<&'a AssetV4>, EditorError> {
    let id = match &e.content {
        ElementContentV4::Video { asset_id, .. }
        | ElementContentV4::Audio { asset_id, .. }
        | ElementContentV4::Image { asset_id } => asset_id,
        ElementContentV4::Text { .. } => return Ok(None),
        _ => return Err(unsupported("Timeline timing requires media or free text")),
    };
    project
        .assets
        .iter()
        .find(|a| a.id == *id)
        .map(Some)
        .ok_or_else(|| missing("Media asset is missing"))
}

pub(super) fn trim(
    project: &ProjectDocumentV4,
    scene: &SequenceV4,
    track_index: usize,
    e: &TimelineElementV4,
    edge: TimelineTrimEdgeV4,
    boundary: &Time,
) -> Result<TimelineElementV4, EditorError> {
    let (fps, classic) = fps(project, &scene.id)?;
    let start = time(&e.timeline_range.start)?;
    let old_end = end(e)?;
    let duration = time(&e.timeline_range.duration)?;
    let raw = time(boundary)?;
    if raw.ticks() < 0 {
        return Err(invalid("Trim boundary must be nonnegative"));
    }
    let min_duration = if classic {
        tick(R::new(i64::from(fps.denominator), fps.numerator)?)?
    } else {
        R::new(i64::from(fps.denominator), fps.numerator)?
    };
    if duration.cmp_exact(min_duration).is_lt() {
        return Err(unsupported(
            "A sub-frame historical element needs an explicit resize profile",
        ));
    }
    let source = resolve_element_source_request_v4(e, asset(project, e)?)?;
    let source_start = time(&source.start)?;
    let source_visible = time(&source.duration)?;
    let mut lo;
    let mut hi;
    let delta = match edge {
        TimelineTrimEdgeV4::Left => raw.checked_sub(start)?,
        TimelineTrimEdgeV4::Right => raw.checked_sub(old_end)?,
    };
    let other = &scene.tracks[track_index].elements;
    match edge {
        TimelineTrimEdgeV4::Left => {
            let neighbor = other
                .iter()
                .filter(|other| other.id != e.id)
                .map(end)
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .filter(|end| !end.cmp_exact(start).is_gt())
                .max_by(|a, b| a.cmp_exact(*b))
                .unwrap_or(zero());
            lo = neighbor.checked_sub(start)?;
            hi = Some(duration.checked_sub(min_duration)?);
            let bounded = match &e.source_span {
                ElementSourceSpanV4::ClassicTrimmed {
                    source_duration, ..
                } => source_duration.is_some(),
                ElementSourceSpanV4::LegacySourceRange { .. } => true,
            };
            if bounded {
                let extension = map(source_visible.checked_add(source_start)?, e, true)?
                    .checked_sub(duration)?;
                lo = max(lo, zero().checked_sub(extension)?);
            }
        }
        TimelineTrimEdgeV4::Right => {
            lo = min_duration.checked_sub(duration)?;
            hi = other
                .iter()
                .filter(|other| other.id != e.id)
                .map(|other| time(&other.timeline_range.start))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .filter(|other| !other.cmp_exact(old_end).is_lt())
                .min_by(|a, b| a.cmp_exact(*b))
                .map(|n| n.checked_sub(old_end))
                .transpose()?;
            let extent = match &e.source_span {
                ElementSourceSpanV4::ClassicTrimmed {
                    source_duration, ..
                } => source_duration.as_ref().map(time).transpose()?,
                ElementSourceSpanV4::LegacySourceRange { .. } => asset(project, e)?
                    .and_then(|a| a.duration.as_ref())
                    .map(time)
                    .transpose()?,
            };
            if let Some(extent) = extent {
                let cap = map(extent.checked_sub(source_start)?, e, true)?.checked_sub(duration)?;
                hi = Some(hi.map_or(cap, |n| min(n, cap)));
            }
        }
    }
    if hi.is_some_and(|hi| hi.cmp_exact(lo).is_lt()) {
        return Err(invalid("Trim constraints have no legal interval"));
    }
    let delta = clamp(snap_delta(clamp(delta, lo, hi), fps, classic)?, lo, hi);
    if delta.ticks() == 0 {
        return Ok(e.clone());
    }
    no_animation(e)?;
    let source_delta = map(delta, e, false)?;
    let mut result = e.clone();
    let new_duration = match edge {
        TimelineTrimEdgeV4::Left => duration.checked_sub(delta)?,
        TimelineTrimEdgeV4::Right => duration.checked_add(delta)?,
    };
    result.timeline_range.duration = new_duration.into();
    if edge == TimelineTrimEdgeV4::Left {
        result.timeline_range.start = start.checked_add(delta)?.into();
    }
    match &mut result.source_span {
        ElementSourceSpanV4::ClassicTrimmed {
            trim_start,
            trim_end,
            ..
        } => match edge {
            TimelineTrimEdgeV4::Left => {
                *trim_start = max(zero(), time(trim_start)?.checked_add(source_delta)?).into()
            }
            TimelineTrimEdgeV4::Right => {
                *trim_end = max(zero(), time(trim_end)?.checked_sub(source_delta)?).into()
            }
        },
        ElementSourceSpanV4::LegacySourceRange { range } => {
            if edge == TimelineTrimEdgeV4::Left {
                range.start = source_start.checked_add(source_delta)?.into();
            }
            range.duration = map(new_duration, e, false)?.into();
        }
    }
    // Classic update pipeline pins the earliest main element to zero. The
    // duration/source patch remains based on the snapped requested drag delta.
    if classic
        && edge == TimelineTrimEdgeV4::Left
        && scene.tracks[track_index].id == scene.main_track_id
    {
        let earliest = other
            .iter()
            .filter(|other| other.id != e.id)
            .map(|e| time(&e.timeline_range.start))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .min_by(|a, b| a.cmp_exact(*b));
        if earliest.is_none_or(|n| {
            !time(&result.timeline_range.start).is_ok_and(|s| s.cmp_exact(n).is_gt())
        }) {
            result.timeline_range.start = zero().into();
        }
    }
    Ok(result)
}

pub(super) fn split(
    project: &ProjectDocumentV4,
    e: &TimelineElementV4,
    at: R,
) -> Result<(TimelineElementV4, TimelineElementV4), EditorError> {
    no_animation(e)?;
    let original = resolve_element_source_request_v4(e, asset(project, e)?)?;
    let left_duration = at.checked_sub(time(&e.timeline_range.start)?)?;
    let right_duration = time(&e.timeline_range.duration)?.checked_sub(left_duration)?;
    // Classic generated text still carries source trims. Round the source
    // split once and derive its complement from the rounded total, while the
    // requested timeline point and durations remain exact. Keep frozen media
    // and historical LegacySourceRange behavior unchanged.
    let classic_text = matches!(e.content, ElementContentV4::Text { .. })
        && matches!(e.source_span, ElementSourceSpanV4::ClassicTrimmed { .. });
    let left_source = map(left_duration, e, false)?;
    let total_source = time(&original.duration)?;
    let (left_source, total_source) = if classic_text {
        (tick(left_source)?, tick(total_source)?)
    } else {
        (left_source, total_source)
    };
    let right_source = total_source.checked_sub(left_source)?;
    if left_source.ticks() <= 0 || right_source.ticks() <= 0 {
        return Err(invalid("Rounded split must retain positive source spans"));
    }
    let mut left = e.clone();
    let mut right = e.clone();
    left.timeline_range.duration = left_duration.into();
    right.timeline_range.start = at.into();
    right.timeline_range.duration = right_duration.into();
    match (&mut left.source_span, &mut right.source_span) {
        (
            ElementSourceSpanV4::ClassicTrimmed { trim_end, .. },
            ElementSourceSpanV4::ClassicTrimmed { trim_start, .. },
        ) => {
            *trim_end = time(trim_end)?.checked_add(right_source)?.into();
            *trim_start = time(trim_start)?.checked_add(left_source)?.into();
        }
        (
            ElementSourceSpanV4::LegacySourceRange { range: left },
            ElementSourceSpanV4::LegacySourceRange { range: right },
        ) => {
            left.duration = left_source.into();
            right.start = time(&original.start)?.checked_add(left_source)?.into();
            right.duration = right_source.into();
        }
        _ => unreachable!(),
    }
    left.name = format!("{} (left)", e.name);
    right.name = format!("{} (right)", e.name);
    Ok((left, right))
}
