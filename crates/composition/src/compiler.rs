use crate::*;
use opencut_contracts::{Asset, AssetKind, Clip, TimeRange, TrackKind};
use opencut_media_contracts::composition::*;
use opencut_time::RationalTime;
use std::collections::HashMap;
fn error(code: &'static str, message: impl Into<String>) -> CompositionError {
    CompositionError {
        code,
        message: message.into(),
    }
}
fn capability(message: impl Into<String>) -> CompositionError {
    error("CAPABILITY_UNAVAILABLE", message)
}
pub fn compile(
    snapshot: &ProjectSnapshot,
    sequence_id: &str,
    scope: CompositionScope,
) -> Result<Composition, CompositionError> {
    opencut_editor_core::validate_project(&snapshot.project)
        .map_err(|e| error("INVALID_PROJECT", e.to_string()))?;
    let sequence = snapshot
        .project
        .sequences
        .iter()
        .find(|s| s.id == sequence_id)
        .ok_or_else(|| error("NOT_FOUND", "sequence does not exist"))?;
    let assets: HashMap<_, _> = snapshot
        .project
        .assets
        .iter()
        .map(|a| (a.id.as_str(), a))
        .collect();
    let (video_clips, image_clips, mut audio_clips, mut end) =
        compile_visual_clips(sequence, &assets, scope == CompositionScope::AudioVisual)?;
    for track in sequence
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Audio && t.enabled && !t.muted)
    {
        if scope == CompositionScope::Visual {
            for clip in &track.clips {
                include_end(&mut end, &clip.timeline_range)?;
            }
            continue;
        }
        validate_params(&track.params, &["volume"], "audio track")?;
        let track_gain = volume(&track.params)?;
        for clip in &track.clips {
            validate_channels(clip, &["volume"])?;
            validate_params(&clip.params, &["volume"], "audio clip")?;
            let asset = clip_asset(&assets, clip)?;
            if !matches!(&asset.kind, AssetKind::Audio | AssetKind::Video) {
                return Err(error(
                    "INVALID_PROJECT",
                    "audio track clip must reference audio or video",
                ));
            }
            let source = clip
                .source_range
                .as_ref()
                .ok_or_else(|| error("INVALID_PROJECT", "media clips require source_range"))?;
            audio_clips.push(TimelineAudioClip {
                input: SourceRef {
                    asset_id: asset.id.clone(),
                    version: asset.version.clone(),
                },
                source_start: time(&source.start)?,
                source_duration: time(&source.duration)?,
                timeline_start: time(&clip.timeline_range.start)?,
                timeline_duration: time(&clip.timeline_range.duration)?,
                gain: track_gain * volume(&clip.params)?,
                optional: false,
                stream: None,
                gain_curve: curve(clip, "volume", track_gain as f64)?,
            });
            include_end(&mut end, &clip.timeline_range)?;
        }
    }
    for track in sequence.tracks.iter().filter(|t| {
        t.enabled
            && !matches!(
                &t.kind,
                TrackKind::Video | TrackKind::Audio | TrackKind::Caption | TrackKind::Graphic
            )
    }) {
        if !track.clips.is_empty() {
            return Err(capability(format!(
                "track kind {:?} rendering is not implemented",
                track.kind
            )));
        }
    }
    let captions = compile_captions(sequence, &mut end)?;
    let end = end.ok_or_else(|| error("INVALID_PROJECT", "sequence has no renderable duration"))?;
    if end.ticks() <= 0 {
        return Err(error(
            "INVALID_PROJECT",
            "sequence has no renderable duration",
        ));
    }
    Ok(Composition {
        format: CompositionFormat::V1,
        revision: snapshot.revision.clone(),
        sequence_id: sequence.id.clone(),
        scope,
        timeline: Timeline {
            width: sequence.width,
            height: sequence.height,
            frame_rate: FrameRate {
                numerator: sequence.fps.numerator,
                denominator: sequence.fps.denominator,
            },
            duration: MediaTime {
                ticks: end.ticks(),
                timebase: end.timebase(),
            },
            video_clips,
            image_clips,
            audio_clips,
            captions,
        },
    })
}
pub fn map_font_policy(
    policy: &opencut_contracts::FontResolutionPolicy,
) -> opencut_media_contracts::FontResolutionPolicy {
    match policy {
        opencut_contracts::FontResolutionPolicy::Strict {} => {
            opencut_media_contracts::FontResolutionPolicy::Strict {}
        }
        opencut_contracts::FontResolutionPolicy::ExplicitFallback { families } => {
            opencut_media_contracts::FontResolutionPolicy::ExplicitFallback {
                families: families.clone(),
            }
        }
    }
}

pub fn map_text_align(align: opencut_contracts::TextAlign) -> opencut_media_contracts::TextAlign {
    match align {
        opencut_contracts::TextAlign::Left => opencut_media_contracts::TextAlign::Left,
        opencut_contracts::TextAlign::Center => opencut_media_contracts::TextAlign::Center,
        opencut_contracts::TextAlign::Right => opencut_media_contracts::TextAlign::Right,
    }
}

pub fn time(value: &Time) -> Result<MediaTime, CompositionError> {
    let ticks = value
        .ticks
        .parse()
        .map_err(|_| error("INVALID_PROJECT", "time ticks are not i64"))?;
    MediaTime::new(ticks, value.timebase).map_err(|e| error("INVALID_PROJECT", e))
}
type CompiledVisualClips = (
    Vec<TimelineVideoClip<SourceRef>>,
    Vec<TimelineImageClip<SourceRef>>,
    Vec<TimelineAudioClip<SourceRef>>,
    Option<RationalTime>,
);

fn compile_visual_clips(
    sequence: &opencut_contracts::Sequence,
    assets: &HashMap<&str, &Asset>,
    include_video_audio: bool,
) -> Result<CompiledVisualClips, CompositionError> {
    let mut video_clips = Vec::new();
    let mut image_clips = Vec::new();
    let mut audio_clips = Vec::new();
    let mut end = None;
    for (track_index, track) in sequence
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, track)| track.kind == TrackKind::Video && track.enabled)
    {
        validate_params(&track.params, &["volume", "opacity"], "video track")?;
        let track_gain = if include_video_audio {
            volume(&track.params)?
        } else {
            1.0
        };
        let track_opacity = opacity(&track.params)?;
        for clip in &track.clips {
            validate_channels(clip, VIDEO_PARAMS)?;
            validate_params(&clip.params, VIDEO_PARAMS, "video clip")?;
            let asset = clip_asset(assets, clip)?;
            if asset.kind != AssetKind::Video {
                return Err(error(
                    "INVALID_PROJECT",
                    "video track clip must reference a video asset",
                ));
            }
            let source = clip
                .source_range
                .as_ref()
                .ok_or_else(|| error("INVALID_PROJECT", "media clips require source_range"))?;
            let input = SourceRef {
                asset_id: asset.id.clone(),
                version: asset.version.clone(),
            };
            video_clips.push(TimelineVideoClip {
                input: input.clone(),
                source_start: time(&source.start)?,
                source_duration: time(&source.duration)?,
                timeline_start: time(&clip.timeline_range.start)?,
                timeline_duration: time(&clip.timeline_range.duration)?,
                stream: None,
                opacity: track_opacity * opacity(&clip.params)?,
                position_x: number(&clip.params, "position_x", 0.0)?,
                position_y: number(&clip.params, "position_y", 0.0)?,
                scale_x: number(&clip.params, "scale_x", 1.0)?,
                scale_y: number(&clip.params, "scale_y", 1.0)?,
                rotation_degrees: number(&clip.params, "rotation_degrees", 0.0)?,
                z_index: i32::try_from(track_index)
                    .map_err(|_| capability("too many visual tracks"))?,
                curves: visual_curves(clip, track_opacity as f64)?,
            });
            if include_video_audio && !track.muted {
                audio_clips.push(TimelineAudioClip {
                    input,
                    source_start: time(&source.start)?,
                    source_duration: time(&source.duration)?,
                    timeline_start: time(&clip.timeline_range.start)?,
                    timeline_duration: time(&clip.timeline_range.duration)?,
                    gain: track_gain * volume(&clip.params)?,
                    optional: true,
                    stream: None,
                    gain_curve: curve(clip, "volume", track_gain as f64)?,
                });
            }
            include_end(&mut end, &clip.timeline_range)?;
        }
    }
    for (track_index, track) in sequence
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, track)| track.kind == TrackKind::Graphic && track.enabled)
    {
        validate_params(&track.params, &["opacity"], "graphic track")?;
        let track_opacity = opacity(&track.params)?;
        for clip in &track.clips {
            validate_channels(clip, VISUAL_PARAMS)?;
            validate_params(&clip.params, VISUAL_PARAMS, "image clip")?;
            let asset = clip_asset(assets, clip)?;
            if asset.kind != AssetKind::Image {
                return Err(capability(
                    "only image assets are implemented on graphic tracks",
                ));
            }
            image_clips.push(TimelineImageClip {
                input: SourceRef {
                    asset_id: asset.id.clone(),
                    version: asset.version.clone(),
                },
                timeline_start: time(&clip.timeline_range.start)?,
                timeline_duration: time(&clip.timeline_range.duration)?,
                stream: None,
                opacity: track_opacity * opacity(&clip.params)?,
                position_x: number(&clip.params, "position_x", 0.0)?,
                position_y: number(&clip.params, "position_y", 0.0)?,
                scale_x: number(&clip.params, "scale_x", 1.0)?,
                scale_y: number(&clip.params, "scale_y", 1.0)?,
                rotation_degrees: number(&clip.params, "rotation_degrees", 0.0)?,
                z_index: i32::try_from(track_index)
                    .map_err(|_| capability("too many visual tracks"))?,
                curves: visual_curves(clip, track_opacity as f64)?,
            });
            include_end(&mut end, &clip.timeline_range)?;
        }
    }
    Ok((video_clips, image_clips, audio_clips, end))
}

fn compile_captions(
    sequence: &opencut_contracts::Sequence,
    end: &mut Option<RationalTime>,
) -> Result<Vec<TimelineCaption>, CompositionError> {
    for track in sequence
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Caption && track.enabled)
    {
        if !track.clips.is_empty() {
            return Err(capability(
                "clip-backed caption tracks are not implemented; use typed caption cues",
            ));
        }
    }
    sequence
        .caption_cues
        .iter()
        .map(|cue| {
            include_end(end, &cue.timeline_range)?;
            Ok(TimelineCaption {
                start: time(&cue.timeline_range.start)?,
                duration: time(&cue.timeline_range.duration)?,
                text: cue.text.clone(),
                font_family: cue.style.font_family.clone(),
                font_size: cue.style.font_size,
                color: cue.style.color.clone(),
                background_color: cue.style.background_color.clone(),
                font_policy: map_font_policy(&cue.style.font_policy),
                max_width: cue.style.max_width,
                line_height: cue.style.line_height,
                text_align: map_text_align(cue.style.text_align),
            })
        })
        .collect()
}

fn include_end(end: &mut Option<RationalTime>, value: &TimeRange) -> Result<(), CompositionError> {
    let candidate = RationalTime::try_from(
        &opencut_time::range_end(value).map_err(|e| error("INVALID_PROJECT", e.to_string()))?,
    )
    .map_err(|e| error("INVALID_PROJECT", e.to_string()))?;
    if end.is_none_or(|current| candidate.cmp_exact(current).is_gt()) {
        *end = Some(candidate);
    }
    Ok(())
}
fn clip_asset<'a>(
    assets: &HashMap<&str, &'a Asset>,
    clip: &Clip,
) -> Result<&'a Asset, CompositionError> {
    let id = clip
        .asset_id
        .as_deref()
        .ok_or_else(|| error("INVALID_PROJECT", "media clip has no asset"))?;
    assets
        .get(id)
        .copied()
        .ok_or_else(|| error("NOT_FOUND", format!("asset {id} does not exist")))
}
pub fn validate_channels(clip: &Clip, allowed: &[&str]) -> Result<(), CompositionError> {
    let mut seen = std::collections::HashSet::new();
    for channel in &clip.channels {
        if !allowed.contains(&channel.parameter.as_str()) {
            return Err(capability(format!(
                "clip {} keyframe parameter {} is not implemented for this track",
                clip.id, channel.parameter
            )));
        }
        if !seen.insert(channel.parameter.as_str()) {
            return Err(error(
                "INVALID_PROJECT",
                format!(
                    "clip {} has duplicate keyframe channel {}",
                    clip.id, channel.parameter
                ),
            ));
        }
    }
    Ok(())
}

fn visual_curves(
    clip: &Clip,
    track_opacity: f64,
) -> Result<TimelineVisualCurves, CompositionError> {
    Ok(TimelineVisualCurves {
        opacity: curve(clip, "opacity", track_opacity)?,
        position_x: curve(clip, "position_x", 1.0)?,
        position_y: curve(clip, "position_y", 1.0)?,
        scale_x: curve(clip, "scale_x", 1.0)?,
        scale_y: curve(clip, "scale_y", 1.0)?,
        rotation_degrees: curve(clip, "rotation_degrees", 1.0)?,
    })
}

fn curve(
    clip: &Clip,
    parameter: &str,
    multiplier: f64,
) -> Result<EvaluatedCurve, CompositionError> {
    let Some(channel) = clip.channels.iter().find(|c| c.parameter == parameter) else {
        return Ok(EvaluatedCurve::default());
    };
    let duration = &clip.timeline_range.duration;
    let mut boundaries = vec![Time {
        ticks: "0".into(),
        timebase: 1,
    }];
    boundaries.extend(channel.keyframes.iter().map(|key| key.time.clone()));
    boundaries.push(duration.clone());
    boundaries.sort_by(|a, b| time_cmp(a, b).unwrap_or(std::cmp::Ordering::Equal));
    boundaries.dedup_by(|a, b| time_cmp(a, b).is_ok_and(|v| v == std::cmp::Ordering::Equal));
    let mut segments = Vec::new();
    for pair in boundaries.windows(2) {
        let start = time(&pair[0])?;
        let end = time(&pair[1])?;
        if time_cmp(&pair[1], &pair[0])? != std::cmp::Ordering::Greater {
            continue;
        }
        let start_value = opencut_editor_core::sample_keyframe_channel(channel, &pair[0])
            .map_err(|e| error("INVALID_PROJECT", e.to_string()))?
            * multiplier;
        let mut end_value = opencut_editor_core::sample_keyframe_channel(channel, &pair[1])
            .map_err(|e| error("INVALID_PROJECT", e.to_string()))?
            * multiplier;
        if let Some(key) = channel
            .keyframes
            .iter()
            .find(|key| time_cmp(&key.time, &pair[0]).is_ok_and(|v| v == std::cmp::Ordering::Equal))
            && key.interpolation == opencut_contracts::KeyframeInterpolation::Hold
        {
            end_value = start_value;
        }
        segments.push(EvaluatedSegment {
            start,
            end,
            start_value,
            end_value,
        });
    }
    Ok(EvaluatedCurve { segments })
}

fn time_cmp(a: &Time, b: &Time) -> Result<std::cmp::Ordering, CompositionError> {
    let a = RationalTime::try_from(a).map_err(|e| error("INVALID_PROJECT", e.to_string()))?;
    let b = RationalTime::try_from(b).map_err(|e| error("INVALID_PROJECT", e.to_string()))?;
    Ok(a.cmp_exact(b))
}
pub fn validate_params(
    params: &std::collections::BTreeMap<String, serde_json::Value>,
    allowed: &[&str],
    where_: &str,
) -> Result<(), CompositionError> {
    for key in params.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(capability(format!(
                "{where_} parameter {key} is not implemented"
            )));
        }
    }
    Ok(())
}
fn volume(
    params: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<f32, CompositionError> {
    match params.get("volume") {
        None => Ok(1.0),
        Some(v) => v
            .as_f64()
            .filter(|v| v.is_finite() && *v >= 0.0)
            .map(|v| v as f32)
            .ok_or_else(|| {
                error(
                    "INVALID_PROJECT",
                    "volume must be a non-negative finite number",
                )
            }),
    }
}
pub const VISUAL_PARAMS: &[&str] = &[
    "opacity",
    "position_x",
    "position_y",
    "scale_x",
    "scale_y",
    "rotation_degrees",
];
pub const VIDEO_PARAMS: &[&str] = &[
    "volume",
    "opacity",
    "position_x",
    "position_y",
    "scale_x",
    "scale_y",
    "rotation_degrees",
];
fn number(
    params: &std::collections::BTreeMap<String, serde_json::Value>,
    key: &str,
    default: f64,
) -> Result<f64, CompositionError> {
    match params.get(key) {
        None => Ok(default),
        Some(v) => v
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or_else(|| error("INVALID_PROJECT", format!("{key} must be finite number"))),
    }
}
fn opacity(
    params: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<f32, CompositionError> {
    let value = number(params, "opacity", 1.0)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(error("INVALID_PROJECT", "opacity must be 0..1"));
    }
    Ok(value as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{Keyframe, KeyframeChannel, KeyframeInterpolation};
    #[test]
    fn curve_compilation_does_not_drop_rational_intervals_that_share_a_float() {
        let time = |ticks: i64| Time {
            ticks: ticks.to_string(),
            timebase: 1,
        };
        let first = 9_007_199_254_740_992;
        let second = first + 1;
        assert_eq!(first as f64, second as f64);
        let clip = Clip {
            id: "large-time".into(),
            asset_id: None,
            timeline_range: TimeRange {
                start: time(0),
                duration: time(second + 1),
            },
            source_range: None,
            params: Default::default(),
            channels: vec![KeyframeChannel {
                parameter: "position_x".into(),
                keyframes: [(first, 0.0), (second, 1.0)]
                    .into_iter()
                    .enumerate()
                    .map(|(index, (ticks, value))| Keyframe {
                        id: format!("key-{index}"),
                        time: time(ticks),
                        value: serde_json::json!(value),
                        interpolation: KeyframeInterpolation::Linear,
                    })
                    .collect(),
            }],
        };
        let compiled = curve(&clip, "position_x", 1.0).unwrap();
        assert_eq!(compiled.segments.len(), 3);
        assert_eq!(compiled.segments[1].start.ticks, first);
        assert_eq!(compiled.segments[1].end.ticks, second);
        assert_eq!(compiled.segments[1].start_value, 0.0);
        assert_eq!(compiled.segments[1].end_value, 1.0);
    }
}
