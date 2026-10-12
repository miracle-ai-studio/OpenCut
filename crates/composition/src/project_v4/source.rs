//! Request clocks and bounded resource reads are separate execution facts.
use super::audio::classic_effective_rate_v4;
use super::*;
use opencut_editor_core::project_v4::resolve_element_source_request_v4;

#[derive(Clone, Debug, PartialEq)]
pub struct SourceClockV4 {
    pub timeline_local: Time,
    pub source_time: Time,
    /// Point sampling identity, independent of the authored request mapping.
    pub point_profile: SourcePointProfileV4,
    /// The rate used for this point; the authored request is not rewritten.
    pub rate: RationalRateV4,
    pub mapping: RetimeMappingV4,
    /// The preserved quantized request, possibly longer than a declared extent.
    pub nominal_request: TimeRange,
    /// Includes explicitly validated legacy transition handles, when active.
    pub requested_read_span: TimeRange,
    pub declared_resize_extent: Option<Time>,
    pub read_window: BoundedReadWindowV4,
    pub sample: SourceReadStateV4,
    /// Pure compilation does not invent verified video hold or audio silence.
    pub eof_policy: EofPolicyV4,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourcePointProfileV4 {
    ExactRational,
    ClassicVideoRoundedTicks,
}
#[derive(Clone, Debug, PartialEq)]
pub enum BoundedReadWindowV4 {
    Known {
        resource_extent: Time,
        range: Option<TimeRange>,
    },
    UnknownResourceExtent,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SourceReadStateV4 {
    InBounds,
    BeforeResource,
    EndOfResource,
    OutsideRequestedWindow,
    NeedsResourceProbe,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EofPolicyV4 {
    RequiresBackendProfile,
}

pub(super) fn clock(
    element: &TimelineElementV4,
    asset: &AssetV4,
    local: &Time,
    head: RationalTime,
    tail: RationalTime,
) -> Result<SourceClockV4, CompositionError> {
    clock_with_profile(
        element,
        asset,
        local,
        head,
        tail,
        SourcePointProfileV4::ExactRational,
    )
}

pub(super) fn visual_clock(
    element: &TimelineElementV4,
    asset: &AssetV4,
    local: &Time,
    head: RationalTime,
    tail: RationalTime,
) -> Result<SourceClockV4, CompositionError> {
    let profile = if element
        .visual
        .as_ref()
        .is_some_and(|v| v.profile == VisualProfileV4::ClassicCanvasV31)
    {
        SourcePointProfileV4::ClassicVideoRoundedTicks
    } else {
        SourcePointProfileV4::ExactRational
    };
    clock_with_profile(element, asset, local, head, tail, profile)
}

fn clock_with_profile(
    element: &TimelineElementV4,
    asset: &AssetV4,
    local: &Time,
    head: RationalTime,
    tail: RationalTime,
    point_profile: SourcePointProfileV4,
) -> Result<SourceClockV4, CompositionError> {
    let retime = match &element.content {
        ElementContentV4::Video { retime, .. } | ElementContentV4::Audio { retime, .. } => retime,
        _ => {
            return Err(unsupported(
                "Nonmedia node cannot request a media source clock",
            ));
        }
    };
    let rate = match point_profile {
        SourcePointProfileV4::ExactRational => retime.rate,
        SourcePointProfileV4::ClassicVideoRoundedTicks => classic_effective_rate_v4(retime.rate)?,
    };
    let nominal_request = resolve_element_source_request_v4(element, Some(asset)).map_err(error)?;
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
        SourcePointProfileV4::ExactRational => source_time,
        SourcePointProfileV4::ClassicVideoRoundedTicks => {
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
            BoundedReadWindowV4::UnknownResourceExtent,
            SourceReadStateV4::NeedsResourceProbe,
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
                SourceReadStateV4::BeforeResource
            } else if source_time.cmp_exact(extent_time) != Ordering::Less {
                SourceReadStateV4::EndOfResource
            } else if !match &range {
                Some(range) => contains(range, source_time)?,
                None => false,
            } {
                SourceReadStateV4::OutsideRequestedWindow
            } else {
                SourceReadStateV4::InBounds
            };
            (
                BoundedReadWindowV4::Known {
                    resource_extent: extent.clone(),
                    range,
                },
                sample,
            )
        }
    };
    Ok(SourceClockV4 {
        timeline_local: local.clone(),
        source_time: source_time.into(),
        point_profile,
        rate,
        mapping: retime.mapping,
        nominal_request,
        requested_read_span,
        declared_resize_extent: match &element.source_span {
            ElementSourceSpanV4::ClassicTrimmed {
                source_duration, ..
            } => source_duration.clone(),
            ElementSourceSpanV4::LegacySourceRange { .. } => None,
        },
        read_window,
        sample,
        eof_policy: EofPolicyV4::RequiresBackendProfile,
    })
}
