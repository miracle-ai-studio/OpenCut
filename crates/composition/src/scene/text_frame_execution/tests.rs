use super::*;

fn t(ticks: i64, timebase: u32) -> Time {
    Time {
        ticks: ticks.to_string(),
        timebase,
    }
}
fn color() -> Color {
    Color {
        encoding: ColorEncoding::Srgb,
        red: 0.25,
        green: 0.5,
        blue: 1.0,
        alpha: 0.75,
    }
}
fn version() -> AssetVersion {
    let hash = "b".repeat(64);
    AssetVersion {
        id: format!("av_{hash}"),
        content_hash: hash,
        size: 1024,
    }
}
fn font() -> FontResource {
    FontResource {
        id: "font".into(),
        family: "Fixed".into(),
        weight: 400,
        style: FontStyle::Normal,
        face_index: 0,
        content: version(),
        origin: ResourceOrigin::LocalImport {
            original_name: "fixed.ttf".into(),
        },
        license: None,
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
        decoration: TextDecoration::None,
        align: SceneTextAlign::Center,
        font_size: FontSize::ClassicHeight90 { value: 15.0 },
        color: color(),
        letter_spacing_px: 0.0,
        line_height_multiplier: 1.2,
        background: TextBackground {
            enabled: false,
            color: color(),
            corner_radius_percent: 0.0,
            padding_x_at_size15: 0.0,
            padding_y_at_size15: 0.0,
            offset_x_px: 0.0,
            offset_y_px: 0.0,
        },
    }
}
fn layer(id: &str, content: SampledContent) -> CompiledLayer {
    CompiledLayer {
        element_id: id.into(),
        track_id: format!("track-{id}"),
        track_order: 0,
        element_order: 0,
        local_time: t(1, 7),
        content,
        visual: Some(SampledVisual {
            profile: VisualProfile::ClassicCanvasV31,
            transform: Transform {
                position_x_px: 0.0,
                position_y_px: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                rotation_degrees: 0.0,
            },
            element_opacity: 0.5,
            track_opacity: 0.5,
            effective_opacity: 0.25,
            opacity_policy: LayerOpacityPolicy::MultiplyTrack,
            blend_mode: BlendMode::Normal,
            effects: vec![],
            masks: vec![],
        }),
    }
}
fn graph() -> SceneComposition {
    let image = || SampledContent::Image {
        asset_id: "image".into(),
    };
    let mut title = layer("title", SampledContent::Text { text: text() });
    title.visual.as_mut().unwrap().transform = Transform {
        position_x_px: 13.0,
        position_y_px: -7.0,
        scale_x: 2.0,
        scale_y: 0.5,
        rotation_degrees: 90.0,
    };
    SceneComposition {
        project_id: "project".into(),
        revision: "fixed-revision".into(),
        sequence_id: "scene".into(),
        scope: CompositionScope::Visual,
        time: t(3, 7),
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
        layers: vec![layer("back", image()), title, layer("front", image())],
        audio: vec![],
        transitions: vec![],
        background: CompiledBackground::Color { color: color() },
        resources: vec![CasResource {
            asset_id: "image".into(),
            version: version(),
            kind: AssetKind::Image,
            duration: None,
            metadata: ProjectMediaMetadata {
                mime_type: Some("image/png".into()),
                intrinsic_size: None,
                frame_rate: None,
                rotation_degrees: None,
                audio: AudioPresence::Absent {},
            },
            license: None,
        }],
        fonts: vec![font()],
        coverage: vec![],
    }
}
fn decoded(id: &str) -> DecodedVisualSource {
    DecodedVisualSource::ReadyImage {
        element_id: id.into(),
        width: 64,
        height: 36,
    }
}
fn binding(plan: &TextRasterPlan) -> TextRasterBinding {
    TextRasterBinding {
        coverage: NativeTextCoverage {
            element_id: plan.identity.element_id.clone(),
            font: plan.font.clone(),
            content: plan.text.content.clone(),
        },
        raster: TextRasterObservation {
            element_id: plan.identity.element_id.clone(),
            font_id: plan.font.id.clone(),
            font_content_hash: plan.font.content.content_hash.clone(),
            font_weight: plan.font.weight,
            font_style: plan.font.style,
            face_index: plan.font.face_index,
            content: plan.text.content.clone(),
            width: plan.identity.width,
            height: plan.identity.height,
        },
    }
}

#[test]
fn mixed_frame_preserves_complete_order_and_delegates_media_without_reapplying_text_transform() {
    let g = graph();
    let prepared = prepare_text_frame(&g).unwrap();
    assert_eq!(prepared.graph(), &g);
    assert_eq!(prepared.identity().profile, CLASSIC_TEXT_FRAME_PROFILE);
    assert_ne!(
        prepared.identity().profile,
        prepared.media().identity().profile
    );
    let bound = bind_text_frame_sources(
        &prepared,
        &[decoded("front"), decoded("back")],
        &[binding(&prepared.texts()[0])],
    )
    .unwrap();
    assert_eq!(bound.identity.time, t(3, 7));
    assert_eq!(
        bound
            .layers
            .iter()
            .map(BoundTextFrameLayer::element_id)
            .collect::<Vec<_>>(),
        ["back", "title", "front"]
    );
    assert_eq!(bound.decoded, [decoded("back"), decoded("front")]);
    assert_eq!(bound.clear, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(bound.background, [0.25, 0.5, 1.0, 0.75]);
    let BoundTextFrameLayer::Media(back) = &bound.layers[0] else {
        panic!()
    };
    let quad = back.quad.as_ref().unwrap();
    assert_eq!(
        (
            quad.center_x,
            quad.center_y,
            quad.width,
            quad.height,
            quad.opacity
        ),
        (320.0, 90.0, 320.0, 180.0, 0.25)
    );
    let BoundTextFrameLayer::Text {
        plan,
        observation,
        quad,
    } = &bound.layers[1]
    else {
        panic!()
    };
    assert_eq!(plan.font, font());
    assert_eq!(observation.content, "AV To\n中文编辑");
    assert_eq!(
        (plan.text.transform.center_x, plan.text.transform.center_y),
        (333.0, 83.0)
    );
    assert_eq!(
        (
            quad.center_x,
            quad.center_y,
            quad.width,
            quad.height,
            quad.rotation_radians,
            quad.opacity
        ),
        (320.0, 90.0, 640.0, 180.0, 0.0, 0.25)
    );
    assert!(!quad.flip_x && !quad.flip_y);
    assert!(prepare_visual_frame_scene(&g).is_err()); // V1 has not gained text admission.
}

#[test]
fn shared_font_multiple_titles_stay_distinct_and_observation_order_does_not_change_layer_order() {
    let mut g = graph();
    let mut second = layer("second-title", SampledContent::Text { text: text() });
    let SampledContent::Text { text } = &mut second.content else {
        panic!()
    };
    text.content = "a distinct title".into();
    g.layers.insert(2, second);
    let prepared = prepare_text_frame(&g).unwrap();
    assert_eq!(prepared.texts().len(), 2);
    let observations = [binding(&prepared.texts()[1]), binding(&prepared.texts()[0])];
    let bound = bind_text_frame_sources(
        &prepared,
        &[decoded("back"), decoded("front")],
        &observations,
    )
    .unwrap();
    assert_eq!(
        bound
            .layers
            .iter()
            .map(BoundTextFrameLayer::element_id)
            .collect::<Vec<_>>(),
        ["back", "title", "second-title", "front"]
    );
    let mut wrong = observations;
    wrong[0].coverage.content = prepared.texts()[0].text.content.clone();
    assert!(
        bind_text_frame_sources(&prepared, &[decoded("back"), decoded("front")], &wrong).is_err()
    );
}

#[test]
fn every_media_and_text_node_requires_one_matching_observation_without_silent_omission() {
    let prepared = prepare_text_frame(&graph()).unwrap();
    let proof = binding(&prepared.texts()[0]);
    for media in [
        vec![],
        vec![decoded("back")],
        vec![decoded("back"), decoded("back")],
        vec![decoded("back"), decoded("other")],
    ] {
        assert!(bind_text_frame_sources(&prepared, &media, std::slice::from_ref(&proof)).is_err());
    }
    let media = [decoded("back"), decoded("front")];
    assert!(bind_text_frame_sources(&prepared, &media, &[]).is_err());
    assert!(bind_text_frame_sources(&prepared, &media, &[proof.clone(), proof.clone()]).is_err());
    let mut wrong = proof;
    wrong.raster.element_id = "unknown".into();
    wrong.coverage.element_id = "unknown".into();
    assert!(bind_text_frame_sources(&prepared, &media, &[wrong]).is_err());
}

#[test]
fn native_coverage_and_browser_font_content_face_and_dimensions_each_remain_fixed() {
    let prepared = prepare_text_frame(&graph()).unwrap();
    for case in 0..11 {
        let mut observed = binding(&prepared.texts()[0]);
        match case {
            0 => observed.coverage.font.content.content_hash = "c".repeat(64),
            1 => observed.coverage.content.push('!'),
            2 => observed.coverage.element_id = "other".into(),
            3 => observed.raster.font_content_hash = "c".repeat(64),
            4 => observed.raster.font_id = "other".into(),
            5 => observed.raster.font_weight = 700,
            6 => observed.raster.font_style = FontStyle::Italic,
            7 => observed.raster.face_index = 1,
            8 => observed.raster.content.push('!'),
            9 => observed.raster.width -= 1,
            10 => observed.raster.height -= 1,
            _ => unreachable!(),
        }
        assert!(
            bind_text_frame_sources(&prepared, &[decoded("back"), decoded("front")], &[observed])
                .is_err(),
            "mismatch case {case}"
        );
    }
}

#[test]
fn unsupported_nodes_and_media_or_text_styles_are_rejected_instead_of_partitioned_away() {
    let mut g = graph();
    g.layers.push(layer(
        "effect",
        SampledContent::EffectLayer {
            effect: SceneEffect {
                id: "blur".into(),
                enabled: true,
                effect: EffectKind::ClassicBlur { intensity: 1.0 },
            },
        },
    ));
    assert!(prepare_text_frame(&g).is_err());
    let mut g = graph();
    g.layers.push(layer(
        "caption",
        SampledContent::Caption {
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
        },
    ));
    assert!(prepare_text_frame(&g).is_err());
    let mut g = graph();
    g.layers.push(layer(
        "sticker",
        SampledContent::Sticker {
            asset_id: "image".into(),
            intrinsic_size: CanvasSize {
                width: 1,
                height: 1,
            },
        },
    ));
    assert!(prepare_text_frame(&g).is_err());
    let mut g = graph();
    g.layers.push(layer(
        "graphic",
        SampledContent::Graphic {
            graphic: Graphic {
                geometry: GraphicGeometry::Rectangle {
                    corner_radius_percent: 0.0,
                },
                source_size: CanvasSize {
                    width: 20,
                    height: 20,
                },
                fill: color(),
                stroke: Stroke {
                    color: color(),
                    width_px: 0.0,
                    align: StrokeAlign::Center,
                },
            },
        },
    ));
    assert!(prepare_text_frame(&g).is_err());
    for index in [0, 1] {
        let mut g = graph();
        g.layers[index].visual.as_mut().unwrap().blend_mode = BlendMode::Screen;
        assert!(prepare_text_frame(&g).is_err());
    }
    let mut g = graph();
    let CompiledBackground::Color { color } = &mut g.background else {
        panic!()
    };
    color.encoding = ColorEncoding::LinearSrgb;
    assert!(prepare_text_frame(&g).is_err());
}

#[test]
fn resources_must_cover_explicit_partitions_and_prepared_frames_cannot_adopt_later_changes() {
    let mut g = graph();
    let prepared = prepare_text_frame(&g).unwrap();
    g.revision = "later".into();
    g.fonts[0].content.content_hash = "c".repeat(64);
    assert_eq!(prepared.identity().revision, "fixed-revision");
    assert_eq!(
        prepared.texts()[0].font.content.content_hash,
        "b".repeat(64)
    );
    let mut g = graph();
    g.fonts.clear();
    assert!(prepare_text_frame(&g).is_err());
    let mut g = graph();
    g.fonts.push(font());
    assert!(prepare_text_frame(&g).is_err());
    let mut g = graph();
    let mut extra = font();
    extra.id = "extra".into();
    g.fonts.push(extra);
    assert!(prepare_text_frame(&g).is_err());
    let mut g = graph();
    g.resources.clear();
    assert!(prepare_text_frame(&g).is_err());
    let mut g = graph();
    let mut extra = g.resources[0].clone();
    extra.asset_id = "extra".into();
    g.resources.push(extra);
    assert!(prepare_text_frame(&g).is_err());
    let mut g = graph();
    g.layers.push(g.layers[1].clone());
    assert!(prepare_text_frame(&g).is_err());
}

#[test]
fn font_only_frames_and_compiled_empty_frames_do_not_require_fake_media_or_unused_font_reads() {
    let mut g = graph();
    g.layers = vec![g.layers[1].clone()];
    g.resources.clear();
    let prepared = prepare_text_frame(&g).unwrap();
    let bound = bind_text_frame_sources(&prepared, &[], &[binding(&prepared.texts()[0])]).unwrap();
    assert_eq!(bound.layers.len(), 1);
    assert!(bound.decoded.is_empty());

    // An imported font belongs to the project but no current frame node uses
    // it. The real compiler reports its exclusion and supplies an empty set.
    let snapshot = EditorSnapshot {
        revision: "empty-frame".into(),
        project: EditorDocument {
            cover: None,
            schema_version: 4,
            id: "project".into(),
            name: "project".into(),
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
                tracks: vec![SceneTrack {
                    id: "main".into(),
                    name: "main".into(),
                    kind: SceneTrackKind::Video,
                    enabled: true,
                    muted: false,
                    locked: false,
                    opacity: 1.0,
                    gain: Gain::Decibels { value: 0.0 },
                    elements: vec![],
                    channels: vec![],
                }],
                captions: vec![],
                transitions: vec![],
                bookmarks: vec![],
            }],
        },
    };
    let graph =
        compile_frame_scene(&snapshot, "scene", &t(0, 1), CompositionScope::Visual).unwrap();
    assert!(graph.fonts.is_empty());
    assert!(graph.coverage.iter().any(|entry| entry.source_id == "font"
        && entry.disposition
            == CoverageDisposition::Excluded {
                reason: Exclusion::UnusedResource
            }));
    let prepared = prepare_text_frame(&graph).unwrap();
    assert!(prepared.texts().is_empty());
    assert!(
        bind_text_frame_sources(&prepared, &[], &[])
            .unwrap()
            .layers
            .is_empty()
    );
}

#[test]
fn video_pts_rejection_and_explicit_no_frame_nodes_keep_the_existing_semantics() {
    let mut g = graph();
    g.layers.pop();
    let range = TimeRange {
        start: t(1, 1),
        duration: t(2, 1),
    };
    g.layers[0].content = SampledContent::Video {
        asset_id: "image".into(),
        stream: None,
        source_audio: SourceAudioDecision::ScopeExcluded,
        source: SourceClock {
            timeline_local: t(1, 7),
            source_time: t(5, 4),
            point_profile: SourcePointProfile::ClassicVideoRoundedTicks,
            rate: RationalRate {
                numerator: 1,
                denominator: 1,
            },
            mapping: RetimeMapping::ClassicRoundedTicks { timebase: 120000 },
            nominal_request: range.clone(),
            requested_read_span: range.clone(),
            declared_resize_extent: Some(t(4, 1)),
            read_window: BoundedReadWindow::Known {
                resource_extent: t(4, 1),
                range: Some(range),
            },
            sample: SourceReadState::InBounds,
            eof_policy: EofPolicy::RequiresBackendProfile,
        },
    };
    g.resources[0].kind = AssetKind::Video;
    let prepared = prepare_text_frame(&g).unwrap();
    let proof = binding(&prepared.texts()[0]);
    let mut observation = DecodedVisualSource::ReadyVideo {
        element_id: "back".into(),
        width: 64,
        height: 36,
        requested_time: t(5, 4),
        selected_video_stream: None,
        actual_timestamp_seconds: 1.2,
        frame_duration_seconds: 0.1,
        eof_held: false,
    };
    assert!(
        bind_text_frame_sources(
            &prepared,
            std::slice::from_ref(&observation),
            std::slice::from_ref(&proof)
        )
        .is_ok()
    );
    let DecodedVisualSource::ReadyVideo {
        actual_timestamp_seconds,
        frame_duration_seconds,
        ..
    } = &mut observation
    else {
        panic!()
    };
    *actual_timestamp_seconds = 1.0;
    *frame_duration_seconds = 0.25;
    assert!(
        bind_text_frame_sources(&prepared, &[observation], std::slice::from_ref(&proof)).is_err()
    );
    let gap = DecodedVisualSource::NoFrame {
        element_id: "back".into(),
        requested_time: t(5, 4),
        reason: VisualNoFrameReason::TimestampGap,
    };
    let bound = bind_text_frame_sources(&prepared, &[gap.clone()], &[proof]).unwrap();
    assert_eq!(bound.layers.len(), 2);
    assert_eq!(bound.decoded, [gap]);
    let BoundTextFrameLayer::Media(media) = &bound.layers[0] else {
        panic!()
    };
    assert!(media.quad.is_none());
    assert_eq!(bound.layers[1].element_id(), "title");
}
