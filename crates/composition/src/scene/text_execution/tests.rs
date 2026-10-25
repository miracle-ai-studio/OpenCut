use super::*;

#[test]
fn frozen_frame_profiles_reject_explicit_synthesis_even_if_blank_or_unreferenced() {
    use opencut_contracts::font_synthesis::{
        FONT_SYNTHESIS_BINDING_ID_PREFIX, FONT_SYNTHESIS_PROVIDER,
    };
    for state in 0..3 {
        let mut g = graph();
        g.fonts[0].weight = 700;
        g.fonts[0].origin = ResourceOrigin::Generated {
            provider: FONT_SYNTHESIS_PROVIDER.into(),
            generation_id: format!("{FONT_SYNTHESIS_BINDING_ID_PREFIX}{}", "a".repeat(64)),
        };
        if let SampledContent::Text { text } = &mut g.layers[0].content {
            text.font_weight = FontWeight::Bold;
            text.font_style = FontStyle::Italic;
            if state == 1 {
                text.content = "  \n".into();
            }
        }
        if state == 2 {
            g.layers.clear();
        }
        let errors = [
            prepare_text_raster(&g, "text").unwrap_err(),
            prepare_text_frame(&g).unwrap_err(),
            prepare_caption_frame(&g).unwrap_err(),
            prepare_graphic_frame(&g).unwrap_err(),
            prepare_blend_frame(&g).unwrap_err(),
            prepare_mask_frame(&g).unwrap_err(),
            prepare_visual_frame_scene(&g).unwrap_err(),
        ];
        for error in errors {
            assert_eq!(error.code, "UNSUPPORTED_V4");
            assert!(error.message.contains("Explicit font synthesis"));
        }
    }
}

fn t(ticks: i64, timebase: u32) -> Time {
    Time {
        ticks: ticks.to_string(),
        timebase,
    }
}

fn color() -> Color {
    Color {
        encoding: ColorEncoding::Srgb,
        red: 1.0,
        green: 0.25,
        blue: 0.0,
        alpha: 0.75,
    }
}

fn text() -> Text {
    Text {
        content: "AV To\n中文编辑".into(),
        font: FontBinding::Locked {
            font_id: "font".into(),
        },
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        decoration: TextDecoration::Underline,
        align: SceneTextAlign::Center,
        font_size: FontSize::ClassicHeight90 { value: 15.0 },
        color: color(),
        letter_spacing_px: -0.5,
        line_height_multiplier: 1.2,
        background: TextBackground {
            enabled: true,
            color: color(),
            corner_radius_percent: 50.0,
            padding_x_at_size15: 10.0,
            padding_y_at_size15: 6.0,
            offset_x_px: 3.0,
            offset_y_px: -4.0,
        },
    }
}

fn font() -> FontResource {
    let hash = "a".repeat(64);
    FontResource {
        id: "font".into(),
        family: "Fixed test face".into(),
        weight: 400,
        style: FontStyle::Normal,
        face_index: 0,
        content: AssetVersion {
            id: format!("av_{hash}"),
            content_hash: hash,
            size: 1024,
        },
        origin: ResourceOrigin::LocalImport {
            original_name: "fixed.ttf".into(),
        },
        license: Some(ResourceLicense {
            identifier: Some("OFL-1.1".into()),
            source_url: None,
            notice: Some("Fixture metadata; no font bytes are loaded in pure tests".into()),
            attribution: None,
        }),
    }
}

fn transform() -> Transform {
    Transform {
        position_x_px: 3.0,
        position_y_px: -4.0,
        scale_x: 2.0,
        scale_y: 0.5,
        rotation_degrees: 90.0,
    }
}

fn graph() -> SceneComposition {
    SceneComposition {
        project_id: "project".into(),
        revision: "revision".into(),
        sequence_id: "scene".into(),
        scope: CompositionScope::Visual,
        time: t(300, 2100),
        format: CompiledFormat {
            fps: FrameRate {
                numerator: 24,
                denominator: 1,
            },
            canvas_size: CanvasSize {
                width: 640,
                height: 180,
            },
            profile: FormatProfile::ClassicGlobal,
        },
        tracks: vec![],
        layers: vec![CompiledLayer {
            element_id: "text".into(),
            track_id: "titles".into(),
            track_order: 1,
            element_order: 0,
            local_time: t(150, 2100),
            content: SampledContent::Text { text: text() },
            visual: Some(SampledVisual {
                profile: VisualProfile::ClassicCanvasV31,
                transform: transform(),
                element_opacity: 2.5,
                track_opacity: 0.5,
                effective_opacity: 1.25,
                opacity_policy: LayerOpacityPolicy::MultiplyTrack,
                blend_mode: BlendMode::Normal,
                effects: vec![],
                masks: vec![],
            }),
        }],
        audio: vec![],
        transitions: vec![],
        resources: vec![],
        fonts: vec![font()],
        coverage: vec![],
        background: CompiledBackground::Color { color: color() },
    }
}

fn sampled_text(graph: &mut SceneComposition) -> &mut Text {
    let SampledContent::Text { text } = &mut graph.layers[0].content else {
        panic!("fixture text")
    };
    text
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-10,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn classic_units_preserve_exact_clocks_fixed_font_and_independent_layout_geometry() {
    let g = graph();
    let original = g.clone();
    let plan = prepare_text_raster(&g, "text").unwrap();
    assert_eq!(g, original);
    assert_eq!(plan.identity.time, t(300, 2100));
    assert_eq!(plan.identity.local_time, t(150, 2100));
    assert_eq!(plan.font, font());
    assert_eq!(plan.text.content, "AV To\n中文编辑");
    assert_eq!(plan.text.font_size_px, 30.0);
    assert_eq!(plan.text.line_height_px, 36.0);
    assert_eq!(plan.text.background.padding_x_px, 10.0);
    assert_eq!(plan.text.background.padding_y_px, 6.0);
    assert_eq!(plan.text.background.offset_x_px, 3.0);
    assert_eq!(plan.text.background.offset_y_px, -4.0);
    assert_eq!(plan.text.letter_spacing_px, -0.5);
    assert_eq!(plan.text.color, [1.0, 0.25, 0.0, 0.75]);
    assert_eq!(plan.opacity, 1.25); // Already includes track opacity exactly once.
    let transform = plan.text.transform;
    assert_eq!((transform.center_x, transform.center_y), (323.0, 86.0));
    assert_eq!((transform.scale_x, transform.scale_y), (2.0, 0.5));
    assert_eq!(transform.rotation_radians, std::f64::consts::FRAC_PI_2);
    // Independently applying T S R to local (10,4) gives (315,91), not the
    // media T R S result (321,106). No measurement/backend output is the oracle.
    let x = transform.center_x
        + transform.scale_x
            * (10.0 * transform.rotation_radians.cos() - 4.0 * transform.rotation_radians.sin());
    let y = transform.center_y
        + transform.scale_y
            * (10.0 * transform.rotation_radians.sin() + 4.0 * transform.rotation_radians.cos());
    close(x, 315.0);
    close(y, 91.0);
}

#[test]
fn classic_and_canvas_pixel_padding_use_authored_size_not_resolved_pixel_size() {
    let mut g = graph();
    sampled_text(&mut g).font_size = FontSize::ClassicHeight90 { value: 30.0 };
    let classic = prepare_text_raster(&g, "text").unwrap();
    assert_eq!(classic.text.font_size_px, 60.0);
    assert_eq!(classic.text.line_height_px, 72.0);
    assert_eq!(classic.text.background.padding_x_px, 20.0);
    assert_eq!(classic.text.background.offset_x_px, 3.0);
    sampled_text(&mut g).font_size = FontSize::CanvasPixels { value: 30.0 };
    let pixels = prepare_text_raster(&g, "text").unwrap();
    assert_eq!(pixels.text.font_size_px, 30.0);
    assert_eq!(pixels.text.line_height_px, 36.0);
    assert_eq!(pixels.text.background.padding_x_px, 20.0);
    assert_eq!(pixels.text.background.padding_y_px, 12.0);
}

#[test]
fn sampled_negative_padding_and_signed_or_zero_scale_are_not_reprojected() {
    let mut g = graph();
    sampled_text(&mut g).background.padding_x_at_size15 = -5.0;
    sampled_text(&mut g).background.padding_y_at_size15 = -2.0;
    sampled_text(&mut g).background.corner_radius_percent = 130.0;
    let transform = &mut g.layers[0].visual.as_mut().unwrap().transform;
    transform.scale_x = -2.0;
    transform.scale_y = 0.0;
    let plan = prepare_text_raster(&g, "text").unwrap();
    assert_eq!(plan.text.background.padding_x_px, -5.0);
    assert_eq!(plan.text.background.padding_y_px, -2.0);
    // Browser applies the Classic radius clamp when actual width is measured.
    assert_eq!(plan.text.background.corner_radius_percent, 130.0);
    assert_eq!(plan.text.transform.scale_x, -2.0);
    assert_eq!(plan.text.transform.scale_y, 0.0);
}

#[test]
fn non_srgb_invalid_physical_samples_and_overflow_fail_before_native_resource_reads() {
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::MAX] {
        let mut g = graph();
        sampled_text(&mut g).font_size = FontSize::ClassicHeight90 { value: bad };
        assert!(prepare_text_raster(&g, "text").is_err());
        let mut g = graph();
        sampled_text(&mut g).line_height_multiplier = bad;
        assert!(prepare_text_raster(&g, "text").is_err());
    }
    let mut g = graph();
    sampled_text(&mut g).color.encoding = ColorEncoding::LinearSrgb;
    assert_eq!(
        prepare_text_raster(&g, "text").unwrap_err().code,
        "UNSUPPORTED_V4"
    );
    let mut g = graph();
    sampled_text(&mut g).background.color.encoding = ColorEncoding::LinearSrgb;
    assert!(prepare_text_raster(&g, "text").is_err());
    let mut g = graph();
    sampled_text(&mut g).color.alpha = 1.01;
    assert!(prepare_text_raster(&g, "text").is_err());
    let mut g = graph();
    g.layers[0].visual.as_mut().unwrap().transform.position_x_px = f64::MAX;
    assert!(prepare_text_raster(&g, "text").is_err());
}

#[test]
fn font_reference_must_be_unique_locked_and_match_static_weight_style_face() {
    let mut g = graph();
    g.fonts.clear();
    assert!(prepare_text_raster(&g, "text").is_err());
    let mut g = graph();
    g.fonts.push(font());
    assert!(prepare_text_raster(&g, "text").is_err());
    let mut g = graph();
    sampled_text(&mut g).font = FontBinding::UnresolvedFamily {
        family: "Arial".into(),
        policy: opencut_contracts::FontResolutionPolicy::Strict {},
    };
    assert_eq!(
        prepare_text_raster(&g, "text").unwrap_err().code,
        "UNSUPPORTED_V4"
    );
    for (weight, style, face) in [
        (700, FontStyle::Normal, 0),
        (400, FontStyle::Italic, 0),
        (400, FontStyle::Normal, 1),
    ] {
        let mut g = graph();
        g.fonts[0].weight = weight;
        g.fonts[0].style = style;
        g.fonts[0].face_index = face;
        assert!(prepare_text_raster(&g, "text").is_err());
    }
    let mut g = graph();
    g.fonts[0].weight = 700;
    g.fonts[0].style = FontStyle::Italic;
    sampled_text(&mut g).font_weight = FontWeight::Bold;
    sampled_text(&mut g).font_style = FontStyle::Italic;
    assert_eq!(prepare_text_raster(&g, "text").unwrap().font, g.fonts[0]);
}

#[test]
fn caption_profile_blend_effect_and_text_mask_require_explicit_other_capabilities() {
    let mut g = graph();
    g.layers[0].content = SampledContent::Caption {
        text: "caption".into(),
        style: SceneCaptionStyle {
            font: text().font,
            font_size: text().font_size,
            color: color(),
            background_color: None,
            max_width_px: None,
            line_height_px: None,
            text_align: SceneTextAlign::Center,
            layout: CaptionLayoutProfile::PinnedCaptionV4,
        },
    };
    assert!(prepare_text_raster(&g, "text").is_err());
    let mut g = graph();
    g.layers[0].visual.as_mut().unwrap().profile = VisualProfile::LegacyCompositionV3;
    assert!(prepare_text_raster(&g, "text").is_err());
    let mut g = graph();
    g.layers[0].visual.as_mut().unwrap().blend_mode = BlendMode::Multiply;
    assert!(prepare_text_raster(&g, "text").is_err());
    let mut g = graph();
    g.layers[0]
        .visual
        .as_mut()
        .unwrap()
        .effects
        .push(SceneEffect {
            id: "blur".into(),
            enabled: true,
            effect: EffectKind::ClassicBlur { intensity: 5.0 },
        });
    assert!(prepare_text_raster(&g, "text").is_err());
    g.layers[0].visual.as_mut().unwrap().effects[0].enabled = false;
    assert!(prepare_text_raster(&g, "text").is_ok());
    g.layers[0].visual.as_mut().unwrap().masks.push(Mask {
        id: "text-mask".into(),
        enabled: true,
        inverted: false,
        feather: MaskFeather::ClassicPixels { value: 0.0 },
        stroke: Stroke {
            color: color(),
            width_px: 0.0,
            align: StrokeAlign::Center,
        },
        shape: LayerMaskShape::Text {
            mask: TextMask {
                content: "mask".into(),
                font: text().font,
                font_size: text().font_size,
                font_weight: FontWeight::Normal,
                font_style: FontStyle::Normal,
                decoration: TextDecoration::None,
                letter_spacing_px: 0.0,
                line_height_multiplier: 1.2,
                center_x: 0.0,
                center_y: 0.0,
                rotation_degrees: 0.0,
                scale: 1.0,
            },
        },
    });
    assert!(prepare_text_raster(&g, "text").is_err());
}

fn source_snapshot() -> EditorSnapshot {
    let element = TimelineElement {
        id: "text".into(),
        name: "title".into(),
        timeline_range: TimeRange {
            start: t(1, 2),
            duration: t(3, 1),
        },
        source_span: ElementSourceSpan::ClassicTrimmed {
            trim_start: t(7, 3),
            trim_end: t(0, 1),
            source_duration: Some(t(16, 3)),
        },
        enabled: true,
        content: ElementContent::Text { text: text() },
        visual: Some(VisualStyle {
            profile: VisualProfile::ClassicCanvasV31,
            transform: transform(),
            opacity: 0.4,
            blend_mode: BlendMode::Normal,
            effects: vec![],
            masks: vec![],
        }),
        channels: vec![AnimationChannel::Scalar {
            id: "size".into(),
            target: ChannelTarget::Element {
                element_id: "text".into(),
            },
            binding: ScalarBinding::Property {
                parameter: ScalarParameter::FontSizeClassicHeight90,
            },
            keys: [("first", 0, 15.0), ("second", 2, 45.0)]
                .into_iter()
                .map(|(id, at, value)| ScalarKey {
                    id: id.into(),
                    segment_id: None,
                    time: t(at, 1),
                    value,
                    left_handle: None,
                    right_handle: None,
                    segment_to_next: ScalarSegment::Linear,
                    tangent_mode: TangentMode::Flat,
                })
                .collect(),
            extrapolation: Extrapolation {
                before: ExtrapolationMode::Hold,
                after: ExtrapolationMode::Hold,
            },
        }],
    };
    let track = |id: &str, kind, elements| SceneTrack {
        id: id.into(),
        name: id.into(),
        kind,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 0.5,
        gain: Gain::Decibels { value: 0.0 },
        elements,
        channels: vec![],
    };
    EditorSnapshot {
        revision: "source-revision".into(),
        project: EditorDocument {
            schema_version: 4,
            id: "project".into(),
            name: "source".into(),
            default_sequence_id: "scene".into(),
            settings: SettingsProfile::ClassicGlobal {
                settings: ProjectSettings {
                    fps: FrameRate {
                        numerator: 24,
                        denominator: 1,
                    },
                    canvas_size: CanvasSize {
                        width: 640,
                        height: 180,
                    },
                    canvas_size_mode: CanvasSizeMode::Preset,
                    last_custom_canvas_size: None,
                    original_canvas_size: None,
                    background: Background::Color { color: color() },
                },
            },
            assets: vec![],
            fonts: vec![font()],
            sequences: vec![Scene {
                id: "scene".into(),
                name: "scene".into(),
                main_track_id: "main".into(),
                tracks: vec![
                    track("main", SceneTrackKind::Video, vec![]),
                    track("titles", SceneTrackKind::Text, vec![element]),
                ],
                captions: vec![],
                transitions: vec![],
                bookmarks: vec![],
            }],
        },
    }
}

#[test]
fn source_to_sampled_ir_to_text_plan_uses_timeline_local_clock_and_preserves_old_media_gate() {
    let source = source_snapshot();
    let original = source.clone();
    let graph = compile_frame_scene(&source, "scene", &t(3, 2), CompositionScope::Visual).unwrap();
    let plan = prepare_text_raster(&graph, "text").unwrap();
    assert_eq!(source, original);
    assert_eq!(plan.identity.time, t(3, 2));
    assert_eq!(
        time(&plan.identity.local_time).unwrap(),
        time(&t(1, 1)).unwrap()
    );
    assert_eq!(plan.text.font_size_px, 60.0); // At local 1 s, authored size is 30.
    assert_eq!(plan.text.line_height_px, 72.0);
    assert_eq!(plan.text.background.padding_x_px, 20.0);
    close(plan.opacity, 0.2);
    assert_eq!(graph.fonts, vec![font()]);
    assert!(prepare_visual_frame_scene(&graph).is_err()); // Frozen media-only gate unchanged.
    let mut later_source = source;
    let ElementContent::Text { text } =
        &mut later_source.project.sequences[0].tracks[1].elements[0].content
    else {
        panic!()
    };
    text.content = "edited after fixed compilation".into();
    // Preparation consumes the graph, never rereads/resamples mutable source.
    assert_eq!(prepare_text_raster(&graph, "text").unwrap(), plan);
}

#[test]
fn selected_layer_identity_is_unique_and_unrelated_media_is_not_reclassified_as_text() {
    let mut g = graph();
    assert!(prepare_text_raster(&g, "missing").is_err());
    g.layers.push(g.layers[0].clone());
    assert!(prepare_text_raster(&g, "text").is_err());
    let mut g = graph();
    let mut media = g.layers[0].clone();
    media.element_id = "image".into();
    media.content = SampledContent::Image {
        asset_id: "asset".into(),
    };
    g.layers.push(media);
    assert!(prepare_text_raster(&g, "text").is_ok());
    assert!(prepare_text_raster(&g, "image").is_err());
}
