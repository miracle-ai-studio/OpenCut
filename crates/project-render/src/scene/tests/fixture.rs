use super::*;
use opencut_contracts::{AssetKind, AssetVersion, TimeRange};
pub(super) fn t(n: i64, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
pub(super) fn color(r: f64, g: f64, b: f64) -> Color {
    Color {
        encoding: ColorEncoding::Srgb,
        red: r,
        green: g,
        blue: b,
        alpha: 1.0,
    }
}
fn stroke() -> Stroke {
    Stroke {
        color: color(0.0, 0.0, 0.0),
        width_px: 0.0,
        align: StrokeAlign::Center,
    }
}
fn style() -> VisualStyle {
    VisualStyle {
        profile: VisualProfile::LegacyCompositionV3,
        transform: Transform {
            position_x_px: 0.0,
            position_y_px: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
        },
        opacity: 0.5,
        blend_mode: BlendMode::Normal,
        effects: vec![],
        masks: vec![],
    }
}
fn element(id: &str, content: ElementContent) -> TimelineElement {
    TimelineElement {
        id: id.into(),
        name: id.into(),
        timeline_range: TimeRange {
            start: t(1, 2),
            duration: t(2, 1),
        },
        source_span: ElementSourceSpan::ClassicTrimmed {
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
fn track(id: &str, kind: SceneTrackKind, elements: Vec<TimelineElement>) -> SceneTrack {
    SceneTrack {
        id: id.into(),
        name: id.into(),
        kind,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 0.25,
        gain: Gain::Decibels { value: 0.0 },
        elements,
        channels: vec![],
    }
}
pub(super) fn snapshot() -> EditorSnapshot {
    let image = element(
        "image",
        ElementContent::Image {
            asset_id: "asset".into(),
        },
    );
    let mut shape = element(
        "shape",
        ElementContent::Graphic {
            graphic: Graphic {
                geometry: GraphicGeometry::LegacyRectangle {
                    width_px: 32.0,
                    height_px: 32.0,
                },
                source_size: CanvasSize {
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
    let key = |id: &str, time: Time, left, right| ScalarKey {
        id: id.into(),
        segment_id: None,
        time,
        value: 0.0,
        left_handle: left,
        right_handle: right,
        segment_to_next: ScalarSegment::Bezier,
        tangent_mode: TangentMode::Flat,
    };
    shape.channels.push(AnimationChannel::Scalar {
        id: "bump".into(),
        target: ChannelTarget::Element {
            element_id: "shape".into(),
        },
        binding: ScalarBinding::Property {
            parameter: ScalarParameter::Opacity,
        },
        keys: vec![
            key(
                "first",
                t(0, 1),
                None,
                Some(CurveHandle {
                    dt: t(2, 3),
                    dv: 2.0 / 3.0,
                }),
            ),
            key(
                "last",
                t(2, 1),
                Some(CurveHandle {
                    dt: t(-2, 3),
                    dv: 2.0 / 3.0,
                }),
                None,
            ),
        ],
        extrapolation: Extrapolation {
            before: ExtrapolationMode::Hold,
            after: ExtrapolationMode::Hold,
        },
    });
    shape.visual.as_mut().unwrap().masks.push(Mask {
        id: "left-half".into(),
        enabled: true,
        shape: LayerMaskShape::LegacyRectangle {
            x: 0.0,
            y: 0.0,
            width: 0.5,
            height: 1.0,
        },
        inverted: false,
        feather: MaskFeather::LegacyCanvasFraction { value: 0.0 },
        stroke: stroke(),
    });
    shape.visual.as_mut().unwrap().effects.push(SceneEffect {
        id: "identity-color".into(),
        enabled: true,
        effect: EffectKind::LegacyColorAdjust {
            exposure_ev: 0.0,
            contrast: 1.0,
            saturation: 1.0,
        },
    });
    let hash = "a".repeat(64);
    EditorSnapshot {
        revision: "native-fixed-r1".into(),
        project: EditorDocument {
            cover: None,
            schema_version: 4,
            id: "project".into(),
            name: "fixture".into(),
            default_sequence_id: "scene".into(),
            settings: SettingsProfile::LegacyPerScene {
                formats: vec![LegacySequenceFormat {
                    sequence_id: "scene".into(),
                    fps: opencut_contracts::FrameRate {
                        numerator: 2,
                        denominator: 1,
                    },
                    canvas_size: CanvasSize {
                        width: 64,
                        height: 48,
                    },
                    background: Background::Color {
                        color: color(0.0, 0.0, 1.0),
                    },
                }],
            },
            assets: vec![ProjectAsset {
                id: "asset".into(),
                name: "red".into(),
                kind: AssetKind::Image,
                version: AssetVersion {
                    id: format!("av_{hash}"),
                    content_hash: hash,
                    size: 64 * 48 * 3 + 13,
                },
                duration: None,
                origin: ResourceOrigin::LocalImport {
                    original_name: "red.ppm".into(),
                },
                metadata: ProjectMediaMetadata {
                    mime_type: Some("image/x-portable-pixmap".into()),
                    intrinsic_size: Some(CanvasSize {
                        width: 64,
                        height: 48,
                    }),
                    frame_rate: None,
                    rotation_degrees: None,
                    audio: AudioPresence::Absent {},
                },
                license: None,
            }],
            fonts: vec![],
            sequences: vec![Scene {
                id: "scene".into(),
                name: "scene".into(),
                main_track_id: "main".into(),
                tracks: vec![
                    track("main", SceneTrackKind::Video, vec![image]),
                    track("graphic", SceneTrackKind::Graphic, vec![shape]),
                ],
                captions: vec![],
                transitions: vec![],
                bookmarks: vec![],
            }],
        },
    }
}
pub(super) fn compile(s: &EditorSnapshot) -> SceneComposition {
    opencut_composition::scene::compile_frame_scene(
        s,
        "scene",
        &t(3, 2),
        opencut_composition::CompositionScope::Visual,
    )
    .unwrap()
}
