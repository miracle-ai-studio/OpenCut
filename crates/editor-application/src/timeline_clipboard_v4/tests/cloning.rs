use super::*;

fn rich_fixture() -> Fixture {
    let mut fixture = Fixture::new();
    let mut visual_asset = fixture.current.project.assets[0].clone();
    visual_asset.id = "image-asset".into();
    visual_asset.kind = AssetKind::Image;
    visual_asset.version = version('c');
    visual_asset.duration = None;
    visual_asset.metadata.audio = AudioPresenceV4::Absent {};
    fixture.current.project.assets.push(visual_asset);
    let mut video_asset = fixture.current.project.assets[0].clone();
    video_asset.id = "video-asset".into();
    video_asset.kind = AssetKind::Video;
    video_asset.version = version('d');
    video_asset.duration = Some(t(6, 1));
    fixture.current.project.assets.push(video_asset);
    let mut video = generated(
        "video",
        ElementContentV4::Video {
            asset_id: "video-asset".into(),
            video_stream: Some(1),
            source_audio_enabled: false,
            audio: AudioStyleV4 {
                stream: None,
                gain: GainV4::Decibels { value: -6.05 },
                muted: true,
            },
            retime: RetimeV4 {
                rate: RationalRateV4 {
                    numerator: 1,
                    denominator: 1,
                },
                maintain_pitch: false,
                mapping: RetimeMappingV4::ExactRational,
            },
        },
    );
    video.source_span = ElementSourceSpanV4::ClassicTrimmed {
        trim_start: t(1, 1),
        trim_end: t(3, 1),
        source_duration: Some(t(6, 1)),
    };
    let mut next = video.clone();
    next.id = "next-video".into();
    next.name = "Next".into();
    next.timeline_range.start = t(7, 3);
    let source = &mut fixture.current.project.sequences[0];
    source.tracks[0].elements = vec![video, next];
    source.transitions.push(TransitionV4 {
        id: "scene-cut".into(),
        outgoing_element_id: "video".into(),
        incoming_element_id: "next-video".into(),
        duration: t(1, 1),
        transition: TransitionKindV4::LegacyCrossfadeV3,
    });
    source
        .bookmarks
        .push(opencut_contracts::scene_bookmarks::SceneBookmarkV1 {
            id: "scene-note".into(),
            sequence_id: "source".into(),
            time: t(1, 1),
            duration: None,
            color: Some("#AABBCC".into()),
            note: Some("Scene-owned annotation".into()),
        });
    source
        .tracks
        .push(track("caption-track", TrackKindV4::Caption, vec![]));
    source.captions.push(CaptionV4 {
        id: "caption".into(),
        track_id: "caption-track".into(),
        timeline_range: TimeRange {
            start: t(0, 1),
            duration: t(1, 1),
        },
        text: "Caption stays in source".into(),
        source_anchor: None,
        style: CaptionStyleV4 {
            font: FontBindingV4::Locked {
                font_id: "font".into(),
            },
            font_size: FontSizeV4::CanvasPixels { value: 15.0 },
            color: color(),
            background_color: None,
            max_width_px: None,
            line_height_px: None,
            text_align: TextAlignV4::Center,
            layout: CaptionLayoutProfileV4::PinnedCaptionV4,
        },
    });
    let mut title = generated("title", ElementContentV4::Text { text: text() });
    let style = title.visual.as_mut().unwrap();
    style.effects.push(EffectV4 {
        id: "blur".into(),
        enabled: false,
        effect: EffectKindV4::ClassicBlur { intensity: 17.0 },
    });
    style.masks.push(MaskV4 {
        id: "text-mask".into(),
        enabled: false,
        shape: MaskShapeV4::Text {
            mask: TextMaskV4 {
                content: "字 Mask".into(),
                font: FontBindingV4::Locked {
                    font_id: "font".into(),
                },
                font_size: FontSizeV4::CanvasPixels { value: 14.0 },
                font_weight: FontWeightV4::Normal,
                font_style: FontStyleV4::Normal,
                decoration: TextDecorationV4::LineThrough,
                letter_spacing_px: 2.0,
                line_height_multiplier: 1.5,
                center_x: 0.25,
                center_y: -0.1,
                rotation_degrees: 45.0,
                scale: 0.8,
            },
        },
        inverted: true,
        feather: MaskFeatherV4::ClassicPixels { value: 3.0 },
        stroke: stroke(),
    });
    style.masks.push(MaskV4 {
        id: "path-mask".into(),
        enabled: true,
        shape: MaskShapeV4::Freeform {
            path: FreeformMaskV4 {
                points: [(-0.5, -0.5), (0.5, -0.5), (0.0, 0.5)]
                    .iter()
                    .enumerate()
                    .map(|(i, (x, y))| FreeformPointV4 {
                        id: format!("point-{i}"),
                        x: *x,
                        y: *y,
                        in_x: *x - 0.01,
                        in_y: *y,
                        out_x: *x + 0.02,
                        out_y: *y,
                    })
                    .collect(),
                closed: true,
                center_x: 0.1,
                center_y: -0.2,
                rotation_degrees: 8.0,
                scale: 1.2,
            },
        },
        inverted: false,
        feather: MaskFeatherV4::ClassicPixels { value: 1.0 },
        stroke: stroke(),
    });
    title.channels.push(scalar(
        "opacity",
        ChannelTargetV4::Element {
            element_id: "title".into(),
        },
        ScalarParameterV4::Opacity,
    ));
    title.channels.push(scalar(
        "blur-curve",
        ChannelTargetV4::Effect {
            element_id: "title".into(),
            effect_id: "blur".into(),
        },
        ScalarParameterV4::ClassicBlurIntensity,
    ));
    title.channels.push(scalar(
        "mask-curve",
        ChannelTargetV4::Mask {
            element_id: "title".into(),
            mask_id: "text-mask".into(),
        },
        ScalarParameterV4::MaskCenterX,
    ));
    title.channels.push(AnimationChannelV4::Discrete {
        id: "mask-stroke".into(),
        target: ChannelTargetV4::Mask {
            element_id: "title".into(),
            mask_id: "path-mask".into(),
        },
        parameter: DiscreteParameterV4::MaskStrokeAlign,
        keys: vec![DiscreteKeyV4 {
            id: "mask-stroke-key".into(),
            time: t(1, 2),
            value: DiscreteValueV4::StrokeAlign {
                value: StrokeAlignV4::Outside,
            },
        }],
    });
    source
        .tracks
        .push(track("title-track", TrackKindV4::Text, vec![title]));
    source.tracks.push(track(
        "image-track",
        TrackKindV4::Video,
        vec![generated(
            "image",
            ElementContentV4::Image {
                asset_id: "image-asset".into(),
            },
        )],
    ));
    source.tracks.push(track(
        "sticker-track",
        TrackKindV4::Graphic,
        vec![generated(
            "sticker",
            ElementContentV4::Sticker {
                asset_id: "image-asset".into(),
                intrinsic_size: CanvasSizeV4 {
                    width: 17,
                    height: 31,
                },
            },
        )],
    ));
    source.tracks.push(track(
        "shape-track",
        TrackKindV4::Graphic,
        vec![generated(
            "shape",
            ElementContentV4::Graphic {
                graphic: GraphicV4 {
                    geometry: GraphicGeometryV4::Polygon {
                        sides: 4.5,
                        corner_radius_percent: 25.0,
                    },
                    source_size: CanvasSizeV4 {
                        width: 200,
                        height: 100,
                    },
                    fill: color(),
                    stroke: stroke(),
                },
            },
        )],
    ));
    let mut effect = generated(
        "effect-layer",
        ElementContentV4::EffectLayer {
            effect: EffectV4 {
                id: "layer-blur".into(),
                enabled: true,
                effect: EffectKindV4::LegacyGaussianBlur { radius_px: 3.0 },
            },
        },
    );
    effect.visual = None;
    effect.channels.push(scalar(
        "layer-curve",
        ChannelTargetV4::Effect {
            element_id: "effect-layer".into(),
            effect_id: "layer-blur".into(),
        },
        ScalarParameterV4::LegacyBlurRadiusPx,
    ));
    source
        .tracks
        .push(track("effect-track", TrackKindV4::Effect, vec![effect]));
    fixture.sync();
    opencut_editor_core::project_v4::validate_project_v4(&fixture.source.project).unwrap();
    fixture
}

// Compare every authored field after independently putting original identity
// labels back. Separately assert channel targets point to the actual new nodes.
pub(super) fn assert_full_clone(old: &TimelineElementV4, new: &TimelineElementV4) {
    let mut normalized = new.clone();
    normalized.id = old.id.clone();
    normalized.timeline_range.start = old.timeline_range.start.clone();
    if let (
        ElementContentV4::EffectLayer { effect: a },
        ElementContentV4::EffectLayer { effect: b },
    ) = (&old.content, &mut normalized.content)
    {
        b.id = a.id.clone();
    }
    let mut effect_ids = BTreeMap::new();
    let mut mask_ids = BTreeMap::new();
    if let (Some(a), Some(b)) = (&old.visual, &mut normalized.visual) {
        for (old, new) in a.effects.iter().zip(&mut b.effects) {
            effect_ids.insert(old.id.clone(), new.id.clone());
            new.id = old.id.clone();
        }
        for (old, new) in a.masks.iter().zip(&mut b.masks) {
            mask_ids.insert(old.id.clone(), new.id.clone());
            new.id = old.id.clone();
            if let (MaskShapeV4::Freeform { path: a }, MaskShapeV4::Freeform { path: b }) =
                (&old.shape, &mut new.shape)
            {
                for (a, b) in a.points.iter().zip(&mut b.points) {
                    assert_ne!(a.id, b.id);
                    b.id = a.id.clone();
                }
            }
        }
    }
    if let (
        ElementContentV4::EffectLayer { effect: a },
        ElementContentV4::EffectLayer { effect: b },
    ) = (&old.content, &new.content)
    {
        effect_ids.insert(a.id.clone(), b.id.clone());
    }
    assert_eq!(old.channels.len(), new.channels.len());
    for (before, after) in old.channels.iter().zip(&mut normalized.channels) {
        let (a, b, target) = match (before, after) {
            (
                AnimationChannelV4::Scalar {
                    id: a,
                    keys: old_keys,
                    ..
                },
                AnimationChannelV4::Scalar {
                    id: b,
                    keys: new_keys,
                    target,
                    ..
                },
            ) => {
                assert_eq!(old_keys.len(), new_keys.len());
                for (a, b) in old_keys.iter().zip(new_keys) {
                    assert_ne!(a.id, b.id);
                    b.id = a.id.clone();
                    if let (Some(a), Some(b)) = (&a.segment_id, &mut b.segment_id) {
                        assert_ne!(a, b);
                        *b = a.clone();
                    }
                }
                (a, b, target)
            }
            (
                AnimationChannelV4::Discrete {
                    id: a,
                    keys: old_keys,
                    ..
                },
                AnimationChannelV4::Discrete {
                    id: b,
                    keys: new_keys,
                    target,
                    ..
                },
            ) => {
                assert_eq!(old_keys.len(), new_keys.len());
                for (a, b) in old_keys.iter().zip(new_keys) {
                    assert_ne!(a.id, b.id);
                    b.id = a.id.clone();
                }
                (a, b, target)
            }
            _ => panic!("Channel type must be preserved"),
        };
        assert_ne!(a, b);
        *b = a.clone();
        match target {
            ChannelTargetV4::Element { element_id } => {
                assert_eq!(element_id, &new.id);
                *element_id = old.id.clone();
            }
            ChannelTargetV4::Effect {
                element_id,
                effect_id,
            } => {
                assert_eq!(element_id, &new.id);
                *element_id = old.id.clone();
                *effect_id = effect_ids
                    .iter()
                    .find(|(_, new)| *new == effect_id)
                    .unwrap()
                    .0
                    .clone();
            }
            ChannelTargetV4::Mask {
                element_id,
                mask_id,
            } => {
                assert_eq!(element_id, &new.id);
                *element_id = old.id.clone();
                *mask_id = mask_ids
                    .iter()
                    .find(|(_, new)| *new == mask_id)
                    .unwrap()
                    .0
                    .clone();
            }
            ChannelTargetV4::Track { .. } => panic!("No copied track-clock channels"),
        }
    }
    assert_eq!(&normalized, old);
}

#[test]
fn every_element_kind_and_nested_identity_clones_without_scene_owned_data_or_paths() {
    let fixture = rich_fixture();
    let selection = [
        ("source-main", "video"),
        ("source-sound", "voice"),
        ("title-track", "title"),
        ("image-track", "image"),
        ("sticker-track", "sticker"),
        ("shape-track", "shape"),
        ("effect-track", "effect-layer"),
    ];
    let copied = fixture.copied(&selection);
    let request = fixture.request(copied.clone());
    let output = plan_paste(&fixture, &request).unwrap();
    assert_eq!(plan_paste(&fixture, &request).unwrap(), output);
    let proposed = proposed(&fixture, &output);
    assert_eq!(proposed.settings, fixture.current.project.settings);
    assert_eq!(proposed.sequences[0], fixture.source.project.sequences[0]);
    assert!(
        proposed.sequences[1].captions.is_empty()
            && proposed.sequences[1].transitions.is_empty()
            && proposed.sequences[1].bookmarks.is_empty()
    );
    for (_, id) in selection {
        assert_full_clone(
            element(&fixture.source.project, id),
            element(&proposed, &format!("clone-{id}")),
        );
    }
    assert_eq!(output.decision.selection.len(), 7);
    for id in &output.decision.created_ids {
        assert!(!identities::collect(&fixture.source.project).contains(id));
    }
    let wire = serde_json::to_string(&(copied, &output)).unwrap();
    assert!(!wire.contains("/private/") && !wire.contains("\"uri\""));
    assert!(
        output
            .transaction
            .operations
            .iter()
            .all(|op| matches!(op, PublicProjectOperationV4::SequencePut { .. }))
    );
    for track in &proposed.sequences[1].tracks {
        assert!(track.channels.is_empty());
    }
}

#[test]
fn exact_relative_time_and_retimed_animation_use_fixed_source_not_later_head() {
    let mut fixture = Fixture::new();
    fixture.current.project.sequences[0].tracks.push(track(
        "second-sound",
        TrackKindV4::Audio,
        vec![audio("second", t(5, 6))],
    ));
    fixture.sync();
    let copied = fixture.copied(&[("source-sound", "voice"), ("second-sound", "second")]);
    // The target head can differ while the fixed copied source remains intact.
    fixture.current.project.sequences[0].tracks[1].elements[0].name = "Later edit".into();
    let before = fixture.current.clone();
    let output = plan_paste(&fixture, &fixture.request(copied)).unwrap();
    let proposed = proposed(&fixture, &output);
    assert_eq!(
        time(&element(&proposed, "clone-voice").timeline_range.start)
            .unwrap()
            .cmp_exact(R::new(1, 7).unwrap()),
        std::cmp::Ordering::Equal
    );
    assert_eq!(
        time(&element(&proposed, "clone-second").timeline_range.start)
            .unwrap()
            .cmp_exact(R::new(9, 14).unwrap()),
        std::cmp::Ordering::Equal
    );
    for id in ["voice", "second"] {
        assert_full_clone(
            element(&fixture.source.project, id),
            element(&proposed, &format!("clone-{id}")),
        );
    }
    assert_eq!(element(&proposed, "voice").name, "Later edit");
    assert_eq!(proposed.sequences[0], before.project.sequences[0]);
    assert_eq!(fixture.current, before);
    assert_eq!(proposed.settings, before.project.settings);
    // No frame snap: 1/7 second is not a frame on 30000/1001 FPS.
    assert_eq!(
        time(&output.decision.actual_starts[0].start)
            .unwrap()
            .cmp_exact(R::new(1, 7).unwrap()),
        std::cmp::Ordering::Equal
    );
}

#[test]
fn historical_resource_restore_aliases_replaced_ids_and_preserves_exact_font_and_license() {
    let mut fixture = rich_fixture();
    let copied = fixture.copied(&[("source-sound", "voice"), ("title-track", "title")]);
    fixture.current.project.assets[0].version = version('e');
    fixture.current.project.fonts[0].content = version('f');
    let original = fixture.current.clone();
    let output = plan_paste(&fixture, &fixture.request(copied)).unwrap();
    assert_eq!(output.decision.restored_asset_ids.len(), 1);
    assert_eq!(output.decision.restored_font_ids.len(), 1);
    let proposed = proposed(&fixture, &output);
    assert_eq!(proposed.assets[0], original.project.assets[0]);
    assert_eq!(proposed.fonts[0], original.project.fonts[0]);
    let asset_id = &output.decision.restored_asset_ids[0];
    let mut expected = fixture.source.project.assets[0].clone();
    expected.id = asset_id.clone();
    assert_eq!(
        proposed.assets.iter().find(|a| &a.id == asset_id).unwrap(),
        &expected
    );
    let font_id = &output.decision.restored_font_ids[0];
    let mut expected = fixture.source.project.fonts[0].clone();
    expected.id = font_id.clone();
    assert_eq!(
        proposed.fonts.iter().find(|a| &a.id == font_id).unwrap(),
        &expected
    );
    assert!(
        matches!(&element(&proposed,"clone-voice").content,ElementContentV4::Audio{asset_id:id,..} if id==asset_id)
    );
    let title = element(&proposed, "clone-title");
    assert!(
        matches!(&title.content,ElementContentV4::Text{text} if text.font==FontBindingV4::Locked{font_id:font_id.clone()})
    );
    assert!(
        matches!(&title.visual.as_ref().unwrap().masks[0].shape,MaskShapeV4::Text{mask} if mask.font==FontBindingV4::Locked{font_id:font_id.clone()})
    );
    assert_eq!(fixture.current, original);
    // If a source resource has disappeared entirely, its original stable ID
    // may be restored, but its fixed selector still supplies the full record.
    let mut fixture = Fixture::new();
    let copied = fixture.copied(&[("source-sound", "voice")]);
    fixture.current.project.sequences[0].tracks[1]
        .elements
        .clear();
    fixture.current.project.sequences[0].tracks[1]
        .channels
        .clear();
    fixture.current.project.assets.clear();
    let output = plan_paste(&fixture, &fixture.request(copied)).unwrap();
    assert_eq!(output.decision.restored_asset_ids, vec!["sound-asset"]);
    assert_eq!(
        super::proposed(&fixture, &output).assets,
        fixture.source.project.assets
    );
}
