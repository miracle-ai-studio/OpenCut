//! Source/timeline clocks, linear gains and stable outgoing curve identities.
use super::*;
use opencut_time::RationalTime;
use serde_json::Value;

const VISUAL_PARAMS: &[&str] = &[
    "opacity",
    "position_x",
    "position_y",
    "scale_x",
    "scale_y",
    "rotation_degrees",
];
const VIDEO_PARAMS: &[&str] = &[
    "volume",
    "opacity",
    "position_x",
    "position_y",
    "scale_x",
    "scale_y",
    "rotation_degrees",
];

pub(super) fn convert_track(
    base: &ProjectDocument,
    sequence: &Sequence,
    track: &Track,
    v3: Option<&ProjectDocumentV3>,
    path: &str,
    context: &mut Context,
) -> TrackV4 {
    let kind = match track.kind {
        TrackKind::Video => TrackKindV4::Video,
        TrackKind::Audio => TrackKindV4::Audio,
        TrackKind::Graphic => TrackKindV4::Graphic,
        TrackKind::Caption => TrackKindV4::Caption,
    };
    // Both old track parameters have a typed storage location even where a
    // renderer does not consume them. Preserve editing data on disabled tracks
    // too; render-capability validation belongs to execution, not conversion.
    check_params(
        &track.params,
        &["opacity", "volume"],
        &format!("{path}.params"),
        context,
    );
    let elements = track
        .clips
        .iter()
        .enumerate()
        .filter_map(|(index, clip)| {
            clip_element(
                base,
                sequence,
                track,
                clip,
                v3,
                &format!("{path}.clips[{index}]"),
                context,
            )
        })
        .collect();
    TrackV4 {
        id: track.id.clone(),
        name: track.name.clone(),
        kind,
        enabled: track.enabled,
        muted: track.muted,
        locked: track.locked,
        opacity: number(&track.params, "opacity", 1.0),
        gain: GainV4::LinearLegacy {
            value: number(&track.params, "volume", 1.0),
        },
        elements,
        channels: Vec::new(),
    }
}

fn clip_element(
    base: &ProjectDocument,
    sequence: &Sequence,
    track: &Track,
    clip: &Clip,
    v3: Option<&ProjectDocumentV3>,
    path: &str,
    context: &mut Context,
) -> Option<TimelineElementV4> {
    let asset = clip
        .asset_id
        .as_ref()
        .and_then(|id| base.assets.iter().find(|a| &a.id == id));
    let Some(asset) = asset else {
        context.issue(
            format!("{path}.asset_id"),
            LegacyConversionIssueKind::UnsupportedClipContent {
                clip_id: clip.id.clone(),
                reason: "a placeholder or unresolved asset has no typed content definition".into(),
            },
        );
        // Still report unrecognized parameters rather than short-circuiting them.
        check_params(&clip.params, &[], &format!("{path}.params"), context);
        channels(clip, sequence, v3, &[], path, context);
        return None;
    };
    let (content, visual_content, allowed): (Option<ElementContentV4>, bool, &[&str]) = match (
        &track.kind,
        &asset.kind,
    ) {
        (TrackKind::Video, AssetKind::Video) => (
            retime(clip, path, context).map(|retime| ElementContentV4::Video {
                asset_id: asset.id.clone(),
                video_stream: None,
                source_audio_enabled: true,
                audio: audio(clip),
                retime,
            }),
            true,
            VIDEO_PARAMS,
        ),
        (TrackKind::Audio, AssetKind::Audio | AssetKind::Video) => (
            retime(clip, path, context).map(|retime| ElementContentV4::Audio {
                asset_id: asset.id.clone(),
                audio: audio(clip),
                retime,
            }),
            false,
            &["volume"],
        ),
        (TrackKind::Video | TrackKind::Graphic, AssetKind::Image | AssetKind::Graphic) => (
            Some(ElementContentV4::Image {
                asset_id: asset.id.clone(),
            }),
            true,
            VISUAL_PARAMS,
        ),
        _ => {
            context.issue(format!("{path}.asset_id"), LegacyConversionIssueKind::UnsupportedClipContent {
                clip_id: clip.id.clone(),
                reason: "legacy Text/Subtitle file clips have no inline text/layout; incompatible media kinds cannot be inferred".into(),
            });
            (None, false, &[])
        }
    };
    check_params(&clip.params, allowed, &format!("{path}.params"), context);
    let channels = channels(clip, sequence, v3, allowed, path, context);
    let content = content?;
    let source_span = match &clip.source_range {
        Some(range) => ElementSourceSpanV4::LegacySourceRange {
            range: range.clone(),
        },
        // Old static images did not declare a source clock. Declared duration
        // equals their local timeline span, with no fabricated media trim.
        None => ElementSourceSpanV4::ClassicTrimmed {
            trim_start: zero(),
            trim_end: zero(),
            source_duration: Some(clip.timeline_range.duration.clone()),
        },
    };
    Some(TimelineElementV4 {
        id: clip.id.clone(),
        name: asset.name.clone(),
        timeline_range: clip.timeline_range.clone(),
        source_span,
        enabled: true,
        content,
        visual: visual_content.then(|| VisualStyleV4 {
            profile: VisualProfileV4::LegacyCompositionV3,
            transform: TransformV4 {
                position_x_px: number(&clip.params, "position_x", 0.0),
                position_y_px: number(&clip.params, "position_y", 0.0),
                scale_x: number(&clip.params, "scale_x", 1.0),
                scale_y: number(&clip.params, "scale_y", 1.0),
                rotation_degrees: number(&clip.params, "rotation_degrees", 0.0),
            },
            opacity: number(&clip.params, "opacity", 1.0),
            blend_mode: BlendModeV4::Normal,
            effects: Vec::new(),
            masks: Vec::new(),
        }),
        channels,
    })
}

fn audio(clip: &Clip) -> AudioStyleV4 {
    AudioStyleV4 {
        stream: None,
        gain: GainV4::LinearLegacy {
            value: number(&clip.params, "volume", 1.0),
        },
        muted: false,
    }
}
fn retime(clip: &Clip, path: &str, context: &mut Context) -> Option<RetimeV4> {
    let Some(source) = &clip.source_range else {
        context.issue(
            format!("{path}.source_range"),
            LegacyConversionIssueKind::MissingSourceRange {
                clip_id: clip.id.clone(),
            },
        );
        return None;
    };
    let rate = (|| {
        let source = RationalTime::try_from(&source.duration).ok()?;
        let timeline = RationalTime::try_from(&clip.timeline_range.duration).ok()?;
        if source.ticks() <= 0 || timeline.ticks() <= 0 {
            return None;
        }
        // Inputs are i64*u32, so their products fit u128. Reduce before narrowing;
        // neither f64 seconds nor a guessed frame grid participates.
        let top = source.ticks() as u128 * timeline.timebase() as u128;
        let bottom = timeline.ticks() as u128 * source.timebase() as u128;
        let divisor = gcd(top, bottom);
        Some(RationalRateV4 {
            numerator: u32::try_from(top / divisor).ok()?,
            denominator: u32::try_from(bottom / divisor).ok()?,
        })
    })();
    match rate {
        Some(rate) => Some(RetimeV4 {
            rate,
            maintain_pitch: false,
            mapping: RetimeMappingV4::ExactRational,
        }),
        None => {
            context.issue(
                format!("{path}.source_range.duration"),
                LegacyConversionIssueKind::UnrepresentableRate {
                    clip_id: clip.id.clone(),
                },
            );
            None
        }
    }
}
fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let next = a % b;
        a = b;
        b = next;
    }
    a
}

fn channels(
    clip: &Clip,
    sequence: &Sequence,
    v3: Option<&ProjectDocumentV3>,
    allowed: &[&str],
    path: &str,
    context: &mut Context,
) -> Vec<AnimationChannelV4> {
    let mut result = Vec::new();
    for (index, channel) in clip.channels.iter().enumerate() {
        let path = format!("{path}.channels[{index}]");
        let parameter = match channel.parameter.as_str() {
            "position_x" => ScalarParameterV4::PositionXPx,
            "position_y" => ScalarParameterV4::PositionYPx,
            "scale_x" => ScalarParameterV4::ScaleX,
            "scale_y" => ScalarParameterV4::ScaleY,
            "rotation_degrees" => ScalarParameterV4::RotationDegrees,
            "opacity" => ScalarParameterV4::Opacity,
            "volume" => ScalarParameterV4::GainLinearLegacy,
            _ => {
                context.issue(
                    format!("{path}.parameter"),
                    LegacyConversionIssueKind::UnsupportedParameter {
                        parameter: channel.parameter.clone(),
                    },
                );
                continue;
            }
        };
        if !allowed.contains(&channel.parameter.as_str()) {
            context.issue(
                format!("{path}.parameter"),
                LegacyConversionIssueKind::UnsupportedParameter {
                    parameter: channel.parameter.clone(),
                },
            );
        }
        let mut keys = Vec::new();
        for (key_index, key) in channel.keyframes.iter().enumerate() {
            let Some(value) = key.value.as_f64().filter(|value| value.is_finite()) else {
                context.issue(
                    format!("{path}.keyframes[{key_index}].value"),
                    LegacyConversionIssueKind::InvalidNumericParameter {
                        parameter: channel.parameter.clone(),
                    },
                );
                continue;
            };
            let curve = v3.and_then(|project| {
                project.keyframe_curves.iter().find(|curve| {
                    curve.sequence_id == sequence.id
                        && curve.clip_id == clip.id
                        && curve.parameter == channel.parameter
                        && curve.keyframe_id == key.id
                })
            });
            keys.push(ScalarKeyV4 {
                id: key.id.clone(),
                segment_id: curve.map(|curve| curve.id.clone()),
                time: key.time.clone(),
                value,
                left_handle: None,
                right_handle: None,
                tangent_mode: TangentModeV4::Broken,
                segment_to_next: match curve {
                    Some(curve) => ScalarSegmentV4::LegacyNormalizedBezier { curve: curve.curve },
                    None => match key.interpolation {
                        KeyframeInterpolation::Hold => ScalarSegmentV4::Step,
                        KeyframeInterpolation::Linear => ScalarSegmentV4::Linear,
                    },
                },
            });
        }
        result.push(AnimationChannelV4::Scalar {
            id: context.generated(&clip.id, GeneratedLegacyIdentityPurpose::AnimationChannel),
            target: ChannelTargetV4::Element {
                element_id: clip.id.clone(),
            },
            binding: ScalarBindingV4::Property { parameter },
            keys,
            extrapolation: ExtrapolationV4 {
                before: ExtrapolationModeV4::Hold,
                after: ExtrapolationModeV4::Hold,
            },
        });
    }
    result
}

fn check_params(
    params: &BTreeMap<String, Value>,
    allowed: &[&str],
    path: &str,
    context: &mut Context,
) {
    for (parameter, value) in params {
        if !allowed.contains(&parameter.as_str()) {
            context.issue(
                format!("{path}.{parameter}"),
                LegacyConversionIssueKind::UnsupportedParameter {
                    parameter: parameter.clone(),
                },
            );
        }
        if value.as_f64().is_none_or(|number| !number.is_finite()) {
            context.issue(
                format!("{path}.{parameter}"),
                LegacyConversionIssueKind::InvalidNumericParameter {
                    parameter: parameter.clone(),
                },
            );
        }
    }
}
fn number(params: &BTreeMap<String, Value>, parameter: &str, default: f64) -> f64 {
    params
        .get(parameter)
        .and_then(Value::as_f64)
        .unwrap_or(default)
}
pub(super) fn zero() -> Time {
    Time {
        ticks: "0".into(),
        timebase: 1,
    }
}
