use opencut_contracts::{Time, TimeRange};
use opencut_media_contracts::MediaTime;
use opencut_time::{FrameRounding, RationalTime, align_to_frame};
/// Selects a presentation frame inside a half-open project render range.
/// Nearest ties select the later frame.
pub fn select_project_frame_time(
    requested: &Time,
    fps: &opencut_contracts::FrameRate,
    alignment: opencut_media_contracts::FrameAlignment,
    duration: MediaTime,
) -> Result<Time, opencut_contracts::EditorError> {
    let end = RationalTime::new(duration.ticks, duration.timebase)?;
    let requested_exact = RationalTime::try_from(requested)?;
    if requested_exact.ticks() < 0 || requested_exact.cmp_exact(end).is_ge() {
        return Err(opencut_contracts::EditorError::new(
            "INVALID_ARGUMENT",
            "requested project time must be inside the sequence's half-open render range",
        ));
    }
    let rounding = match alignment {
        opencut_media_contracts::FrameAlignment::Exact => FrameRounding::Exact,
        opencut_media_contracts::FrameAlignment::Nearest => FrameRounding::Nearest,
        opencut_media_contracts::FrameAlignment::Floor => FrameRounding::Floor,
        opencut_media_contracts::FrameAlignment::Ceil => FrameRounding::Ceil,
    };
    let selected = align_to_frame(requested, fps, rounding)?;
    if RationalTime::try_from(&selected)?.cmp_exact(end).is_ge() {
        return Err(opencut_contracts::EditorError::new(
            "INVALID_ARGUMENT",
            "aligned frame lies beyond the sequence's half-open render range",
        ));
    }
    Ok(selected)
}

/// Aligns both endpoints of a positive half-open sequence range. The result is
/// never clamped: alignment that collapses or exceeds the sequence is rejected.
pub fn select_project_range(
    requested: &TimeRange,
    fps: &opencut_contracts::FrameRate,
    alignment: opencut_media_contracts::FrameAlignment,
    duration: MediaTime,
) -> Result<TimeRange, opencut_contracts::EditorError> {
    opencut_time::validate_range(requested, true)?;
    let full_end = RationalTime::new(duration.ticks, duration.timebase)?;
    let requested_start = RationalTime::try_from(&requested.start)?;
    let requested_end_time = opencut_time::range_end(requested)?;
    let requested_end = RationalTime::try_from(&requested_end_time)?;
    if requested_start.ticks() < 0 || requested_end.cmp_exact(full_end).is_gt() {
        return Err(opencut_contracts::EditorError::new(
            "INVALID_ARGUMENT",
            "requested project range must be inside the sequence's half-open render range",
        ));
    }
    let rounding = match alignment {
        opencut_media_contracts::FrameAlignment::Exact => FrameRounding::Exact,
        opencut_media_contracts::FrameAlignment::Nearest => FrameRounding::Nearest,
        opencut_media_contracts::FrameAlignment::Floor => FrameRounding::Floor,
        opencut_media_contracts::FrameAlignment::Ceil => FrameRounding::Ceil,
    };
    let selected_start = align_to_frame(&requested.start, fps, rounding)?;
    let selected_end = align_to_frame(&requested_end_time, fps, rounding)?;
    let selected_start_exact = RationalTime::try_from(&selected_start)?;
    let selected_end_exact = RationalTime::try_from(&selected_end)?;
    if selected_start_exact.cmp_exact(selected_end_exact).is_ge() {
        return Err(opencut_contracts::EditorError::new(
            "INVALID_ARGUMENT",
            "aligned project range must contain at least one frame interval",
        ));
    }
    if selected_end_exact.cmp_exact(full_end).is_gt() {
        return Err(opencut_contracts::EditorError::new(
            "INVALID_ARGUMENT",
            "aligned project range extends beyond the sequence render range",
        ));
    }
    Ok(TimeRange {
        start: selected_start,
        duration: selected_end_exact.checked_sub(selected_start_exact)?.into(),
    })
}
