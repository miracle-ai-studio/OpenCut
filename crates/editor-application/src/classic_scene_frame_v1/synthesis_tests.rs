use super::*;
use opencut_contracts::{font_derived_v1::ImportDerivedFontV1, project_v4::*};

fn graph() -> CompositionV4 {
    let mut graph = crate::classic_text_frame_v2::tests::graph();
    graph.fonts = vec![
        crate::font_synthesis_v1::expected_import_resource(&ImportDerivedFontV1 {
            project_id: graph.project_id.clone(),
            expected_revision: graph.revision.clone(),
            idempotency_key: "scene-synthesis".into(),
            catalog_id: "google-gudea-bold-synthetic-italic-v1".into(),
            font_id: "font".into(),
            required_text: "AV".into(),
        })
        .unwrap(),
    ];
    for layer in &mut graph.layers {
        if let SampledContentV4::Text { text } = &mut layer.content {
            text.content = "AV".into();
            text.font_weight = FontWeightV4::Bold;
            text.font_style = FontStyleV4::Italic;
        }
    }
    graph
}
fn plan(graph: &CompositionV4) -> Result<ClassicSceneFramePlanV1, EditorError> {
    let prepared =
        prepare_scene_frame_v1(graph).map_err(|e| EditorError::new(e.code, e.message))?;
    let text = prepared.execution().execution().execution();
    let mut coverage = crate::classic_text_frame_v2::tests::proof(text);
    coverage.extend(
        prepared
            .mask_texts()
            .iter()
            .map(|mask| NativeTextCoverageV1 {
                element_id: mask.mask_id.clone(),
                font: mask.font.clone(),
                content: mask.mask.content.clone(),
            }),
    );
    let mut policy = crate::classic_text_frame_v2::tests::policy();
    policy.max_encoded_bytes = 32 * 1024 * 1024;
    policy.max_rgba_bytes = 128 * 1024 * 1024;
    plan_v1(&prepared, &policy, &coverage)
}
#[test]
fn scene_keeps_authored_italic_source_actual_font_and_explicit_owner_binding() {
    let source = graph();
    let before = source.clone();
    let prepared = prepare_scene_frame_v1(&source).unwrap();
    assert_eq!(prepared.source_graph(), &before);
    let native = &prepared.execution().execution().execution().texts()[0];
    assert_eq!(native.font.style, FontStyleV4::Normal);
    assert_eq!(native.font.content, source.fonts[0].content);
    let result = plan(&source).unwrap();
    assert_eq!(result.font_synthesis.len(), 1);
    let binding = &result.font_synthesis[0];
    assert_eq!(binding.element_id, "title");
    assert_eq!(binding.owner_id, "title");
    assert_eq!(binding.owner_kind, SceneFontSynthesisOwnerV1::Text);
    assert_eq!(binding.binding.actual.style, FontStyleV4::Normal);
    assert_eq!(binding.binding.requested.style, FontStyleV4::Italic);
    assert_eq!(
        binding.binding.actual.content_hash,
        source.fonts[0].content.content_hash
    );
    assert_eq!(source, before);
}
#[test]
fn scene_rejects_forged_licensed_resource_and_nonitalic_request_before_execution() {
    let original = graph();
    let mut forged = original.clone();
    forged.fonts[0].license.as_mut().unwrap().attribution = None;
    assert!(plan(&forged).is_err());
    for style in [FontStyleV4::Normal] {
        let mut changed = original.clone();
        if let SampledContentV4::Text { text } = &mut changed.layers[0].content {
            text.font_style = style;
        }
        assert!(prepare_scene_frame_v1(&changed).is_err());
    }
}
#[test]
fn scene_text_mask_keeps_requested_style_and_distinct_mask_owner() {
    let mut source = graph();
    let mask = MaskV4 {
        id: "italic-mask".into(),
        enabled: true,
        inverted: false,
        shape: MaskShapeV4::Text {
            mask: TextMaskV4 {
                content: "AV".into(),
                font: FontBindingV4::Locked {
                    font_id: "font".into(),
                },
                font_size: FontSizeV4::CanvasPixels { value: 12. },
                font_weight: FontWeightV4::Bold,
                font_style: FontStyleV4::Italic,
                decoration: TextDecorationV4::None,
                letter_spacing_px: 0.,
                line_height_multiplier: 1.,
                center_x: 0.,
                center_y: 0.,
                rotation_degrees: 0.,
                scale: 1.,
            },
        },
        feather: MaskFeatherV4::ClassicPixels { value: 0. },
        stroke: StrokeV4 {
            color: ColorV4 {
                encoding: ColorEncodingV4::Srgb,
                red: 0.,
                green: 0.,
                blue: 0.,
                alpha: 1.,
            },
            width_px: 0.,
            align: StrokeAlignV4::Outside,
        },
    };
    source.layers[1].visual.as_mut().unwrap().masks.push(mask);
    let result = plan(&source).unwrap();
    assert_eq!(result.font_synthesis.len(), 2);
    let entry = result
        .font_synthesis
        .iter()
        .find(|e| e.owner_kind == SceneFontSynthesisOwnerV1::TextMask)
        .unwrap();
    assert_eq!(entry.element_id, "front");
    assert_eq!(entry.owner_id, "italic-mask");
    assert_eq!(result.mask_fonts[0].font.style, FontStyleV4::Normal);
    let MaskShapeV4::Text { mask } = &result.stacks[1].masks[0].shape else {
        panic!()
    };
    assert_eq!(mask.font_style, FontStyleV4::Italic);
}

#[test]
fn scene_delegates_sticker_bytes_to_image_without_rewriting_its_source_node() {
    let mut source = crate::classic_text_frame_v2::tests::graph();
    source.layers.retain(|layer| layer.element_id == "front");
    source.fonts.clear();
    source.layers[0].content = SampledContentV4::Sticker {
        asset_id: "image".into(),
        intrinsic_size: CanvasSizeV4 {
            width: 37,
            height: 29,
        },
    };
    let before = source.clone();
    // Frozen text profiles still reject the original sticker node.
    assert!(prepare_text_frame_v2(&source).is_err());
    let prepared = prepare_scene_frame_v1(&source).unwrap();
    assert_eq!(prepared.source_graph(), &before);
    let media = prepared.execution().execution().execution().media();
    assert!(
        matches!(&media.layers()[0].content, SampledContentV4::Image { asset_id } if asset_id == "image")
    );
    assert_eq!(media.resources(), source.resources.as_slice());
    assert_eq!(prepared.stacks()[0].element_id, "front");
    assert_eq!(source, before);
}
