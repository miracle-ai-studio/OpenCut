use super::*;
use opencut_contracts::{Time, project_v4::*};
pub(crate) fn graphic() -> GraphicV4 {
    let color = |red| ColorV4 {
        encoding: ColorEncodingV4::Srgb,
        red,
        green: 0.0,
        blue: 0.0,
        alpha: 1.0,
    };
    GraphicV4 {
        geometry: GraphicGeometryV4::Rectangle {
            corner_radius_percent: 0.0,
        },
        source_size: CanvasSizeV4 {
            width: 20,
            height: 10,
        },
        fill: color(1.0),
        stroke: StrokeV4 {
            color: color(0.0),
            width_px: 1.0,
            align: StrokeAlignV4::Center,
        },
    }
}
fn fixture() -> PreparedV4GraphicVideoMp4V1 {
    let mut input = video::tests::input();
    let sequence = &mut input.snapshot.project.sequences[0];
    let mut track = sequence.tracks[0].clone();
    track.id = "graphics".into();
    track.kind = TrackKindV4::Graphic;
    track.elements[0].id = "shape".into();
    track.elements[0].content = ElementContentV4::Graphic { graphic: graphic() };
    track.elements[0].timeline_range.duration = Time {
        ticks: "3".into(),
        timebase: 1,
    };
    sequence.tracks.push(track);
    input
}
#[test]
fn graphic_video_keeps_original_cfr_audio_resources_and_old_profile_rejection() {
    let input = fixture();
    let plan = validate_prepared(&input).unwrap();
    assert_eq!(plan.identity.frame_count, "90");
    assert_eq!(plan.identity.audio_frame_count, "144000");
    assert_eq!(plan.asset_ids, ["image"]);
    assert!(plan.font_ids.is_empty());
    assert!(video::validate_prepared(&input).is_err());
    assert!(crate::project_v4_caption_video_jobs::validate_prepared(&input).is_err());
    let frame = video::prepare_profiled_frame(
        &input.snapshot,
        "scene",
        &Time {
            ticks: "0".into(),
            timebase: 1,
        },
        video::VideoFrameProfile::GraphicV1,
    )
    .unwrap();
    frame.admit(&input.processing_policy.visual).unwrap();
    let video::PreparedVideoFrame::Graphic(frame) = frame else {
        panic!("graphic profile required")
    };
    assert_eq!(frame.graphics().len(), 1);
    assert_eq!(frame.order().len(), 2);
    assert_ne!(
        render_input_sha256(&input.snapshot, "scene").unwrap(),
        video::render_input_sha256(&input.snapshot, "scene").unwrap()
    );
}
#[test]
fn graphic_video_admission_counts_generated_sources_before_acceptance() {
    let mut input = fixture();
    let policy = &mut input.processing_policy.visual;
    policy.max_decoded_pixels = 600; // graphic source including miter padding alone is 30 * 20
    assert_eq!(
        validate_prepared(&input).err().unwrap().code,
        "VISUAL_BUDGET_EXCEEDED"
    );
    let frame = video::prepare_profiled_frame(
        &input.snapshot,
        "scene",
        &Time {
            ticks: "0".into(),
            timebase: 1,
        },
        video::VideoFrameProfile::GraphicV1,
    )
    .unwrap();
    assert_eq!(
        frame
            .admit(&input.processing_policy.visual)
            .unwrap_err()
            .code,
        "VISUAL_BUDGET_EXCEEDED"
    );
}
#[test]
fn graphic_prepared_payload_requires_matching_job_kind_scope_and_locks() {
    let mut input = fixture();
    let public = SubmitV4GraphicVideoMp4V1 {
        project_id: input.snapshot.project.id.clone(),
        revision: input.snapshot.revision.clone(),
        sequence_id: "scene".into(),
        idempotency_key: "fixed".into(),
    };
    let mut request = JobRequest {
        kind: V4_GRAPHIC_VIDEO_MP4_KIND.into(),
        idempotency_key: "fixed".into(),
        input: serde_json::to_value(public).unwrap(),
    };
    validate_prepared_request(&request, &input).unwrap();
    request.kind = video::V4_VIDEO_MP4_KIND.into();
    assert!(validate_prepared_request(&request, &input).is_err());
    request.kind = V4_GRAPHIC_VIDEO_MP4_KIND.into();
    input.asset_lock.assets.clear();
    assert!(validate_prepared_request(&request, &input).is_err());
}
