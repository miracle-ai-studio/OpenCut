use super::*;
use serde_json::json;

fn time(ticks: i64, timebase: u32) -> Time {
    Time {
        ticks: ticks.to_string(),
        timebase,
    }
}
fn range(start: i64, duration: i64, timebase: u32) -> TimeRange {
    TimeRange {
        start: time(start, timebase),
        duration: time(duration, timebase),
    }
}
fn version() -> AssetVersion {
    let hash = "a".repeat(64);
    AssetVersion {
        id: format!("av_{hash}"),
        content_hash: hash,
        size: 123,
    }
}
fn asset(id: &str, kind: AssetKind) -> Asset {
    Asset {
        id: id.into(),
        name: id.into(),
        kind,
        uri: format!("legacy://{id}"),
        duration: Some(time(60, 1)),
        version: Some(version()),
        metadata: BTreeMap::new(),
    }
}
fn sequence(id: &str) -> Sequence {
    Sequence {
        id: id.into(),
        name: id.into(),
        width: 1920,
        height: 1080,
        fps: FrameRate {
            numerator: 30,
            denominator: 1,
        },
        tracks: Vec::new(),
        caption_cues: Vec::new(),
    }
}
fn base() -> ProjectDocument {
    ProjectDocument {
        id: "project".into(),
        name: "Project".into(),
        schema_version: 2,
        default_sequence_id: "scene".into(),
        sequences: vec![sequence("scene")],
        assets: Vec::new(),
    }
}
fn track(id: &str, kind: TrackKind, clips: Vec<Clip>) -> Track {
    Track {
        id: id.into(),
        name: id.into(),
        kind,
        enabled: true,
        muted: false,
        locked: false,
        clips,
        params: BTreeMap::new(),
    }
}
fn clip(id: &str, asset: &str, timeline_range: TimeRange, source_range: Option<TimeRange>) -> Clip {
    Clip {
        id: id.into(),
        asset_id: Some(asset.into()),
        timeline_range,
        source_range,
        params: BTreeMap::new(),
        channels: Vec::new(),
    }
}
fn converted(result: LegacyConversionResult) -> (EditorDocument, LegacyConversionReport) {
    match result {
        LegacyConversionResult::Converted { project, report } => {
            super::super::validate_project_scene(&project).unwrap();
            (*project, report)
        }
        LegacyConversionResult::Rejected { report } => {
            panic!("conversion rejected: {:?}", report.issues)
        }
    }
}

#[test]
fn v1_requires_explicit_cas_and_preserves_real_import_observations() {
    let mut old = base();
    old.schema_version = 1;
    let mut resource = asset("media", AssetKind::Video);
    resource.version = None;
    resource.metadata = BTreeMap::from([
        ("content_hash".into(), json!(version().content_hash)),
        ("size".into(), json!(123)),
        (
            "media_url".into(),
            json!(format!("/v1/media/{}", version().content_hash)),
        ),
        ("mime_type".into(), json!("video/mp4")),
        ("has_audio".into(), json!(true)),
        ("width".into(), json!(1920)),
        ("height".into(), json!(1080)),
        (
            "frame_rate".into(),
            json!({"numerator":60000,"denominator":1001}),
        ),
    ]);
    old.assets.push(resource);
    let inputs = LegacyConversionInputs {
        asset_versions: BTreeMap::from([("media".into(), version())]),
    };
    let (new, report) = converted(convert_legacy_project(
        LegacyProjectInput::Base(&old),
        &inputs,
    ));
    assert_eq!(new.assets[0].version, version());
    assert_eq!(
        new.assets[0].metadata.audio,
        AudioPresence::Present { metadata: None }
    );
    assert_eq!(
        new.assets[0].metadata.mime_type.as_deref(),
        Some("video/mp4")
    );
    assert_eq!(
        new.assets[0].metadata.frame_rate,
        Some(FrameRate {
            numerator: 60000,
            denominator: 1001
        })
    );
    assert!(report.resource_bindings[0].supplied);
    assert!(
        report
            .normalizations
            .contains(&LegacyNormalization::MediaUrlDerived {
                asset_id: "media".into()
            })
    );
}

#[test]
fn audio_probe_format_requires_consistent_presence_and_valid_typed_fields() {
    let valid = json!({"sample_rate":48000,"channels":1,"channel_layout":"mono"});
    for (presence, metadata) in [
        (Some(false), valid.clone()),
        (None, valid.clone()),
        (
            Some(true),
            json!({"sample_rate":0,"channels":1,"channel_layout":null}),
        ),
        (
            Some(true),
            json!({"sample_rate":48000,"channels":-1,"channel_layout":null}),
        ),
        (
            Some(true),
            json!({"sample_rate":48000,"channels":1,"channel_layout":42}),
        ),
        (
            Some(true),
            json!({"sample_rate":48000,"channels":1,"channel_layout":null,"unknown":1}),
        ),
    ] {
        let mut old = base();
        let mut resource = asset("media", AssetKind::Video);
        resource.metadata.insert("audio_metadata".into(), metadata);
        if let Some(presence) = presence {
            resource
                .metadata
                .insert("has_audio".into(), json!(presence));
        }
        // Unrelated unknown fields must still be returned in the same report.
        resource.metadata.insert("other_unknown".into(), json!(1));
        old.assets.push(resource);
        let LegacyConversionResult::Rejected { report } = convert_legacy_project(
            LegacyProjectInput::Base(&old),
            &LegacyConversionInputs::default(),
        ) else {
            panic!("inconsistent or malformed audio observations must reject");
        };
        assert!(report.issues.iter().any(|issue| matches!(
            &issue.kind, LegacyConversionIssueKind::InvalidMetadata { field, .. }
                if field == "audio_metadata"
        )));
        assert!(report.issues.iter().any(|issue| matches!(
            &issue.kind, LegacyConversionIssueKind::UnknownMetadata { field }
                if field == "other_unknown"
        )));
    }
}

#[test]
fn exact_media_time_linear_gain_and_caption_overlay_survive() {
    let mut old = base();
    old.assets = vec![
        asset("video", AssetKind::Video),
        asset("audio", AssetKind::Audio),
        asset("image", AssetKind::Image),
    ];
    let mut video = clip(
        "video_clip",
        "video",
        range(3003, 6006, 3000),
        Some(range(1001, 3003, 3000)),
    );
    video.params = BTreeMap::from([
        ("volume".into(), json!(0.0000001)),
        ("scale_x".into(), json!(0.0)),
    ]);
    video.channels.push(KeyframeChannel {
        parameter: "volume".into(),
        keyframes: vec![
            Keyframe {
                id: "gain_zero".into(),
                time: time(0, 90000),
                value: json!(0.0),
                interpolation: KeyframeInterpolation::Hold,
            },
            Keyframe {
                id: "gain_end".into(),
                time: time(6006, 3000),
                value: json!(0.0000001),
                interpolation: KeyframeInterpolation::Linear,
            },
        ],
    });
    let mut videos = track("videos", TrackKind::Video, vec![video.clone()]);
    videos.muted = true;
    videos.locked = true;
    videos.params = BTreeMap::from([
        ("opacity".into(), json!(0.3)),
        ("volume".into(), json!(0.2)),
    ]);
    let audio = clip("audio_clip", "audio", range(0, 2, 1), Some(range(3, 4, 1)));
    let image = clip("image_clip", "image", range(0, 2, 1), Some(range(5, 3, 1)));
    let mut old_caption_track = track("old_caption", TrackKind::Caption, Vec::new());
    old_caption_track.enabled = false;
    old.sequences[0].tracks = vec![
        videos,
        track("audio_track", TrackKind::Audio, vec![audio]),
        track("graphic", TrackKind::Graphic, vec![image]),
        old_caption_track,
    ];
    old.sequences[0].caption_cues.push(CaptionCue {
        id: "caption".into(),
        timeline_range: range(0, 2, 1),
        text: "Caption".into(),
        source_anchor: Some(CaptionSourceAnchor {
            asset_id: "audio".into(),
            source_range: range(3, 4, 1),
        }),
        style: CaptionStyle {
            font_family: "Missing Family".into(),
            font_size: 44.0,
            color: "#12AB34".into(),
            background_color: Some("black@0.5".into()),
            font_policy: FontResolutionPolicy::ExplicitFallback {
                families: vec!["Fallback".into()],
            },
            max_width: Some(1000),
            line_height: Some(54.0),
            text_align: TextAlign::Center,
        },
    });
    let (new, _) = converted(convert_legacy_project(
        LegacyProjectInput::Base(&old),
        &LegacyConversionInputs::default(),
    ));
    let track = &new.sequences[0].tracks[0];
    let element = &track.elements[0];
    assert_eq!(element.timeline_range, video.timeline_range);
    assert_eq!(
        element.source_span,
        ElementSourceSpan::LegacySourceRange {
            range: video.source_range.unwrap()
        }
    );
    assert!(track.muted && track.locked);
    assert_eq!(track.opacity, 0.3);
    match &element.content {
        ElementContent::Video {
            audio,
            retime,
            source_audio_enabled,
            ..
        } => {
            assert_eq!(audio.gain, Gain::LinearLegacy { value: 0.0000001 });
            assert_eq!(
                retime.rate,
                RationalRate {
                    numerator: 1,
                    denominator: 2
                }
            );
            assert!(*source_audio_enabled);
            assert!(!retime.maintain_pitch);
        }
        _ => panic!("expected video"),
    }
    let AnimationChannel::Scalar { keys, .. } = &element.channels[0] else {
        panic!("expected scalar");
    };
    assert_eq!(keys[0].id, "gain_zero");
    assert_eq!(keys[0].time, time(0, 90000));
    assert_eq!(keys[0].segment_to_next, ScalarSegment::Step);
    let cue = &new.sequences[0].captions[0];
    let overlay = new.sequences[0].tracks.last().unwrap();
    assert_eq!(cue.track_id, overlay.id);
    assert!(overlay.enabled && !overlay.muted);
    assert_ne!(cue.track_id, "old_caption");
    assert_eq!(
        cue.source_anchor,
        old.sequences[0].caption_cues[0].source_anchor
    );
    assert_eq!(cue.style.layout, CaptionLayoutProfile::LegacyCaptionV2);
    assert_eq!(cue.style.background_color.unwrap().alpha, 128.0 / 255.0);
}

#[test]
fn distinct_scene_formats_empty_names_and_generated_id_collisions_survive() {
    let mut old = base();
    old.id = "legacy_v4_0".into();
    old.sequences[0].name.clear();
    let mut second = sequence("second");
    second.width = 720;
    second.height = 1280;
    second.fps = FrameRate {
        numerator: 24000,
        denominator: 1001,
    };
    let mut empty = track("legacy_v4_1", TrackKind::Graphic, Vec::new());
    empty.name.clear();
    second.tracks.push(empty);
    old.sequences.push(second);
    let a = convert_legacy_project(
        LegacyProjectInput::Base(&old),
        &LegacyConversionInputs::default(),
    );
    let b = convert_legacy_project(
        LegacyProjectInput::Base(&old),
        &LegacyConversionInputs::default(),
    );
    assert_eq!(a, b);
    let (new, report) = converted(a);
    let SettingsProfile::LegacyPerScene { formats } = new.settings else {
        panic!("legacy scope lost");
    };
    assert_eq!(
        formats[1].canvas_size,
        CanvasSize {
            width: 720,
            height: 1280
        }
    );
    assert_eq!(formats[1].fps, old.sequences[1].fps);
    assert_eq!(new.sequences[0].name, "");
    assert_eq!(new.sequences[1].tracks[1].name, "");
    assert_eq!(report.generated_identities[0].id, "legacy_v4_2");
    assert!(
        new.sequences
            .iter()
            .all(|s| s.tracks.iter().any(|t| t.id == s.main_track_id))
    );
}

#[test]
fn all_resource_and_field_issues_are_returned_without_partial_project() {
    let mut old = base();
    let mut first = asset("first", AssetKind::Image);
    first.version = None;
    first.metadata = BTreeMap::from([
        ("mystery".into(), json!(1)),
        ("width".into(), json!(100)),
        ("media_url".into(), json!("https://elsewhere")),
    ]);
    let mut second = asset("second", AssetKind::Video);
    second
        .metadata
        .insert("content_hash".into(), json!("b".repeat(64)));
    old.assets = vec![first, second];
    let inputs = LegacyConversionInputs {
        asset_versions: BTreeMap::from([("unknown".into(), version())]),
    };
    let LegacyConversionResult::Rejected { report } =
        convert_legacy_project(LegacyProjectInput::Base(&old), &inputs)
    else {
        panic!("must reject");
    };
    assert_eq!(report.issues.len(), 6);
    assert!(report.issues.iter().any(|i| matches!(
        i.kind,
        LegacyConversionIssueKind::MissingAssetVersion { .. }
    )));
    assert!(
        report
            .issues
            .iter()
            .any(|i| matches!(i.kind, LegacyConversionIssueKind::UnknownMetadata { .. }))
    );
    assert!(report.issues.iter().any(|i| matches!(
        i.kind,
        LegacyConversionIssueKind::UnknownVersionBinding { .. }
    )));
}

#[test]
fn v3_graph_layer_order_effects_masks_curve_ids_and_transition_are_preserved() {
    let mut old = base();
    old.assets = vec![
        asset("image", AssetKind::Image),
        asset("video", AssetKind::Video),
    ];
    let image = clip("image_clip", "image", range(0, 2, 1), None);
    let mut outgoing = clip("out", "video", range(0, 2, 1), Some(range(1, 2, 1)));
    outgoing.channels.push(KeyframeChannel {
        parameter: "opacity".into(),
        keyframes: vec![
            Keyframe {
                id: "key_a".into(),
                time: time(0, 1),
                value: json!(0.2),
                interpolation: KeyframeInterpolation::Linear,
            },
            Keyframe {
                id: "key_b".into(),
                time: time(1, 1),
                value: json!(0.6),
                interpolation: KeyframeInterpolation::Linear,
            },
            Keyframe {
                id: "key_c".into(),
                time: time(2, 1),
                value: json!(1.0),
                interpolation: KeyframeInterpolation::Linear,
            },
        ],
    });
    let incoming = clip("in", "video", range(2, 2, 1), Some(range(3, 2, 1)));
    old.sequences[0].tracks = vec![
        track("videos", TrackKind::Video, vec![outgoing, incoming]),
        track("graphics", TrackKind::Graphic, vec![image]),
    ];
    let shape = |id: &str, primitive| ShapeClip {
        id: id.into(),
        track_id: "graphics".into(),
        timeline_range: range(0, 2, 1),
        primitive,
        fill: Rgba {
            red: 12,
            green: 34,
            blue: 56,
            alpha: 128,
        },
        position_x: 30.5,
        position_y: -20.0,
        rotation_degrees: 15.0,
        opacity: 0.4,
    };
    let curve = |id: &str, key: &str| HistoricalVisualKeyframeCurve {
        id: id.into(),
        schema_version: 1,
        sequence_id: "scene".into(),
        clip_id: "out".into(),
        parameter: "opacity".into(),
        keyframe_id: key.into(),
        curve: HistoricalVisualCubicBezier {
            x1: 0.2,
            y1: 0.3,
            x2: 0.8,
            y2: 0.9,
        },
    };
    let old = HistoricalVisualDocument {
        schema_version: 3,
        base: old,
        keyframe_curves: vec![curve("curve_a", "key_a"), curve("curve_b", "key_b")],
        visual: vec![HistoricalVisualSequenceVisual {
            sequence_id: "scene".into(),
            shapes: vec![
                shape(
                    "rectangle",
                    ShapePrimitive::Rectangle {
                        width_px: 110.5,
                        height_px: 80.0,
                    },
                ),
                shape(
                    "ellipse",
                    ShapePrimitive::Ellipse {
                        radius_x_px: 50.0,
                        radius_y_px: 40.5,
                    },
                ),
            ],
            effects: vec![
                EffectNode {
                    id: "blur".into(),
                    target_id: "rectangle".into(),
                    effect: VisualEffect::GaussianBlur { radius_px: 3.5 },
                },
                EffectNode {
                    id: "adjust".into(),
                    target_id: "rectangle".into(),
                    effect: VisualEffect::ColorAdjust {
                        exposure_ev: 1.0,
                        contrast: 0.5,
                        saturation: 1.2,
                    },
                },
            ],
            masks: vec![MaskNode {
                id: "mask".into(),
                target_id: "ellipse".into(),
                shape: MaskShape::Ellipse {
                    x: 0.1,
                    y: 0.2,
                    width: 0.6,
                    height: 0.7,
                },
                inverted: true,
                feather: 0.05,
            }],
            transitions: vec![TransitionNode {
                id: "transition".into(),
                outgoing_clip_id: "out".into(),
                incoming_clip_id: "in".into(),
                duration: time(1, 1),
                transition: Transition::Crossfade,
            }],
        }],
    };
    let (new, report) = converted(convert_legacy_project(
        LegacyProjectInput::V3(&old),
        &LegacyConversionInputs::default(),
    ));
    let graphics = &new.sequences[0].tracks[1];
    assert_eq!(
        graphics
            .elements
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        vec!["image_clip", "rectangle", "ellipse"]
    );
    assert_eq!(
        graphics.elements[1]
            .visual
            .as_ref()
            .unwrap()
            .effects
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        vec!["blur", "adjust"]
    );
    assert_eq!(
        graphics.elements[2].visual.as_ref().unwrap().masks[0].shape,
        LayerMaskShape::LegacyEllipse {
            x: 0.1,
            y: 0.2,
            width: 0.6,
            height: 0.7
        }
    );
    let AnimationChannel::Scalar { keys, .. } = &new.sequences[0].tracks[0].elements[0].channels[0]
    else {
        panic!("scalar");
    };
    assert_eq!(keys[0].segment_id.as_deref(), Some("curve_a"));
    assert_eq!(keys[1].segment_id.as_deref(), Some("curve_b"));
    assert_eq!(
        keys[0].segment_to_next,
        ScalarSegment::LegacyNormalizedBezier {
            curve: old.keyframe_curves[0].curve
        }
    );
    assert_eq!(new.sequences[0].transitions[0].id, "transition");
    assert_eq!(
        report
            .normalizations
            .iter()
            .filter(|n| matches!(n, LegacyNormalization::ShapeOrderMadeExplicit { .. }))
            .count(),
        2
    );
}
