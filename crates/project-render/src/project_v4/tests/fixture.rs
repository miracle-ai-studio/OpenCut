use super::*;
use opencut_contracts::{AssetKind, AssetVersion, TimeRange};
pub(super) fn t(n: i64, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
pub(super) fn color(r: f64, g: f64, b: f64) -> ColorV4 {
    ColorV4 {
        encoding: ColorEncodingV4::Srgb,
        red: r,
        green: g,
        blue: b,
        alpha: 1.0,
    }
}
fn stroke() -> StrokeV4 {
    StrokeV4 {
        color: color(0.0, 0.0, 0.0),
        width_px: 0.0,
        align: StrokeAlignV4::Center,
    }
}
fn style() -> VisualStyleV4 {
    VisualStyleV4 {
        profile: VisualProfileV4::LegacyCompositionV3,
        transform: TransformV4 {
            position_x_px: 0.0,
            position_y_px: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
        },
        opacity: 0.5,
        blend_mode: BlendModeV4::Normal,
        effects: vec![],
        masks: vec![],
    }
}
fn element(id: &str, content: ElementContentV4) -> TimelineElementV4 {
    TimelineElementV4 {
        id: id.into(),
        name: id.into(),
        timeline_range: TimeRange {
            start: t(1, 2),
            duration: t(2, 1),
        },
        source_span: ElementSourceSpanV4::ClassicTrimmed {
            trim_start: t(0, 1),
            trim_end: t(0, 1),
            source_duration: None,
        },
        enabled: true,
        content,
        visual: Some(style()),
        channels: vec![],
    }
}
fn track(id: &str, kind: TrackKindV4, elements: Vec<TimelineElementV4>) -> TrackV4 {
    TrackV4 {
        id: id.into(),
        name: id.into(),
        kind,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 0.25,
        gain: GainV4::Decibels { value: 0.0 },
        elements,
        channels: vec![],
    }
}
pub(super) fn snapshot() -> ProjectSnapshotV4 {
    let image = element(
        "image",
        ElementContentV4::Image {
            asset_id: "asset".into(),
        },
    );
    let mut shape = element(
        "shape",
        ElementContentV4::Graphic {
            graphic: GraphicV4 {
                geometry: GraphicGeometryV4::LegacyRectangle {
                    width_px: 32.0,
                    height_px: 32.0,
                },
                source_size: CanvasSizeV4 {
                    width: 32,
                    height: 32,
                },
                fill: color(1.0, 1.0, 1.0),
                stroke: stroke(),
            },
        },
    );
    // At element-local one second, equal endpoints zero and handles +2/3
    // produce 0.5. Track opacity 0.25 must not attenuate legacy shapes.
    let key = |id: &str, time: Time, left, right| ScalarKeyV4 {
        id: id.into(),
        segment_id: None,
        time,
        value: 0.0,
        left_handle: left,
        right_handle: right,
        segment_to_next: ScalarSegmentV4::Bezier,
        tangent_mode: TangentModeV4::Flat,
    };
    shape.channels.push(AnimationChannelV4::Scalar {
        id: "bump".into(),
        target: ChannelTargetV4::Element {
            element_id: "shape".into(),
        },
        binding: ScalarBindingV4::Property {
            parameter: ScalarParameterV4::Opacity,
        },
        keys: vec![
            key(
                "first",
                t(0, 1),
                None,
                Some(CurveHandleV4 {
                    dt: t(2, 3),
                    dv: 2.0 / 3.0,
                }),
            ),
            key(
                "last",
                t(2, 1),
                Some(CurveHandleV4 {
                    dt: t(-2, 3),
                    dv: 2.0 / 3.0,
                }),
                None,
            ),
        ],
        extrapolation: ExtrapolationV4 {
            before: ExtrapolationModeV4::Hold,
            after: ExtrapolationModeV4::Hold,
        },
    });
    shape.visual.as_mut().unwrap().masks.push(MaskV4 {
        id: "left-half".into(),
        enabled: true,
        shape: MaskShapeV4::LegacyRectangle {
            x: 0.0,
            y: 0.0,
            width: 0.5,
            height: 1.0,
        },
        inverted: false,
        feather: MaskFeatherV4::LegacyCanvasFraction { value: 0.0 },
        stroke: stroke(),
    });
    shape.visual.as_mut().unwrap().effects.push(EffectV4 {
        id: "identity-color".into(),
        enabled: true,
        effect: EffectKindV4::LegacyColorAdjust {
            exposure_ev: 0.0,
            contrast: 1.0,
            saturation: 1.0,
        },
    });
    let hash = "a".repeat(64);
    ProjectSnapshotV4 {
        revision: "native-fixed-r1".into(),
        project: ProjectDocumentV4 {
            schema_version: 4,
            id: "project".into(),
            name: "fixture".into(),
            default_sequence_id: "scene".into(),
            settings: SettingsProfileV4::LegacyPerScene {
                formats: vec![LegacySequenceFormatV4 {
                    sequence_id: "scene".into(),
                    fps: opencut_contracts::FrameRate {
                        numerator: 2,
                        denominator: 1,
                    },
                    canvas_size: CanvasSizeV4 {
                        width: 64,
                        height: 48,
                    },
                    background: BackgroundV4::Color {
                        color: color(0.0, 0.0, 1.0),
                    },
                }],
            },
            assets: vec![AssetV4 {
                id: "asset".into(),
                name: "red".into(),
                kind: AssetKind::Image,
                version: AssetVersion {
                    id: format!("av_{hash}"),
                    content_hash: hash,
                    size: 64 * 48 * 3 + 13,
                },
                duration: None,
                origin: ResourceOriginV4::LocalImport {
                    original_name: "red.ppm".into(),
                },
                metadata: MediaMetadataV4 {
                    mime_type: Some("image/x-portable-pixmap".into()),
                    intrinsic_size: Some(CanvasSizeV4 {
                        width: 64,
                        height: 48,
                    }),
                    frame_rate: None,
                    rotation_degrees: None,
                    audio: AudioPresenceV4::Absent {},
                },
                license: None,
            }],
            fonts: vec![],
            sequences: vec![SequenceV4 {
                id: "scene".into(),
                name: "scene".into(),
                main_track_id: "main".into(),
                tracks: vec![
                    track("main", TrackKindV4::Video, vec![image]),
                    track("graphic", TrackKindV4::Graphic, vec![shape]),
                ],
                captions: vec![],
                transitions: vec![],
                bookmarks: vec![],
            }],
        },
    }
}
pub(super) fn compile(s: &ProjectSnapshotV4) -> CompositionV4 {
    opencut_composition::project_v4::compile_frame_v4(
        s,
        "scene",
        &t(3, 2),
        opencut_composition::CompositionScope::Visual,
    )
    .unwrap()
}
