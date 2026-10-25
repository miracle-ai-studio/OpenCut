use super::*;
use opencut_contracts::{scene::*, visual_frame::BoundVisualLayer};
fn prepared() -> PreparedGraphicFrame {
    let mut graph = crate::classic_text_frame::tests::graph();
    let mut shape = graph.layers[1].clone();
    shape.element_id = "shape".into();
    shape.content = SampledContent::Graphic {
        graphic: Graphic {
            geometry: GraphicGeometry::Star {
                points: 4.5,
                depth_percent: 45.0,
            },
            source_size: CanvasSize {
                width: 20,
                height: 10,
            },
            fill: Color {
                encoding: ColorEncoding::Srgb,
                red: 1.0,
                green: 0.5,
                blue: 0.0,
                alpha: 0.75,
            },
            stroke: Stroke {
                color: Color {
                    encoding: ColorEncoding::Srgb,
                    red: 0.0,
                    green: 0.0,
                    blue: 1.0,
                    alpha: 0.5,
                },
                width_px: 1.0,
                align: StrokeAlign::Center,
            },
        },
    };
    shape.visual.as_mut().unwrap().transform.scale_x = -2.0;
    graph.layers.insert(1, shape);
    prepare_graphic_frame(&graph).unwrap()
}
#[test]
fn graphic_partition_preserves_source_graph_and_full_original_order() {
    let prepared = prepared();
    assert_eq!(prepared.order(), &["back", "shape", "title", "front"]);
    assert!(matches!(
        prepared.source_graph().layers[1].content,
        SampledContent::Graphic { .. }
    ));
    assert_eq!(prepared.execution().source_graph().layers.len(), 3);
    assert!(prepare_caption_frame(prepared.source_graph()).is_err());
    assert!(matches!(
        prepared.graphics()[0].graphic.geometry,
        GraphicGeometryPlan::Star { points: 4.5, .. }
    ));
}
#[test]
fn graphic_padding_and_quad_use_logical_contain_miter_and_flip() {
    let prepared = prepared();
    let raster = graphic_raster_receipts(&prepared).unwrap().remove(0);
    assert_eq!(
        raster,
        GraphicRasterReceipt {
            element_id: "shape".into(),
            width: 30,
            height: 20,
            origin_x: 5,
            origin_y: 5
        }
    );
    let quad = prepared.graphics()[0].quad.as_ref().unwrap();
    assert_eq!(quad.width, 12.0);
    assert_eq!(quad.height, 4.0);
    assert!(quad.flip_x);
    assert!(!quad.flip_y);
    assert_eq!(quad.opacity, 0.25);
    let mut plan = prepared.graphics()[0].clone();
    plan.graphic.stroke.align = StrokeAlign::Outside;
    let outside = graphic_raster_receipt(&plan).unwrap();
    assert_eq!(
        (outside.width, outside.height, outside.origin_x),
        (40, 30, 10)
    );
    plan.graphic.stroke.align = StrokeAlign::Inside;
    assert_eq!(graphic_raster_receipt(&plan).unwrap().width, 20);
    plan.graphic.stroke.width_px = 0.0;
    plan.graphic.stroke.align = StrokeAlign::Outside;
    assert_eq!(graphic_raster_receipt(&plan).unwrap().origin_x, 0);
    plan.graphic.stroke.width_px = f64::MAX;
    assert!(graphic_raster_receipt(&plan).is_err());
    plan.graphic.stroke.align = StrokeAlign::Inside;
    assert!(graphic_raster_receipt(&plan).is_err());
    plan.graphic.stroke.width_px = 0.0;
    plan.graphic.source_width = 16385;
    assert!(graphic_raster_receipt(&plan).is_err());
}
#[test]
fn graphic_receipts_reject_missing_extra_wrong_dimensions_and_origin() {
    let p = prepared();
    let correct = graphic_raster_receipts(&p).unwrap();
    validate_graphic_receipts(&p, &correct).unwrap();
    assert!(validate_graphic_receipts(&p, &[]).is_err());
    let mut extra = correct.clone();
    extra.push(extra[0].clone());
    assert!(validate_graphic_receipts(&p, &extra).is_err());
    for case in 0..3 {
        let mut bad = correct.clone();
        match case {
            0 => bad[0].width += 1,
            1 => bad[0].origin_x += 1,
            _ => bad[0].element_id = "other".into(),
        };
        assert!(validate_graphic_receipts(&p, &bad).is_err());
    }
}
#[test]
fn graphic_prepare_rejects_legacy_effects_linear_color_and_nonfinite_quad() {
    for case in 0..4 {
        let p = prepared();
        let mut graph = p.source_graph().clone();
        let layer = &mut graph.layers[1];
        match case {
            0 => {
                let SampledContent::Graphic { graphic } = &mut layer.content else {
                    panic!()
                };
                graphic.geometry = GraphicGeometry::LegacyRectangle {
                    width_px: 20.0,
                    height_px: 10.0,
                };
            }
            1 => layer.visual.as_mut().unwrap().effects.push(SceneEffect {
                id: "active-blur".into(),
                enabled: true,
                effect: EffectKind::ClassicBlur { intensity: 1.0 },
            }),
            2 => {
                let SampledContent::Graphic { graphic } = &mut layer.content else {
                    panic!()
                };
                graphic.fill.encoding = ColorEncoding::LinearSrgb;
            }
            _ => layer.visual.as_mut().unwrap().transform.position_x_px = f64::MAX,
        };
        assert!(prepare_graphic_frame(&graph).is_err());
    }
}
#[test]
fn graphic_admission_reserves_generated_pixels_and_preserves_full_runtime_policy() {
    let p = prepared();
    let policy = crate::classic_text_frame::tests::policy();
    let coverage = crate::classic_text_frame::tests::proof(p.execution().execution());
    let session = ClassicGraphicFrameSession::new(&p, &policy, &coverage).unwrap();
    assert_eq!(session.policy(), &policy);
    let mut small = policy.clone();
    small.max_decoded_pixels = 600;
    assert_eq!(
        admit_graphic_frame(&p, &small).unwrap_err().code,
        "VISUAL_BUDGET_EXCEEDED"
    );
    small.max_decoded_pixels = 500;
    assert_eq!(
        admit_graphic_frame(&p, &small).unwrap_err().code,
        "VISUAL_BUDGET_EXCEEDED"
    );
    let mut small = policy;
    small.max_rgba_bytes = 9000;
    assert_eq!(
        admit_graphic_frame(&p, &small).unwrap_err().code,
        "VISUAL_BUDGET_EXCEEDED"
    );
}
#[test]
fn graphic_bound_merges_child_layers_without_dropping_fields_or_original_order() {
    let p = prepared();
    let policy = crate::classic_text_frame::tests::policy();
    let coverage = crate::classic_text_frame::tests::proof(p.execution().execution());
    let plan = plan_frame(&p, &policy, &coverage).unwrap();
    let child = BoundTextFrame {
        identity: plan.execution.execution.identity.clone(),
        clear: [0.0; 4],
        background: [0.1, 0.2, 0.3, 1.0],
        layers: p
            .execution()
            .execution()
            .layers()
            .iter()
            .map(|l| BoundVisualLayer {
                element_id: l.element_id.clone(),
                asset_id: format!("child:{}", l.element_id),
                quad: None,
            })
            .collect(),
        decoded: vec![],
        rasters: vec![],
    };
    let receipts = graphic_raster_receipts(&p).unwrap();
    let bound = bind_graphic_frame(&p, &child, &[], &receipts).unwrap();
    assert_eq!(
        bound
            .layers
            .iter()
            .map(|l| l.element_id.clone())
            .collect::<Vec<_>>(),
        p.order()
    );
    assert_eq!(bound.layers[0], child.layers[0]);
    assert_eq!(bound.layers[2], child.layers[1]);
    assert_eq!(bound.layers[3], child.layers[2]);
    assert_eq!(bound.layers[1].asset_id, "graphic:shape");
    assert_eq!(bound.layers[1].quad, p.graphics()[0].quad);
    assert_eq!(bound.background, child.background);
    assert_eq!(bound.decoded, child.decoded);
    let mut bad = child;
    bad.identity.revision = "other".into();
    assert!(bind_graphic_frame(&p, &bad, &[], &receipts).is_err());
}

#[test]
fn graphic_session_requires_caption_binding_before_graphics_or_pixels() {
    use crate::{classic_text_frame::*, classic_visual::*};
    struct NoResource;
    impl ClassicVisualResources for NoResource {
        fn resolve(&mut self, _: &VisualResource, _: &str) -> Result<(String, u64), EditorError> {
            panic!("No resource opening in pre-bind rejection")
        }
        fn read(&mut self, _: &str, _: u64, _: u32) -> Result<(Vec<u8>, bool), EditorError> {
            panic!()
        }
        fn release(&mut self, _: &str) -> Result<(), EditorError> {
            panic!()
        }
        fn checkpoint(&mut self) -> Result<(), EditorError> {
            Ok(())
        }
    }
    impl ClassicTextFrameResources for NoResource {
        fn resolve_font(
            &mut self,
            _: &TextFontResource,
            _: &str,
        ) -> Result<(String, u64), EditorError> {
            panic!()
        }
    }
    impl ClassicVisualRgbaSink for NoResource {
        fn chunk(&mut self, _: u64, _: &[u8]) -> Result<(), EditorError> {
            panic!("Pre-bind pixels must not reach sink")
        }
    }
    let p = prepared();
    let policy = crate::classic_text_frame::tests::policy();
    let coverage = crate::classic_text_frame::tests::proof(p.execution().execution());
    let mut session = ClassicGraphicFrameSession::new(&p, &policy, &coverage).unwrap();
    assert!(
        session
            .callback(
                GraphicFrameCallback::Graphic(GraphicCallback::Bind {
                    graphics: graphic_raster_receipts(&p).unwrap()
                }),
                &mut NoResource
            )
            .is_err()
    );
    assert!(session.rgba_chunk(0, &[0; 32], &mut NoResource).is_err());
    // The actual decode callback must leave the generated raster reservation
    // available, rather than returning the larger media-only child grant.
    let request = serde_json::from_value(serde_json::json!({
        "op": "admitDecode", "args": { "elementId": "back" }
    }))
    .unwrap();
    let admission: VisualDecodeAdmission =
        serde_json::from_value(session.callback(request, &mut NoResource).unwrap()).unwrap();
    let media = p.execution().execution().media();
    let expected = ClassicVisualSession::new(media, &graphic_media_policy(&p, &policy).unwrap())
        .unwrap()
        .decode_admission()
        .unwrap();
    assert_eq!(admission.max_decoded_bytes, expected.max_decoded_bytes);
    assert_eq!(admission.max_pixels, expected.max_pixels);
}
