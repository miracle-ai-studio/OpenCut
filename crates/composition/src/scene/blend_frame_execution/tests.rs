use super::*;

const MODES: [BlendMode; 17] = [
    BlendMode::Normal,
    BlendMode::Darken,
    BlendMode::Multiply,
    BlendMode::ColorBurn,
    BlendMode::Lighten,
    BlendMode::Screen,
    BlendMode::PlusLighter,
    BlendMode::ColorDodge,
    BlendMode::Overlay,
    BlendMode::SoftLight,
    BlendMode::HardLight,
    BlendMode::Difference,
    BlendMode::Exclusion,
    BlendMode::Hue,
    BlendMode::Saturation,
    BlendMode::Color,
    BlendMode::Luminosity,
];
fn t(ticks: i64) -> Time {
    Time {
        ticks: ticks.to_string(),
        timebase: 1,
    }
}
fn color() -> Color {
    Color {
        encoding: ColorEncoding::Srgb,
        red: 0.2,
        green: 0.5,
        blue: 0.8,
        alpha: 0.75,
    }
}
fn source() -> EditorSnapshot {
    let mut tracks = vec![SceneTrack {
        id: "main".into(),
        name: "Main".into(),
        kind: SceneTrackKind::Video,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1.0,
        gain: Gain::Decibels { value: 0.0 },
        elements: vec![],
        channels: vec![],
    }];
    for (index, blend_mode) in MODES.iter().enumerate() {
        let mut track = tracks[0].clone();
        track.id = format!("track-{index}");
        track.kind = SceneTrackKind::Graphic;
        track.opacity = 0.5;
        track.elements.push(TimelineElement {
            id: format!("graphic-{index}"),
            name: "Graphic".into(),
            timeline_range: TimeRange {
                start: t(0),
                duration: t(2),
            },
            source_span: ElementSourceSpan::ClassicTrimmed {
                trim_start: t(0),
                trim_end: t(0),
                source_duration: None,
            },
            enabled: true,
            channels: vec![],
            content: ElementContent::Graphic {
                graphic: Graphic {
                    geometry: GraphicGeometry::Rectangle {
                        corner_radius_percent: 0.0,
                    },
                    source_size: CanvasSize {
                        width: 8,
                        height: 8,
                    },
                    fill: color(),
                    stroke: Stroke {
                        color: color(),
                        width_px: 0.0,
                        align: StrokeAlign::Center,
                    },
                },
            },
            visual: Some(VisualStyle {
                profile: VisualProfile::ClassicCanvasV31,
                transform: Transform {
                    position_x_px: 0.0,
                    position_y_px: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation_degrees: 0.0,
                },
                opacity: 0.5,
                blend_mode: *blend_mode,
                effects: vec![],
                masks: vec![],
            }),
        });
        tracks.push(track);
    }
    EditorSnapshot {
        revision: "fixed-blends".into(),
        project: EditorDocument {
            schema_version: 4,
            id: "project".into(),
            name: "Blends".into(),
            default_sequence_id: "scene".into(),
            settings: SettingsProfile::ClassicGlobal {
                settings: ProjectSettings {
                    fps: FrameRate {
                        numerator: 30,
                        denominator: 1,
                    },
                    canvas_size: CanvasSize {
                        width: 16,
                        height: 8,
                    },
                    canvas_size_mode: CanvasSizeMode::Custom,
                    last_custom_canvas_size: None,
                    original_canvas_size: None,
                    background: Background::Color { color: color() },
                },
            },
            assets: vec![],
            fonts: vec![],
            sequences: vec![Scene {
                id: "scene".into(),
                name: "Scene".into(),
                main_track_id: "main".into(),
                tracks,
                captions: vec![],
                transitions: vec![],
                bookmarks: vec![],
            }],
        },
    }
}
fn graph() -> SceneComposition {
    compile_frame_scene(&source(), "scene", &t(1), CompositionScope::Visual).unwrap()
}

#[test]
fn all_seventeen_authored_modes_survive_sampling_partition_and_order() {
    let graph = graph();
    assert_eq!(
        graph
            .layers
            .iter()
            .map(|l| l.visual.as_ref().unwrap().blend_mode)
            .collect::<Vec<_>>(),
        MODES
    );
    let prepared = prepare_blend_frame(&graph).unwrap();
    assert_eq!(prepared.source_graph(), &graph);
    assert_eq!(
        prepared
            .blends()
            .iter()
            .map(|l| l.blend_mode)
            .collect::<Vec<_>>(),
        MODES
    );
    assert!(
        graph
            .layers
            .iter()
            .all(|l| l.visual.as_ref().unwrap().effective_opacity == 0.25)
    );
    let mut expected_partition = graph.clone();
    for layer in &mut expected_partition.layers {
        layer.visual.as_mut().unwrap().blend_mode = BlendMode::Normal;
    }
    assert_eq!(prepared.execution().source_graph(), &expected_partition);
    assert_eq!(
        prepared.execution().order(),
        prepared
            .blends()
            .iter()
            .map(|l| l.element_id.clone())
            .collect::<Vec<_>>()
    );
    assert!(prepare_graphic_frame(&graph).is_err());
    assert!(prepare_graphic_frame(&expected_partition).is_ok());
}

#[test]
fn blend_facts_require_exact_identity_order_modes_and_no_quad_layers() {
    let mut graph = graph();
    graph.layers[2].visual.as_mut().unwrap().transform.scale_x = 0.0;
    let prepared = prepare_blend_frame(&graph).unwrap();
    assert!(prepared.execution().graphics()[2].quad.is_none());
    assert_eq!(prepared.blends()[2].blend_mode, BlendMode::Multiply);
    validate_blend_layers(&prepared, prepared.blends()).unwrap();
    for case in 0..5 {
        let mut changed = prepared.blends().to_vec();
        match case {
            0 => {
                changed.pop();
            }
            1 => changed.push(changed[0].clone()),
            2 => changed.swap(0, 1),
            3 => changed[0].element_id = "other".into(),
            _ => changed[2].blend_mode = BlendMode::Normal,
        }
        assert!(validate_blend_layers(&prepared, &changed).is_err());
    }
}

#[test]
fn blend_wrapper_does_not_relax_other_execution_capabilities() {
    for case in 0..6 {
        let mut graph = graph();
        match case {
            0 => {
                graph.layers[0].visual.as_mut().unwrap().profile =
                    VisualProfile::LegacyCompositionV3
            }
            1 => graph.layers[0]
                .visual
                .as_mut()
                .unwrap()
                .effects
                .push(SceneEffect {
                    id: "effect".into(),
                    enabled: true,
                    effect: EffectKind::ClassicBlur { intensity: 1.0 },
                }),
            2 => graph.layers[0].visual.as_mut().unwrap().masks.push(Mask {
                id: "mask".into(),
                enabled: true,
                inverted: false,
                feather: MaskFeather::ClassicPixels { value: 0.0 },
                stroke: Stroke {
                    color: color(),
                    width_px: 0.0,
                    align: StrokeAlign::Center,
                },
                shape: LayerMaskShape::Split {
                    center_x: 0.0,
                    center_y: 0.0,
                    rotation_degrees: 0.0,
                },
            }),
            3 => {
                graph.background = CompiledBackground::MainTrackBlur {
                    intensity: 1.0,
                    main_track_id: "main".into(),
                    source_layers: vec![],
                }
            }
            4 => graph.layers[1].element_id = graph.layers[0].element_id.clone(),
            _ => graph.layers[0].visual = None,
        }
        assert!(prepare_blend_frame(&graph).is_err(), "case {case}");
    }
}
