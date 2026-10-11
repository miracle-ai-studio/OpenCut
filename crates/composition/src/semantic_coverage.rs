//! Source-to-IR audit independent of the compiler's node construction.
//! Checks all currently supported base media, caption and keyframe semantics.
use crate::{Composition, CompositionError, CompositionScope, SourceRef};
use opencut_contracts::{Clip, ProjectSnapshot, Time, TrackKind, project_v3::KeyframeCurveV1};
use opencut_media_contracts::composition::{EvaluatedCurve, TimelineVisualCurves};
use opencut_time::RationalTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticCoverageReceipt {
    pub revision: String,
    pub sequence_id: String,
    pub entries: Vec<CoverageEntry>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageEntry {
    pub source_id: String,
    pub semantic: String,
    pub disposition: String,
}
fn mismatch(message: impl Into<String>) -> CompositionError {
    CompositionError {
        code: "COMPOSITION_COVERAGE_MISMATCH",
        message: message.into(),
    }
}
fn check(ok: bool, id: &str, field: &str) -> Result<(), CompositionError> {
    if ok {
        Ok(())
    } else {
        Err(mismatch(format!("Source {id}: missing or changed {field}")))
    }
}
fn rational(t: &Time) -> Result<RationalTime, CompositionError> {
    RationalTime::try_from(t).map_err(|e| mismatch(e.message))
}
fn time(t: &Time) -> Result<opencut_media_contracts::MediaTime, CompositionError> {
    crate::validation::time(t)
}
fn number(
    params: &std::collections::BTreeMap<String, serde_json::Value>,
    name: &str,
    default: f64,
) -> f64 {
    params
        .get(name)
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(default)
}
fn source(
    snapshot: &ProjectSnapshot,
    clip: &Clip,
    actual: &SourceRef,
) -> Result<(), CompositionError> {
    let asset = snapshot
        .project
        .assets
        .iter()
        .find(|a| Some(a.id.as_str()) == clip.asset_id.as_deref())
        .ok_or_else(|| mismatch("Missing source asset"))?;
    check(
        actual.asset_id == asset.id && actual.version == asset.version,
        &clip.id,
        "asset identity",
    )
}
fn curve(
    clip: &Clip,
    name: &str,
    multiplier: f64,
    seq: &str,
    curves: &[KeyframeCurveV1],
    actual: &EvaluatedCurve,
) -> Result<(), CompositionError> {
    let Some(channel) = clip.channels.iter().find(|c| c.parameter == name) else {
        return check(actual.segments.is_empty(), &clip.id, name);
    };
    let mut points = vec![
        rational(&Time {
            ticks: "0".into(),
            timebase: 1,
        })?,
        rational(&clip.timeline_range.duration)?,
    ];
    for key in &channel.keyframes {
        points.push(rational(&key.time)?);
    }
    points.sort_by(|a, b| a.cmp_exact(*b));
    points.dedup_by(|a, b| a.cmp_exact(*b).is_eq());
    check(actual.segments.len() == points.len() - 1, &clip.id, name)?;
    for (pair, segment) in points.windows(2).zip(&actual.segments) {
        let start: Time = pair[0].into();
        let end: Time = pair[1].into();
        check(
            rational(&Time {
                ticks: segment.start.ticks.to_string(),
                timebase: segment.start.timebase,
            })?
            .cmp_exact(pair[0])
            .is_eq()
                && rational(&Time {
                    ticks: segment.end.ticks.to_string(),
                    timebase: segment.end.timebase,
                })?
                .cmp_exact(pair[1])
                .is_eq(),
            &clip.id,
            "keyframe interval",
        )?;
        let sample = |t: &Time| {
            opencut_editor_core::sample_keyframe_channel(channel, t)
                .map(|v| v * multiplier)
                .map_err(|e| mismatch(e.message))
        };
        let start_value = sample(&start)?;
        let key = channel
            .keyframes
            .iter()
            .find(|k| rational(&k.time).is_ok_and(|t| t.cmp_exact(pair[0]).is_eq()));
        let end_value = if key
            .is_some_and(|k| k.interpolation == opencut_contracts::KeyframeInterpolation::Hold)
        {
            start_value
        } else {
            sample(&end)?
        };
        check(
            segment.start_value == start_value && segment.end_value == end_value,
            &clip.id,
            "keyframe values",
        )?;
        let expected = key.and_then(|key| {
            curves.iter().find(|c| {
                c.sequence_id == seq
                    && c.clip_id == clip.id
                    && c.parameter == name
                    && c.keyframe_id == key.id
            })
        });
        check(
            match (expected, segment.cubic_bezier) {
                (None, None) => true,
                (Some(c), Some(v)) => {
                    c.curve.x1 == v.x1
                        && c.curve.y1 == v.y1
                        && c.curve.x2 == v.x2
                        && c.curve.y2 == v.y2
                }
                _ => false,
            },
            &clip.id,
            "keyframe interpolation",
        )?;
    }
    Ok(())
}
fn visual_curves(
    clip: &Clip,
    opacity: f64,
    sequence: &str,
    curves: &[KeyframeCurveV1],
    actual: &TimelineVisualCurves,
) -> Result<(), CompositionError> {
    for (name, value) in [
        ("opacity", &actual.opacity),
        ("position_x", &actual.position_x),
        ("position_y", &actual.position_y),
        ("scale_x", &actual.scale_x),
        ("scale_y", &actual.scale_y),
        ("rotation_degrees", &actual.rotation_degrees),
    ] {
        curve(
            clip,
            name,
            if name == "opacity" { opacity } else { 1.0 },
            sequence,
            curves,
            value,
        )?;
    }
    Ok(())
}

pub fn audit(
    snapshot: &ProjectSnapshot,
    compiled: &Composition,
    curves: &[KeyframeCurveV1],
) -> Result<SemanticCoverageReceipt, CompositionError> {
    check(
        snapshot.revision == compiled.revision,
        "project",
        "revision",
    )?;
    let seq = snapshot
        .project
        .sequences
        .iter()
        .find(|s| s.id == compiled.sequence_id)
        .ok_or_else(|| mismatch("Unknown sequence"))?;
    let t = &compiled.timeline;
    check(
        t.width == seq.width
            && t.height == seq.height
            && t.frame_rate.numerator == seq.fps.numerator
            && t.frame_rate.denominator == seq.fps.denominator,
        &seq.id,
        "canvas",
    )?;
    let mut entries = Vec::new();
    let mut video = 0;
    let mut image = 0;
    let mut audio = 0;
    let mut entry = |id: &str, semantic: &str, disposition: &str| {
        entries.push(CoverageEntry {
            source_id: id.into(),
            semantic: semantic.into(),
            disposition: disposition.into(),
        })
    };
    // Visual inventory order is independently counted by track kind.
    for (z, track) in seq.tracks.iter().enumerate() {
        for clip in &track.clips {
            if !track.enabled {
                entry(&clip.id, "clip", "disabled_track");
                continue;
            }
            let opacity = number(&track.params, "opacity", 1.0) as f32;
            macro_rules! visual {
                ($actual:expr) => {{
                    let a =
                        $actual.ok_or_else(|| mismatch(format!("Missing visual {}", clip.id)))?;
                    source(snapshot, clip, &a.input)?;
                    check(
                        a.timeline_start == time(&clip.timeline_range.start)?
                            && a.timeline_duration == time(&clip.timeline_range.duration)?
                            && a.stream.is_none()
                            && a.z_index == z as i32,
                        &clip.id,
                        "placement",
                    )?;
                    check(
                        a.opacity == opacity * number(&clip.params, "opacity", 1.0) as f32
                            && a.position_x == number(&clip.params, "position_x", 0.0)
                            && a.position_y == number(&clip.params, "position_y", 0.0)
                            && a.scale_x == number(&clip.params, "scale_x", 1.0)
                            && a.scale_y == number(&clip.params, "scale_y", 1.0)
                            && a.rotation_degrees == number(&clip.params, "rotation_degrees", 0.0),
                        &clip.id,
                        "transform",
                    )?;
                    visual_curves(clip, opacity as f64, &seq.id, curves, &a.curves)?;
                    a
                }};
            }
            match track.kind {
                TrackKind::Video => {
                    let a = visual!(t.video_clips.get(video));
                    video += 1;
                    let r = clip
                        .source_range
                        .as_ref()
                        .ok_or_else(|| mismatch("Missing video source range"))?;
                    check(
                        a.source_start == time(&r.start)?
                            && a.source_duration == time(&r.duration)?,
                        &clip.id,
                        "source range",
                    )?;
                    entry(&clip.id, "video", "compiled");
                }
                TrackKind::Graphic => {
                    visual!(t.image_clips.get(image));
                    image += 1;
                    entry(&clip.id, "image", "compiled");
                }
                _ => {}
            }
        }
    }
    // Source audio from video clips precedes dedicated audio tracks.
    for kind in [TrackKind::Video, TrackKind::Audio] {
        for track in seq.tracks.iter().filter(|t| t.kind == kind) {
            for clip in &track.clips {
                let excluded = if !track.enabled {
                    Some("disabled_track")
                } else if track.muted {
                    Some("muted_track")
                } else if compiled.scope == CompositionScope::Visual {
                    Some("visual_scope")
                } else {
                    None
                };
                if let Some(reason) = excluded {
                    entry(&clip.id, "audio", reason);
                    continue;
                }
                let a = t
                    .audio_clips
                    .get(audio)
                    .ok_or_else(|| mismatch(format!("Missing audio {}", clip.id)))?;
                audio += 1;
                source(snapshot, clip, &a.input)?;
                let r = clip
                    .source_range
                    .as_ref()
                    .ok_or_else(|| mismatch("Missing audio range"))?;
                let gain = number(&track.params, "volume", 1.0) as f32;
                check(
                    a.source_start == time(&r.start)?
                        && a.source_duration == time(&r.duration)?
                        && a.timeline_start == time(&clip.timeline_range.start)?
                        && a.timeline_duration == time(&clip.timeline_range.duration)?
                        && a.gain == gain * number(&clip.params, "volume", 1.0) as f32
                        && a.optional == (kind == TrackKind::Video)
                        && a.stream.is_none(),
                    &clip.id,
                    "audio timing/gain",
                )?;
                curve(clip, "volume", gain as f64, &seq.id, curves, &a.gain_curve)?;
                entry(&clip.id, "audio", "compiled");
            }
        }
    }
    check(
        video == t.video_clips.len()
            && image == t.image_clips.len()
            && audio == t.audio_clips.len(),
        &seq.id,
        "media inventory",
    )?;
    check(
        seq.caption_cues.len() == t.captions.len(),
        &seq.id,
        "caption inventory",
    )?;
    for (cue, a) in seq.caption_cues.iter().zip(&t.captions) {
        let style = &cue.style;
        check(
            a.start == time(&cue.timeline_range.start)?
                && a.duration == time(&cue.timeline_range.duration)?
                && a.text == cue.text
                && a.font_family == style.font_family
                && a.font_size == style.font_size
                && a.color == style.color
                && a.background_color == style.background_color
                && a.font_policy == crate::compiler::map_font_policy(&style.font_policy)
                && a.max_width == style.max_width
                && a.line_height == style.line_height
                && a.text_align == crate::compiler::map_text_align(style.text_align),
            &cue.id,
            "caption timing/text/style",
        )?;
        entry(&cue.id, "caption", "compiled");
    }
    Ok(SemanticCoverageReceipt {
        revision: snapshot.revision.clone(),
        sequence_id: seq.id.clone(),
        entries,
    })
}

/// Duration is a source invariant, including audio tails and a schema-3 shape
/// extent supplied by the caller. Use exact rationals across mixed timebases.
pub fn audit_duration(
    snapshot: &ProjectSnapshot,
    compiled: &Composition,
    visual_extent: Option<RationalTime>,
) -> Result<(), CompositionError> {
    let sequence = snapshot
        .project
        .sequences
        .iter()
        .find(|s| s.id == compiled.sequence_id)
        .ok_or_else(|| mismatch("Unknown sequence"))?;
    let mut end = visual_extent;
    let mut include = |range: &opencut_contracts::TimeRange| -> Result<(), CompositionError> {
        let candidate =
            rational(&opencut_time::range_end(range).map_err(|e| mismatch(e.message))?)?;
        if end.is_none_or(|value| candidate.cmp_exact(value).is_gt()) {
            end = Some(candidate);
        }
        Ok(())
    };
    for track in &sequence.tracks {
        if track.enabled
            && (matches!(track.kind, TrackKind::Video | TrackKind::Graphic)
                || (track.kind == TrackKind::Audio && !track.muted))
        {
            for clip in &track.clips {
                include(&clip.timeline_range)?;
            }
        }
    }
    for caption in &sequence.caption_cues {
        include(&caption.timeline_range)?;
    }
    let actual = rational(&Time {
        ticks: compiled.timeline.duration.ticks.to_string(),
        timebase: compiled.timeline.duration.timebase,
    })?;
    check(
        end.is_some_and(|v| v.cmp_exact(actual).is_eq()),
        &sequence.id,
        "composition duration",
    )
}
