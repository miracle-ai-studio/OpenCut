use super::*;
use crate::scene::new_document;
use opencut_contracts::{
    AssetVersion, CreateProject, FrameRate, Time, TimeRange, scene::*, text_frame::FontLock,
};
fn fixture() -> PreparedCaptionVideoMp4 {
    let mut project = new_document(&CreateProject {
        id: "caption-project".into(),
        name: "Captions".into(),
        sequence_id: "scene".into(),
        width: 320,
        height: 180,
        fps: FrameRate {
            numerator: 30,
            denominator: 1,
        },
    })
    .unwrap();
    let version = AssetVersion {
        id: format!("av_{}", "f".repeat(64)),
        content_hash: "f".repeat(64),
        size: 100,
    };
    project.fonts.push(FontResource {
        id: "font".into(),
        family: "Example".into(),
        weight: 400,
        style: FontStyle::Normal,
        face_index: 0,
        content: version,
        origin: ResourceOrigin::LegacyLocator {
            uri: "/private/test-font".into(),
        },
        license: None,
    });
    let mut track = project.sequences[0].tracks[0].clone();
    track.id = "captions".into();
    track.name = "Captions".into();
    track.kind = SceneTrackKind::Caption;
    track.elements.clear();
    project.sequences[0].tracks.push(track);
    project.sequences[0].captions.push(Caption {
        id: "cue".into(),
        track_id: "captions".into(),
        timeline_range: TimeRange {
            start: Time {
                ticks: "1".into(),
                timebase: 1,
            },
            duration: Time {
                ticks: "2".into(),
                timebase: 1,
            },
        },
        text: "AV To captions".into(),
        source_anchor: None,
        style: SceneCaptionStyle {
            font: FontBinding::Locked {
                font_id: "font".into(),
            },
            font_size: FontSize::CanvasPixels { value: 24.0 },
            color: Color {
                encoding: ColorEncoding::Srgb,
                red: 1.0,
                green: 1.0,
                blue: 1.0,
                alpha: 1.0,
            },
            background_color: None,
            max_width_px: None,
            line_height_px: None,
            text_align: SceneTextAlign::Center,
            layout: CaptionLayoutProfile::PinnedCaptionV4,
        },
    });
    let mut visual = crate::classic_text_frame::tests::policy();
    visual.max_frame_pixels = 1_000_000;
    visual.max_decoded_pixels = 1_000_000;
    visual.max_gpu_bytes = 10_000_000;
    visual.max_rgba_bytes = 10_000_000;
    visual.max_output_bytes = 20_000_000;
    PreparedCaptionVideoMp4 {
        snapshot: EditorSnapshot { project, revision: "fixed".into() },
        sequence_id: "scene".into(),
        asset_lock: opencut_contracts::AssetLockManifest { version: 1, revision: "fixed".into(), sequence_id: "scene".into(), assets: vec![] },
        font_lock: vec![FontLock { font_id: "font".into(), content_hash: "f".repeat(64), size: 100 }],
        authority: video::TrustedJobAuthority::LocalOwner,
        processing_policy: LocalVideoExecutionPolicy {
            visual,
            audio: opencut_job_execution::scene_audio::LocalAudioExecutionPolicy {
                profile: opencut_job_execution::scene_audio::LocalAudioProcessingProfile::LocalClassicAudioV1,
                max_pcm_bytes: 256 * 1024 * 1024,
                max_encoded_bytes: 32 * 1024 * 1024,
                max_rss_bytes: 2 * 1024 * 1024 * 1024,
                deadline_ms: 120000,
            },
            max_frames: 1800,
            max_output_bytes: 256 * 1024 * 1024,
            deadline_ms: None,
            stalled_after_ms: Some(300000),
        },
    }
}
#[test]
fn caption_only_tail_extends_exact_cfr_and_original_whole_scene_silent_pcm() {
    let input = fixture();
    let plan = validate_prepared(&input).unwrap();
    assert_eq!(plan.identity.frame_count, "90");
    assert_eq!(plan.identity.audio_frame_count, "144000");
    assert_eq!(plan.audio.duration_samples(), 144000);
    assert!(plan.asset_ids.is_empty());
    assert_eq!(plan.font_ids, vec!["font"]);
    assert!(video::validate_prepared(&input).is_err());
    let frame = video::prepare_profiled_frame(
        &input.snapshot,
        "scene",
        &Time {
            ticks: "2".into(),
            timebase: 1,
        },
        video::VideoFrameProfile::CaptionV3,
    )
    .unwrap();
    let video::PreparedVideoFrame::Caption(frame) = frame else {
        panic!("caption version required")
    };
    assert!(matches!(
        frame.source_graph().layers[0].content,
        opencut_composition::scene::SampledContent::Caption { .. }
    ));
    assert_eq!(frame.captions()[0].max_width_px, 256.0);
    assert_eq!(frame.captions()[0].bottom_margin_px, 9.0);
}
#[test]
fn prepared_payload_never_selects_a_profile_without_the_matching_original_job_kind_and_locks() {
    let mut input = fixture();
    let public = FixedVideoInput {
        project_id: input.snapshot.project.id.clone(),
        revision: "fixed".into(),
        sequence_id: "scene".into(),
        idempotency_key: "original".into(),
    };
    let mut request = JobRequest {
        kind: CAPTION_VIDEO_MP4_KIND.into(),
        idempotency_key: "original".into(),
        input: serde_json::to_value(public).unwrap(),
    };
    validate_prepared_request(&request, &input).unwrap();
    request.kind = video::MEDIA_VIDEO_MP4_KIND.into();
    assert!(validate_prepared_request(&request, &input).is_err());
    request.kind = CAPTION_VIDEO_MP4_KIND.into();
    input.font_lock.clear();
    assert!(validate_prepared_request(&request, &input).is_err());
}
