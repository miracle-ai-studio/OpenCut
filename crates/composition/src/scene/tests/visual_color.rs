use super::super::*;
use super::fixture::channel;
use super::fixture::*;

#[test]
fn main_blur_uses_visible_main_media_and_preserves_only_legacy_sticker_participation() {
    let mut s = snapshot();
    // Mixed element profiles remain authoritative even under legacy settings.
    s.project.settings = SettingsProfile::LegacyPerScene {
        formats: vec![LegacySequenceFormat {
            sequence_id: "scene".into(),
            fps: FrameRate {
                numerator: 30,
                denominator: 1,
            },
            canvas_size: CanvasSize {
                width: 160,
                height: 90,
            },
            background: Background::MainTrackBlur { intensity: 20.0 },
        }],
    };
    let mut image_asset = s.project.assets[0].clone();
    image_asset.id = "image-asset".into();
    image_asset.kind = AssetKind::Image;
    image_asset.duration = None;
    image_asset.metadata.frame_rate = None;
    image_asset.metadata.mime_type = Some("image/png".into());
    image_asset.metadata.audio = AudioPresence::Absent {};
    s.project.assets.push(image_asset);
    let image_content = || ElementContent::Image {
        asset_id: "image-asset".into(),
    };
    let sticker_content = || ElementContent::Sticker {
        asset_id: "image-asset".into(),
        intrinsic_size: CanvasSize {
            width: 10,
            height: 10,
        },
    };
    // An opaque overlay does not become a backdrop source or hide the original
    // main-track sources from background preparation.
    let mut overlay = element("overlay-image", image_content());
    overlay.visual.as_mut().unwrap().opacity = 1.0;
    let mut overlay_track = track("overlay", SceneTrackKind::Video, vec![overlay]);
    overlay_track.opacity = 1.0;
    s.project.sequences[0].tracks.push(overlay_track);
    let video_content = s.project.sequences[0].tracks[0].elements[0].content.clone();
    for (content, profile, participates, exclusion) in [
        (video_content, VisualProfile::ClassicCanvasV31, true, None),
        (image_content(), VisualProfile::ClassicCanvasV31, true, None),
        (
            sticker_content(),
            VisualProfile::ClassicCanvasV31,
            false,
            None,
        ),
        (
            sticker_content(),
            VisualProfile::LegacyCompositionV3,
            true,
            None,
        ),
        (
            image_content(),
            VisualProfile::ClassicCanvasV31,
            false,
            Some(Exclusion::DisabledElement),
        ),
        (
            image_content(),
            VisualProfile::ClassicCanvasV31,
            false,
            Some(Exclusion::OutsideFrame),
        ),
        (
            image_content(),
            VisualProfile::ClassicCanvasV31,
            false,
            Some(Exclusion::DisabledTrack),
        ),
    ] {
        // One element per edit track: every case is a valid nonoverlapping
        // aggregate. Test both settings profiles to avoid treating settings as
        // the authority for each layer's rendering profile.
        for classic_settings in [false, true] {
            let mut case = s.clone();
            if classic_settings {
                case.project.settings = snapshot().project.settings;
                let SettingsProfile::ClassicGlobal { settings } = &mut case.project.settings else {
                    panic!()
                };
                settings.background = Background::MainTrackBlur { intensity: 20.0 };
            }
            let mut source = element("main-source", content.clone());
            source.visual.as_mut().unwrap().profile = profile;
            if exclusion == Some(Exclusion::DisabledElement) {
                source.enabled = false;
            }
            if exclusion == Some(Exclusion::OutsideFrame) {
                source.timeline_range.start = t(3, 1);
            }
            let main = &mut case.project.sequences[0].tracks[0];
            main.elements = vec![source];
            main.enabled = exclusion != Some(Exclusion::DisabledTrack);
            let graph = compile(&case);
            let CompiledBackground::MainTrackBlur { source_layers, .. } = &graph.background else {
                panic!()
            };
            let expected: Vec<String> = if participates {
                vec!["main-source".into()]
            } else {
                vec![]
            };
            assert_eq!(source_layers, &expected);
            if let Some(reason) = exclusion {
                excluded(&graph, "main-source", reason);
            } else {
                assert!(graph.layers.iter().any(|l| l.element_id == "main-source"));
            }
            assert!(graph.layers.iter().any(|l| l.element_id == "overlay-image"));
        }
    }
}

#[test]
fn fixed_snapshot_compiles_independent_layers_cas_and_legacy_opacity_policy() {
    let s = snapshot();
    let before = s.clone();
    let graph = compile(&s);
    assert_eq!(graph.revision, "fixed-r7");
    assert_eq!(graph.time, t(3, 2));
    assert_eq!(
        graph.format.fps,
        FrameRate {
            numerator: 30000,
            denominator: 1001
        }
    );
    assert_eq!(graph.resources[0].version, version());
    assert_eq!(
        graph
            .layers
            .iter()
            .map(|l| l.element_id.as_str())
            .collect::<Vec<_>>(),
        vec!["video", "shape"]
    );
    close(
        graph.layers[0].visual.as_ref().unwrap().effective_opacity,
        0.2,
    );
    close(
        graph.layers[1].visual.as_ref().unwrap().effective_opacity,
        0.8,
    );
    assert_eq!(
        graph.layers[1].visual.as_ref().unwrap().opacity_policy,
        LayerOpacityPolicy::LegacyShapeElementOnly
    );
    let SampledContent::Video { source, .. } = &graph.layers[0].content else {
        panic!()
    };
    assert_eq!(
        time(&source.timeline_local)
            .unwrap()
            .cmp_exact(time(&t(1, 1)).unwrap()),
        Ordering::Equal
    );
    assert_eq!(
        time(&source.source_time)
            .unwrap()
            .cmp_exact(time(&t(2, 1)).unwrap()),
        Ordering::Equal
    );
    assert_eq!(
        s, before,
        "compilation cannot create a mutable second document"
    );
    let ids: Vec<_> = graph
        .coverage
        .iter()
        .filter(|e| e.kind != SourceNodeKind::Settings)
        .map(|e| e.source_id.as_str())
        .collect();
    assert_eq!(ids.len(), ids.iter().copied().collect::<HashSet<_>>().len());
}

#[test]
fn mask_units_and_stable_effect_targets_reach_sampled_execution_fields() {
    let mut s = snapshot();
    let e = &mut s.project.sequences[0].tracks[0].elements[0];
    let style = e.visual.as_mut().unwrap();
    style.masks.push(Mask {
        id: "pixels-mask".into(),
        enabled: true,
        shape: LayerMaskShape::Rectangle {
            geometry: BoxMaskGeometry {
                center_x: 0.1,
                center_y: -0.1,
                width: 0.001,
                height: 0.25,
                rotation_degrees: 0.0,
                scale: 1.0,
            },
        },
        inverted: false,
        feather: MaskFeather::ClassicPixels { value: 4.0 },
        stroke: Stroke {
            color: color(),
            width_px: 0.0,
            align: StrokeAlign::Center,
        },
    });
    style.masks.push(Mask {
        id: "canvas-mask".into(),
        enabled: true,
        shape: LayerMaskShape::LegacyEllipse {
            x: 0.0,
            y: 0.0,
            width: 0.5,
            height: 0.5,
        },
        inverted: true,
        feather: MaskFeather::LegacyCanvasFraction { value: 0.12 },
        stroke: Stroke {
            color: color(),
            width_px: 0.0,
            align: StrokeAlign::Center,
        },
    });
    style.effects.push(SceneEffect {
        id: "blur".into(),
        enabled: true,
        effect: EffectKind::ClassicBlur { intensity: 10.0 },
    });
    e.channels.push(channel(
        "mask-feather",
        ChannelTarget::Mask {
            element_id: "video".into(),
            mask_id: "pixels-mask".into(),
        },
        ScalarParameter::MaskFeatherClassicPixels,
        vec![
            key("feather-a", t(0, 1), 2.0),
            key("feather-b", t(2, 1), 6.0),
        ],
    ));
    e.channels.push(channel(
        "blur-c",
        ChannelTarget::Effect {
            element_id: "video".into(),
            effect_id: "blur".into(),
        },
        ScalarParameter::ClassicBlurIntensity,
        vec![key("blur-a", t(0, 1), 0.0), key("blur-b", t(2, 1), 200.0)],
    ));
    let graph = compile(&s);
    let v = graph.layers[0].visual.as_ref().unwrap();
    assert_eq!(
        v.masks[0].feather,
        MaskFeather::ClassicPixels { value: 4.0 }
    );
    assert_eq!(
        v.masks[1].feather,
        MaskFeather::LegacyCanvasFraction { value: 0.12 }
    );
    let LayerMaskShape::Rectangle { geometry } = &v.masks[0].shape else {
        panic!()
    };
    close(geometry.width, 0.01);
    assert_eq!(
        v.effects[0].effect,
        EffectKind::ClassicBlur { intensity: 100.0 }
    );
    let requirements = required_capabilities_scene(&graph);
    assert!(
        requirements
            .iter()
            .any(|r| r.feature == NativeFeature::MaskFeatherPixels)
    );
    assert!(
        requirements
            .iter()
            .any(|r| r.feature == NativeFeature::MaskFeatherCanvasFraction)
    );
    assert_eq!(
        ensure_capabilities_scene(&graph, &[]).unwrap_err().code,
        "UNSUPPORTED_V4"
    );
}

#[test]
fn color_component_channels_convert_untouched_srgb_components_and_background_requires_linear_support()
 {
    let mut s = snapshot();
    let shape = &mut s.project.sequences[0].tracks[1].elements[0];
    if let ElementContent::Graphic { graphic } = &mut shape.content {
        graphic.fill = Color {
            red: 0.5,
            green: 0.25,
            blue: 0.75,
            alpha: 0.8,
            encoding: ColorEncoding::Srgb,
        };
    }
    shape.channels.push(AnimationChannel::Scalar {
        id: "red-channel".into(),
        target: ChannelTarget::Element {
            element_id: "shape".into(),
        },
        binding: ScalarBinding::ColorComponent {
            parameter: ColorParameter::GraphicFill,
            component: ColorComponent::Red,
        },
        keys: vec![key("red-key", t(0, 1), 0.1)],
        extrapolation: Extrapolation {
            before: ExtrapolationMode::Hold,
            after: ExtrapolationMode::Hold,
        },
    });
    if let SettingsProfile::ClassicGlobal { settings } = &mut s.project.settings {
        settings.background = Background::Color {
            color: Color {
                encoding: ColorEncoding::LinearSrgb,
                ..color()
            },
        };
    }
    let graph = compile(&s);
    let SampledContent::Graphic { graphic } = &graph.layers[1].content else {
        panic!()
    };
    assert_eq!(graphic.fill.encoding, ColorEncoding::LinearSrgb);
    close(graphic.fill.red, 0.1);
    close(graphic.fill.green, 0.05087608817155679);
    close(graphic.fill.blue, 0.5225215539683921);
    close(graphic.fill.alpha, 0.8);
    assert!(
        required_capabilities_scene(&graph)
            .iter()
            .any(|r| r.source_id == "scene" && r.feature == NativeFeature::LinearSrgbColor)
    );
}

#[test]
fn legacy_crossfade_retains_source_handles_and_alpha_over_instead_of_double_fading() {
    let mut s = snapshot();
    let first = &mut s.project.sequences[0].tracks[0].elements[0];
    first.timeline_range.duration = t(1, 1);
    first.source_span = ElementSourceSpan::LegacySourceRange {
        range: range(t(1, 1), t(1, 1)),
    };
    let mut next = first.clone();
    next.id = "incoming".into();
    next.timeline_range.start = t(3, 2);
    s.project.sequences[0].tracks[0].elements.push(next);
    s.project.sequences[0].transitions.push(SceneTransition {
        id: "fade".into(),
        outgoing_element_id: "video".into(),
        incoming_element_id: "incoming".into(),
        duration: t(1, 1),
        transition: TransitionKind::LegacyCrossfadeV3,
    });
    let graph = compile_frame_scene(&s, "scene", &t(5, 4), CompositionScope::Visual).unwrap();
    assert_eq!(graph.transitions[0].incoming_weight, 0.25);
    assert_eq!(graph.transitions[0].outgoing_weight, 1.0);
    let incoming = graph
        .layers
        .iter()
        .find(|l| l.element_id == "incoming")
        .unwrap();
    let SampledContent::Video { source, .. } = &incoming.content else {
        panic!()
    };
    assert_eq!(
        time(&source.timeline_local)
            .unwrap()
            .cmp_exact(time(&t(-1, 4)).unwrap()),
        Ordering::Equal
    );
    assert_eq!(
        time(&source.source_time)
            .unwrap()
            .cmp_exact(time(&t(3, 4)).unwrap()),
        Ordering::Equal
    );
    assert_eq!(
        time(&source.requested_read_span.start)
            .unwrap()
            .cmp_exact(time(&t(1, 2)).unwrap()),
        Ordering::Equal
    );
    assert_eq!(source.sample, SourceReadState::InBounds);
}

#[test]
fn scene_frame_delegates_crossfade_weights_without_losing_source_handles_or_identity() {
    let mut s = snapshot();
    s.project.sequences[0].tracks.truncate(1);
    let track = &mut s.project.sequences[0].tracks[0];
    track.elements.truncate(1);
    track.opacity = 1.0;
    let first = &mut track.elements[0];
    first.visual.as_mut().unwrap().profile = VisualProfile::ClassicCanvasV31;
    first.visual.as_mut().unwrap().opacity = 1.0;
    first.timeline_range.start = t(0, 1);
    first.timeline_range.duration = t(1, 1);
    first.source_span = ElementSourceSpan::LegacySourceRange {
        range: range(t(1, 1), t(1, 1)),
    };
    let mut next = first.clone();
    next.id = "incoming".into();
    next.timeline_range.start = t(1, 1);
    track.elements.push(next);
    s.project.sequences[0].transitions.push(SceneTransition {
        id: "fade".into(),
        outgoing_element_id: "video".into(),
        incoming_element_id: "incoming".into(),
        duration: t(1, 2),
        transition: TransitionKind::LegacyCrossfadeV3,
    });
    for (at, expected) in [(t(3, 4), 0.0), (t(1, 1), 0.5), (t(9, 8), 0.75)] {
        let graph = compile_frame_scene(&s, "scene", &at, CompositionScope::Visual).unwrap();
        let prepared = prepare_scene_frame(&graph).unwrap();
        assert_eq!(prepared.source_graph().transitions.len(), 1);
        let media = prepared.execution().execution().execution().media();
        assert_eq!(media.layers().len(), 2);
        assert_eq!(
            media.layers()[0].visual.as_ref().unwrap().effective_opacity,
            1.0
        );
        assert_eq!(
            media.layers()[1].visual.as_ref().unwrap().effective_opacity,
            expected
        );
        assert_eq!(media.layers()[1].content, graph.layers[1].content);
        assert_eq!(
            graph.layers[1].visual.as_ref().unwrap().effective_opacity,
            1.0
        );
    }
    let graph = compile_frame_scene(&s, "scene", &t(1, 1), CompositionScope::Visual).unwrap();
    let mut invalid = graph.clone();
    invalid.transitions[0].incoming_weight = f64::NAN;
    assert!(prepare_scene_frame(&invalid).is_err());
    let mut invalid = graph.clone();
    invalid.transitions.push(invalid.transitions[0].clone());
    assert!(prepare_scene_frame(&invalid).is_err());
    let mut invalid = graph;
    invalid.layers.remove(0);
    assert!(prepare_scene_frame(&invalid).is_err());
}
