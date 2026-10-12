use super::super::*;
use super::fixture::channel;
use super::fixture::*;

#[test]
fn main_blur_uses_visible_main_media_and_preserves_only_legacy_sticker_participation() {
    let mut s = snapshot();
    // Mixed element profiles remain authoritative even under legacy settings.
    s.project.settings = SettingsProfileV4::LegacyPerScene {
        formats: vec![LegacySequenceFormatV4 {
            sequence_id: "scene".into(),
            fps: FrameRate {
                numerator: 30,
                denominator: 1,
            },
            canvas_size: CanvasSizeV4 {
                width: 160,
                height: 90,
            },
            background: BackgroundV4::MainTrackBlur { intensity: 20.0 },
        }],
    };
    let mut image_asset = s.project.assets[0].clone();
    image_asset.id = "image-asset".into();
    image_asset.kind = AssetKind::Image;
    image_asset.duration = None;
    image_asset.metadata.frame_rate = None;
    image_asset.metadata.mime_type = Some("image/png".into());
    image_asset.metadata.audio = AudioPresenceV4::Absent {};
    s.project.assets.push(image_asset);
    let image_content = || ElementContentV4::Image {
        asset_id: "image-asset".into(),
    };
    let sticker_content = || ElementContentV4::Sticker {
        asset_id: "image-asset".into(),
        intrinsic_size: CanvasSizeV4 {
            width: 10,
            height: 10,
        },
    };
    // An opaque overlay does not become a backdrop source or hide the original
    // main-track sources from background preparation.
    let mut overlay = element("overlay-image", image_content());
    overlay.visual.as_mut().unwrap().opacity = 1.0;
    let mut overlay_track = track("overlay", TrackKindV4::Video, vec![overlay]);
    overlay_track.opacity = 1.0;
    s.project.sequences[0].tracks.push(overlay_track);
    let video_content = s.project.sequences[0].tracks[0].elements[0].content.clone();
    for (content, profile, participates, exclusion) in [
        (video_content, VisualProfileV4::ClassicCanvasV31, true, None),
        (
            image_content(),
            VisualProfileV4::ClassicCanvasV31,
            true,
            None,
        ),
        (
            sticker_content(),
            VisualProfileV4::ClassicCanvasV31,
            false,
            None,
        ),
        (
            sticker_content(),
            VisualProfileV4::LegacyCompositionV3,
            true,
            None,
        ),
        (
            image_content(),
            VisualProfileV4::ClassicCanvasV31,
            false,
            Some(ExclusionV4::DisabledElement),
        ),
        (
            image_content(),
            VisualProfileV4::ClassicCanvasV31,
            false,
            Some(ExclusionV4::OutsideFrame),
        ),
        (
            image_content(),
            VisualProfileV4::ClassicCanvasV31,
            false,
            Some(ExclusionV4::DisabledTrack),
        ),
    ] {
        // One element per edit track: every case is a valid nonoverlapping
        // aggregate. Test both settings profiles to avoid treating settings as
        // the authority for each layer's rendering profile.
        for classic_settings in [false, true] {
            let mut case = s.clone();
            if classic_settings {
                case.project.settings = snapshot().project.settings;
                let SettingsProfileV4::ClassicGlobal { settings } = &mut case.project.settings
                else {
                    panic!()
                };
                settings.background = BackgroundV4::MainTrackBlur { intensity: 20.0 };
            }
            let mut source = element("main-source", content.clone());
            source.visual.as_mut().unwrap().profile = profile;
            if exclusion == Some(ExclusionV4::DisabledElement) {
                source.enabled = false;
            }
            if exclusion == Some(ExclusionV4::OutsideFrame) {
                source.timeline_range.start = t(3, 1);
            }
            let main = &mut case.project.sequences[0].tracks[0];
            main.elements = vec![source];
            main.enabled = exclusion != Some(ExclusionV4::DisabledTrack);
            let graph = compile(&case);
            let CompiledBackgroundV4::MainTrackBlur { source_layers, .. } = &graph.background
            else {
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
        LayerOpacityPolicyV4::LegacyShapeElementOnly
    );
    let SampledContentV4::Video { source, .. } = &graph.layers[0].content else {
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
        .filter(|e| e.kind != SourceNodeKindV4::Settings)
        .map(|e| e.source_id.as_str())
        .collect();
    assert_eq!(ids.len(), ids.iter().copied().collect::<HashSet<_>>().len());
}

#[test]
fn mask_units_and_stable_effect_targets_reach_sampled_execution_fields() {
    let mut s = snapshot();
    let e = &mut s.project.sequences[0].tracks[0].elements[0];
    let style = e.visual.as_mut().unwrap();
    style.masks.push(MaskV4 {
        id: "pixels-mask".into(),
        enabled: true,
        shape: MaskShapeV4::Rectangle {
            geometry: BoxMaskGeometryV4 {
                center_x: 0.1,
                center_y: -0.1,
                width: 0.001,
                height: 0.25,
                rotation_degrees: 0.0,
                scale: 1.0,
            },
        },
        inverted: false,
        feather: MaskFeatherV4::ClassicPixels { value: 4.0 },
        stroke: StrokeV4 {
            color: color(),
            width_px: 0.0,
            align: StrokeAlignV4::Center,
        },
    });
    style.masks.push(MaskV4 {
        id: "canvas-mask".into(),
        enabled: true,
        shape: MaskShapeV4::LegacyEllipse {
            x: 0.0,
            y: 0.0,
            width: 0.5,
            height: 0.5,
        },
        inverted: true,
        feather: MaskFeatherV4::LegacyCanvasFraction { value: 0.12 },
        stroke: StrokeV4 {
            color: color(),
            width_px: 0.0,
            align: StrokeAlignV4::Center,
        },
    });
    style.effects.push(EffectV4 {
        id: "blur".into(),
        enabled: true,
        effect: EffectKindV4::ClassicBlur { intensity: 10.0 },
    });
    e.channels.push(channel(
        "mask-feather",
        ChannelTargetV4::Mask {
            element_id: "video".into(),
            mask_id: "pixels-mask".into(),
        },
        ScalarParameterV4::MaskFeatherClassicPixels,
        vec![
            key("feather-a", t(0, 1), 2.0),
            key("feather-b", t(2, 1), 6.0),
        ],
    ));
    e.channels.push(channel(
        "blur-c",
        ChannelTargetV4::Effect {
            element_id: "video".into(),
            effect_id: "blur".into(),
        },
        ScalarParameterV4::ClassicBlurIntensity,
        vec![key("blur-a", t(0, 1), 0.0), key("blur-b", t(2, 1), 200.0)],
    ));
    let graph = compile(&s);
    let v = graph.layers[0].visual.as_ref().unwrap();
    assert_eq!(
        v.masks[0].feather,
        MaskFeatherV4::ClassicPixels { value: 4.0 }
    );
    assert_eq!(
        v.masks[1].feather,
        MaskFeatherV4::LegacyCanvasFraction { value: 0.12 }
    );
    let MaskShapeV4::Rectangle { geometry } = &v.masks[0].shape else {
        panic!()
    };
    close(geometry.width, 0.01);
    assert_eq!(
        v.effects[0].effect,
        EffectKindV4::ClassicBlur { intensity: 100.0 }
    );
    let requirements = required_capabilities_v4(&graph);
    assert!(
        requirements
            .iter()
            .any(|r| r.feature == NativeFeatureV4::MaskFeatherPixels)
    );
    assert!(
        requirements
            .iter()
            .any(|r| r.feature == NativeFeatureV4::MaskFeatherCanvasFraction)
    );
    assert_eq!(
        ensure_capabilities_v4(&graph, &[]).unwrap_err().code,
        "UNSUPPORTED_V4"
    );
}

#[test]
fn color_component_channels_convert_untouched_srgb_components_and_background_requires_linear_support()
 {
    let mut s = snapshot();
    let shape = &mut s.project.sequences[0].tracks[1].elements[0];
    if let ElementContentV4::Graphic { graphic } = &mut shape.content {
        graphic.fill = ColorV4 {
            red: 0.5,
            green: 0.25,
            blue: 0.75,
            alpha: 0.8,
            encoding: ColorEncodingV4::Srgb,
        };
    }
    shape.channels.push(AnimationChannelV4::Scalar {
        id: "red-channel".into(),
        target: ChannelTargetV4::Element {
            element_id: "shape".into(),
        },
        binding: ScalarBindingV4::ColorComponent {
            parameter: ColorParameterV4::GraphicFill,
            component: ColorComponentV4::Red,
        },
        keys: vec![key("red-key", t(0, 1), 0.1)],
        extrapolation: ExtrapolationV4 {
            before: ExtrapolationModeV4::Hold,
            after: ExtrapolationModeV4::Hold,
        },
    });
    if let SettingsProfileV4::ClassicGlobal { settings } = &mut s.project.settings {
        settings.background = BackgroundV4::Color {
            color: ColorV4 {
                encoding: ColorEncodingV4::LinearSrgb,
                ..color()
            },
        };
    }
    let graph = compile(&s);
    let SampledContentV4::Graphic { graphic } = &graph.layers[1].content else {
        panic!()
    };
    assert_eq!(graphic.fill.encoding, ColorEncodingV4::LinearSrgb);
    close(graphic.fill.red, 0.1);
    close(graphic.fill.green, 0.05087608817155679);
    close(graphic.fill.blue, 0.5225215539683921);
    close(graphic.fill.alpha, 0.8);
    assert!(
        required_capabilities_v4(&graph)
            .iter()
            .any(|r| r.source_id == "scene" && r.feature == NativeFeatureV4::LinearSrgbColor)
    );
}

#[test]
fn legacy_crossfade_retains_source_handles_and_alpha_over_instead_of_double_fading() {
    let mut s = snapshot();
    let first = &mut s.project.sequences[0].tracks[0].elements[0];
    first.timeline_range.duration = t(1, 1);
    first.source_span = ElementSourceSpanV4::LegacySourceRange {
        range: range(t(1, 1), t(1, 1)),
    };
    let mut next = first.clone();
    next.id = "incoming".into();
    next.timeline_range.start = t(3, 2);
    s.project.sequences[0].tracks[0].elements.push(next);
    s.project.sequences[0].transitions.push(TransitionV4 {
        id: "fade".into(),
        outgoing_element_id: "video".into(),
        incoming_element_id: "incoming".into(),
        duration: t(1, 1),
        transition: TransitionKindV4::LegacyCrossfadeV3,
    });
    let graph = compile_frame_v4(&s, "scene", &t(5, 4), CompositionScope::Visual).unwrap();
    assert_eq!(graph.transitions[0].incoming_weight, 0.25);
    assert_eq!(graph.transitions[0].outgoing_weight, 1.0);
    let incoming = graph
        .layers
        .iter()
        .find(|l| l.element_id == "incoming")
        .unwrap();
    let SampledContentV4::Video { source, .. } = &incoming.content else {
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
    assert_eq!(source.sample, SourceReadStateV4::InBounds);
}
