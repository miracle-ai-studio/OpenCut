//! Independent source fixtures following Classic masks/types.ts, effects/types.ts
//! and effect-param-channel.ts. These are structural/sampling tests, not pixels.
use super::*;
use opencut_composition::{CompositionScope, project_v4::compile_frame_v4};
use serde_json::json;
fn mask(kind: &str) -> Value {
    let mut params = json!({"centerX":-0.125,"centerY":0.25,"rotation":-31.5,"feather":12.5,"inverted":true,"strokeColor":"#12345678","strokeWidth":3.25,"strokeAlign":"outside"});
    let extra = match kind {
        "split" => json!({}),
        "text" => {
            json!({"content":"Mask\n中文","fontFamily":"Ma Shan Zheng","fontWeight":"normal","fontStyle":"normal","fontSize":17.5,"textDecoration":"line-through","letterSpacing":1.25,"lineHeight":1.4,"scale":1.5})
        }
        "freeform" => json!({"path":[
            {"id":"point-a","x":-0.4,"y":-0.2,"inX":-0.1,"inY":0.125,"outX":0.25,"outY":0.0},
            {"id":"point-b","x":0.4,"y":-0.2,"inX":-0.25,"inY":0.0,"outX":0.1,"outY":0.125},
            {"id":"point-c","x":0.0,"y":0.4,"inX":-0.125,"inY":-0.125,"outX":0.125,"outY":-0.125}
        ],"closed":true,"scale":0.75}),
        _ => json!({"width":0.75,"height":0.375,"scale":2.25}),
    };
    params
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    json!({"id":format!("mask-{kind}"),"type":kind,"params":params})
}
fn source(masks: Vec<Value>, effects: Vec<Value>) -> ClassicAnalyzeV4 {
    changed(|v| {
        v["scenes"][0]["tracks"]["overlay"] = json!([{"id":"graphic-track","name":"Shape","type":"graphic","hidden":false,"elements":[{
            "id":"shape","name":"Shape","type":"graphic","definitionId":"rectangle","startTime":120000,"duration":240000,"trimStart":60000,"trimEnd":90000,
            "params":{},"masks":masks,"effects":effects,"animations":{}
        }]}]);
    })
}
fn element(v: &mut Value) -> &mut Value {
    &mut v["scenes"][0]["tracks"]["overlay"][0]["elements"][0]
}
fn modified(input: &ClassicAnalyzeV4, edit: impl FnOnce(&mut Value)) -> ClassicAnalyzeV4 {
    let mut i = input.clone();
    let mut v: Value = serde_json::from_str(&i.content).unwrap();
    edit(&mut v);
    i.content = v.to_string();
    i
}
fn parsed(input: &ClassicAnalyzeV4, res: &ClassicImportResourcesV4) -> ProjectDocumentV4 {
    let result = analyze(input, res).unwrap();
    assert!(
        result.analysis.blockers.is_empty(),
        "{:?}",
        result.analysis.blockers
    );
    assert!(result.analysis.losses.is_empty());
    result.document.unwrap()
}
fn blur(id: &str, enabled: bool, intensity: f64) -> Value {
    json!({"id":id,"type":"blur","enabled":enabled,"params":{"intensity":intensity}})
}
#[test]
fn nine_source_shapes_stack_inversion_strokes_and_locked_text_font_are_preserved() {
    let names = [
        "split",
        "cinematic-bars",
        "rectangle",
        "ellipse",
        "heart",
        "diamond",
        "star",
        "text",
        "freeform",
    ];
    let i = source(names.into_iter().map(mask).collect(), vec![]);
    let mut res = resources();
    res.fonts.push(font());
    let result = analyze(&i, &res).unwrap();
    assert!(
        result.analysis.blockers.is_empty(),
        "{:?}",
        result.analysis.blockers
    );
    assert_eq!(result.analysis.losses.len(), 1);
    assert_eq!(
        result.analysis.losses[0].code,
        "CLASSIC_INACTIVE_MASKS_RETAINED"
    );
    let document = result.document.unwrap();
    let e = &document.sequences[0].tracks[1].elements[0];
    let masks = &e.visual.as_ref().unwrap().masks;
    assert_eq!(masks.len(), 9);
    for (index, (name, mask)) in names.into_iter().zip(masks).enumerate() {
        assert_eq!(mask.id, format!("mask-{name}"));
        assert_eq!(mask.enabled, index == 0);
        assert!(mask.inverted);
        assert_eq!(mask.feather, MaskFeatherV4::ClassicPixels { value: 12.5 });
        assert_eq!(mask.stroke.width_px, 3.25);
        assert_eq!(mask.stroke.align, StrokeAlignV4::Outside);
        assert_eq!(mask.stroke.color, color("#12345678").unwrap());
    }
    assert!(matches!(
        masks[0].shape,
        MaskShapeV4::Split {
            center_x: -0.125,
            center_y: 0.25,
            rotation_degrees: -31.5
        }
    ));
    for mask in &masks[1..7] {
        let geometry = match mask.shape {
            MaskShapeV4::CinematicBars { geometry }
            | MaskShapeV4::Rectangle { geometry }
            | MaskShapeV4::Ellipse { geometry }
            | MaskShapeV4::Heart { geometry }
            | MaskShapeV4::Diamond { geometry }
            | MaskShapeV4::Star { geometry } => geometry,
            _ => panic!(),
        };
        assert_eq!(
            geometry,
            BoxMaskGeometryV4 {
                center_x: -0.125,
                center_y: 0.25,
                width: 0.75,
                height: 0.375,
                rotation_degrees: -31.5,
                scale: 2.25
            }
        );
    }
    let MaskShapeV4::Text { mask } = &masks[7].shape else {
        panic!()
    };
    assert_eq!(
        mask.font,
        FontBindingV4::Locked {
            font_id: "fixed-font".into()
        }
    );
    assert_eq!(mask.content, "Mask\n中文");
    assert_eq!(mask.font_size, FontSizeV4::ClassicHeight90 { value: 17.5 });
    assert_eq!(mask.decoration, TextDecorationV4::LineThrough);
    assert_eq!(mask.letter_spacing_px, 1.25);
    assert_eq!(mask.line_height_multiplier, 1.4);
    assert_eq!(
        (
            mask.center_x,
            mask.center_y,
            mask.rotation_degrees,
            mask.scale
        ),
        (-0.125, 0.25, -31.5, 1.5)
    );
    let MaskShapeV4::Freeform { path } = &masks[8].shape else {
        panic!()
    };
    assert!(path.closed);
    assert_eq!(path.points[0].id, "point-a");
    assert_eq!(path.points[0].in_x, -0.5);
    assert_eq!(path.points[0].in_y, -0.07500000000000001);
    assert_eq!(path.points[0].out_x, -0.15000000000000002);
    assert_eq!(path.points[0].out_y, -0.2);
    assert_eq!(e.timeline_range.start, t(120000));
    assert!(matches!(
        e.source_span,
        ElementSourceSpanV4::ClassicTrimmed { .. }
    ));
    let recovered: ProjectDocumentV4 =
        serde_json::from_str(&serde_json::to_string(&document).unwrap()).unwrap();
    assert_eq!(
        recovered.sequences[0].tracks[1].elements[0]
            .visual
            .as_ref()
            .unwrap()
            .masks
            .len(),
        9
    );
}
#[test]
fn text_mask_shares_explicit_font_resolution_and_never_uses_a_system_fallback() {
    let i = source(vec![mask("text")], vec![]);
    let result = analyze(&i, &resources()).unwrap();
    assert!(result.analysis.blockers.is_empty());
    assert_eq!(result.analysis.unresolved_fonts, vec![font().request]);
    let d = result.document.unwrap();
    let MaskShapeV4::Text { mask } = &d.sequences[0].tracks[1].elements[0]
        .visual
        .as_ref()
        .unwrap()
        .masks[0]
        .shape
    else {
        panic!()
    };
    assert!(
        matches!(&mask.font,FontBindingV4::UnresolvedFamily{family,policy:FontResolutionPolicy::Strict{}} if family=="Ma Shan Zheng")
    );
    let mut res = resources();
    let mut wrong = font();
    wrong.font.weight = 700;
    res.fonts.push(wrong);
    let result = analyze(&i, &res).unwrap();
    assert!(result.document.is_none());
    assert!(
        result
            .analysis
            .blockers
            .iter()
            .any(|b| b.code == "FONT_FACE_MISMATCH")
    );
}
#[test]
fn open_freeform_pen_drafts_keep_zero_one_or_two_points_and_original_enabled_state() {
    for count in 0..=2 {
        let mut m = mask("freeform");
        m["params"]["closed"] = json!(false);
        m["params"]["path"].as_array_mut().unwrap().truncate(count);
        let d = parsed(&source(vec![m], vec![]), &resources());
        let m = &d.sequences[0].tracks[1].elements[0]
            .visual
            .as_ref()
            .unwrap()
            .masks[0];
        assert!(m.enabled);
        let MaskShapeV4::Freeform { path } = &m.shape else {
            panic!()
        };
        assert!(!path.closed);
        assert_eq!(path.points.len(), count);
    }
}
#[test]
fn nested_blur_uses_actual_classic_effect_path_and_preserves_curve_and_disabled_instances() {
    let i = modified(
        &source(
            vec![mask("heart")],
            vec![blur("blur-one", true, 15.), blur("blur-two", false, 25.)],
        ),
        |v| {
            element(v)["animations"] = json!({"effects.blur-one.params.intensity":{"keys":[
            {"id":"blur-key-a","time":0,"value":5,"segmentToNext":"bezier","tangentMode":"broken","rightHandle":{"dt":40000,"dv":8}},
            {"id":"blur-key-b","time":240000,"value":25,"segmentToNext":"linear","tangentMode":"auto","leftHandle":{"dt":-40000,"dv":-8}}
        ],"extrapolation":{"before":"linear","after":"hold"}}});
        },
    );
    let d = parsed(&i, &resources());
    let e = &d.sequences[0].tracks[1].elements[0];
    let effects = &e.visual.as_ref().unwrap().effects;
    assert_eq!(
        effects.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["blur-one", "blur-two"]
    );
    assert!(!effects[1].enabled);
    let AnimationChannelV4::Scalar {
        target,
        binding,
        keys,
        extrapolation,
        ..
    } = &e.channels[0]
    else {
        panic!()
    };
    assert_eq!(
        target,
        &ChannelTargetV4::Effect {
            element_id: "shape".into(),
            effect_id: "blur-one".into()
        }
    );
    assert_eq!(
        binding,
        &ScalarBindingV4::Property {
            parameter: ScalarParameterV4::ClassicBlurIntensity
        }
    );
    assert_eq!(keys[0].right_handle.as_ref().unwrap().dt, t(40000));
    assert_eq!(keys[0].right_handle.as_ref().unwrap().dv, 8.);
    assert_eq!(keys[1].left_handle.as_ref().unwrap().dv, -8.);
    assert_eq!(extrapolation.before, ExtrapolationModeV4::Linear);
    let graph = compile_frame_v4(
        &ProjectSnapshotV4 {
            project: d,
            revision: "fixed-scene-import".into(),
        },
        "scene",
        &t(240000),
        CompositionScope::Visual,
    )
    .unwrap();
    let sampled = &graph.layers[0].visual.as_ref().unwrap().effects;
    let EffectKindV4::ClassicBlur { intensity } = sampled[0].effect else {
        panic!()
    };
    assert!((intensity - 15.).abs() < 1e-9);
}
#[test]
fn adjustment_track_has_one_stable_blur_at_its_original_layer_boundary() {
    let i = changed(|v| {
        v["scenes"][0]["tracks"]["overlay"] = json!([
            {"id":"adjustment","name":"Adjustment","type":"effect","hidden":true,"elements":[{"id":"adjustment-element","name":"Blur","type":"effect","effectType":"blur","startTime":120000,"duration":240000,"trimStart":60000,"trimEnd":90000,"params":{"intensity":32.5},"animations":{}}]},
            {"id":"below","name":"Below","type":"graphic","hidden":false,"elements":[{"id":"shape","name":"Shape","type":"graphic","definitionId":"rectangle","startTime":0,"duration":480000,"trimStart":0,"trimEnd":0,"params":{}}]}
        ]);
    });
    let a = parsed(&i, &resources());
    let b = parsed(&i, &resources());
    assert_eq!(a, b);
    assert_eq!(a.sequences[0].tracks[1].id, "below");
    let track = &a.sequences[0].tracks[2];
    assert_eq!(track.kind, TrackKindV4::Effect);
    assert!(!track.enabled);
    let e = &track.elements[0];
    assert!(e.visual.is_none());
    assert_eq!(e.timeline_range.start, t(120000));
    let ElementContentV4::EffectLayer { effect } = &e.content else {
        panic!()
    };
    assert_eq!(effect.effect, EffectKindV4::ClassicBlur { intensity: 32.5 });
    assert!(effect.enabled);
    assert_ne!(effect.id, e.id);
}
#[test]
fn unknown_paths_and_invalid_effect_or_mask_geometry_fail_closed() {
    let base = source(vec![mask("freeform")], vec![blur("blur-one", true, 15.)]);
    for case in 0..13 {
        let i = modified(&base, |v| {
            let e = element(v);
            match case {
                0 => e["masks"][0]["params"]["path"][0]["x"] = json!("0.5"),
                1 => {
                    e["masks"][0]["params"]["path"]
                        .as_array_mut()
                        .unwrap()
                        .truncate(2);
                }
                2 => e["masks"][0]["params"]["path"][1]["id"] = json!("point-a"),
                3 => {
                    e["masks"][0]["params"]["path"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("outX");
                }
                4 => e["masks"][0]["params"]["path"] = json!("[]"),
                5 => e["effects"][0]["params"]["intensity"] = json!(-1),
                6 => {
                    e["effects"][0]["params"]
                        .as_object_mut()
                        .unwrap()
                        .remove("intensity");
                }
                7 => e["effects"][0]["type"] = json!("future"),
                8 => e["effects"][0]["enabled"] = json!("false"),
                9 => e["effects"][0]["params"]["radius"] = json!(3),
                10 => e["animations"] = json!({"effects.missing.params.intensity":{"keys":[]}}),
                11 => e["animations"] = json!({"effects.blur-one.params.radius":{"keys":[]}}),
                _ => e["animations"] = json!({"masks.mask-freeform.params.feather":{"keys":[]}}),
            }
        });
        let result = analyze(&i, &resources()).unwrap();
        assert!(result.document.is_none(), "case {case}");
        assert!(!result.analysis.blockers.is_empty(), "case {case}");
    }
}

#[test]
fn nested_effects_preserve_text_image_video_and_graphic_owners() {
    for kind in ["text", "image", "video", "graphic"] {
        let mut res = resources();
        res.fonts.push(font());
        let i = if kind == "text" {
            changed(|v| {
                title(v)["effects"] = json!([blur("blur-on-title", true, 20.)]);
            })
        } else {
            modified(
                &source(vec![], vec![blur("blur-on-shape", true, 20.)]),
                |v| {
                    if kind != "graphic" {
                        let e = element(v);
                        e["type"] = json!(kind);
                        e.as_object_mut().unwrap().remove("definitionId");
                        e["mediaId"] = json!("media");
                        v["scenes"][0]["tracks"]["overlay"][0]["type"] = json!("video");
                    }
                },
            )
        };
        if matches!(kind, "image" | "video") {
            let mut a = asset();
            a.kind = if kind == "image" {
                AssetKind::Image
            } else {
                AssetKind::Video
            };
            a.duration = (kind == "video").then(|| t(1200000));
            res.media.insert("media".into(), a);
        }
        let d = parsed(&i, &res);
        let e = &d.sequences[0].tracks[1].elements[0];
        assert_eq!(
            e.visual.as_ref().unwrap().effects[0].effect,
            EffectKindV4::ClassicBlur { intensity: 20. }
        );
    }
}

#[test]
fn every_new_shape_requires_its_complete_actual_source_parameters() {
    for kind in [
        "split",
        "cinematic-bars",
        "heart",
        "diamond",
        "star",
        "text",
        "freeform",
    ] {
        let m = mask(kind);
        for key in m["params"].as_object().unwrap().keys() {
            let mut m = m.clone();
            m["params"].as_object_mut().unwrap().remove(key);
            let result = analyze(&source(vec![m], vec![]), &resources()).unwrap();
            assert!(result.document.is_none(), "{kind}.{key}");
            assert!(
                result
                    .analysis
                    .blockers
                    .iter()
                    .any(|b| b.code == "MISSING_MASK_PARAMETER" && b.path.ends_with(key)),
                "{kind}.{key}"
            );
        }
    }
}

#[test]
fn standalone_effects_do_not_invent_classic_animation_or_visual_params() {
    let base = changed(|v| {
        v["scenes"][0]["tracks"]["overlay"] = json!([{"id":"effect-track","name":"Effects","type":"effect","hidden":false,"elements":[{"id":"effect-element","name":"Blur","type":"effect","effectType":"blur","startTime":0,"duration":240000,"trimStart":0,"trimEnd":0,"params":{"intensity":15},"animations":{}}]}]);
    });
    parsed(&base, &resources());
    for case in 0..5 {
        let i = modified(&base, |v| {
            let e = element(v);
            match case {
                0 => e["animations"] = json!({"params.intensity":{"keys":[]}}),
                1 => {
                    e["animations"] = json!({"effects.effect-element.params.intensity":{"keys":[]}})
                }
                2 => e["animations"] = json!({"opacity":{"keys":[]}}),
                3 => e["params"]["opacity"] = json!(0.5),
                _ => e["effectType"] = json!("unknown"),
            }
        });
        let result = analyze(&i, &resources()).unwrap();
        assert!(result.document.is_none(), "case {case}");
    }
}

#[test]
fn classic_second_masks_are_retained_disabled_and_cannot_change_the_sampled_picture() {
    for open_first in [false, true] {
        let mut first = mask(if open_first { "freeform" } else { "rectangle" });
        if open_first {
            first["params"]["closed"] = json!(false);
            first["params"]["path"] = json!([]);
        }
        let alone = parsed(&source(vec![first.clone()], vec![]), &resources());
        let result = analyze(
            &source(vec![first, mask("ellipse"), mask("star")], vec![]),
            &resources(),
        )
        .unwrap();
        assert!(result.analysis.blockers.is_empty());
        assert_eq!(result.analysis.losses.len(), 1);
        assert_eq!(
            result.analysis.losses[0].code,
            "CLASSIC_INACTIVE_MASKS_RETAINED"
        );
        assert!(result.analysis.losses[0].detail.contains("disabled"));
        let retained = result.document.unwrap();
        let masks = &retained.sequences[0].tracks[1].elements[0]
            .visual
            .as_ref()
            .unwrap()
            .masks;
        assert_eq!(
            masks.iter().map(|m| m.enabled).collect::<Vec<_>>(),
            [true, false, false]
        );
        assert_eq!(masks[1].id, "mask-ellipse");
        assert_eq!(masks[2].id, "mask-star");
        assert!(masks[1].inverted);
        assert_eq!(masks[1].stroke.width_px, 3.25);
        // Independent source rule is frame-descriptor.ts masks?.[0]. The
        // sampled render inputs must match the first-mask-only project exactly,
        // including when that first mask is an inactive open pen draft. This
        // assertion is structural, not a claim of independently checked pixels.
        let compile = |project| {
            compile_frame_v4(
                &ProjectSnapshotV4 {
                    project,
                    revision: "same-fixed-revision".into(),
                },
                "scene",
                &t(240000),
                CompositionScope::Visual,
            )
            .unwrap()
        };
        let before = compile(alone);
        let after = compile(retained);
        assert_eq!(before.layers.len(), after.layers.len());
        assert_eq!(before.layers[0].visual, after.layers[0].visual);
        assert_eq!(before.layers[0].content, after.layers[0].content);
        assert_eq!(before.layers[0].local_time, after.layers[0].local_time);
    }
}
