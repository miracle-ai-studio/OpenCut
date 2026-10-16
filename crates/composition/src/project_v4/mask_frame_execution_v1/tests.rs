use super::*;

fn t(n: i64, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
fn color() -> ColorV4 {
    ColorV4 {
        encoding: ColorEncodingV4::Srgb,
        red: 0.2,
        green: 0.4,
        blue: 0.6,
        alpha: 0.25,
    }
}
fn mask(ellipse: bool) -> MaskV4 {
    let geometry = BoxMaskGeometryV4 {
        center_x: 0.15,
        center_y: -0.2,
        width: 0.6,
        height: 0.35,
        rotation_degrees: 27.5,
        scale: 3.25,
    };
    MaskV4 {
        id: "mask".into(),
        enabled: true,
        inverted: false,
        shape: if ellipse {
            MaskShapeV4::Ellipse { geometry }
        } else {
            MaskShapeV4::Rectangle { geometry }
        },
        feather: MaskFeatherV4::ClassicPixels { value: 12.5 },
        stroke: StrokeV4 {
            color: color(),
            width_px: 0.0,
            align: StrokeAlignV4::Outside,
        },
    }
}
#[test]
fn feather_keeps_classic_f32_hard_mask_but_rejects_zero_radius_shader() {
    let mut m = mask(false);
    m.feather = MaskFeatherV4::ClassicPixels { value: 1e-50 };
    assert!(validate_mask(&m).is_ok());
    m.feather = MaskFeatherV4::ClassicPixels {
        value: f64::from(f32::from_bits(1)),
    };
    assert!(validate_mask(&m).is_err());
    m.feather = MaskFeatherV4::ClassicPixels { value: 12.0 };
    assert!(validate_mask(&m).is_ok());
}
fn graph() -> CompositionV4 {
    let mut graphic = CompiledLayerV4 {
        element_id: "graphic".into(),
        track_id: "main".into(),
        track_order: 0,
        element_order: 0,
        local_time: t(1, 7),
        content: SampledContentV4::Graphic {
            graphic: GraphicV4 {
                geometry: GraphicGeometryV4::Ellipse,
                source_size: CanvasSizeV4 {
                    width: 512,
                    height: 512,
                },
                fill: color(),
                stroke: StrokeV4 {
                    color: color(),
                    width_px: 2.0,
                    align: StrokeAlignV4::Outside,
                },
            },
        },
        visual: Some(SampledVisualV4 {
            profile: VisualProfileV4::ClassicCanvasV31,
            transform: TransformV4 {
                position_x_px: 3.0,
                position_y_px: -4.0,
                scale_x: -2.0,
                scale_y: 0.5,
                rotation_degrees: 90.0,
            },
            element_opacity: 1.25,
            track_opacity: 1.0,
            effective_opacity: 1.25,
            opacity_policy: LayerOpacityPolicyV4::MultiplyTrack,
            blend_mode: BlendModeV4::Multiply,
            effects: vec![],
            masks: vec![mask(false)],
        }),
    };
    let mut image = graphic.clone();
    image.element_id = "image".into();
    image.element_order = 1;
    image.content = SampledContentV4::Image {
        asset_id: "asset".into(),
    };
    image.visual.as_mut().unwrap().masks = vec![mask(true)];
    let mut disabled = graphic.clone();
    disabled.element_id = "disabled".into();
    disabled.element_order = 2;
    disabled.visual.as_mut().unwrap().masks[0].enabled = false;
    // Disabled unsupported facts do not produce an effective mask.
    disabled.visual.as_mut().unwrap().masks[0].inverted = true;
    graphic.element_order = 0;
    CompositionV4 {
        project_id: "project".into(),
        revision: "fixed-r1".into(),
        sequence_id: "scene".into(),
        scope: CompositionScope::Visual,
        time: t(1, 7),
        format: CompiledFormatV4 {
            fps: FrameRate {
                numerator: 30,
                denominator: 1,
            },
            canvas_size: CanvasSizeV4 {
                width: 64,
                height: 48,
            },
            profile: FormatProfileV4::ClassicGlobal,
        },
        tracks: vec![],
        layers: vec![graphic, image, disabled],
        audio: vec![],
        transitions: vec![],
        fonts: vec![],
        coverage: vec![],
        background: CompiledBackgroundV4::Color { color: color() },
        resources: vec![CasResourceV4 {
            asset_id: "asset".into(),
            version: AssetVersion {
                id: format!("av_{}", "a".repeat(64)),
                content_hash: "a".repeat(64),
                size: 16,
            },
            kind: AssetKind::Image,
            duration: None,
            metadata: MediaMetadataV4 {
                mime_type: None,
                intrinsic_size: Some(CanvasSizeV4 {
                    width: 1,
                    height: 1,
                }),
                frame_rate: None,
                rotation_degrees: None,
                audio: AudioPresenceV4::Absent {},
            },
            license: None,
        }],
    }
}
fn decoded() -> Vec<DecodedVisualSourceV4> {
    vec![DecodedVisualSourceV4::ReadyImage {
        element_id: "image".into(),
        width: 320,
        height: 90,
    }]
}
fn bound(prepared: &PreparedMaskFrameV1, decoded: &[DecodedVisualSourceV4]) -> BoundGraphicFrameV1 {
    let graphic = prepared.execution();
    let text = graphic.execution().execution();
    let fixed = text.identity();
    let media = bind_visual_sources_v4(text.media(), decoded).unwrap();
    let layers = graphic
        .order()
        .iter()
        .map(|id| {
            if let Some(graphic) = graphic
                .graphics()
                .iter()
                .find(|graphic| &graphic.element_id == id)
            {
                BoundVisualLayerV4 {
                    element_id: id.clone(),
                    asset_id: format!("graphic:{id}"),
                    quad: graphic.quad.clone(),
                }
            } else {
                media
                    .layers
                    .iter()
                    .find(|layer| &layer.element_id == id)
                    .unwrap()
                    .clone()
            }
        })
        .collect();
    BoundGraphicFrameV1 {
        identity: opencut_contracts::text_frame_v2::TextFrameIdentityV2 {
            project_id: fixed.project_id.clone(),
            revision: fixed.revision.clone(),
            sequence_id: fixed.sequence_id.clone(),
            time: fixed.time.clone(),
            width: fixed.width,
            height: fixed.height,
            profile: CLASSIC_GRAPHIC_FRAME_PROFILE_V1.into(),
        },
        clear: media.clear,
        background: media.background,
        layers,
        decoded: media.decoded,
        rasters: vec![],
        captions: vec![],
        graphics: graphic_raster_receipts_v1(graphic).unwrap(),
    }
}

#[test]
fn mask_partition_keeps_original_graph_order_blends_and_disabled_facts() {
    let graph = graph();
    let prepared = prepare_mask_frame_v1(&graph).unwrap();
    assert_eq!(prepared.source_graph(), &graph);
    assert_eq!(
        prepared
            .masks()
            .iter()
            .map(|m| m.element_id.as_str())
            .collect::<Vec<_>>(),
        ["graphic", "image", "disabled"]
    );
    assert_eq!(prepared.masks()[0].mask, Some(mask(false)));
    assert_eq!(prepared.masks()[1].mask, Some(mask(true)));
    assert!(prepared.masks()[2].mask.is_none());
    let mut delegated = graph.clone();
    for layer in &mut delegated.layers {
        layer.visual.as_mut().unwrap().masks.clear();
    }
    assert_eq!(prepared.blend().source_graph(), &delegated);
    assert!(
        prepared
            .blends()
            .iter()
            .all(|b| b.blend_mode == BlendModeV4::Multiply)
    );
    // The previously published profiles retain their mask rejection contract.
    assert!(prepare_blend_frame_v1(&graph).is_err());
    assert!(prepare_graphic_frame_v1(&graph).is_err());
    assert!(prepare_blend_frame_v1(&delegated).is_ok());
}

#[test]
fn mask_binding_uses_logical_graphic_size_and_observed_media_size() {
    let prepared = prepare_mask_frame_v1(&graph()).unwrap();
    let child = bound(&prepared, &decoded());
    assert_eq!(child.layers[0].quad.as_ref().unwrap().width, 103.5);
    let masks = bind_mask_layers_v1(&prepared, &child).unwrap();
    let graphic = masks[0].quad.as_ref().unwrap();
    assert_eq!(
        (
            graphic.center_x,
            graphic.center_y,
            graphic.width,
            graphic.height
        ),
        (35.0, 20.0, 96.0, 24.0)
    );
    assert_eq!(graphic.rotation_radians, std::f64::consts::FRAC_PI_2);
    assert!(graphic.flip_x && !graphic.flip_y);
    assert_eq!(graphic.opacity, 1.25);
    assert_eq!(masks[0].mask, Some(mask(false))); // scale=3.25 is retained, not applied.
    let media = masks[1].quad.as_ref().unwrap();
    assert_eq!((media.width, media.height), (128.0, 9.0));
    assert!(masks[2].mask.is_none() && masks[2].quad.is_none());
    assert_eq!(prepared.source_graph().time, t(1, 7));
}

#[test]
fn masks_require_complete_exact_facts_and_fixed_bound_geometry() {
    let prepared = prepare_mask_frame_v1(&graph()).unwrap();
    validate_mask_layers_v1(&prepared, prepared.masks()).unwrap();
    for case in 0..6 {
        let mut masks = prepared.masks().to_vec();
        match case {
            0 => {
                masks.pop();
            }
            1 => masks.push(masks[0].clone()),
            2 => masks.swap(0, 1),
            3 => masks[0].element_id = "other".into(),
            4 => {
                masks[0].mask.as_mut().unwrap().feather =
                    MaskFeatherV4::ClassicPixels { value: 99.0 }
            }
            _ => masks[2].mask = Some(mask(false)),
        }
        assert!(
            validate_mask_layers_v1(&prepared, &masks).is_err(),
            "facts case {case}"
        );
    }
    for case in 0..8 {
        let mut child = bound(&prepared, &decoded());
        match case {
            0 => child.identity.revision = "other".into(),
            1 => child.identity.profile = CLASSIC_BLEND_FRAME_PROFILE_V1.into(),
            2 => child.layers.swap(0, 1),
            3 => {
                child.layers.pop();
            }
            4 => child.layers[0].quad.as_mut().unwrap().width += 1.0,
            5 => child.layers[1].quad.as_mut().unwrap().width += 1.0,
            6 => child.graphics[0].origin_x += 1,
            _ => child.decoded.clear(),
        }
        assert!(
            bind_mask_layers_v1(&prepared, &child).is_err(),
            "bound case {case}"
        );
    }
}

#[test]
fn zero_scale_and_no_frame_keep_mask_identity_without_inventing_a_quad() {
    let mut graph = graph();
    graph.layers[0].visual.as_mut().unwrap().transform.scale_x = 0.0;
    let range = TimeRange {
        start: t(0, 1),
        duration: t(2, 1),
    };
    graph.layers[1].content = SampledContentV4::Video {
        asset_id: "asset".into(),
        stream: None,
        source_audio: SourceAudioDecisionV4::ScopeExcluded,
        source: SourceClockV4 {
            timeline_local: t(1, 7),
            source_time: t(1, 7),
            point_profile: SourcePointProfileV4::ClassicVideoRoundedTicks,
            rate: RationalRateV4 {
                numerator: 1,
                denominator: 1,
            },
            mapping: RetimeMappingV4::ClassicRoundedTicks { timebase: 120000 },
            nominal_request: range.clone(),
            requested_read_span: range.clone(),
            declared_resize_extent: Some(t(2, 1)),
            read_window: BoundedReadWindowV4::Known {
                resource_extent: t(2, 1),
                range: Some(range),
            },
            sample: SourceReadStateV4::InBounds,
            eof_policy: EofPolicyV4::RequiresBackendProfile,
        },
    };
    graph.resources[0].kind = AssetKind::Video;
    let prepared = prepare_mask_frame_v1(&graph).unwrap();
    let child = bound(
        &prepared,
        &[DecodedVisualSourceV4::NoFrame {
            element_id: "image".into(),
            requested_time: t(1, 7),
            reason: VisualNoFrameReasonV4::TimestampGap,
        }],
    );
    let masks = bind_mask_layers_v1(&prepared, &child).unwrap();
    assert!(masks[0].quad.is_none() && masks[0].mask.is_some());
    assert!(masks[1].quad.is_none() && masks[1].mask.is_some());
    assert_eq!(masks.len(), 3);
}

#[test]
fn logical_graphic_retains_f64_extent_instead_of_shrinking_rounded_gpu_padding() {
    let mut graph = graph();
    graph.layers[0].visual.as_mut().unwrap().transform.scale_x = -1e-47;
    let SampledContentV4::Graphic { graphic } = &mut graph.layers[0].content else {
        unreachable!()
    };
    graphic.stroke.width_px = 100.0;
    let prepared = prepare_mask_frame_v1(&graph).unwrap();
    let child = bound(&prepared, &decoded());
    // The logical extent is not derived from rounded GPU padding. Its f64
    // value remains positive even when another f32 cast would underflow.
    assert!(child.layers[0].quad.as_ref().unwrap().width > 0.0);
    let masks = bind_mask_layers_v1(&prepared, &child).unwrap();
    assert_eq!(masks[0].quad.as_ref().unwrap().width, 48.0 * 1e-47);
    assert!(masks[0].mask.is_some());
}

#[test]
fn classic_mask_rounding_and_rotation_use_original_f64_before_gpu_uniform_rounding() {
    let mut graph = graph();
    for (index, contain_width) in [(0, 48.0), (1, 64.0)] {
        let style = graph.layers[index].visual.as_mut().unwrap();
        style.transform.scale_x = 10.4999999 / contain_width;
        style.transform.position_x_px = 3.00000000001;
        style.transform.rotation_degrees = 17.123456789;
        style.effective_opacity = 0.123456789012;
    }
    let prepared = prepare_mask_frame_v1(&graph).unwrap();
    let child = bound(&prepared, &decoded());
    assert_eq!(child.layers[1].quad.as_ref().unwrap().width, 10.5);
    let masks = bind_mask_layers_v1(&prepared, &child).unwrap();
    for mask in &masks[..2] {
        let q = mask.quad.as_ref().unwrap();
        assert!((q.width - 10.4999999).abs() < 1e-12);
        assert_eq!(q.width.round(), 10.0);
        assert_eq!(q.center_x, 32.0 + 3.00000000001);
        assert_eq!(
            q.rotation_radians,
            17.123456789 * std::f64::consts::PI / 180.0
        );
        assert_eq!(q.opacity, 0.123456789012);
    }
}

#[test]
fn text_and_caption_are_explicitly_rejected_as_mask_targets() {
    let font = FontBindingV4::Locked {
        font_id: "font".into(),
    };
    let text = TextV4 {
        content: "text".into(),
        font: font.clone(),
        font_weight: FontWeightV4::Normal,
        font_style: FontStyleV4::Normal,
        decoration: TextDecorationV4::None,
        align: TextAlignV4::Center,
        font_size: FontSizeV4::ClassicHeight90 { value: 15.0 },
        color: color(),
        letter_spacing_px: 0.0,
        line_height_multiplier: 1.2,
        background: TextBackgroundV4 {
            enabled: false,
            color: color(),
            corner_radius_percent: 0.0,
            padding_x_at_size15: 0.0,
            padding_y_at_size15: 0.0,
            offset_x_px: 0.0,
            offset_y_px: 0.0,
        },
    };
    for content in [
        SampledContentV4::Text { text },
        SampledContentV4::Caption {
            text: "caption".into(),
            style: CaptionStyleV4 {
                font,
                font_size: FontSizeV4::ClassicHeight90 { value: 15.0 },
                color: color(),
                background_color: None,
                max_width_px: None,
                line_height_px: None,
                text_align: TextAlignV4::Center,
                layout: CaptionLayoutProfileV4::PinnedCaptionV4,
            },
        },
    ] {
        let mut graph = graph();
        graph.layers[0].content = content;
        let error = prepare_mask_frame_v1(&graph).unwrap_err();
        assert!(
            error.message.contains("Only video, image and graphic"),
            "{error}"
        );
    }
}

#[test]
fn mask_profile_rejects_unsupported_semantics_and_preserves_other_gates() {
    for case in 0..13 {
        let mut graph = graph();
        let visual = graph.layers[0].visual.as_mut().unwrap();
        match case {
            0 => visual.masks.push(mask(true)),
            1 => visual.masks[0].inverted = true,
            2 => visual.masks[0].stroke.width_px = 1.0,
            3 => visual.masks[0].feather = MaskFeatherV4::LegacyCanvasFraction { value: 0.1 },
            4 => {
                visual.masks[0].shape = MaskShapeV4::Split {
                    center_x: 0.0,
                    center_y: 0.0,
                    rotation_degrees: 0.0,
                }
            }
            5 => visual.profile = VisualProfileV4::LegacyCompositionV3,
            6 => visual.effects.push(EffectV4 {
                id: "effect".into(),
                enabled: true,
                effect: EffectKindV4::ClassicBlur { intensity: 1.0 },
            }),
            7 => visual.masks[0].feather = MaskFeatherV4::ClassicPixels { value: f64::NAN },
            8 => {
                let MaskShapeV4::Rectangle { geometry } = &mut visual.masks[0].shape else {
                    unreachable!()
                };
                geometry.width = 0.0;
            }
            9 => {
                graph.layers[0].content = SampledContentV4::EffectLayer {
                    effect: EffectV4 {
                        id: "effect".into(),
                        enabled: true,
                        effect: EffectKindV4::ClassicBlur { intensity: 1.0 },
                    },
                }
            }
            10 => graph.layers[1].element_id = graph.layers[0].element_id.clone(),
            11 => visual.masks[0].feather = MaskFeatherV4::ClassicPixels { value: f64::MAX },
            _ => graph.layers[0].visual = None,
        }
        assert!(prepare_mask_frame_v1(&graph).is_err(), "case {case}");
    }
}

#[test]
fn scene_adjustment_without_visual_style_retains_source_and_order() {
    let mut source = graph();
    let mut adjustment = source.layers[0].clone();
    adjustment.element_id = "adjustment".into();
    adjustment.visual = None;
    adjustment.content = SampledContentV4::EffectLayer {
        effect: EffectV4 {
            id: "blur".into(),
            enabled: true,
            effect: EffectKindV4::ClassicBlur { intensity: 17.5 },
        },
    };
    source.layers.insert(1, adjustment);
    let prepared = crate::project_v4::prepare_scene_frame_v1(&source).unwrap();
    assert_eq!(prepared.source_graph(), &source);
    assert_eq!(prepared.steps().len(), 4);
    assert!(
        matches!(&prepared.steps()[1],opencut_contracts::scene_frame_v1::SceneStepV1::Effect {element_id,effects} if element_id=="adjustment"&&effects.len()==1)
    );
    assert!(
        prepared
            .execution()
            .order()
            .iter()
            .all(|id| id != "adjustment")
    );
    assert!(prepared.source_graph().layers[1].visual.is_none());
}
