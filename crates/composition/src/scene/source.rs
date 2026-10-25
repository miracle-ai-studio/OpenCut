//! Request clocks and bounded resource reads are separate execution facts.
use super::audio::classic_effective_rate_scene;
use super::*;
use opencut_editor_core::scene::resolve_element_source_request_scene;

#[derive(Clone, Debug, PartialEq)]
pub struct SourceClock {
    pub timeline_local: Time,
    pub source_time: Time,
    /// Point sampling identity, independent of the authored request mapping.
    pub point_profile: SourcePointProfile,
    /// The rate used for this point; the authored request is not rewritten.
    pub rate: RationalRate,
    pub mapping: RetimeMapping,
    /// The preserved quantized request, possibly longer than a declared extent.
    pub nominal_request: TimeRange,
    /// Includes explicitly validated legacy transition handles, when active.
    pub requested_read_span: TimeRange,
    pub declared_resize_extent: Option<Time>,
    pub read_window: BoundedReadWindow,
    pub sample: SourceReadState,
    /// Pure compilation does not invent verified video hold or audio silence.
    pub eof_policy: EofPolicy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourcePointProfile {
    ExactRational,
    ClassicVideoRoundedTicks,
}
#[derive(Clone, Debug, PartialEq)]
pub enum BoundedReadWindow {
    Known {
        resource_extent: Time,
        range: Option<TimeRange>,
    },
    UnknownResourceExtent,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SourceReadState {
    InBounds,
    BeforeResource,
    EndOfResource,
    OutsideRequestedWindow,
    NeedsResourceProbe,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EofPolicy {
    RequiresBackendProfile,
}

pub(super) fn clock(
    element: &TimelineElement,
    asset: &ProjectAsset,
    local: &Time,
    head: RationalTime,
    tail: RationalTime,
) -> Result<SourceClock, CompositionError> {
    clock_with_profile(
        element,
        asset,
        local,
        head,
        tail,
        SourcePointProfile::ExactRational,
    )
}

pub(super) fn visual_clock(
    element: &TimelineElement,
    asset: &ProjectAsset,
    local: &Time,
    head: RationalTime,
    tail: RationalTime,
) -> Result<SourceClock, CompositionError> {
    let profile = if element
        .visual
        .as_ref()
        .is_some_and(|v| v.profile == VisualProfile::ClassicCanvasV31)
    {
        SourcePointProfile::ClassicVideoRoundedTicks
    } else {
        SourcePointProfile::ExactRational
    };
    clock_with_profile(element, asset, local, head, tail, profile)
}

fn clock_with_profile(
    element: &TimelineElement,
    asset: &ProjectAsset,
    local: &Time,
    head: RationalTime,
    tail: RationalTime,
    point_profile: SourcePointProfile,
) -> Result<SourceClock, CompositionError> {
    let retime = match &element.content {
        ElementContent::Video { retime, .. } | ElementContent::Audio { retime, .. } => retime,
        _ => {
            return Err(unsupported(
                "Nonmedia node cannot request a media source clock",
            ));
        }
    };
    let rate = match point_profile {
        SourcePointProfile::ExactRational => retime.rate,
        SourcePointProfile::ClassicVideoRoundedTicks => classic_effective_rate_scene(retime.rate)?,
    };
    let nominal_request =
        resolve_element_source_request_scene(element, Some(asset)).map_err(error)?;
    let offset = time(local)?
        .checked_scale(
            RationalTime::new(i64::from(rate.numerator), 1).map_err(error)?,
            RationalTime::new(i64::from(rate.denominator), 1).map_err(error)?,
        )
        .map_err(error)?;
    let source_time = time(&nominal_request.start)?
        .checked_add(offset)
        .map_err(error)?;
    // Classic resolveVideoNode/resolveBackdropSource round the entire source
    // point after adding trimStart. Audio retains its separate exact clock.
    let source_time = match point_profile {
        SourcePointProfile::ExactRational => source_time,
        SourcePointProfile::ClassicVideoRoundedTicks => {
            let top = i128::from(source_time.ticks()) * 120000;
            let bottom = i128::from(source_time.timebase());
            let magnitude = top.abs();
            let rounded = magnitude / bottom + i128::from((magnitude % bottom) * 2 >= bottom);
            let signed = if top < 0 { -rounded } else { rounded };
            RationalTime::new(
                i64::try_from(signed)
                    .map_err(|_| invalid("Classic video source point overflows"))?,
                120000,
            )
            .map_err(error)?
        }
    };
    // Transition validation permits only 1:1 source mappings; these are exact
    // source handles, not another inferred retime quantization operation.
    let start = time(&nominal_request.start)?
        .checked_sub(head)
        .map_err(error)?;
    let duration = time(&nominal_request.duration)?
        .checked_add(head)
        .map_err(error)?
        .checked_add(tail)
        .map_err(error)?;
    let requested_read_span = TimeRange {
        start: start.into(),
        duration: duration.into(),
    };
    let (read_window, sample) = match &asset.duration {
        None => (
            BoundedReadWindow::UnknownResourceExtent,
            SourceReadState::NeedsResourceProbe,
        ),
        Some(extent) => {
            let extent_time = time(extent)?;
            let zero = RationalTime::new(0, 1).map_err(error)?;
            let bounded_start = if start.cmp_exact(zero) == Ordering::Less {
                zero
            } else {
                start
            };
            let requested_end = end(&requested_read_span)?;
            let bounded_end = if requested_end.cmp_exact(extent_time) == Ordering::Greater {
                extent_time
            } else {
                requested_end
            };
            let range = if bounded_start.cmp_exact(bounded_end) == Ordering::Less {
                Some(TimeRange {
                    start: bounded_start.into(),
                    duration: bounded_end
                        .checked_sub(bounded_start)
                        .map_err(error)?
                        .into(),
                })
            } else {
                None
            };
            let sample = if source_time.ticks() < 0 {
                SourceReadState::BeforeResource
            } else if source_time.cmp_exact(extent_time) != Ordering::Less {
                SourceReadState::EndOfResource
            } else if !match &range {
                Some(range) => contains(range, source_time)?,
                None => false,
            } {
                SourceReadState::OutsideRequestedWindow
            } else {
                SourceReadState::InBounds
            };
            (
                BoundedReadWindow::Known {
                    resource_extent: extent.clone(),
                    range,
                },
                sample,
            )
        }
    };
    Ok(SourceClock {
        timeline_local: local.clone(),
        source_time: source_time.into(),
        point_profile,
        rate,
        mapping: retime.mapping,
        nominal_request,
        requested_read_span,
        declared_resize_extent: match &element.source_span {
            ElementSourceSpan::ClassicTrimmed {
                source_duration, ..
            } => source_duration.clone(),
            ElementSourceSpan::LegacySourceRange { .. } => None,
        },
        read_window,
        sample,
        eof_policy: EofPolicy::RequiresBackendProfile,
    })
}
