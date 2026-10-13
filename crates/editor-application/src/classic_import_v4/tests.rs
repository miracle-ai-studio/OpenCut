use super::*;
use opencut_contracts::AssetVersion;
fn input() -> ClassicAnalyzeV4 {
    ClassicAnalyzeV4 {
        content: include_str!("fixture.json").into(),
        target_project_id: "target".into(),
        media_mappings: vec![],
        font_mappings: vec![],
    }
}
fn resources() -> ClassicImportResourcesV4 {
    ClassicImportResourcesV4 {
        media: BTreeMap::new(),
        fonts: vec![],
    }
}
fn changed(edit: impl FnOnce(&mut Value)) -> ClassicAnalyzeV4 {
    let mut i = input();
    let mut v: Value = serde_json::from_str(&i.content).unwrap();
    edit(&mut v);
    i.content = v.to_string();
    i
}
fn title(v: &mut Value) -> &mut Value {
    &mut v["scenes"][0]["tracks"]["overlay"][0]["elements"][0]
}
fn font() -> ResolvedClassicFontV4 {
    ResolvedClassicFontV4 {
        request: ClassicFontRequestV4 {
            family: "Ma Shan Zheng".into(),
            font_weight: FontWeightV4::Normal,
            font_style: FontStyleV4::Normal,
        },
        font: FontResourceV4 {
            id: "fixed-font".into(),
            family: "Ma Shan Zheng".into(),
            weight: 400,
            style: FontStyleV4::Normal,
            face_index: 0,
            content: AssetVersion {
                id: format!("av_{}", "a".repeat(64)),
                content_hash: "a".repeat(64),
                size: 1,
            },
            origin: ResourceOriginV4::LegacyLocator {
                uri: "/private/not-public/font.ttf".into(),
            },
            license: Some(ResourceLicenseV4 {
                identifier: Some("OFL-1.1".into()),
                source_url: None,
                notice: Some("Stored license".into()),
                attribution: None,
            }),
        },
    }
}
#[test]
fn classic_import_v4_independent_text_preserves_source_style_and_reports_unresolved_without_fallback()
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
        ElementSourceSpanV4::ClassicTrimmed {
            trim_start: t(60000),
            trim_end: t(90000),
            source_duration: None
        }
    );
    let ElementContentV4::Text { text } = &e.content else {
        panic!()
    };
    assert_eq!(text.content, "中文\nTitle");
    assert_eq!(text.decoration, TextDecorationV4::LineThrough);
    assert_eq!(text.font_size, FontSizeV4::ClassicHeight90 { value: 12. });
    assert_eq!(text.background.padding_x_at_size15, 3.);
    assert_eq!(text.background.corner_radius_percent, 50.);
    assert_eq!(text.line_height_multiplier, 1.25);
    assert!(matches!(
        &text.font,
        FontBindingV4::UnresolvedFamily {
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
fn classic_import_v4_fixed_font_preserves_version_license_and_public_projection_hides_locator() {
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
    let ElementContentV4::Text { text } =
        &p.document.unwrap().sequences[0].tracks[1].elements[0].content
    else {
        panic!()
    };
    assert_eq!(
        text.font,
        FontBindingV4::Locked {
            font_id: "fixed-font".into()
        }
    );
}
fn key(id: &str, value: f64) -> Value {
    serde_json::json!({"id":id,"time":0,"value":value,"segmentToNext":"bezier","tangentMode":"broken","rightHandle":{"dt":10000,"dv":0.2}})
}
#[test]
fn classic_import_v4_scalar_color_curves_preserve_typed_units_handles_and_extrapolation() {
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
                AnimationChannelV4::Scalar {
                    binding: ScalarBindingV4::Property {
                        parameter: ScalarParameterV4::PositionXPx
                    },
                    ..
                }
            )
        })
        .unwrap();
    let AnimationChannelV4::Scalar {
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
        Some(CurveHandleV4 {
            dt: t(10000),
            dv: 0.2
        })
    );
    assert_eq!(keys[0].segment_to_next, ScalarSegmentV4::Bezier);
    assert_eq!(extrapolation.before, ExtrapolationModeV4::Linear);
    assert_eq!(
        serde_json::to_value(&analyze(&i, &resources()).unwrap().document).unwrap(),
        serde_json::to_value(&analyze(&i, &resources()).unwrap().document).unwrap()
    );
}
fn asset() -> AssetV4 {
    AssetV4 {
        id: "source-media".into(),
        name: "unused fixed media".into(),
        kind: AssetKind::Image,
        version: AssetVersion {
            id: format!("av_{}", "b".repeat(64)),
            content_hash: "b".repeat(64),
            size: 1,
        },
        duration: None,
        origin: ResourceOriginV4::LegacyLocator {
            uri: "/private/media.png".into(),
        },
        metadata: MediaMetadataV4 {
            mime_type: Some("image/png".into()),
            intrinsic_size: Some(CanvasSizeV4 {
                width: 320,
                height: 180,
            }),
            frame_rate: None,
            rotation_degrees: None,
            audio: AudioPresenceV4::Absent {},
        },
        license: None,
    }
}
#[test]
fn classic_import_v4_used_and_unused_media_preserve_fixed_records_without_projection() {
    let mut res = resources();
    res.media.insert("unused".into(), asset());
    let mut used = asset();
    used.kind = AssetKind::Video;
    used.metadata.audio = AudioPresenceV4::Present { metadata: None };
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
    let ElementContentV4::Video {
        audio,
        retime,
        source_audio_enabled,
        ..
    } = &e.content
    else {
        panic!()
    };
    assert!(!source_audio_enabled);
    assert_eq!(audio.gain, GainV4::Decibels { value: -6.05 });
    assert!(audio.muted);
    assert_eq!(
        retime.rate,
        RationalRateV4 {
            numerator: 3,
            denominator: 2
        }
    );
    assert!(retime.maintain_pitch);
}
#[test]
fn classic_import_v4_unknown_editing_fields_block_candidate_with_original_source_identity() {
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
            crate::classic_import::source_content_id(&i.content)
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
fn classic_import_v4_registry_defaults_and_bottom_to_top_order_are_preserved() {
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
    let ElementContentV4::Text { text } = &e.content else {
        panic!()
    };
    assert_eq!(text.content, "Default text");
    assert_eq!(text.font_size, FontSizeV4::ClassicHeight90 { value: 15. });
    assert_eq!(text.background.padding_x_at_size15, 30.);
    assert_eq!(text.background.padding_y_at_size15, 42.);
    assert_eq!(text.line_height_multiplier, 1.2);
    assert_eq!(e.visual.as_ref().unwrap().transform.scale_x, 1.);
}
#[test]
fn classic_import_v4_invalid_types_and_duplicate_ids_never_publish_partial_candidates() {
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
fn classic_import_v4_source_curve_enum_strings_map_explicitly_and_target_objects_are_rejected() {
    for (source, expected) in [
        ("step", ScalarSegmentV4::Step),
        ("linear", ScalarSegmentV4::Linear),
        ("bezier", ScalarSegmentV4::Bezier),
    ] {
        for (source_tangent, expected_tangent) in [
            ("auto", TangentModeV4::Auto),
            ("aligned", TangentModeV4::Aligned),
            ("broken", TangentModeV4::Broken),
            ("flat", TangentModeV4::Flat),
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
            let AnimationChannelV4::Scalar {
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
                Some(CurveHandleV4 {
                    dt: t(10000),
                    dv: 0.2
                })
            );
            assert_eq!(extrapolation.before, ExtrapolationModeV4::Hold);
            assert_eq!(extrapolation.after, ExtrapolationModeV4::Linear);
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
