use super::*;
use opencut_time::{FrameRounding, align_to_frame, range_end, validate_range};
use std::cmp::Ordering;

fn audio() -> AudioStyle {
    AudioStyle {
        stream: None,
        gain: Gain::Decibels { value: 0.0 },
        muted: false,
    }
}

fn require_audio(asset: &ProjectAsset) -> Result<(), EditorError> {
    match asset.metadata.audio {
        AudioPresence::Present { .. } => Ok(()),
        AudioPresence::Absent {} => Err(EditorError::new(
            "NO_AUDIO_TRACK",
            "Observed media has no audio track",
        )),
        AudioPresence::Unknown {} => Err(EditorError::new(
            "MEDIA_METADATA_REQUIRED",
            "Audio presence must be probed before enabling or placing audio",
        )),
    }
}

fn media_span(
    asset: &ProjectAsset,
    timing: &MediaTiming,
    classic: bool,
) -> Result<(Time, ElementSourceSpan, Retime, TimeRange), EditorError> {
    let (rate, maintain_pitch) = match timing {
        MediaTiming::SourceRange {
            rate,
            maintain_pitch,
            ..
        }
        | MediaTiming::PinnedTimeline {
            rate,
            maintain_pitch,
            ..
        } => (*rate, *maintain_pitch),
    };
    let retime = Retime {
        rate,
        maintain_pitch,
        mapping: if classic {
            RetimeMapping::ClassicRoundedTicks { timebase: 120_000 }
        } else {
            RetimeMapping::ExactRational
        },
    };
    let (range, timeline) = match timing {
        MediaTiming::SourceRange { range, .. } => {
            validate_range(range, true)?;
            let timeline = opencut_editor_core::scene::map_retime_duration_scene(
                time(&range.duration)?,
                &retime,
                true,
            )?;
            (range.clone(), timeline)
        }
        MediaTiming::PinnedTimeline {
            source_start,
            duration,
            ..
        } => {
            let timeline = time(duration)?;
            if timeline.ticks() <= 0 {
                return Err(invalid("Pinned timeline duration must be positive"));
            }
            let source =
                opencut_editor_core::scene::map_retime_duration_scene(timeline, &retime, false)?;
            (
                TimeRange {
                    start: source_start.clone(),
                    duration: source.into(),
                },
                timeline,
            )
        }
    };
    if time(&range.start)?.ticks() < 0
        || timeline.ticks() <= 0
        || time(&range.duration)?.ticks() <= 0
    {
        return Err(invalid(
            "Media source and timeline spans must be positive and nonnegative",
        ));
    }
    let extent = asset.duration.as_ref().ok_or_else(|| {
        EditorError::new(
            "MEDIA_METADATA_REQUIRED",
            "Media placement requires observed source duration",
        )
    })?;
    let end = time(&range_end(&range)?)?;
    let extent_time = time(extent)?;
    let trim_end = if end.cmp_exact(extent_time) == Ordering::Greater {
        // Classic rate edits preserve the timeline-pinned inverse-rounded
        // boundary. Its nominal forward request can exceed the immutable source
        // extent; this is not permission to enlarge that extent or decoder reads.
        let available = extent_time.checked_sub(time(&range.start)?)?;
        if !classic
            || !matches!(timing, MediaTiming::PinnedTimeline { .. })
            || available.ticks() <= 0
            || opencut_editor_core::scene::map_retime_duration_scene(available, &retime, true)?
                .cmp_exact(timeline)
                != Ordering::Equal
        {
            return Err(invalid("Media placement exceeds observed source duration"));
        }
        RationalTime::new(0, 1)?
    } else {
        extent_time.checked_sub(end)?
    };
    let span = if classic {
        ElementSourceSpan::ClassicTrimmed {
            trim_start: range.start.clone(),
            trim_end: trim_end.into(),
            source_duration: Some(extent.clone()),
        }
    } else {
        ElementSourceSpan::LegacySourceRange {
            range: range.clone(),
        }
    };
    Ok((timeline.into(), span, retime, range))
}

pub(super) fn build(
    input: &MediaPlacement,
    track: &SceneTrack,
    occupied: &[TimelineElement],
    main_track: &str,
    asset: &ProjectAsset,
    fps: &FrameRate,
    classic: bool,
) -> Result<(TimelineElement, MediaPlacementDecision), EditorError> {
    if track.locked {
        return Err(EditorError::new(
            "CONSTRAINT_VIOLATION",
            "Media placement target is locked",
        ));
    }
    let aligned = align_to_frame(&input.start, fps, FrameRounding::Nearest)?;
    let mut start = aligned.clone();
    if classic && track.id == main_track {
        let earliest = occupied
            .iter()
            .map(|e| time(&e.timeline_range.start))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .min_by(|a, b| a.cmp_exact(*b));
        if earliest.is_none_or(|earliest| {
            time(&aligned).is_ok_and(|start| start.cmp_exact(earliest) != Ordering::Greater)
        }) {
            start = RationalTime::new(0, 1)?.into();
        }
    }
    let (duration, source_span, content, source_range) = match &input.content {
        MediaPlacementContent::Video {
            timing,
            source_audio_enabled,
        } => {
            if asset.kind != AssetKind::Video || track.kind != SceneTrackKind::Video {
                return Err(invalid(
                    "Video needs video media and an explicit video track",
                ));
            }
            if *source_audio_enabled {
                require_audio(asset)?;
            }
            let (duration, span, retime, range) = media_span(asset, timing, classic)?;
            (
                duration,
                span,
                ElementContent::Video {
                    asset_id: asset.id.clone(),
                    video_stream: None,
                    source_audio_enabled: *source_audio_enabled,
                    audio: audio(),
                    retime,
                },
                Some(range),
            )
        }
        MediaPlacementContent::Audio { timing } => {
            if !matches!(asset.kind, AssetKind::Audio | AssetKind::Video)
                || track.kind != SceneTrackKind::Audio
            {
                return Err(invalid(
                    "Audio needs audio-bearing media and an explicit audio track",
                ));
            }
            require_audio(asset)?;
            let (duration, span, retime, range) = media_span(asset, timing, classic)?;
            (
                duration,
                span,
                ElementContent::Audio {
                    asset_id: asset.id.clone(),
                    audio: audio(),
                    retime,
                },
                Some(range),
            )
        }
        MediaPlacementContent::Image { duration } => {
            if !matches!(asset.kind, AssetKind::Image | AssetKind::Graphic)
                || !matches!(track.kind, SceneTrackKind::Video | SceneTrackKind::Graphic)
            {
                return Err(invalid(
                    "Image needs visual image media and a video or graphic track",
                ));
            }
            if time(duration)?.ticks() <= 0 {
                return Err(invalid("Image duration must be explicitly positive"));
            }
            let zero: Time = RationalTime::new(0, 1)?.into();
            let span = if classic {
                ElementSourceSpan::ClassicTrimmed {
                    trim_start: zero.clone(),
                    trim_end: zero,
                    source_duration: None,
                }
            } else {
                ElementSourceSpan::LegacySourceRange {
                    range: TimeRange {
                        start: zero,
                        duration: duration.clone(),
                    },
                }
            };
            (
                duration.clone(),
                span,
                ElementContent::Image {
                    asset_id: asset.id.clone(),
                },
                None,
            )
        }
    };
    let visual = if matches!(content, ElementContent::Audio { .. }) {
        None
    } else {
        Some(VisualStyle {
            profile: if classic {
                VisualProfile::ClassicCanvasV31
            } else {
                VisualProfile::LegacyCompositionV3
            },
            transform: Transform {
                position_x_px: 0.0,
                position_y_px: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                rotation_degrees: 0.0,
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: vec![],
            masks: vec![],
        })
    };
    let element = TimelineElement {
        id: input.element_id.clone(),
        name: input.name.clone(),
        timeline_range: TimeRange {
            start: start.clone(),
            duration: duration.clone(),
        },
        source_span,
        enabled: true,
        content,
        visual,
        channels: vec![],
    };
    let resolved_source =
        opencut_editor_core::scene::resolve_element_source_request_scene(&element, Some(asset))?;
    let source_range = source_range.map(|_| resolved_source);
    Ok((
        element,
        MediaPlacementDecision {
            element_id: input.element_id.clone(),
            requested_start: input.start.clone(),
            main_track_start_adjusted: start != aligned,
            resolved_start: start,
            timeline_duration: duration,
            source_range,
        },
    ))
}
