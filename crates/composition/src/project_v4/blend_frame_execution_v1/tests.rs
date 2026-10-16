use super::*;

const MODES: [BlendModeV4; 17] = [
    BlendModeV4::Normal,
    BlendModeV4::Darken,
    BlendModeV4::Multiply,
    BlendModeV4::ColorBurn,
    BlendModeV4::Lighten,
    BlendModeV4::Screen,
    BlendModeV4::PlusLighter,
    BlendModeV4::ColorDodge,
    BlendModeV4::Overlay,
    BlendModeV4::SoftLight,
    BlendModeV4::HardLight,
    BlendModeV4::Difference,
    BlendModeV4::Exclusion,
    BlendModeV4::Hue,
    BlendModeV4::Saturation,
    BlendModeV4::Color,
    BlendModeV4::Luminosity,
];
fn t(ticks: i64) -> Time {
    Time {
        ticks: ticks.to_string(),
        timebase: 1,
    }
}
fn color() -> ColorV4 {
    ColorV4 {
        encoding: ColorEncodingV4::Srgb,
        red: 0.2,
        green: 0.5,
        blue: 0.8,
        alpha: 0.75,
    }
}
fn source() -> ProjectSnapshotV4 {
    let mut tracks = vec![TrackV4 {
        id: "main".into(),
        name: "Main".into(),
        kind: TrackKindV4::Video,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1.0,
        gain: GainV4::Decibels { value: 0.0 },
        elements: vec![],
        channels: vec![],
    }];
    for (index, blend_mode) in MODES.iter().enumerate() {
        let mut track = tracks[0].clone();
        track.id = format!("track-{index}");
        track.kind = TrackKindV4::Graphic;
        track.opacity = 0.5;
        track.elements.push(TimelineElementV4 {
            id: format!("graphic-{index}"),
            name: "Graphic".into(),
            timeline_range: TimeRange {
                start: t(0),
                duration: t(2),
            },
            source_span: ElementSourceSpanV4::ClassicTrimmed {
                trim_start: t(0),
                trim_end: t(0),
                source_duration: None,
            },
            enabled: true,
            channels: vec![],
            content: ElementContentV4::Graphic {
                graphic: GraphicV4 {
                    geometry: GraphicGeometryV4::Rectangle {
                        corner_radius_percent: 0.0,
                    },
                    source_size: CanvasSizeV4 {
                        width: 8,
                        height: 8,
                    },
                    fill: color(),
                    stroke: StrokeV4 {
                        color: color(),
                        width_px: 0.0,
                        align: StrokeAlignV4::Center,
                    },
                },
            },
            visual: Some(VisualStyleV4 {
                profile: VisualProfileV4::ClassicCanvasV31,
                transform: TransformV4 {
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
    ProjectSnapshotV4 {
        revision: "fixed-blends".into(),
        project: ProjectDocumentV4 {
            schema_version: 4,
            id: "project".into(),
            name: "Blends".into(),
            default_sequence_id: "scene".into(),
            settings: SettingsProfileV4::ClassicGlobal {
                settings: ProjectSettingsV4 {
                    fps: FrameRate {
                        numerator: 30,
                        denominator: 1,
                    },
                    canvas_size: CanvasSizeV4 {
                        width: 16,
                        height: 8,
                    },
                    canvas_size_mode: CanvasSizeModeV4::Custom,
                    last_custom_canvas_size: None,
                    original_canvas_size: None,
                    background: BackgroundV4::Color { color: color() },
                },
            },
            assets: vec![],
            fonts: vec![],
            sequences: vec![SequenceV4 {
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
fn graph() -> CompositionV4 {
    compile_frame_v4(&source(), "scene", &t(1), CompositionScope::Visual).unwrap()
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
    let prepared = prepare_blend_frame_v1(&graph).unwrap();
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
        layer.visual.as_mut().unwrap().blend_mode = BlendModeV4::Normal;
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
    assert!(prepare_graphic_frame_v1(&graph).is_err());
    assert!(prepare_graphic_frame_v1(&expected_partition).is_ok());
}

#[test]
fn blend_facts_require_exact_identity_order_modes_and_no_quad_layers() {
    let mut graph = graph();
    graph.layers[2].visual.as_mut().unwrap().transform.scale_x = 0.0;
    let prepared = prepare_blend_frame_v1(&graph).unwrap();
    assert!(prepared.execution().graphics()[2].quad.is_none());
    assert_eq!(prepared.blends()[2].blend_mode, BlendModeV4::Multiply);
    validate_blend_layers_v1(&prepared, prepared.blends()).unwrap();
    for case in 0..5 {
        let mut changed = prepared.blends().to_vec();
        match case {
            0 => {
                changed.pop();
            }
            1 => changed.push(changed[0].clone()),
            2 => changed.swap(0, 1),
            3 => changed[0].element_id = "other".into(),
            _ => changed[2].blend_mode = BlendModeV4::Normal,
        }
        assert!(validate_blend_layers_v1(&prepared, &changed).is_err());
    }
}

#[test]
fn blend_wrapper_does_not_relax_other_execution_capabilities() {
    for case in 0..6 {
        let mut graph = graph();
        match case {
            0 => {
                graph.layers[0].visual.as_mut().unwrap().profile =
                    VisualProfileV4::LegacyCompositionV3
            }
            1 => graph.layers[0]
                .visual
                .as_mut()
                .unwrap()
                .effects
                .push(EffectV4 {
                    id: "effect".into(),
                    enabled: true,
                    effect: EffectKindV4::ClassicBlur { intensity: 1.0 },
                }),
            2 => graph.layers[0].visual.as_mut().unwrap().masks.push(MaskV4 {
                id: "mask".into(),
                enabled: true,
                inverted: false,
                feather: MaskFeatherV4::ClassicPixels { value: 0.0 },
                stroke: StrokeV4 {
                    color: color(),
                    width_px: 0.0,
                    align: StrokeAlignV4::Center,
                },
                shape: MaskShapeV4::Split {
                    center_x: 0.0,
                    center_y: 0.0,
                    rotation_degrees: 0.0,
                },
            }),
            3 => {
                graph.background = CompiledBackgroundV4::MainTrackBlur {
                    intensity: 1.0,
                    main_track_id: "main".into(),
                    source_layers: vec![],
                }
            }
            4 => graph.layers[1].element_id = graph.layers[0].element_id.clone(),
            _ => graph.layers[0].visual = None,
        }
        assert!(prepare_blend_frame_v1(&graph).is_err(), "case {case}");
    }
}
