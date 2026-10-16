use super::*;
use crate::project_v4::new_document;
use opencut_contracts::{
    AssetKind, AssetVersion, CreateProject, FrameRate, LockedAsset, TimeRange,
};

fn t(n: i64, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
fn policy() -> LocalVideoExecutionPolicyV1 {
    LocalVideoExecutionPolicyV1 { visual: opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1 {
        max_frame_pixels: 16_000_000,
        max_decoded_pixels: 64_000_000,
        max_gpu_bytes: 256 * 1024 * 1024,
        max_rgba_bytes: 256 * 1024 * 1024,
        max_encoded_bytes: 32 * 1024 * 1024,
        max_output_bytes: 129 * 1024 * 1024,
        max_rss_bytes: 2 * 1024 * 1024 * 1024,
        deadline_ms: 120000,
    }, audio: opencut_job_execution::project_v4_audio::LocalAudioExecutionPolicyV1 {
        profile: opencut_job_execution::project_v4_audio::LocalAudioProcessingProfileV1::LocalClassicAudioV1,
        max_pcm_bytes: 256 * 1024 * 1024, max_encoded_bytes: 32 * 1024 * 1024, max_rss_bytes: 2 * 1024 * 1024 * 1024, deadline_ms: 120000,
    }, max_frames: 1800, max_output_bytes: 256 * 1024 * 1024, deadline_ms: 300000 }
}
pub(crate) fn input() -> PreparedV4VideoMp4V1 {
    let mut project = new_document(&CreateProject {
        id: "project".into(),
        name: "Picture".into(),
        sequence_id: "scene".into(),
        width: 64,
        height: 48,
        fps: FrameRate {
            numerator: 30,
            denominator: 1,
        },
    })
    .unwrap();
    project.sequences[0].main_track_id = "main".into();
    project.sequences[0].tracks[0].id = "main".into();
    project.assets.push(AssetV4 {
        id: "image".into(),
        name: "Picture".into(),
        kind: AssetKind::Image,
        version: AssetVersion {
            id: format!("av_{}", "a".repeat(64)),
            content_hash: "a".repeat(64),
            size: 16,
        },
        duration: None,
        origin: ResourceOriginV4::LegacyLocator {
            uri: "/private/never-publish.png".into(),
        },
        metadata: MediaMetadataV4 {
            mime_type: Some("image/png".into()),
            intrinsic_size: Some(CanvasSizeV4 {
                width: 1,
                height: 1,
            }),
            frame_rate: None,
            rotation_degrees: None,
            audio: AudioPresenceV4::Absent {},
        },
        license: None,
    });
    project.sequences[0].tracks[0]
        .elements
        .push(TimelineElementV4 {
            id: "element".into(),
            name: "Picture".into(),
            timeline_range: TimeRange {
                start: t(0, 1),
                duration: t(2, 1),
            },
            source_span: ElementSourceSpanV4::ClassicTrimmed {
                trim_start: t(0, 1),
                trim_end: t(0, 1),
                source_duration: None,
            },
            enabled: true,
            content: ElementContentV4::Image {
                asset_id: "image".into(),
            },
            channels: vec![],
            visual: Some(VisualStyleV4 {
                profile: VisualProfileV4::ClassicCanvasV31,
                transform: TransformV4 {
                    position_x_px: 0.0,
                    position_y_px: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation_degrees: 0.0,
                },
                opacity: 1.0,
                blend_mode: BlendModeV4::Normal,
                effects: vec![],
                masks: vec![],
            }),
        });
    PreparedV4VideoMp4V1 {
        snapshot: ProjectSnapshotV4 {
            project,
            revision: "fixed-r1".into(),
        },
        sequence_id: "scene".into(),
        asset_lock: AssetLockManifest {
            version: 1,
            revision: "fixed-r1".into(),
            sequence_id: "scene".into(),
            assets: vec![LockedAsset {
                asset_id: "image".into(),
                content_hash: "a".repeat(64),
                size: 16,
            }],
        },
        font_lock: vec![],
        authority: TrustedJobAuthorityV1::LocalOwner,
        processing_policy: policy(),
    }
}

#[test]
fn cfr_uses_exact_global_fps_and_keeps_original_master_duration() {
    let mut fixed = input();
    let SettingsProfileV4::ClassicGlobal { settings } = &mut fixed.snapshot.project.settings else {
        panic!()
    };
    settings.fps = FrameRate {
        numerator: 30000,
        denominator: 1001,
    };
    fixed.snapshot.project.sequences[0].tracks[0].elements[0]
        .timeline_range
        .duration = t(1001, 1000);
    let plan = validate_prepared(&fixed).unwrap();
    assert_eq!(plan.identity.frame_count, "30");
    assert_eq!(plan.identity.audio_frame_count, "48048");
    assert_eq!(
        RationalTime::try_from(&plan.identity.video_duration)
            .unwrap()
            .cmp_exact(RationalTime::try_from(&t(1001, 1000)).unwrap()),
        std::cmp::Ordering::Equal
    );
    assert_eq!(frame_time(&plan.identity, 29).unwrap(), t(29029, 30000));
    assert!(frame_time(&plan.identity, 30).is_err());
    fixed.snapshot.project.sequences[0].tracks[0].elements[0]
        .timeline_range
        .duration = t(1001001, 1000000);
    let plan = validate_prepared(&fixed).unwrap();
    assert_eq!(plan.identity.frame_count, "31");
    assert_eq!(plan.identity.audio_frame_count, "48049");
    assert_eq!(plan.identity.timeline_duration, t(1001001, 1000000));
}
#[test]
fn full_scene_media_and_font_locks_are_exact_and_disjoint() {
    let mut fixed = input();
    validate_prepared(&fixed).unwrap();
    fixed.font_lock.push(FontLockV2 {
        font_id: "image".into(),
        content_hash: "a".repeat(64),
        size: 16,
    });
    assert_eq!(
        validate_prepared(&fixed).err().unwrap().code,
        "FONT_LOCK_INVALID"
    );
    fixed.font_lock.clear();
    fixed.asset_lock.assets.clear();
    assert_eq!(
        validate_prepared(&fixed).err().unwrap().code,
        "ASSET_LOCK_INVALID"
    );
}
#[test]
fn whole_scene_admission_rejects_late_unsupported_blend_and_budget() {
    let mut fixed = input();
    let mut late = fixed.snapshot.project.sequences[0].tracks[0].elements[0].clone();
    late.id = "late".into();
    late.timeline_range.start = t(2, 1);
    late.visual.as_mut().unwrap().blend_mode = BlendModeV4::Multiply;
    fixed.snapshot.project.sequences[0].tracks[0]
        .elements
        .push(late);
    assert!(validate_prepared(&fixed).is_err());
    let mut fixed = input();
    fixed.processing_policy.max_frames = 59;
    assert_eq!(
        validate_prepared(&fixed).err().unwrap().code,
        "VIDEO_BUDGET_EXCEEDED"
    );
}
#[test]
fn fixed_request_binding_rejects_changed_revision_and_private_scope() {
    let fixed = input();
    let public = SubmitV4VideoMp4V1 {
        idempotency_key: "key".into(),
        project_id: "project".into(),
        revision: "fixed-r1".into(),
        sequence_id: "scene".into(),
    };
    let mut request = JobRequest {
        idempotency_key: "key".into(),
        kind: V4_VIDEO_MP4_KIND.into(),
        input: serde_json::to_value(&public).unwrap(),
    };
    validate_prepared_request(&request, &fixed).unwrap();
    request.input["revision"] = serde_json::json!("changed");
    assert_eq!(
        validate_prepared_request(&request, &fixed)
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
}

#[test]
fn stored_video_projection_validates_identity_codec_counts_hashes_and_private_artifact() {
    use opencut_job_contracts::project_v4_audio::*;
    use opencut_job_contracts::project_v4_frame::{
        V4FrameArtifactMetadataV1, V4FrameExecutionSummaryV1,
    };
    let fixed = input();
    let plan = validate_prepared(&fixed).unwrap();
    let digest = "c".repeat(64);
    let mut stored = StoredV4VideoMp4ResultV1 {
        public: V4VideoMp4ResultV1 {
            schema_version: 1,
            identity: plan.identity.clone(),
            asset_lock: fixed.asset_lock.clone(),
            font_lock: fixed.font_lock.clone(),
            render_input_sha256: render_input_sha256(&fixed.snapshot, &fixed.sequence_id).unwrap(),
            execution: V4VideoExecutionSummaryV1 {
                visual: V4FrameExecutionSummaryV1 {
                    host_sha256: digest.clone(),
                    browser_version: "test-browser".into(),
                    browser_sha256: digest.clone(),
                    module_sha256: digest.clone(),
                    shader_sha256: digest.clone(),
                    playwright_version: "1.63.0".into(),
                    mediabunny_version: "1.41.0".into(),
                    gpu_backend: "webgpu".into(),
                    gpu_device: "test-gpu".into(),
                    texture_format: "bgra8unorm".into(),
                    decoder_profile:
                        crate::project_v4_frame_jobs::CLASSIC_VISUAL_DECODER_PROFILE_V4.into(),
                },
                audio: V4AudioExecutionSummaryV1 {
                    host_sha256: digest.clone(),
                    browser_version: "test-browser".into(),
                    browser_sha256: digest.clone(),
                    module_sha256: digest.clone(),
                    playwright_version: "1.63.0".into(),
                    mediabunny_version: "1.41.0".into(),
                    soundtouch_version: "0.3.0".into(),
                    decoder_profiles: vec![],
                    pitch_profiles: vec![],
                    mastering_profile:
                        V4AudioMasteringProfileV1::ClassicExportWholeSceneWebAudioV31,
                    mastering_applied: false,
                },
                encoder: V4VideoEncoderSummaryV1 {
                    ffmpeg_sha256: digest.clone(),
                    ffprobe_sha256: digest.clone(),
                    video_codec: "h264".into(),
                    audio_codec: "aac".into(),
                    pixel_format: "yuv420p".into(),
                    crf: 18,
                    audio_bitrate: 192000,
                    observed_video_frames: "60".into(),
                    observed_video_duration_seconds: 2.0,
                    observed_audio_duration_seconds: 2.0,
                },
                frame_receipts_sha256: digest.clone(),
                frames_rendered: "60".into(),
                rgba_bytes: (60u64 * 64 * 48 * 4).to_string(),
            },
            artifact: V4FrameArtifactMetadataV1 {
                mime_type: "video/mp4".into(),
                size_bytes: "100".into(),
                content_hash: digest.clone(),
            },
        },
        artifact: ArtifactDescriptor {
            path: "/private/attempt/video.mp4".into(),
            mime_type: "video/mp4".into(),
            size: 100,
            content_hash: digest,
        },
    };
    validate_stored_result(&fixed, &stored).unwrap();
    let original = stored.clone();
    stored.public.identity.frame_count = "59".into();
    assert_eq!(
        validate_stored_result(&fixed, &stored).unwrap_err().code,
        "INVALID_STORED_RESULT"
    );
    stored = original.clone();
    stored.public.execution.encoder.observed_video_frames = "59".into();
    assert!(validate_stored_result(&fixed, &stored).is_err());
    stored = original.clone();
    stored.public.execution.encoder.ffprobe_sha256.clear();
    assert!(validate_stored_result(&fixed, &stored).is_err());
    stored = original.clone();
    stored.artifact.size += 1;
    assert!(validate_stored_result(&fixed, &stored).is_err());
    stored = original.clone();
    stored
        .public
        .execution
        .encoder
        .observed_video_duration_seconds = f64::NAN;
    assert!(validate_stored_result(&fixed, &stored).is_err());
    let public =
        serde_json::to_string(&validate_stored_result(&fixed, &original).unwrap()).unwrap();
    assert!(!public.contains("/private/"));
}

#[test]
fn lock_covers_media_entering_after_the_first_output_frame() {
    let mut fixed = input();
    let mut asset = fixed.snapshot.project.assets[0].clone();
    asset.id = "later-image".into();
    asset.version.id = format!("av_{}", "b".repeat(64));
    asset.version.content_hash = "b".repeat(64);
    fixed.snapshot.project.assets.push(asset);
    let mut element = fixed.snapshot.project.sequences[0].tracks[0].elements[0].clone();
    element.id = "later-element".into();
    element.timeline_range.start = t(2, 1);
    element.content = ElementContentV4::Image {
        asset_id: "later-image".into(),
    };
    fixed.snapshot.project.sequences[0].tracks[0]
        .elements
        .push(element);
    assert_eq!(
        validate_prepared(&fixed).err().unwrap().code,
        "ASSET_LOCK_INVALID"
    );
    fixed.asset_lock.assets.push(LockedAsset {
        asset_id: "later-image".into(),
        content_hash: "b".repeat(64),
        size: 16,
    });
    let plan = validate_prepared(&fixed).unwrap();
    assert_eq!(plan.asset_ids, vec!["image", "later-image"]);
    assert_eq!(plan.identity.frame_count, "120");
    let first = prepare_frame(
        &fixed.snapshot,
        "scene",
        &frame_time(&plan.identity, 0).unwrap(),
    )
    .unwrap();
    assert_eq!(first.media().resources().len(), 1);
    let later = prepare_frame(
        &fixed.snapshot,
        "scene",
        &frame_time(&plan.identity, 90).unwrap(),
    )
    .unwrap();
    assert_eq!(later.media().resources()[0].asset_id, "later-image");
}
