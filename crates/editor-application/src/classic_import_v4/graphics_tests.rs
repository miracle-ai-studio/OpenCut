use super::*;
use opencut_composition::{
    CompositionScope,
    project_v4::{SampledContentV4, compile_frame_v4},
};
use serde_json::json;

fn track(id: &str, definition: &str, params: Value) -> Value {
    json!({"id":id,"name":format!("Original {id}"),"type":"graphic","hidden":false,"elements":[{
        "id":format!("{id}-shape"),"name":"Original graphic","type":"graphic","definitionId":definition,
        "startTime":120000,"duration":240000,"trimStart":60000,"trimEnd":90000,
        "params":params,"effects":[],"masks":[],"animations":{}
    }]})
}
fn graphic_input(definition: &str, params: Value) -> ClassicAnalyzeV4 {
    changed(|v| {
        v["scenes"][0]["tracks"]["overlay"] = json!([track("graphics", definition, params)])
    })
}
fn parsed(input: &ClassicAnalyzeV4) -> ProjectDocumentV4 {
    let prepared = analyze(input, &resources()).unwrap();
    assert!(
        prepared.analysis.blockers.is_empty(),
        "{:?}",
        prepared.analysis.blockers
    );
    assert!(
        prepared.analysis.losses.is_empty(),
        "{:?}",
        prepared.analysis.losses
    );
    assert!(prepared.analysis.unresolved_fonts.is_empty());
    prepared.document.unwrap()
}
fn shape(document: &ProjectDocumentV4) -> &GraphicV4 {
    let ElementContentV4::Graphic { graphic } =
        &document.sequences[0].tracks[1].elements[0].content
    else {
        panic!()
    };
    graphic
}
fn mutate(input: &ClassicAnalyzeV4, edit: impl FnOnce(&mut Value)) -> ClassicAnalyzeV4 {
    let mut input = ClassicAnalyzeV4 {
        content: input.content.clone(),
        ..super::input()
    };
    let mut value = serde_json::from_str(&input.content).unwrap();
    edit(&mut value);
    input.content = value.to_string();
    input
}
fn element(value: &mut Value) -> &mut Value {
    &mut value["scenes"][0]["tracks"]["overlay"][0]["elements"][0]
}
fn scalar_key(id: &str, time: i64, value: f64) -> Value {
    json!({"id":id,"time":time,"value":value,"segmentToNext":"linear","tangentMode":"auto"})
}
fn color_channels(prefix: &str) -> Value {
    json!({
        "r":{"keys":[scalar_key(&format!("{prefix}-r"),0,0.25)]},
        "g":{"keys":[scalar_key(&format!("{prefix}-g"),0,0.5)]},
        "b":{"keys":[scalar_key(&format!("{prefix}-b"),0,0.75)]},
        "a":{"keys":[scalar_key(&format!("{prefix}-a"),0,0.3)]}
    })
}
fn sample(
    document: &ProjectDocumentV4,
    ticks: i64,
) -> opencut_composition::project_v4::CompiledLayerV4 {
    let snapshot = ProjectSnapshotV4 {
        project: document.clone(),
        revision: "converted-fixed".into(),
    };
    let graph = compile_frame_v4(&snapshot, "scene", &t(ticks), CompositionScope::Visual).unwrap();
    assert_eq!(graph.layers.len(), 1);
    graph.layers.into_iter().next().unwrap()
}

#[test]
fn classic_import_v4_four_graphic_defaults_match_classic_definitions_and_source_size() {
    for (definition, geometry) in [
        (
            "rectangle",
            GraphicGeometryV4::Rectangle {
                corner_radius_percent: 0.,
            },
        ),
        ("ellipse", GraphicGeometryV4::Ellipse),
        (
            "polygon",
            GraphicGeometryV4::Polygon {
                sides: 5.,
                corner_radius_percent: 0.,
            },
        ),
        (
            "star",
            GraphicGeometryV4::Star {
                points: 5.,
                depth_percent: 45.,
            },
        ),
    ] {
        let document = parsed(&graphic_input(definition, json!({})));
        let graphic = shape(&document);
        assert_eq!(graphic.geometry, geometry);
        assert_eq!(
            graphic.source_size,
            CanvasSizeV4 {
                width: 512,
                height: 512
            }
        );
        assert_eq!(graphic.fill, color("#ffffff").unwrap());
        assert_eq!(graphic.stroke.color, color("#000000").unwrap());
        assert_eq!(graphic.stroke.width_px, 0.);
        assert_eq!(graphic.stroke.align, StrokeAlignV4::Center);
        assert!(document.assets.is_empty() && document.fonts.is_empty());
        assert_eq!(document.sequences[0].tracks[1].kind, TrackKindV4::Graphic);
    }
}

#[test]
fn classic_import_v4_graphics_preserve_authored_parameters_style_trims_and_hidden_state() {
    for (definition, specific, geometry) in [
        (
            "rectangle",
            json!({"cornerRadius":75}),
            GraphicGeometryV4::Rectangle {
                corner_radius_percent: 75.,
            },
        ),
        ("ellipse", json!({}), GraphicGeometryV4::Ellipse),
        (
            "polygon",
            json!({"sides":24.5,"cornerRadius":25.25}),
            GraphicGeometryV4::Polygon {
                sides: 24.5,
                corner_radius_percent: 25.25,
            },
        ),
        (
            "star",
            json!({"points":2.5,"depth":100}),
            GraphicGeometryV4::Star {
                points: 2.5,
                depth_percent: 100.,
            },
        ),
    ] {
        for (align, expected_align) in [
            ("inside", StrokeAlignV4::Inside),
            ("center", StrokeAlignV4::Center),
            ("outside", StrokeAlignV4::Outside),
        ] {
            let mut params = json!({"fill":"#10203040","stroke":"#abcdef80","strokeWidth":90.5,"strokeAlign":align,
                "transform.positionX":23,"transform.positionY":-11,"transform.scaleX":1.5,"transform.scaleY":0.75,"transform.rotate":-17,"opacity":0.5,"blendMode":"multiply"});
            params
                .as_object_mut()
                .unwrap()
                .extend(specific.as_object().unwrap().clone());
            let input = mutate(&graphic_input(definition, params), |v| {
                v["scenes"][0]["tracks"]["overlay"][0]["hidden"] = json!(true);
                element(v)["hidden"] = json!(true);
            });
            let document = parsed(&input);
            let graphic = shape(&document);
            assert_eq!(graphic.geometry, geometry);
            assert_eq!(graphic.fill, color("#10203040").unwrap());
            assert_eq!(graphic.stroke.color, color("#abcdef80").unwrap());
            assert_eq!(graphic.stroke.width_px, 90.5);
            assert_eq!(graphic.stroke.align, expected_align);
            let track = &document.sequences[0].tracks[1];
            let element = &track.elements[0];
            assert!(!track.enabled && !element.enabled);
            assert_eq!(element.name, "Original graphic");
            assert_eq!(
                element.timeline_range,
                TimeRange {
                    start: t(120000),
                    duration: t(240000)
                }
            );
            assert_eq!(
                element.source_span,
                ElementSourceSpanV4::ClassicTrimmed {
                    trim_start: t(60000),
                    trim_end: t(90000),
                    source_duration: None
                }
            );
            let visual = element.visual.as_ref().unwrap();
            assert_eq!(
                visual.transform,
                TransformV4 {
                    position_x_px: 23.,
                    position_y_px: -11.,
                    scale_x: 1.5,
                    scale_y: 0.75,
                    rotation_degrees: -17.
                }
            );
            assert_eq!(visual.opacity, 0.5);
            assert_eq!(visual.blend_mode, BlendModeV4::Multiply);
        }
    }
}

#[test]
fn classic_import_v4_mixed_graphic_text_overlays_preserve_bottom_to_top_order() {
    let input = changed(|v| {
        let text = v["scenes"][0]["tracks"]["overlay"][0].clone();
        v["scenes"][0]["tracks"]["overlay"] = json!([
            track("top", "star", json!({})),
            text,
            track("bottom", "ellipse", json!({}))
        ]);
    });
    let prepared = analyze(&input, &resources()).unwrap();
    assert!(
        prepared.analysis.blockers.is_empty(),
        "{:?}",
        prepared.analysis.blockers
    );
    let document = prepared.document.unwrap();
    assert_eq!(
        document.sequences[0]
            .tracks
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        ["video", "bottom", "text-track", "top"]
    );
    assert!(matches!(
        document.sequences[0].tracks[2].elements[0].content,
        ElementContentV4::Text { .. }
    ));
    assert!(document.sequences[0].captions.is_empty());
}

#[test]
fn classic_import_v4_graphic_scalar_color_and_discrete_curves_preserve_original_clock() {
    let input = mutate(
        &graphic_input("polygon", json!({"strokeAlign":"center"})),
        |v| {
            element(v)["animations"] = json!({
                "transform.positionX":{"keys":[scalar_key("x0",0,10.),scalar_key("x1",120000,20.)]},
                "params.sides":{"keys":[scalar_key("sides",0,3.5)]},
                "params.cornerRadius":{"keys":[key("radius",75.)],"extrapolation":{"before":"linear","after":"hold"}},
                "params.strokeWidth":{"keys":[scalar_key("width",0,6.25)]},
                "params.fill":color_channels("fill"),"params.stroke":color_channels("stroke"),
                "params.strokeAlign":{"keys":[{"id":"align-in","time":60000,"value":"inside"},{"id":"align-out","time":120000,"value":"outside"}]}
            });
        },
    );
    let document = parsed(&input);
    let channels = &document.sequences[0].tracks[1].elements[0].channels;
    assert_eq!(channels.len(), 13);
    let AnimationChannelV4::Scalar {
        keys,
        extrapolation,
        ..
    } = channels
        .iter()
        .find(|c| {
            matches!(
                c,
                AnimationChannelV4::Scalar {
                    binding: ScalarBindingV4::Property {
                        parameter: ScalarParameterV4::GraphicCornerRadiusPercent
                    },
                    ..
                }
            )
        })
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(keys[0].value, 75.);
    assert_eq!(keys[0].id, "radius");
    assert_eq!(
        keys[0].right_handle,
        Some(CurveHandleV4 {
            dt: t(10000),
            dv: 0.2
        })
    );
    assert_eq!(extrapolation.before, ExtrapolationModeV4::Linear);
    let AnimationChannelV4::Discrete { keys, .. } = channels
        .iter()
        .find(|c| matches!(c, AnimationChannelV4::Discrete { .. }))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(keys.len(), 3);
    assert_eq!(keys[0].time, t(0));
    assert_eq!(
        keys[0].value,
        DiscreteValueV4::StrokeAlign {
            value: StrokeAlignV4::Center
        }
    );
    assert_eq!(
        (&keys[1].id, &keys[1].time),
        (&"align-in".to_owned(), &t(60000))
    );
    assert_eq!(
        (&keys[2].id, &keys[2].time),
        (&"align-out".to_owned(), &t(120000))
    );
    assert_eq!(parsed(&input), document);
    for (time, align) in [
        (120000, StrokeAlignV4::Center),
        (179999, StrokeAlignV4::Center),
        (180000, StrokeAlignV4::Inside),
        (240000, StrokeAlignV4::Outside),
    ] {
        let layer = sample(&document, time);
        assert_eq!(
            opencut_time::RationalTime::try_from(&layer.local_time)
                .unwrap()
                .cmp_exact(opencut_time::RationalTime::try_from(&t(time - 120000)).unwrap()),
            std::cmp::Ordering::Equal
        );
        let SampledContentV4::Graphic { graphic } = layer.content else {
            panic!()
        };
        assert_eq!(graphic.stroke.align, align);
        assert_eq!(
            graphic.geometry,
            GraphicGeometryV4::Polygon {
                sides: 4.,
                corner_radius_percent: 50.
            }
        );
        assert_eq!(graphic.stroke.width_px, 6.25);
        assert_eq!(
            graphic.fill,
            ColorV4 {
                encoding: ColorEncodingV4::LinearSrgb,
                red: 0.25,
                green: 0.5,
                blue: 0.75,
                alpha: 0.3
            }
        );
        assert_eq!(graphic.stroke.color, graphic.fill);
        if time == 180000 {
            assert_eq!(layer.visual.unwrap().transform.position_x_px, 15.);
        }
    }
}

#[test]
fn classic_import_v4_star_curves_keep_fractional_counts_and_only_sample_clamps() {
    let input = mutate(&graphic_input("star", json!({})), |v| {
        element(v)["animations"] = json!({"params.points":{"keys":[scalar_key("points",0,2.5)]},"params.depth":{"keys":[scalar_key("depth",0,100.)]}});
    });
    let document = parsed(&input);
    let raw = document.sequences[0].tracks[1].elements[0]
        .channels
        .iter()
        .map(|channel| {
            let AnimationChannelV4::Scalar { keys, .. } = channel else {
                panic!()
            };
            keys[0].value
        })
        .collect::<Vec<_>>();
    assert!(raw.contains(&2.5) && raw.contains(&100.));
    let SampledContentV4::Graphic { graphic } = sample(&document, 120000).content else {
        panic!()
    };
    assert_eq!(
        graphic.geometry,
        GraphicGeometryV4::Star {
            points: 3.,
            depth_percent: 99.
        }
    );
}

#[test]
fn classic_import_v4_unknown_graphic_fields_paths_and_discrete_semantics_block_without_loss() {
    let base = graphic_input("ellipse", json!({}));
    let cases: Vec<(&str, Value, &str)> = vec![
        ("definitionId", json!("custom-shape"), "UNSUPPORTED_GRAPHIC"),
        ("params", json!({"cornerRadius":5}), "UNSUPPORTED_FIELD"),
        (
            "params",
            json!({"fill":"rgba(255,0,0,.5)"}),
            "UNSUPPORTED_COLOR",
        ),
        (
            "params",
            json!({"strokeAlign":"outer"}),
            "UNSUPPORTED_VALUE",
        ),
        ("params", json!({"strokeWidth":"5"}), "INVALID_NUMBER"),
        ("mediaId", json!("unused"), "UNSUPPORTED_FIELD"),
        ("retime", json!({"rate":2}), "UNSUPPORTED_RETIME"),
        (
            "effects",
            json!([{"id":"unmapped-effect"}]),
            "UNSUPPORTED_DEPENDENCY",
        ),
        ("masks", json!([{"id":"unmapped-mask"}]), "UNSUPPORTED_MASK"),
        (
            "animations",
            json!({"params.sides":{"keys":[scalar_key("sides",0,5.)]}}),
            "UNSUPPORTED_ANIMATION",
        ),
        (
            "animations",
            json!({"strokeWidth":{"keys":[scalar_key("width",0,5.)]}}),
            "UNSUPPORTED_ANIMATION",
        ),
        (
            "animations",
            json!({"params.strokeAlign":{"keys":[{"id":"align","time":0,"value":"outer"}]}}),
            "UNSUPPORTED_VALUE",
        ),
        (
            "animations",
            json!({"params.strokeAlign":{"keys":[{"id":"align","time":0,"value":"inside","segmentToNext":"linear"}]}}),
            "UNSUPPORTED_FIELD",
        ),
        (
            "animations",
            json!({"params.strokeAlign":{"keys":[{"id":"align","time":0,"value":"inside"}],"extrapolation":{"before":"linear"}}}),
            "UNSUPPORTED_FIELD",
        ),
        (
            "animations",
            json!({"params.fill":{"r":{"keys":[scalar_key("red",0,0.5)]}}}),
            "INVALID_COLOR_CHANNEL",
        ),
    ];
    for (field, value, code) in cases {
        let input = mutate(&base, |v| element(v)[field] = value);
        let prepared = analyze(&input, &resources()).unwrap();
        assert!(
            prepared.document.is_none() && prepared.analysis.proposed_project.is_none(),
            "accepted {field}"
        );
        assert!(
            prepared
                .analysis
                .blockers
                .iter()
                .any(|finding| finding.code == code && finding.path.contains(field)),
            "{field}: {:?}",
            prepared.analysis.blockers
        );
        assert_eq!(
            prepared.analysis.source_content_id,
            crate::classic_import::source_content_id(&input.content)
        );
    }
    let muted = mutate(&base, |v| {
        v["scenes"][0]["tracks"]["overlay"][0]["muted"] = json!(true)
    });
    assert!(analyze(&muted, &resources()).unwrap().document.is_none());
    let misplaced = changed(|v| title(v)["definitionId"] = json!("rectangle"));
    assert!(
        analyze(&misplaced, &resources())
            .unwrap()
            .document
            .is_none()
    );
}
