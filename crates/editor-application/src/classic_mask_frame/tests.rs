use super::*;
use crate::{
    classic_blend_frame::*, classic_caption_frame::*, classic_graphic_frame::*,
    classic_text_frame::*, classic_visual::*,
};
use opencut_contracts::{scene::*, text_frame::BoundTextFrame, visual_frame::DecodedVisualSource};
use serde_json::{Value, json};

pub(crate) fn mask() -> Mask {
    let mut stroke = crate::scene_graphic_video_jobs::tests::graphic().stroke;
    stroke.width_px = 0.0;
    Mask {
        id: "mask".into(),
        enabled: true,
        inverted: false,
        feather: MaskFeather::ClassicPixels { value: 2.5 },
        stroke,
        shape: LayerMaskShape::Rectangle {
            geometry: BoxMaskGeometry {
                center_x: 0.125,
                center_y: -0.1,
                width: 0.6,
                height: 0.7,
                rotation_degrees: 20.0,
                scale: 3.0,
            },
        },
    }
}
fn prepared() -> PreparedMaskFrame {
    let mut graph = crate::classic_blend_frame::tests::prepared()
        .source_graph()
        .clone();
    for index in [0, 1] {
        let mut mask = mask();
        mask.id = format!("mask-{index}");
        graph.layers[index]
            .visual
            .as_mut()
            .unwrap()
            .masks
            .push(mask);
    }
    prepare_mask_frame(&graph).unwrap()
}
#[derive(Default)]
struct Resources {
    denied: bool,
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
        _: &TextFontResource,
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
    session: &mut ClassicMaskFrameSession<'_>,
    resources: &mut Resources,
    value: Value,
) -> Result<Value, EditorError> {
    session.callback(serde_json::from_value(value).unwrap(), resources)
}
fn bind_child(
    session: &mut ClassicMaskFrameSession<'_>,
    prepared: &PreparedMaskFrame,
    resources: &mut Resources,
) -> (BoundTextFrame, BoundBlendFrame) {
    let plan = session.typed_plan().clone();
    let text_plan = &plan.execution.execution.execution.execution;
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
        .map(|id| DecodedVisualSource::ReadyImage {
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
    let raster = crate::classic_text_frame::tests::raster(text_plan);
    let text = call(
        session,
        resources,
        json!({"op":"bindCaptionFrameV3","args":{"rasters":[raster],"captions":[]}}),
    )
    .unwrap();
    let receipts = graphic_raster_receipts(prepared.execution()).unwrap();
    call(
        session,
        resources,
        json!({"op":"bindGraphicFrameV1","args":{"graphics":receipts}}),
    )
    .unwrap();
    let blend = call(
        session,
        resources,
        json!({"op":"bindBlendFrameV1","args":{}}),
    )
    .unwrap();
    (
        serde_json::from_value(text).unwrap(),
        serde_json::from_value(blend).unwrap(),
    )
}
fn receipt(
    session: &ClassicMaskFrameSession<'_>,
    text: BoundTextFrame,
    blend: &BoundBlendFrame,
    peak: u64,
) -> ClassicMaskFrameReceipt {
    let plan = session.typed_plan();
    ClassicMaskFrameReceipt {
        identity: plan.identity.clone(),
        engine_identity: CLASSIC_MASK_FRAME_PROFILE.into(),
        masks: plan.masks.clone(),
        execution: ClassicBlendFrameReceipt {
            identity: plan.execution.identity.clone(),
            engine_identity: CLASSIC_BLEND_FRAME_PROFILE.into(),
            blends: plan.execution.blends.clone(),
            execution: ClassicGraphicFrameReceipt {
                identity: plan.execution.execution.identity.clone(),
                engine_identity: CLASSIC_GRAPHIC_FRAME_PROFILE.into(),
                graphics: blend.execution.graphics.clone(),
                execution: ClassicCaptionFrameReceipt {
                    identity: plan.execution.execution.execution.identity.clone(),
                    engine_identity: CLASSIC_CAPTION_FRAME_PROFILE.into(),
                    captions: vec![],
                    execution: ClassicTextFrameReceipt {
                        identity: text.identity.clone(),
                        engine_identity: CLASSIC_TEXT_FRAME_PROFILE.into(),
                        bound: text,
                        rgba_bytes: "32".into(),
                        peak_accounted_bytes: peak.to_string(),
                    },
                },
            },
        },
    }
}

#[test]
fn mask_session_requires_final_binding_before_pixels_and_rejects_forged_receipts() {
    let prepared = prepared();
    let policy = crate::classic_text_frame::tests::policy();
    let coverage =
        crate::classic_text_frame::tests::proof(prepared.execution().execution().execution());
    let mut session = ClassicMaskFrameSession::new(&prepared, &policy, &coverage).unwrap();
    let mut resources = Resources::default();
    let mut sink = Sink::default();
    assert!(
        call(
            &mut session,
            &mut resources,
            json!({"op":"bindMaskFrameV1","args":{}})
        )
        .is_err()
    );
    assert!(session.rgba_chunk(0, &[0; 32], &mut sink).is_err());
    let (text, blend) = bind_child(&mut session, &prepared, &mut resources);
    assert!(session.rgba_chunk(0, &[0; 32], &mut sink).is_err());
    let bound: BoundMaskFrame = serde_json::from_value(
        call(
            &mut session,
            &mut resources,
            json!({"op":"bindMaskFrameV1","args":{}}),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(bound.execution, blend);
    assert!(bound.masks[0].mask.is_some() && bound.masks[1].mask.is_some());
    assert!(bound.masks[2].mask.is_none() && bound.masks[2].quad.is_none());
    assert!(
        call(
            &mut session,
            &mut resources,
            json!({"op":"bindMaskFrameV1","args":{}})
        )
        .is_err()
    );
    session.rgba_chunk(0, &[0; 32], &mut sink).unwrap();
    let receipt = receipt(&session, text, &blend, 2000);
    session.validate_receipt(&receipt).unwrap();
    for case in 0..7 {
        let mut bad = receipt.clone();
        match case {
            0 => bad.masks.swap(0, 1),
            1 => {
                bad.masks.pop();
            }
            2 => {
                bad.masks[0].mask.as_mut().unwrap().feather =
                    MaskFeather::ClassicPixels { value: 9.0 }
            }
            3 => bad.masks[2].mask = Some(mask()),
            4 => bad.identity.revision = "other".into(),
            5 => bad.execution.blends[0].blend_mode = BlendMode::Normal,
            _ => bad.execution.execution.graphics[0].width += 1,
        }
        assert!(session.validate_receipt(&bad).is_err(), "case {case}");
    }
    assert_eq!(sink.0, 32);
    assert!(
        serde_json::from_value::<MaskFrameCallback>(
            json!({"op":"bindMaskFrameV1","args":{"masks":[]}})
        )
        .is_err()
    );
}

#[test]
fn mask_final_authorization_and_exact_child_binding_cannot_be_bypassed() {
    let prepared = prepared();
    let policy = crate::classic_text_frame::tests::policy();
    let coverage =
        crate::classic_text_frame::tests::proof(prepared.execution().execution().execution());
    let mut session = ClassicMaskFrameSession::new(&prepared, &policy, &coverage).unwrap();
    let mut resources = Resources::default();
    let (_, child) = bind_child(&mut session, &prepared, &mut resources);
    for case in 0..5 {
        let mut bad = child.clone();
        match case {
            0 => bad.blends.swap(0, 1),
            1 => bad.execution.layers.swap(0, 1),
            2 => bad.execution.layers[0].quad.as_mut().unwrap().width += 1.0,
            3 => bad.execution.graphics[0].origin_x += 1,
            _ => bad.identity.revision = "other".into(),
        }
        assert!(bind_mask_frame(&prepared, &bad).is_err(), "case {case}");
    }
    resources.denied = true;
    assert_eq!(
        call(
            &mut session,
            &mut resources,
            json!({"op":"bindMaskFrameV1","args":{}})
        )
        .unwrap_err()
        .code,
        "NOT_AUTHORIZED"
    );
    let mut sink = Sink::default();
    assert!(session.rgba_chunk(0, &[0; 32], &mut sink).is_err());
    assert_eq!(sink.0, 0);
}

#[test]
fn mask_admission_reserves_sources_but_one_aggregate_peak_uses_the_full_policy() {
    let prepared = prepared();
    let policy = crate::classic_text_frame::tests::policy();
    let coverage =
        crate::classic_text_frame::tests::proof(prepared.execution().execution().execution());
    let delegated = mask_execution_policy(&prepared, &policy).unwrap();
    assert!(delegated.max_rgba_bytes < policy.max_rgba_bytes);
    assert!(delegated.max_gpu_bytes < policy.max_gpu_bytes);
    admit_mask_frame(&prepared, &policy).unwrap();
    let mut too_small = policy.clone();
    too_small.max_rgba_bytes = policy.max_rgba_bytes - delegated.max_rgba_bytes - 1;
    assert_eq!(
        admit_mask_frame(&prepared, &too_small).unwrap_err().code,
        "VISUAL_BUDGET_EXCEEDED"
    );
    let mut session = ClassicMaskFrameSession::new(&prepared, &policy, &coverage).unwrap();
    assert_eq!(session.policy(), &policy); // Native observes the caller's full policy.
    let mut resources = Resources::default();
    let (text, blend) = bind_child(&mut session, &prepared, &mut resources);
    call(
        &mut session,
        &mut resources,
        json!({"op":"bindMaskFrameV1","args":{}}),
    )
    .unwrap();
    session
        .rgba_chunk(0, &[0; 32], &mut Sink::default())
        .unwrap();
    let good = receipt(&session, text.clone(), &blend, policy.max_rgba_bytes - 1);
    session.validate_receipt(&good).unwrap();
    let bad = receipt(&session, text, &blend, policy.max_rgba_bytes + 1);
    assert!(session.validate_receipt(&bad).is_err());
}

#[test]
fn mask_profile_keeps_unmasked_text_and_blends_without_relaxing_old_profiles() {
    let prepared = prepared();
    assert_eq!(
        prepared
            .masks()
            .iter()
            .map(|m| m.element_id.as_str())
            .collect::<Vec<_>>(),
        ["back", "shape", "title", "front"]
    );
    assert_eq!(
        prepared
            .blends()
            .iter()
            .map(|m| m.blend_mode)
            .collect::<Vec<_>>(),
        [
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Hue,
            BlendMode::Normal
        ]
    );
    assert!(prepare_blend_frame(prepared.source_graph()).is_err());
    assert!(prepare_graphic_frame(prepared.source_graph()).is_err());
    assert_eq!(
        prepared.execution().execution().execution().texts().len(),
        1
    );
}
