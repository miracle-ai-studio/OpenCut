use super::*;
use opencut_composition::{
    CompositionScope,
    project_v4::{SampledContentV4, compile_frame_v4, prepare_mask_frame_v1},
};
use serde_json::json;

fn mask(kind: &str) -> Value {
    json!({"id":"original-mask","type":kind,"params":{
        "centerX":0,"centerY":0,"width":0.6,"height":0.6,"rotation":0,"scale":1,
        "feather":0,"inverted":false,"strokeColor":"#ffffff","strokeWidth":0,"strokeAlign":"center"
    }})
}
fn masked_input(kind: &str, mask_type: &str) -> ClassicAnalyzeV4 {
    changed(|value| {
        let mut element = json!({
            "id":"masked-element","name":"Original masked element","type":kind,
            "startTime":120000,"duration":240000,"trimStart":60000,"trimEnd":90000,
            "params":{},"effects":[],"masks":[mask(mask_type)],"animations":{}
        });
        if kind == "graphic" {
            element["definitionId"] = json!("rectangle");
        } else {
            element["mediaId"] = json!("media");
        }
        value["scenes"][0]["tracks"]["overlay"] = json!([{
            "id":"masked-track","name":"Original track","type":if kind == "graphic" { "graphic" } else { "video" },
            "hidden":false,"elements":[element]
        }]);
    })
}
fn edit(input: &ClassicAnalyzeV4, change: impl FnOnce(&mut Value)) -> ClassicAnalyzeV4 {
    let mut result = super::input();
    let mut value: Value = serde_json::from_str(&input.content).unwrap();
    change(&mut value);
    result.content = value.to_string();
    result
}
fn element(value: &mut Value) -> &mut Value {
    &mut value["scenes"][0]["tracks"]["overlay"][0]["elements"][0]
}
fn parsed(input: &ClassicAnalyzeV4, resources: &ClassicImportResourcesV4) -> ProjectDocumentV4 {
    let result = analyze(input, resources).unwrap();
    assert!(
        result.analysis.blockers.is_empty(),
        "{:?}",
        result.analysis.blockers
    );
    assert!(
        result.analysis.losses.is_empty(),
        "{:?}",
        result.analysis.losses
    );
    result.document.unwrap()
}
fn converted_mask(document: &ProjectDocumentV4) -> &MaskV4 {
    &document.sequences[0].tracks[1].elements[0]
        .visual
        .as_ref()
        .unwrap()
        .masks[0]
}

#[test]
fn classic_import_v4_box_mask_complete_creation_values_match_source_units() {
    for kind in ["rectangle", "ellipse"] {
        let document = parsed(&masked_input("graphic", kind), &resources());
        let mask = converted_mask(&document);
        let geometry = BoxMaskGeometryV4 {
            center_x: 0.0,
            center_y: 0.0,
            width: 0.6,
            height: 0.6,
            rotation_degrees: 0.0,
            scale: 1.0,
        };
        assert_eq!(
            mask.shape,
            if kind == "rectangle" {
                MaskShapeV4::Rectangle { geometry }
            } else {
                MaskShapeV4::Ellipse { geometry }
            }
        );
        assert_eq!(mask.id, "original-mask");
        assert!(mask.enabled && !mask.inverted);
        assert_eq!(mask.feather, MaskFeatherV4::ClassicPixels { value: 0.0 });
        assert_eq!(
            mask.stroke,
            StrokeV4 {
                color: color("#ffffff").unwrap(),
                width_px: 0.0,
                align: StrokeAlignV4::Center
            }
        );
    }
}

#[test]
fn classic_import_v4_masks_preserve_geometry_inert_scale_zero_stroke_and_original_clock() {
    for (align, expected_align) in [
        ("inside", StrokeAlignV4::Inside),
        ("center", StrokeAlignV4::Center),
        ("outside", StrokeAlignV4::Outside),
    ] {
        let input = edit(&masked_input("graphic", "ellipse"), |value| {
            let element = element(value);
            element["masks"][0]["params"] = json!({"centerX":-0.25,"centerY":0.125,"width":0.001,"height":0.45,"rotation":-377.5,"scale":3.25,"feather":1234.5,"inverted":false,"strokeColor":"#10203040","strokeWidth":0,"strokeAlign":align});
            element["params"] = json!({"transform.scaleX":-2,"transform.scaleY":0.5,"transform.rotate":23,"blendMode":"screen"});
            element["animations"] = json!({"transform.positionX":{"keys":[
                {"id":"original-zero","time":0,"value":5,"segmentToNext":"linear","tangentMode":"auto"},
                {"id":"original-later","time":120000,"value":25,"segmentToNext":"linear","tangentMode":"auto"}
            ]}});
        });
        let document = parsed(&input, &resources());
        let source_element = &document.sequences[0].tracks[1].elements[0];
        assert_eq!(source_element.timeline_range.start, t(120000));
        assert_eq!(
            source_element.source_span,
            ElementSourceSpanV4::ClassicTrimmed {
                trim_start: t(60000),
                trim_end: t(90000),
                source_duration: None
            }
        );
        assert_eq!(source_element.channels.len(), 1);
        let saved_mask = converted_mask(&document);
        assert_eq!(
            saved_mask.shape,
            MaskShapeV4::Ellipse {
                geometry: BoxMaskGeometryV4 {
                    center_x: -0.25,
                    center_y: 0.125,
                    width: 0.001,
                    height: 0.45,
                    rotation_degrees: -377.5,
                    scale: 3.25
                }
            }
        );
        assert_eq!(
            saved_mask.feather,
            MaskFeatherV4::ClassicPixels { value: 1234.5 }
        );
        assert_eq!(saved_mask.stroke.color, color("#10203040").unwrap());
        assert_eq!(saved_mask.stroke.align, expected_align);
        let graph = compile_frame_v4(
            &ProjectSnapshotV4 {
                project: document.clone(),
                revision: "fixed-mask-import".into(),
            },
            "scene",
            &t(180000),
            CompositionScope::Visual,
        )
        .unwrap();
        assert_eq!(graph.layers.len(), 1);
        let sampled = graph.layers[0].visual.as_ref().unwrap();
        assert_eq!(sampled.transform.position_x_px, 15.0);
        assert_eq!(sampled.transform.scale_x, -2.0);
        assert_eq!(sampled.blend_mode, BlendModeV4::Screen);
        let MaskShapeV4::Ellipse { geometry } = sampled.masks[0].shape else {
            panic!()
        };
        assert_eq!(geometry.width, 0.01); // Existing V4 projection owns Classic's floor.
        assert_eq!(geometry.scale, 3.25);
        assert_eq!(
            graph.layers[0].local_time,
            Time {
                ticks: "1".into(),
                timebase: 2
            }
        );
        let prepared = prepare_mask_frame_v1(&graph).unwrap();
        assert_eq!(
            prepared.masks()[0].mask.as_ref().unwrap(),
            &sampled.masks[0]
        );
        let recovered: ProjectDocumentV4 =
            serde_json::from_str(&serde_json::to_string(&document).unwrap()).unwrap();
        let recovered_mask = converted_mask(&recovered);
        // Shared color parsing can differ by one f64 ULP after decimal JSON.
        // All new mask geometry/units/flags and the original clock stay exact.
        for (actual, expected) in [
            (recovered_mask.stroke.color.red, saved_mask.stroke.color.red),
            (
                recovered_mask.stroke.color.green,
                saved_mask.stroke.color.green,
            ),
            (
                recovered_mask.stroke.color.blue,
                saved_mask.stroke.color.blue,
            ),
            (
                recovered_mask.stroke.color.alpha,
                saved_mask.stroke.color.alpha,
            ),
        ] {
            assert!((actual - expected).abs() <= f64::EPSILON);
        }
        let mut expected_mask = saved_mask.clone();
        expected_mask.stroke.color = recovered_mask.stroke.color.clone();
        assert_eq!(recovered_mask, &expected_mask);
        let before = &document.sequences[0].tracks[1].elements[0];
        let after = &recovered.sequences[0].tracks[1].elements[0];
        assert_eq!(after.timeline_range, before.timeline_range);
        assert_eq!(after.source_span, before.source_span);
        assert_eq!(after.channels, before.channels);
    }
}

#[test]
fn classic_import_v4_masks_keep_media_binding_and_top_to_bottom_source_order() {
    for kind in ["image", "video"] {
        let mut resources = resources();
        let mut media = asset();
        media.kind = if kind == "image" {
            AssetKind::Image
        } else {
            AssetKind::Video
        };
        media.duration = (kind == "video").then(|| t(1200000));
        resources.media.insert("media".into(), media);
        let input = edit(&masked_input(kind, "rectangle"), |value| {
            if kind == "video" {
                element(value)["retime"] = json!({"rate":1.5,"maintainPitch":true});
            }
            let mut other = value["scenes"][0]["tracks"]["overlay"][0].clone();
            other["id"] = json!("second-track");
            other["elements"][0]["id"] = json!("second-element");
            other["elements"][0]["masks"][0]["id"] = json!("second-mask");
            value["scenes"][0]["tracks"]["overlay"]
                .as_array_mut()
                .unwrap()
                .push(other);
        });
        let document = parsed(&input, &resources);
        assert_eq!(document.sequences[0].tracks[1].id, "second-track");
        assert_eq!(document.sequences[0].tracks[2].id, "masked-track");
        assert_eq!(document.assets.len(), 1);
        let graph = compile_frame_v4(
            &ProjectSnapshotV4 {
                project: document,
                revision: "fixed-media-masks".into(),
            },
            "scene",
            &t(180000),
            CompositionScope::Visual,
        )
        .unwrap();
        let prepared = prepare_mask_frame_v1(&graph).unwrap();
        assert_eq!(
            prepared
                .masks()
                .iter()
                .map(|m| m.element_id.as_str())
                .collect::<Vec<_>>(),
            ["second-element", "masked-element"]
        );
        assert_eq!(prepared.masks()[0].mask.as_ref().unwrap().id, "second-mask");
        if let SampledContentV4::Video { source, .. } = &graph.layers[0].content {
            assert_eq!(
                opencut_time::RationalTime::try_from(&source.source_time).unwrap(),
                opencut_time::RationalTime::try_from(&t(150000)).unwrap()
            );
        }
    }
}

#[test]
fn classic_import_v4_masks_reject_missing_parameters_without_guessing_defaults() {
    for key in [
        "centerX",
        "centerY",
        "width",
        "height",
        "rotation",
        "scale",
        "feather",
        "inverted",
        "strokeColor",
        "strokeWidth",
        "strokeAlign",
    ] {
        let input = edit(&masked_input("graphic", "rectangle"), |value| {
            element(value)["masks"][0]["params"]
                .as_object_mut()
                .unwrap()
                .remove(key);
        });
        let result = analyze(&input, &resources()).unwrap();
        assert!(result.document.is_none(), "{key}");
        assert!(
            result
                .analysis
                .blockers
                .iter()
                .any(|b| b.code == "MISSING_MASK_PARAMETER" && b.path.ends_with(key)),
            "{key}"
        );
    }
}

#[test]
fn classic_import_v4_masks_fail_closed_for_unknown_or_unsupported_semantics() {
    for case in 0..13 {
        let input = edit(&masked_input("graphic", "rectangle"), |value| {
            let element = element(value);
            match case {
                0 => element["masks"][0]["extra"] = json!(true),
                1 => element["masks"][0]["params"]["cornerRadius"] = json!(10),
                2 => element["masks"][0]["params"]["inverted"] = json!("true"),
                3 => element["masks"][0]["params"]["strokeWidth"] = json!(-1),
                4 => element["masks"][0]["type"] = json!("future-mask"),
                5 => element["masks"][0]["enabled"] = json!(false), // Not a Classic field.
                6 => element["masks"][0]["params"]["strokeAlign"] = json!("future"),
                7 => element["masks"][0]["params"]["feather"] = json!("10"),
                8 => element["masks"][0]["params"]["strokeColor"] = json!("red"),
                9 => element["masks"] = json!([mask("rectangle"), mask("ellipse")]), // Duplicate stable ID.
                10 => element["masks"] = json!({}),
                11 => element["animations"] = json!({"masks.original-mask.feather":{"keys":[]}}),
                _ => element["masks"][0]["params"]["width"] = json!(-1),
            }
        });
        let result = analyze(&input, &resources()).unwrap();
        assert!(result.document.is_none(), "case {case}");
        assert!(!result.analysis.blockers.is_empty(), "case {case}");
    }
}

#[test]
fn classic_import_v4_masks_keep_absent_empty_and_text_boundaries() {
    for remove in [false, true] {
        let input = edit(&masked_input("graphic", "rectangle"), |value| {
            if remove {
                element(value).as_object_mut().unwrap().remove("masks");
            } else {
                element(value)["masks"] = json!([]);
            }
        });
        assert!(
            parsed(&input, &resources()).sequences[0].tracks[1].elements[0]
                .visual
                .as_ref()
                .unwrap()
                .masks
                .is_empty()
        );
    }
    for masks in [json!([]), json!([mask("rectangle")])] {
        let input = changed(|value| {
            title(value)["masks"] = masks;
        });
        let result = analyze(&input, &resources()).unwrap();
        assert!(result.document.is_none());
        assert!(
            result
                .analysis
                .blockers
                .iter()
                .any(|b| b.path.ends_with(".masks"))
        );
    }
}
