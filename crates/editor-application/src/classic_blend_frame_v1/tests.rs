use super::*;
use crate::{
    classic_graphic_frame_v1::*, classic_text_frame_v2::*, classic_text_frame_v3::*,
    classic_visual::*,
};
use opencut_contracts::{
    project_v4::BlendModeV4, text_frame_v2::BoundTextFrameV2,
    visual_frame_v4::DecodedVisualSourceV4,
};
use serde_json::{Value, json};

pub(crate) fn prepared() -> PreparedBlendFrameV1 {
    let mut graph = crate::classic_text_frame_v2::tests::graph();
    let mut graphic = graph.layers[1].clone();
    graphic.element_id = "shape".into();
    graphic.content = SampledContentV4::Graphic {
        graphic: crate::project_v4_graphic_video_jobs::tests::graphic(),
    };
    graph.layers.insert(1, graphic);
    for (layer, mode) in graph.layers.iter_mut().zip([
        BlendModeV4::Multiply,
        BlendModeV4::Screen,
        BlendModeV4::Hue,
        BlendModeV4::Normal,
    ]) {
        layer.visual.as_mut().unwrap().blend_mode = mode;
    }
    prepare_blend_frame_v1(&graph).unwrap()
}
#[derive(Default)]
struct Resources {
    denied: bool,
    checkpoints: usize,
}
impl ClassicVisualResources for Resources {
    fn resolve(&mut self, _: &VisualResource, _: &str) -> Result<(String, u64), EditorError> {
        Ok(("media".into(), 7))
    }
    fn read(&mut self, _: &str, _: u64, _: u32) -> Result<(Vec<u8>, bool), EditorError> {
        Ok((vec![], true))
    }
    fn release(&mut self, _: &str) -> Result<(), EditorError> {
        Ok(())
    }
    fn checkpoint(&mut self) -> Result<(), EditorError> {
        self.checkpoints += 1;
        if self.denied {
            Err(EditorError::new("NOT_AUTHORIZED", "Revoked"))
        } else {
            Ok(())
        }
    }
}
impl ClassicTextFrameResources for Resources {
    fn resolve_font(
        &mut self,
        _: &TextFontResourceV2,
        _: &str,
    ) -> Result<(String, u64), EditorError> {
        Ok(("font".into(), 10))
    }
}
#[derive(Default)]
struct Sink(usize);
impl ClassicVisualRgbaSink for Sink {
    fn chunk(&mut self, offset: u64, bytes: &[u8]) -> Result<(), EditorError> {
        assert_eq!(offset, self.0 as u64);
        self.0 += bytes.len();
        Ok(())
    }
}
fn call(
    session: &mut ClassicBlendFrameSessionV1<'_>,
    resources: &mut Resources,
    value: Value,
) -> Result<Value, EditorError> {
    session.callback(serde_json::from_value(value).unwrap(), resources)
}
fn bind_child(
    session: &mut ClassicBlendFrameSessionV1<'_>,
    prepared: &PreparedBlendFrameV1,
    resources: &mut Resources,
) -> (BoundTextFrameV2, BoundGraphicFrameV1) {
    let plan = session.typed_plan().clone();
    let text_plan = &plan.execution.execution.execution;
    let font = &text_plan.texts[0].font;
    call(session, resources, json!({"op":"resolveVerifiedFontBytes","args":{"font":{
        "fontId":font.font_id,"contentHash":font.content_hash,"size":font.size,"weight":font.weight,"style":font.style,"faceIndex":font.face_index
    },"handle":"font-handle"}})).unwrap();
    for id in ["back", "front"] {
        call(
            session,
            resources,
            json!({"op":"admitDecode","args":{"elementId":id}}),
        )
        .unwrap();
    }
    let decoded: Vec<_> = ["back", "front"]
        .iter()
        .map(|id| DecodedVisualSourceV4::ReadyImage {
            element_id: (*id).into(),
            width: 1,
            height: 1,
        })
        .collect();
    call(
        session,
        resources,
        json!({"op":"bindFrame","args":{"decoded":decoded}}),
    )
    .unwrap();
    let raster = crate::classic_text_frame_v2::tests::raster(text_plan);
    let text = call(
        session,
        resources,
        json!({"op":"bindCaptionFrameV3","args":{"rasters":[raster],"captions":[]}}),
    )
    .unwrap();
    let receipts = graphic_raster_receipts_v1(prepared.execution()).unwrap();
    let graphic = call(
        session,
        resources,
        json!({"op":"bindGraphicFrameV1","args":{"graphics":receipts}}),
    )
    .unwrap();
    (
        serde_json::from_value(text).unwrap(),
        serde_json::from_value(graphic).unwrap(),
    )
}
#[test]
fn mixed_sources_keep_all_blends_and_legacy_normal_preparation_rejects_them() {
    let p = prepared();
    let policy = crate::classic_text_frame_v2::tests::policy();
    let coverage =
        crate::classic_text_frame_v2::tests::proof(p.execution().execution().execution());
    let plan = super::plan_v1(&p, &policy, &coverage).unwrap();
    assert_eq!(plan.identity.profile, CLASSIC_BLEND_FRAME_PROFILE_V1);
    assert_eq!(
        plan.execution.identity.profile,
        CLASSIC_GRAPHIC_FRAME_PROFILE_V1
    );
    assert_eq!(
        plan.blends
            .iter()
            .map(|b| b.element_id.as_str())
            .collect::<Vec<_>>(),
        ["back", "shape", "title", "front"]
    );
    assert_eq!(
        plan.blends.iter().map(|b| b.blend_mode).collect::<Vec<_>>(),
        [
            BlendModeV4::Multiply,
            BlendModeV4::Screen,
            BlendModeV4::Hue,
            BlendModeV4::Normal
        ]
    );
    assert!(prepare_graphic_frame_v1(p.source_graph()).is_err());
    let mut text_graph = p.source_graph().clone();
    text_graph.layers.remove(1);
    assert!(prepare_text_frame_v3(&text_graph).is_err());
    text_graph
        .layers
        .retain(|l| !matches!(l.content, SampledContentV4::Text { .. }));
    text_graph.fonts.clear();
    assert!(prepare_visual_frame_v4(&text_graph).is_err());
    assert!(
        serde_json::from_value::<LayerBlendV1>(json!({"elementId":"back","blendMode":"unknown"}))
            .is_err()
    );
}

#[test]
fn final_binding_precedes_pixels_and_binds_exact_scope_order_and_receipt() {
    let p = prepared();
    let policy = crate::classic_text_frame_v2::tests::policy();
    let coverage =
        crate::classic_text_frame_v2::tests::proof(p.execution().execution().execution());
    let mut session = ClassicBlendFrameSessionV1::new(&p, &policy, &coverage).unwrap();
    let mut resources = Resources::default();
    let mut sink = Sink::default();
    assert!(
        call(
            &mut session,
            &mut resources,
            json!({"op":"bindBlendFrameV1","args":{}})
        )
        .is_err()
    );
    assert!(session.rgba_chunk(0, &[0; 32], &mut sink).is_err());
    let (text, graphic) = bind_child(&mut session, &p, &mut resources);
    assert!(session.rgba_chunk(0, &[0; 32], &mut sink).is_err());
    let value = call(
        &mut session,
        &mut resources,
        json!({"op":"bindBlendFrameV1","args":{}}),
    )
    .unwrap();
    let bound: BoundBlendFrameV1 = serde_json::from_value(value).unwrap();
    assert_eq!(bound.execution, graphic);
    assert_eq!(bound.blends, p.blends());
    assert!(
        call(
            &mut session,
            &mut resources,
            json!({"op":"bindBlendFrameV1","args":{}})
        )
        .is_err()
    );
    session.rgba_chunk(0, &[0; 32], &mut sink).unwrap();
    let plan = session.typed_plan();
    let receipt = ClassicBlendFrameReceiptV1 {
        identity: plan.identity.clone(),
        engine_identity: CLASSIC_BLEND_FRAME_PROFILE_V1.into(),
        blends: p.blends().to_vec(),
        execution: ClassicGraphicFrameReceiptV1 {
            identity: plan.execution.identity.clone(),
            engine_identity: CLASSIC_GRAPHIC_FRAME_PROFILE_V1.into(),
            graphics: graphic.graphics.clone(),
            execution: ClassicCaptionFrameReceiptV3 {
                identity: plan.execution.execution.identity.clone(),
                engine_identity: CLASSIC_TEXT_FRAME_PROFILE_V3.into(),
                captions: vec![],
                execution: ClassicTextFrameReceiptV2 {
                    identity: text.identity.clone(),
                    engine_identity: CLASSIC_TEXT_FRAME_PROFILE_V2.into(),
                    bound: text,
                    rgba_bytes: "32".into(),
                    peak_accounted_bytes: "2000".into(),
                },
            },
        },
    };
    session.validate_receipt(&receipt).unwrap();
    for case in 0..6 {
        let mut bad = receipt.clone();
        match case {
            0 => bad.blends[0].blend_mode = BlendModeV4::Normal,
            1 => bad.blends.swap(0, 1),
            2 => {
                bad.blends.pop();
            }
            3 => bad.identity.revision = "other".into(),
            4 => bad.execution.graphics[0].width += 1,
            _ => {
                bad.execution.execution.execution.peak_accounted_bytes =
                    (policy.max_rgba_bytes + 1).to_string()
            }
        }
        assert!(session.validate_receipt(&bad).is_err(), "case {case}");
    }
    for case in 0..4 {
        let mut bad = graphic.clone();
        match case {
            0 => bad.identity.profile = CLASSIC_BLEND_FRAME_PROFILE_V1.into(),
            1 => bad.layers.swap(0, 1),
            2 => {
                bad.layers.pop();
            }
            _ => bad.graphics[0].origin_x += 1,
        }
        assert!(bind_blend_frame_v1(&p, &bad).is_err());
    }
    assert_eq!(session.policy(), &policy);
    assert_eq!(sink.0, 32);
}

#[test]
fn wrapper_reuses_admission_and_authorization_without_a_second_raster_budget() {
    let p = prepared();
    let policy = crate::classic_text_frame_v2::tests::policy();
    let coverage =
        crate::classic_text_frame_v2::tests::proof(p.execution().execution().execution());
    admit_blend_frame_v1(&p, &policy).unwrap();
    let mut reduced = policy.clone();
    reduced.max_decoded_pixels = 1;
    assert_eq!(
        admit_blend_frame_v1(&p, &reduced).unwrap_err().code,
        admit_graphic_frame_v1(p.execution(), &reduced)
            .unwrap_err()
            .code
    );
    let mut session = ClassicBlendFrameSessionV1::new(&p, &policy, &coverage).unwrap();
    let mut resources = Resources {
        denied: true,
        ..Resources::default()
    };
    assert_eq!(
        call(
            &mut session,
            &mut resources,
            json!({"op":"bindBlendFrameV1","args":{}})
        )
        .unwrap_err()
        .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(resources.checkpoints, 1);
    assert!(
        serde_json::from_value::<BlendFrameCallbackV1>(
            json!({"op":"bindBlendFrameV1","args":{"blends":[]}})
        )
        .is_err()
    );
}
