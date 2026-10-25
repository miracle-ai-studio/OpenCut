use super::*;
use opencut_contracts::AssetVersion;
#[path = "graphics_tests.rs"]
mod graphics;
#[path = "masks_tests.rs"]
mod masks;
#[path = "scene_import_tests.rs"]
mod scene;
fn input() -> AnalyzeClassic {
    AnalyzeClassic {
        content: include_str!("fixture.json").into(),
        target_project_id: "target".into(),
        media_mappings: vec![],
        font_mappings: vec![],
    }
}
fn resources() -> ClassicResources {
    ClassicResources {
        media: BTreeMap::new(),
        fonts: vec![],
    }
}
fn changed(edit: impl FnOnce(&mut Value)) -> AnalyzeClassic {
    let mut i = input();
    let mut v: Value = serde_json::from_str(&i.content).unwrap();
    edit(&mut v);
    i.content = v.to_string();
    i
}
fn title(v: &mut Value) -> &mut Value {
    &mut v["scenes"][0]["tracks"]["overlay"][0]["elements"][0]
}
fn font() -> ResolvedClassicFont {
    ResolvedClassicFont {
        request: ClassicFontRequest {
            family: "Ma Shan Zheng".into(),
            font_weight: FontWeight::Normal,
            font_style: FontStyle::Normal,
        },
        font: FontResource {
            id: "fixed-font".into(),
            family: "Ma Shan Zheng".into(),
            weight: 400,
            style: FontStyle::Normal,
            face_index: 0,
            content: AssetVersion {
                id: format!("av_{}", "a".repeat(64)),
                content_hash: "a".repeat(64),
                size: 1,
            },
            origin: ResourceOrigin::LegacyLocator {
                uri: "/private/not-public/font.ttf".into(),
            },
            license: Some(ResourceLicense {
                identifier: Some("OFL-1.1".into()),
                source_url: None,
                notice: Some("Stored license".into()),
                attribution: None,
            }),
        },
    }
}
#[test]
fn classic_import_scene_independent_text_preserves_source_style_and_reports_unresolved_without_fallback()
 {
    let p = analyze(&input(), &resources()).unwrap();
    assert!(p.analysis.blockers.is_empty(), "{:?}", p.analysis.blockers);
    assert_eq!(p.analysis.unresolved_fonts.len(), 1);
    let d = p.document.unwrap();
    assert_eq!(d.schema_version, 4);
    assert!(d.sequences[0].captions.is_empty());
    let e = &d.sequences[0].tracks[1].elements[0];
    assert_eq!(e.id, "title");
    assert_eq!(e.timeline_range.start, t(40000));
    assert_eq!(
        e.source_span,
        ElementSourceSpan::ClassicTrimmed {
            trim_start: t(60000),
            trim_end: t(90000),
            source_duration: None
        }
    );
    let ElementContent::Text { text } = &e.content else {
        panic!()
    };
    assert_eq!(text.content, "中文\nTitle");
    assert_eq!(text.decoration, TextDecoration::LineThrough);
    assert_eq!(text.font_size, FontSize::ClassicHeight90 { value: 12. });
    assert_eq!(text.background.padding_x_at_size15, 3.);
    assert_eq!(text.background.corner_radius_percent, 50.);
    assert_eq!(text.line_height_multiplier, 1.25);
    assert!(matches!(
        &text.font,
        FontBinding::UnresolvedFamily {
            policy: FontResolutionPolicy::Strict {},
            ..
        }
    ));
    let visual = e.visual.as_ref().unwrap();
    assert_eq!(visual.transform.position_x_px, 32.);
    assert_eq!(visual.transform.scale_y, 0.75);
    assert_eq!(visual.opacity, 0.8);
}
#[test]
fn classic_import_scene_fixed_font_preserves_version_license_and_public_projection_hides_locator() {
    let mut res = resources();
    res.fonts.push(font());
    let p = analyze(&input(), &res).unwrap();
    assert!(p.analysis.blockers.is_empty());
    assert!(p.analysis.unresolved_fonts.is_empty());
    assert_eq!(p.document.as_ref().unwrap().fonts[0], res.fonts[0].font);
    assert!(
        !serde_json::to_string(&p.analysis)
            .unwrap()
            .contains("/private/")
    );
    let ElementContent::Text { text } =
        &p.document.unwrap().sequences[0].tracks[1].elements[0].content
    else {
        panic!()
    };
    assert_eq!(
        text.font,
        FontBinding::Locked {
            font_id: "fixed-font".into()
        }
    );
}
fn key(id: &str, value: f64) -> Value {
    serde_json::json!({"id":id,"time":0,"value":value,"segmentToNext":"bezier","tangentMode":"broken","rightHandle":{"dt":10000,"dv":0.2}})
}
#[test]
fn classic_import_scene_scalar_color_curves_preserve_typed_units_handles_and_extrapolation() {
    let i = changed(|v| {
        title(v)["animations"] = serde_json::json!({"transform.positionX":{"keys":[key("position-key",32.)],"extrapolation":{"before":"linear","after":"hold"}},"color":{"r":{"keys":[key("red-key",0.1)]},"g":{"keys":[key("green-key",0.2)]},"b":{"keys":[key("blue-key",0.3)]},"a":{"keys":[key("alpha-key",0.8)]}}});
    });
    let p = analyze(&i, &resources()).unwrap();
    assert!(p.analysis.blockers.is_empty(), "{:?}", p.analysis.blockers);
    let e = &p.document.unwrap().sequences[0].tracks[1].elements[0];
    assert_eq!(e.channels.len(), 5);
    let c = e
        .channels
        .iter()
        .find(|c| {
            matches!(
                c,
                AnimationChannel::Scalar {
                    binding: ScalarBinding::Property {
                        parameter: ScalarParameter::PositionXPx
                    },
                    ..
                }
            )
        })
        .unwrap();
    let AnimationChannel::Scalar {
        keys,
        extrapolation,
        ..
    } = c
    else {
        panic!()
    };
    assert_eq!(keys[0].id, "position-key");
    assert_eq!(
        keys[0].right_handle,
        Some(CurveHandle {
            dt: t(10000),
            dv: 0.2
        })
    );
    assert_eq!(keys[0].segment_to_next, ScalarSegment::Bezier);
    assert_eq!(extrapolation.before, ExtrapolationMode::Linear);
    assert_eq!(
        serde_json::to_value(&analyze(&i, &resources()).unwrap().document).unwrap(),
        serde_json::to_value(&analyze(&i, &resources()).unwrap().document).unwrap()
    );
}
fn asset() -> ProjectAsset {
    ProjectAsset {
        id: "source-media".into(),
        name: "unused fixed media".into(),
        kind: AssetKind::Image,
        version: AssetVersion {
            id: format!("av_{}", "b".repeat(64)),
            content_hash: "b".repeat(64),
            size: 1,
        },
        duration: None,
        origin: ResourceOrigin::LegacyLocator {
            uri: "/private/media.png".into(),
        },
        metadata: ProjectMediaMetadata {
            mime_type: Some("image/png".into()),
            intrinsic_size: Some(CanvasSize {
                width: 320,
                height: 180,
            }),
            frame_rate: None,
            rotation_degrees: None,
            audio: AudioPresence::Absent {},
        },
        license: None,
    }
}
#[test]
fn classic_import_scene_used_and_unused_media_preserve_fixed_records_without_projection() {
    let mut res = resources();
    res.media.insert("unused".into(), asset());
    let mut used = asset();
    used.kind = AssetKind::Video;
    used.metadata.audio = AudioPresence::Present { metadata: None };
    used.duration = Some(t(1200000));
    res.media.insert("movie".into(), used.clone());
    let i = changed(|v| {
        v["scenes"][0]["tracks"]["main"]["elements"] = serde_json::json!([{"id":"clip","name":"Original name","type":"video","mediaId":"movie","startTime":0,"duration":240000,"trimStart":120000,"trimEnd":720000,"sourceDuration":1200000,"retime":{"rate":1.5,"maintainPitch":true},"isSourceAudioEnabled":false,"hidden":true,"params":{"volume":-6.05,"muted":true},"effects":[],"masks":[],"animations":{}}]);
    });
    let p = analyze(&i, &res).unwrap();
    assert!(p.analysis.blockers.is_empty(), "{:?}", p.analysis.blockers);
    let d = p.document.unwrap();
    assert_eq!(d.assets.len(), 2);
    let saved = d
        .assets
        .iter()
        .find(|a| a.id == crate::content_id("classic_asset", b"movie"))
        .unwrap();
    let mut expected = used;
    expected.id = saved.id.clone();
    assert_eq!(*saved, expected);
    let e = &d.sequences[0].tracks[0].elements[0];
    assert!(!e.enabled);
    assert_eq!(e.name, "Original name");
    let ElementContent::Video {
        audio,
        retime,
        source_audio_enabled,
        ..
    } = &e.content
    else {
        panic!()
    };
    assert!(!source_audio_enabled);
    assert_eq!(audio.gain, Gain::Decibels { value: -6.05 });
    assert!(audio.muted);
    assert_eq!(
        retime.rate,
        RationalRate {
            numerator: 3,
            denominator: 2
        }
    );
    assert!(retime.maintain_pitch);
}
#[test]
fn classic_import_scene_unknown_editing_fields_block_candidate_with_original_source_identity() {
    let mut source = input();
    let mut raw: Value = serde_json::from_str(&source.content).unwrap();
    raw["id"] = raw["metadata"]["id"].clone();
    raw["name"] = serde_json::json!("obsolete title");
    raw["fps"] = serde_json::json!(60);
    source.content = raw.to_string();
    let reviewed = analyze(&source, &resources()).unwrap();
    assert!(
        reviewed.analysis.blockers.is_empty(),
        "{:?}",
        reviewed.analysis.blockers
    );
    assert_eq!(
        reviewed.analysis.source_content_id,
        source_content_id(&source.content)
    );
    assert!(
        reviewed
            .analysis
            .losses
            .iter()
            .any(|f| f.path == "$.name" && f.code == "LEGACY_SHADOW_FIELD")
    );
    raw["id"] = serde_json::json!("conflicting source identity");
    source.content = raw.to_string();
    assert!(analyze(&source, &resources()).unwrap().document.is_none());

    for (field, value, code) in [
        (
            "effects",
            serde_json::json!([{"id":"blur","type":"unknown"}]),
            "UNSUPPORTED_DEPENDENCY",
        ),
        ("mystery", Value::Bool(true), "UNSUPPORTED_FIELD"),
        (
            "animations",
            serde_json::json!({"fontFamily":{"keys":[]}}),
            "UNSUPPORTED_ANIMATION",
        ),
    ] {
        let i = changed(|v| {
            title(v)[field] = value;
        });
        let p = analyze(&i, &resources()).unwrap();
        assert!(p.document.is_none());
        assert!(p.analysis.proposed_project.is_none());
        assert_eq!(
            p.analysis.source_content_id,
            crate::classic_restore::source_content_id(&i.content)
        );
        assert!(
            p.analysis
                .blockers
                .iter()
                .any(|b| b.code == code && b.path.contains(field))
        );
    }
}

#[test]
fn classic_import_scene_registry_defaults_and_bottom_to_top_order_are_preserved() {
    let i = changed(|v| {
        title(v)["params"] = serde_json::json!({});
        let mut second = v["scenes"][0]["tracks"]["overlay"][0].clone();
        second["id"] = Value::String("bottom-text".into());
        second["elements"][0]["id"] = Value::String("bottom-title".into());
        v["scenes"][0]["tracks"]["overlay"]
            .as_array_mut()
            .unwrap()
            .push(second);
    });
    let p = analyze(&i, &resources()).unwrap();
    assert!(p.analysis.blockers.is_empty(), "{:?}", p.analysis.blockers);
    let d = p.document.unwrap();
    assert_eq!(
        d.sequences[0]
            .tracks
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        vec!["video", "bottom-text", "text-track"]
    );
    let e = &d.sequences[0].tracks[2].elements[0];
    let ElementContent::Text { text } = &e.content else {
        panic!()
    };
    assert_eq!(text.content, "Default text");
    assert_eq!(text.font_size, FontSize::ClassicHeight90 { value: 15. });
    assert_eq!(text.background.padding_x_at_size15, 30.);
    assert_eq!(text.background.padding_y_at_size15, 42.);
    assert_eq!(text.line_height_multiplier, 1.2);
    assert_eq!(e.visual.as_ref().unwrap().transform.scale_x, 1.);
}
#[test]
fn classic_import_scene_invalid_types_and_duplicate_ids_never_publish_partial_candidates() {
    for i in [
        changed(|v| {
            title(v)["trimStart"] = serde_json::json!(0.5);
        }),
        changed(|v| {
            title(v)["params"]["opacity"] = Value::String("bad".into());
        }),
        changed(|v| {
            title(v)["mediaId"] = Value::String("discarded".into());
        }),
        changed(|v| {
            title(v)["id"] = Value::String("scene".into());
        }),
    ] {
        let p = analyze(&i, &resources()).unwrap();
        assert!(p.document.is_none());
        assert!(!p.analysis.blockers.is_empty());
    }
}

#[test]
fn classic_import_scene_source_curve_enum_strings_map_explicitly_and_target_objects_are_rejected() {
    for (source, expected) in [
        ("step", ScalarSegment::Step),
        ("linear", ScalarSegment::Linear),
        ("bezier", ScalarSegment::Bezier),
    ] {
        for (source_tangent, expected_tangent) in [
            ("auto", TangentMode::Auto),
            ("aligned", TangentMode::Aligned),
            ("broken", TangentMode::Broken),
            ("flat", TangentMode::Flat),
        ] {
            let i = changed(|v| {
                let mut k = key("original-key", 32.);
                k["segmentToNext"] = Value::String(source.into());
                k["tangentMode"] = Value::String(source_tangent.into());
                title(v)["animations"] = serde_json::json!({"transform.positionX":{"keys":[k],"extrapolation":{"before":"hold","after":"linear"}}});
            });
            let p = analyze(&i, &resources()).unwrap();
            assert!(p.analysis.blockers.is_empty(), "{:?}", p.analysis.blockers);
            let d = p.document.unwrap();
            let AnimationChannel::Scalar {
                keys,
                extrapolation,
                ..
            } = &d.sequences[0].tracks[1].elements[0].channels[0]
            else {
                panic!()
            };
            assert_eq!(keys[0].segment_to_next, expected);
            assert_eq!(keys[0].tangent_mode, expected_tangent);
            assert_eq!(
                keys[0].right_handle,
                Some(CurveHandle {
                    dt: t(10000),
                    dv: 0.2
                })
            );
            assert_eq!(extrapolation.before, ExtrapolationMode::Hold);
            assert_eq!(extrapolation.after, ExtrapolationMode::Linear);
        }
    }
    for (field, value) in [
        ("segmentToNext", serde_json::json!({"kind":"bezier"})),
        ("segmentToNext", Value::String("hold".into())),
        ("tangentMode", Value::String("unknown".into())),
    ] {
        let i = changed(|v| {
            let mut k = key("bad-key", 32.);
            k[field] = value;
            title(v)["animations"] = serde_json::json!({"transform.positionX":{"keys":[k]}});
        });
        let p = analyze(&i, &resources()).unwrap();
        assert!(p.document.is_none());
        assert!(p.analysis.blockers.iter().any(|b| b.path.ends_with(field)));
    }
}
