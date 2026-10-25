use super::*;
use opencut_contracts::{Time, scene::*, text_frame::FontLock};

fn t(ticks: i64) -> Time {
    Time {
        ticks: ticks.to_string(),
        timebase: 1,
    }
}
fn fixture() -> PreparedBlendVideoMp4 {
    let mut input = video::tests::input();
    let graph = crate::classic_text_frame::tests::graph();
    let font = graph.fonts[0].clone();
    input.font_lock.push(FontLock {
        font_id: font.id.clone(),
        content_hash: font.content.content_hash.clone(),
        size: font.content.size,
    });
    input.snapshot.project.fonts.push(font);
    let sequence = &mut input.snapshot.project.sequences[0];
    sequence.tracks[0].elements[0]
        .visual
        .as_mut()
        .unwrap()
        .blend_mode = BlendMode::Multiply;
    let mut titles = sequence.tracks[0].clone();
    titles.id = "titles".into();
    titles.kind = SceneTrackKind::Text;
    let title = &mut titles.elements[0];
    title.id = "title".into();
    let opencut_composition::scene::SampledContent::Text { text } = graph.layers[1].content.clone()
    else {
        panic!()
    };
    title.content = ElementContent::Text { text };
    title.timeline_range.start = t(1);
    title.timeline_range.duration = t(1);
    title.visual.as_mut().unwrap().blend_mode = BlendMode::Screen;
    sequence.tracks.push(titles);
    input
}
#[test]
fn blended_media_and_late_text_keep_source_modes_and_full_cfr_resource_scope() {
    let input = fixture();
    let plan = validate_prepared(&input).unwrap();
    assert_eq!(plan.identity.frame_count, "60");
    assert_eq!(plan.identity.audio_frame_count, "96000");
    assert_eq!(plan.asset_ids, ["image"]);
    assert_eq!(plan.font_ids, ["font"]);
    assert!(video::validate_prepared(&input).is_err());
    assert!(crate::scene_caption_video_jobs::validate_prepared(&input).is_err());
    assert!(crate::scene_graphic_video_jobs::validate_prepared(&input).is_err());
    for (at, expected) in [
        (t(0), vec![BlendMode::Multiply]),
        (t(1), vec![BlendMode::Multiply, BlendMode::Screen]),
    ] {
        let selected = video::prepare_profiled_frame(
            &input.snapshot,
            "scene",
            &at,
            video::VideoFrameProfile::BlendV1,
        )
        .unwrap();
        selected.admit(&input.processing_policy.visual).unwrap();
        let video::PreparedVideoFrame::Blend(frame) = selected else {
            panic!("Blend profile required")
        };
        assert_eq!(
            frame
                .blends()
                .iter()
                .map(|b| b.blend_mode)
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            frame
                .source_graph()
                .layers
                .iter()
                .map(|l| l.visual.as_ref().unwrap().blend_mode)
                .collect::<Vec<_>>(),
            expected
        );
        assert!(
            frame.execution().source_graph().layers.iter().all(|l| l
                .visual
                .as_ref()
                .unwrap()
                .blend_mode
                == BlendMode::Normal)
        );
    }
}
#[test]
fn disabled_blend_is_admitted_but_authored_effects_and_masks_still_reject() {
    let mut input = fixture();
    input.snapshot.project.sequences[0].tracks[1].elements[0].enabled = false;
    input.font_lock.clear();
    let plan = validate_prepared(&input).unwrap();
    assert!(plan.font_ids.is_empty());
    assert_eq!(plan.asset_ids, ["image"]);
    assert!(crate::scene_graphic_video_jobs::validate_prepared(&input).is_err());
    for case in 0..2 {
        let mut bad = input.clone();
        let style = bad.snapshot.project.sequences[0].tracks[1].elements[0]
            .visual
            .as_mut()
            .unwrap();
        if case == 0 {
            style.effects.push(SceneEffect {
                id: "effect".into(),
                enabled: true,
                effect: EffectKind::ClassicBlur { intensity: 1.0 },
            });
        } else {
            style.masks.push(Mask {
                id: "mask".into(),
                enabled: true,
                inverted: false,
                feather: MaskFeather::ClassicPixels { value: 0.0 },
                stroke: crate::scene_graphic_video_jobs::tests::graphic().stroke,
                shape: LayerMaskShape::Split {
                    center_x: 0.0,
                    center_y: 0.0,
                    rotation_degrees: 0.0,
                },
            });
        }
        assert_eq!(
            validate_prepared(&bad).err().unwrap().code,
            "UNSUPPORTED_VIDEO_PROFILE"
        );
    }
}
#[test]
fn late_frame_budget_and_every_asset_and_font_lock_are_required() {
    for case in 0..5 {
        let mut input = fixture();
        match case {
            0 => input.asset_lock.assets.clear(),
            1 => input.asset_lock.assets[0].content_hash = "b".repeat(64),
            2 => input.font_lock.clear(),
            3 => input.font_lock[0].size += 1,
            _ => input.processing_policy.visual.max_decoded_pixels = 64 * 48,
        }
        assert!(validate_prepared(&input).is_err(), "case {case}");
    }
    let mut input = fixture();
    input.processing_policy.visual.max_decoded_pixels = 64 * 48;
    // The initial media-only frame fits; the later text frame is still checked.
    let first = video::prepare_profiled_frame(
        &input.snapshot,
        "scene",
        &t(0),
        video::VideoFrameProfile::BlendV1,
    )
    .unwrap();
    first.admit(&input.processing_policy.visual).unwrap();
    assert_eq!(
        validate_prepared(&input).err().unwrap().code,
        "VISUAL_BUDGET_EXCEEDED"
    );
}
#[test]
fn kind_scope_and_render_hash_bind_original_blend_semantics() {
    let input = fixture();
    let public = FixedVideoInput {
        project_id: input.snapshot.project.id.clone(),
        revision: input.snapshot.revision.clone(),
        sequence_id: "scene".into(),
        idempotency_key: "fixed-blend".into(),
    };
    let mut request = JobRequest {
        kind: BLEND_VIDEO_MP4_KIND.into(),
        idempotency_key: public.idempotency_key.clone(),
        input: serde_json::to_value(&public).unwrap(),
    };
    validate_prepared_request(&request, &input).unwrap();
    request.kind = opencut_job_contracts::scene_graphic_video::GRAPHIC_VIDEO_MP4_KIND.into();
    assert!(validate_prepared_request(&request, &input).is_err());
    request.kind = BLEND_VIDEO_MP4_KIND.into();
    request.input["revision"] = serde_json::json!("other");
    assert!(validate_prepared_request(&request, &input).is_err());
    let digest = render_input_sha256(&input.snapshot, "scene").unwrap();
    assert_ne!(
        digest,
        crate::scene_graphic_video_jobs::render_input_sha256(&input.snapshot, "scene").unwrap()
    );
    let mut changed = input.snapshot.clone();
    changed.project.sequences[0].tracks[1].elements[0]
        .visual
        .as_mut()
        .unwrap()
        .blend_mode = BlendMode::Normal;
    assert_ne!(digest, render_input_sha256(&changed, "scene").unwrap());
    assert_eq!(
        public_identity(&validate_prepared(&input).unwrap().identity).profile,
        BlendVideoProfile::ClassicV31CfrH264AacBlendV1
    );
}
