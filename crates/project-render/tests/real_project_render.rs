use opencut_contracts::*;
use opencut_media::{
    CancellationToken, Container, FfmpegBackend, FrameAlignment, StreamKind, VideoCodec,
};
use opencut_project_render::{
    ProjectRenderAdapter, ProjectRenderRequest, ResolvedProjectFrameRequest,
    ResolvedProjectRangeRequest,
};
use std::{collections::BTreeMap, path::Path, process::Command};

fn t(ticks: i64, timebase: u32) -> Time {
    Time {
        ticks: ticks.to_string(),
        timebase,
    }
}
fn range(start: i64, duration: i64) -> TimeRange {
    TimeRange {
        start: t(start, 10),
        duration: t(duration, 10),
    }
}
fn fixture(ffmpeg: &Path, output: &Path, color: &str, frequency: u32) {
    let status = Command::new(ffmpeg)
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("color=c={color}:s=160x90:r=10:d=1"))
        .args(["-f", "lavfi", "-i"])
        .arg(format!(
            "sine=frequency={frequency}:sample_rate=48000:duration=1"
        ))
        .args([
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(output)
        .status()
        .unwrap();
    assert!(status.success());
}
fn clip(
    id: &str,
    asset: &str,
    timeline: TimeRange,
    source: TimeRange,
    volume: Option<f64>,
) -> Clip {
    let mut params = BTreeMap::new();
    if let Some(v) = volume {
        params.insert("volume".into(), serde_json::json!(v));
    }
    Clip {
        id: id.into(),
        asset_id: Some(asset.into()),
        timeline_range: timeline,
        source_range: Some(source),
        params,
        channels: vec![],
    }
}
fn track(id: &str, kind: TrackKind, clips: Vec<Clip>, volume: Option<f64>) -> Track {
    let mut params = BTreeMap::new();
    if let Some(v) = volume {
        params.insert("volume".into(), serde_json::json!(v));
    }
    Track {
        id: id.into(),
        name: id.into(),
        kind,
        enabled: true,
        muted: false,
        locked: false,
        clips,
        params,
    }
}

fn direct_frame(
    backend: &FfmpegBackend,
    snapshot: ProjectSnapshot,
    requested_time: Time,
    output: &Path,
) -> Vec<u8> {
    let resolved = ProjectRenderAdapter::resolve_visual_assets(snapshot, "seq", |_| {
        unreachable!("legacy fixtures do not require immutable asset resolution")
    })
    .unwrap();
    let plan = ProjectRenderAdapter::plan_frame_resolved(&ResolvedProjectFrameRequest {
        resolved: &resolved,
        requested_time,
        alignment: FrameAlignment::Exact,
        output: output.to_path_buf(),
        overwrite: false,
    })
    .unwrap();
    backend
        .render_timeline_frame(&plan.media_request, &CancellationToken::new(), &mut |_| {})
        .unwrap();
    let decoded = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(output)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .unwrap();
    assert!(decoded.status.success());
    decoded.stdout
}

fn range_preview(
    backend: &FfmpegBackend,
    snapshot: ProjectSnapshot,
    requested_range: TimeRange,
    output: &Path,
) {
    let resolved = ProjectRenderAdapter::resolve_assets(snapshot, "seq", |_| {
        unreachable!("legacy fixtures do not require immutable asset resolution")
    })
    .unwrap();
    let plan = ProjectRenderAdapter::plan_range_resolved(&ResolvedProjectRangeRequest {
        resolved: &resolved,
        requested_range,
        alignment: FrameAlignment::Exact,
        output: output.to_path_buf(),
        overwrite: false,
    })
    .unwrap();
    backend
        .render_timeline_range(&plan.media_request, &CancellationToken::new(), &mut |_| {})
        .unwrap();
}

#[test]
fn range_preview_keeps_split_audio_and_video_on_the_same_absolute_window() {
    let Ok(backend) = FfmpegBackend::discover() else {
        eprintln!("skipping: ffmpeg unavailable");
        return;
    };
    let ffmpeg = if Path::new("/usr/local/bin/ffmpeg").is_file() {
        Path::new("/usr/local/bin/ffmpeg")
    } else {
        Path::new("ffmpeg")
    };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("av-source.mp4");
    assert!(
        Command::new(ffmpeg)
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg("testsrc2=s=160x90:r=10:d=2")
            .args([
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=523:sample_rate=48000:duration=2"
            ])
            .args([
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest"
            ])
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    let asset = Asset {
        id: "source-asset".into(),
        name: "source".into(),
        kind: AssetKind::Video,
        uri: source.to_string_lossy().into(),
        duration: Some(t(2, 1)),
        version: None,
        metadata: BTreeMap::new(),
    };
    let whole = clip(
        "whole-clip",
        "source-asset",
        TimeRange {
            start: t(0, 1),
            duration: t(2, 1),
        },
        TimeRange {
            start: t(0, 1),
            duration: t(2, 1),
        },
        None,
    );
    let left = clip(
        "left-clip",
        "source-asset",
        range(0, 10),
        range(0, 10),
        None,
    );
    let right = clip(
        "right-clip",
        "source-asset",
        range(10, 10),
        range(10, 10),
        None,
    );
    let make_snapshot = |clips| ProjectSnapshot {
        project: ProjectDocument {
            id: "range-project".into(),
            name: "range".into(),
            schema_version: 1,
            default_sequence_id: "seq".into(),
            sequences: vec![Sequence {
                id: "seq".into(),
                name: "seq".into(),
                width: 160,
                height: 90,
                fps: FrameRate {
                    numerator: 10,
                    denominator: 1,
                },
                tracks: vec![track("video-track", TrackKind::Video, clips, None)],
                caption_cues: vec![],
            }],
            assets: vec![asset.clone()],
        },
        revision: "range-revision".into(),
    };
    let window = TimeRange {
        start: t(8, 10),
        duration: t(4, 10),
    };
    let whole_output = dir.path().join("whole-window.mp4");
    let split_output = dir.path().join("split-window.mp4");
    range_preview(
        &backend,
        make_snapshot(vec![whole]),
        window.clone(),
        &whole_output,
    );
    range_preview(
        &backend,
        make_snapshot(vec![left, right]),
        window,
        &split_output,
    );
    let decode = |path: &Path, map: &str, format: &str, codec_args: &[&str]| {
        let mut command = Command::new(ffmpeg);
        command
            .args(["-v", "error", "-i"])
            .arg(path)
            .args(["-map", map]);
        command.args(codec_args).args(["-f", format, "-"]);
        let result = command.output().unwrap();
        assert!(result.status.success());
        result.stdout
    };
    let whole_rgb = decode(&whole_output, "0:v:0", "rawvideo", &["-pix_fmt", "rgb24"]);
    let split_rgb = decode(&split_output, "0:v:0", "rawvideo", &["-pix_fmt", "rgb24"]);
    assert_eq!(whole_rgb, split_rgb, "split changed window video timing");
    let whole_pcm = decode(&whole_output, "0:a:0", "s16le", &[]);
    let split_pcm = decode(&split_output, "0:a:0", "s16le", &[]);
    assert_eq!(whole_pcm.len(), split_pcm.len());
    let mean_delta = whole_pcm
        .chunks_exact(2)
        .zip(split_pcm.chunks_exact(2))
        .map(|(a, b)| {
            (i16::from_le_bytes([a[0], a[1]]) as i32 - i16::from_le_bytes([b[0], b[1]]) as i32)
                .unsigned_abs() as u64
        })
        .sum::<u64>() as f64
        / (whole_pcm.len() / 2).max(1) as f64;
    assert!(
        mean_delta < 300.0,
        "split shifted window audio: {mean_delta}"
    );
}

#[test]
fn direct_frames_preserve_animated_split_pixels_and_ignore_missing_audio() {
    let Ok(backend) = FfmpegBackend::discover() else {
        eprintln!("skipping: ffmpeg unavailable");
        return;
    };
    let ffmpeg = if Path::new("/usr/local/bin/ffmpeg").is_file() {
        Path::new("/usr/local/bin/ffmpeg")
    } else {
        Path::new("ffmpeg")
    };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("changing.mp4");
    assert!(
        Command::new(ffmpeg)
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=s=160x90:r=20:d=1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    let asset = Asset {
        id: "visual".into(),
        name: "visual".into(),
        kind: AssetKind::Video,
        uri: source.to_string_lossy().into(),
        duration: Some(t(1, 1)),
        version: None,
        metadata: BTreeMap::new(),
    };
    let missing_audio = Asset {
        id: "missing-audio".into(),
        name: "missing audio".into(),
        kind: AssetKind::Audio,
        uri: dir.path().join("absent.wav").to_string_lossy().into(),
        duration: Some(t(3, 1)),
        version: None,
        metadata: BTreeMap::new(),
    };
    let opacity = |id: &str, duration: Time, first: f64, last: f64| KeyframeChannel {
        parameter: "opacity".into(),
        keyframes: vec![
            Keyframe {
                id: format!("{id}-start"),
                time: t(0, 1),
                value: serde_json::json!(first),
                interpolation: KeyframeInterpolation::Linear,
            },
            Keyframe {
                id: format!("{id}-end"),
                time: duration,
                value: serde_json::json!(last),
                interpolation: KeyframeInterpolation::Linear,
            },
        ],
    };
    let mut whole = clip(
        "whole",
        "visual",
        TimeRange {
            start: t(0, 1),
            duration: t(2, 1),
        },
        TimeRange {
            start: t(0, 1),
            duration: t(1, 1),
        },
        None,
    );
    whole.channels = vec![opacity("whole", t(2, 1), 0.2, 1.0)];
    let mut left = clip("left", "visual", range(0, 10), range(0, 5), None);
    left.channels = vec![opacity("left", t(1, 1), 0.2, 0.6)];
    let mut right = clip("right", "visual", range(10, 10), range(5, 5), None);
    right.channels = vec![opacity("right", t(1, 1), 0.6, 1.0)];
    let audio = clip(
        "audio",
        "missing-audio",
        TimeRange {
            start: t(0, 1),
            duration: t(3, 1),
        },
        TimeRange {
            start: t(0, 1),
            duration: t(3, 1),
        },
        None,
    );
    let make_snapshot = |clips| ProjectSnapshot {
        project: ProjectDocument {
            id: "project".into(),
            name: "direct frame".into(),
            schema_version: 1,
            default_sequence_id: "seq".into(),
            sequences: vec![Sequence {
                id: "seq".into(),
                name: "seq".into(),
                width: 160,
                height: 90,
                fps: FrameRate {
                    numerator: 10,
                    denominator: 1,
                },
                tracks: vec![
                    track("video", TrackKind::Video, clips, None),
                    track("audio-track", TrackKind::Audio, vec![audio.clone()], None),
                ],
                caption_cues: vec![],
            }],
            assets: vec![asset.clone(), missing_audio.clone()],
        },
        revision: "revision".into(),
    };
    let unsplit = make_snapshot(vec![whole]);
    let split = make_snapshot(vec![left, right]);
    for (index, at) in [t(9, 10), t(1, 1), t(11, 10)].into_iter().enumerate() {
        let original = direct_frame(
            &backend,
            unsplit.clone(),
            at.clone(),
            &dir.path().join(format!("whole-{index}.png")),
        );
        let divided = direct_frame(
            &backend,
            split.clone(),
            at,
            &dir.path().join(format!("split-{index}.png")),
        );
        assert_eq!(original, divided, "split changed pixels at sample {index}");
    }

    let black = direct_frame(
        &backend,
        split,
        t(5, 2),
        &dir.path().join("audio-only-black.png"),
    );
    assert!(
        black.iter().all(|component| *component < 5),
        "audio-only interval was not black"
    );
}

#[test]
fn renders_fixed_snapshot_with_gap_audio_mix_retime_and_caption() {
    let Ok(backend) = FfmpegBackend::discover() else {
        eprintln!("skipping: ffmpeg unavailable");
        return;
    };
    let ffmpeg = if Path::new("/usr/local/bin/ffmpeg").is_file() {
        Path::new("/usr/local/bin/ffmpeg")
    } else {
        Path::new("ffmpeg")
    };
    let dir = tempfile::tempdir().unwrap();
    let red = dir.path().join("red.mp4");
    let blue = dir.path().join("blue.mp4");
    fixture(ffmpeg, &red, "red", 440);
    fixture(ffmpeg, &blue, "blue", 880);
    let assets = vec![
        Asset {
            id: "red".into(),
            name: "red".into(),
            kind: AssetKind::Video,
            uri: red.to_string_lossy().into(),
            duration: Some(t(10, 10)),
            version: None,
            metadata: BTreeMap::new(),
        },
        Asset {
            id: "blue".into(),
            name: "blue".into(),
            kind: AssetKind::Video,
            uri: format!("file://{}", blue.display()),
            duration: Some(t(10, 10)),
            version: None,
            metadata: BTreeMap::new(),
        },
        Asset {
            id: "red-audio".into(),
            name: "red audio".into(),
            kind: AssetKind::Audio,
            uri: red.to_string_lossy().into(),
            duration: Some(t(10, 10)),
            version: None,
            metadata: BTreeMap::new(),
        },
        Asset {
            id: "blue-audio".into(),
            name: "blue audio".into(),
            kind: AssetKind::Audio,
            uri: format!("file://{}", blue.display()),
            duration: Some(t(10, 10)),
            version: None,
            metadata: BTreeMap::new(),
        },
    ];
    let sequence = Sequence {
        id: "seq".into(),
        name: "Main".into(),
        width: 120,
        height: 120,
        fps: FrameRate {
            numerator: 10,
            denominator: 1,
        },
        tracks: vec![
            track(
                "video",
                TrackKind::Video,
                vec![
                    clip("v1", "red", range(0, 4), range(0, 4), None),
                    clip("v2", "blue", range(6, 4), range(0, 8), None),
                ],
                None,
            ),
            track(
                "audio1",
                TrackKind::Audio,
                vec![clip(
                    "a1",
                    "red-audio",
                    range(0, 10),
                    range(0, 10),
                    Some(0.5),
                )],
                Some(0.8),
            ),
            track(
                "audio2",
                TrackKind::Audio,
                vec![clip(
                    "a2",
                    "blue-audio",
                    range(2, 6),
                    range(0, 6),
                    Some(0.25),
                )],
                None,
            ),
        ],
        caption_cues: vec![CaptionCue {
            id: "caption".into(),
            timeline_range: range(2, 6),
            text: "Editable caption".into(),
            source_anchor: None,
            style: CaptionStyle {
                font_family: "Arial".into(),
                font_size: 18.0,
                color: "white".into(),
                background_color: Some("black@0.5".into()),
                font_policy: Default::default(),
                max_width: None,
                line_height: None,
                text_align: Default::default(),
            },
        }],
    };
    let snapshot = ProjectSnapshot {
        project: ProjectDocument {
            id: "project".into(),
            name: "Render test".into(),
            schema_version: 1,
            default_sequence_id: "seq".into(),
            sequences: vec![sequence],
            assets,
        },
        revision: "rev-fixed".into(),
    };
    let output = dir.path().join("project.mp4");
    let request = ProjectRenderRequest {
        snapshot,
        sequence_id: "seq".into(),
        output: output.clone(),
        container: Container::Mp4,
        video_codec: VideoCodec::H264,
        overwrite: false,
    };
    let plan = ProjectRenderAdapter::plan(&request).unwrap();
    assert_eq!(plan.revision, "rev-fixed");
    assert_eq!(plan.media_request.video_clips.len(), 2);
    assert_eq!(plan.media_request.audio_clips.len(), 4);
    assert_eq!(plan.media_request.captions.len(), 1);
    let result =
        ProjectRenderAdapter::execute(&plan, &backend, &CancellationToken::new(), &mut |_| {})
            .unwrap();
    assert!(result.verification.passed);
    assert_eq!(
        result
            .metadata
            .streams
            .iter()
            .filter(|s| s.codec_type == StreamKind::Video)
            .count(),
        1
    );
    let pcm = Command::new(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(&output)
        .args(["-map", "0:a:0", "-f", "s16le", "-"])
        .output()
        .unwrap();
    assert!(pcm.status.success());
    assert!(
        pcm.stdout
            .chunks_exact(2)
            .any(|sample| i16::from_le_bytes([sample[0], sample[1]]).unsigned_abs() > 10),
        "exported audio was silent"
    );
    assert_eq!(
        result
            .metadata
            .streams
            .iter()
            .filter(|s| s.codec_type == StreamKind::Audio)
            .count(),
        1
    );
    let raw = Command::new(ffmpeg)
        .args(["-v", "error", "-ss", "0.5", "-i"])
        .arg(&output)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .unwrap();
    assert!(raw.status.success());
    let mean = raw.stdout.iter().map(|v| *v as u64).sum::<u64>() as f64 / raw.stdout.len() as f64;
    assert!(mean < 15.0, "gap was not black: mean={mean}");
}

#[test]
fn compiles_supported_keyframes_with_shared_evaluator() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.mp4");
    std::fs::write(&source, b"placeholder").unwrap();
    let channel = KeyframeChannel {
        parameter: "volume".into(),
        keyframes: vec![Keyframe {
            id: "keyframe".into(),
            time: t(0, 1),
            value: serde_json::json!(1.0),
            interpolation: KeyframeInterpolation::Linear,
        }],
    };
    let mut c = clip("clip", "asset", range(0, 1), range(0, 1), None);
    c.channels.push(channel);
    let snapshot = ProjectSnapshot {
        project: ProjectDocument {
            id: "p".into(),
            name: "p".into(),
            schema_version: 1,
            default_sequence_id: "s".into(),
            sequences: vec![Sequence {
                id: "s".into(),
                name: "s".into(),
                width: 10,
                height: 10,
                fps: FrameRate {
                    numerator: 1,
                    denominator: 1,
                },
                tracks: vec![track("v", TrackKind::Video, vec![c], None)],
                caption_cues: vec![],
            }],
            assets: vec![Asset {
                id: "asset".into(),
                name: "a".into(),
                kind: AssetKind::Video,
                uri: source.to_string_lossy().into(),
                duration: Some(t(1, 1)),
                version: None,
                metadata: BTreeMap::new(),
            }],
        },
        revision: "r".into(),
    };
    let plan = ProjectRenderAdapter::plan(&ProjectRenderRequest {
        snapshot,
        sequence_id: "s".into(),
        output: dir.path().join("x.mp4"),
        container: Container::Mp4,
        video_codec: VideoCodec::H264,
        overwrite: false,
    })
    .unwrap();
    let curve = &plan.media_request.audio_clips[0].gain_curve;
    assert_eq!(curve.segments.len(), 1);
    assert_eq!(curve.segments[0].start_value, 1.0);
    assert_eq!(curve.segments[0].end_value, 1.0);
}

#[test]
fn composites_image_above_video_with_transform_and_opacity() {
    let Ok(backend) = FfmpegBackend::discover() else {
        return;
    };
    let ffmpeg = if Path::new("/usr/local/bin/ffmpeg").is_file() {
        Path::new("/usr/local/bin/ffmpeg")
    } else {
        Path::new("ffmpeg")
    };
    let dir = tempfile::tempdir().unwrap();
    let video = dir.path().join("red.mp4");
    fixture(ffmpeg, &video, "red", 440);
    let blue_video = dir.path().join("blue.mp4");
    fixture(ffmpeg, &blue_video, "blue", 660);
    let image = dir.path().join("green.png");
    assert!(
        Command::new(ffmpeg)
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=lime:s=20x10",
                "-frames:v",
                "1"
            ])
            .arg(&image)
            .status()
            .unwrap()
            .success()
    );
    let assets = vec![
        Asset {
            id: "video".into(),
            name: "video".into(),
            kind: AssetKind::Video,
            uri: video.to_string_lossy().into(),
            duration: Some(t(10, 10)),
            version: None,
            metadata: BTreeMap::new(),
        },
        Asset {
            id: "image".into(),
            name: "image".into(),
            kind: AssetKind::Image,
            uri: image.to_string_lossy().into(),
            duration: None,
            version: None,
            metadata: BTreeMap::new(),
        },
        Asset {
            id: "blue-video".into(),
            name: "blue video".into(),
            kind: AssetKind::Video,
            uri: blue_video.to_string_lossy().into(),
            duration: Some(t(10, 10)),
            version: None,
            metadata: BTreeMap::new(),
        },
    ];
    let bottom = track(
        "bottom",
        TrackKind::Video,
        vec![clip("v", "video", range(0, 10), range(0, 10), None)],
        None,
    );
    let mut middle_clip = clip("blue-v", "blue-video", range(0, 10), range(0, 10), None);
    middle_clip
        .params
        .insert("opacity".into(), serde_json::json!(1.0));
    middle_clip
        .params
        .insert("scale_x".into(), serde_json::json!(0.8));
    middle_clip
        .params
        .insert("scale_y".into(), serde_json::json!(0.8));
    let middle = track("middle", TrackKind::Video, vec![middle_clip], None);
    let mut params = BTreeMap::new();
    params.insert("opacity".into(), serde_json::json!(0.5));
    params.insert("position_x".into(), serde_json::json!(20));
    params.insert("scale_x".into(), serde_json::json!(0.4));
    params.insert("scale_y".into(), serde_json::json!(0.4));
    params.insert("rotation_degrees".into(), serde_json::json!(90));
    let overlay = Track {
        id: "top".into(),
        name: "top".into(),
        kind: TrackKind::Graphic,
        enabled: true,
        muted: false,
        locked: false,
        clips: vec![Clip {
            id: "image-clip".into(),
            asset_id: Some("image".into()),
            timeline_range: range(0, 10),
            source_range: None,
            params,
            channels: [
                ("opacity", 0.5, 0.5),
                ("position_x", 20.0, 20.0),
                ("position_y", 0.0, 0.0),
                ("scale_x", 0.4, 0.4),
                ("scale_y", 0.4, 0.4),
                ("rotation_degrees", 90.0, 90.0),
            ]
            .into_iter()
            .map(|(parameter, start, end)| KeyframeChannel {
                parameter: parameter.into(),
                keyframes: vec![
                    Keyframe {
                        id: format!("{parameter}-start"),
                        time: t(0, 10),
                        value: serde_json::json!(start),
                        interpolation: KeyframeInterpolation::Linear,
                    },
                    Keyframe {
                        id: format!("{parameter}-end"),
                        time: t(10, 10),
                        value: serde_json::json!(end),
                        interpolation: KeyframeInterpolation::Linear,
                    },
                ],
            })
            .collect(),
        }],
        params: BTreeMap::new(),
    };
    let sequence = Sequence {
        id: "s".into(),
        name: "s".into(),
        width: 100,
        height: 100,
        fps: FrameRate {
            numerator: 10,
            denominator: 1,
        },
        tracks: vec![bottom, middle, overlay],
        caption_cues: vec![],
    };
    let snapshot = ProjectSnapshot {
        project: ProjectDocument {
            id: "p".into(),
            name: "p".into(),
            schema_version: 1,
            default_sequence_id: "s".into(),
            sequences: vec![sequence],
            assets,
        },
        revision: "r".into(),
    };
    let output = dir.path().join("layers.mp4");
    let plan = ProjectRenderAdapter::plan(&ProjectRenderRequest {
        snapshot,
        sequence_id: "s".into(),
        output: output.clone(),
        container: Container::Mp4,
        video_codec: VideoCodec::H264,
        overwrite: false,
    })
    .unwrap();
    assert_eq!(plan.media_request.image_clips.len(), 1);
    ProjectRenderAdapter::execute(&plan, &backend, &CancellationToken::new(), &mut |_| {}).unwrap();
    let raw = Command::new(ffmpeg)
        .args(["-v", "error", "-ss", "0.2", "-i"])
        .arg(&output)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .unwrap()
        .stdout;
    let pixel = |x: usize, y: usize| {
        let i = (y * 100 + x) * 3;
        (raw[i], raw[i + 1], raw[i + 2])
    };
    let outside = pixel(5, 50);
    assert!(outside.0 > 180 && outside.1 < 60, "outside={outside:?}");
    let layered = pixel(40, 50);
    assert!(layered.2 > 120, "video layers={layered:?}");
    let mixed = pixel(70, 35);
    assert!(mixed.1 > 70 && mixed.2 > 70, "mixed={mixed:?}");
    let pcm = Command::new(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(&output)
        .args(["-map", "0:a:0", "-f", "s16le", "-"])
        .output()
        .unwrap()
        .stdout;
    assert!(
        pcm.chunks_exact(2)
            .any(|s| i16::from_le_bytes([s[0], s[1]]).unsigned_abs() > 10)
    );
}

#[test]
fn renders_clip_local_hold_linear_animation_with_retime_and_audio_gain() {
    let Ok(backend) = FfmpegBackend::discover() else {
        return;
    };
    let ffmpeg = if Path::new("/usr/local/bin/ffmpeg").is_file() {
        Path::new("/usr/local/bin/ffmpeg")
    } else {
        Path::new("ffmpeg")
    };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("animated-source.mp4");
    assert!(
        Command::new(ffmpeg)
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=red:s=160x90:r=2:d=1",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000:duration=1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest"
            ])
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    let channel = |parameter: &str, values: &[(i64, f64, KeyframeInterpolation)]| KeyframeChannel {
        parameter: parameter.into(),
        keyframes: values
            .iter()
            .enumerate()
            .map(|(i, (at, value, interpolation))| Keyframe {
                id: format!("{parameter}-{i}"),
                time: t(*at, 10),
                value: serde_json::json!(value),
                interpolation: interpolation.clone(),
            })
            .collect(),
    };
    let mut animated = clip("animated", "asset", range(2, 8), range(0, 4), None);
    animated.channels = vec![
        channel(
            "opacity",
            &[
                (0, 0.0, KeyframeInterpolation::Hold),
                (2, 1.0, KeyframeInterpolation::Linear),
                (8, 0.5, KeyframeInterpolation::Linear),
            ],
        ),
        channel(
            "scale_x",
            &[
                (0, 0.0, KeyframeInterpolation::Hold),
                (2, 0.5, KeyframeInterpolation::Linear),
                (8, 1.0, KeyframeInterpolation::Linear),
            ],
        ),
        channel(
            "scale_y",
            &[
                (0, 0.0, KeyframeInterpolation::Hold),
                (2, 0.5, KeyframeInterpolation::Linear),
                (8, 1.0, KeyframeInterpolation::Linear),
            ],
        ),
        channel(
            "position_x",
            &[
                (0, -25.0, KeyframeInterpolation::Linear),
                (8, 25.0, KeyframeInterpolation::Linear),
            ],
        ),
        channel(
            "position_y",
            &[
                (0, 0.0, KeyframeInterpolation::Linear),
                (8, 10.0, KeyframeInterpolation::Linear),
            ],
        ),
        channel(
            "rotation_degrees",
            &[
                (0, 0.0, KeyframeInterpolation::Linear),
                (8, 30.0, KeyframeInterpolation::Linear),
            ],
        ),
        channel(
            "volume",
            &[
                (0, 0.0, KeyframeInterpolation::Hold),
                (2, 0.0, KeyframeInterpolation::Linear),
                (8, 1.0, KeyframeInterpolation::Linear),
            ],
        ),
    ];
    let snapshot = ProjectSnapshot {
        project: ProjectDocument {
            id: "p".into(),
            name: "p".into(),
            schema_version: 1,
            default_sequence_id: "s".into(),
            sequences: vec![Sequence {
                id: "s".into(),
                name: "s".into(),
                width: 100,
                height: 100,
                fps: FrameRate {
                    numerator: 10,
                    denominator: 1,
                },
                tracks: vec![track("v", TrackKind::Video, vec![animated], Some(0.5))],
                caption_cues: vec![],
            }],
            assets: vec![Asset {
                id: "asset".into(),
                name: "asset".into(),
                kind: AssetKind::Video,
                uri: source.to_string_lossy().into(),
                duration: Some(t(1, 1)),
                version: None,
                metadata: BTreeMap::new(),
            }],
        },
        revision: "r".into(),
    };
    let output = dir.path().join("animated.mp4");
    let plan = ProjectRenderAdapter::plan(&ProjectRenderRequest {
        snapshot,
        sequence_id: "s".into(),
        output: output.clone(),
        container: Container::Mp4,
        video_codec: VideoCodec::H264,
        overwrite: false,
    })
    .unwrap();
    assert_eq!(
        plan.media_request.video_clips[0].curves.opacity.segments[0].start_value,
        0.0
    );
    assert_eq!(
        plan.media_request.video_clips[0].curves.opacity.segments[0].end_value,
        0.0
    );
    assert_eq!(
        plan.media_request.audio_clips[0].gain_curve.segments[1].start_value,
        0.0
    );
    ProjectRenderAdapter::execute(&plan, &backend, &CancellationToken::new(), &mut |_| {}).unwrap();
    let frame = |at: &str| {
        Command::new(ffmpeg)
            .args(["-v", "error", "-ss", at, "-i"])
            .arg(&output)
            .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .output()
            .unwrap()
            .stdout
    };
    let early = frame("0.1");
    let held_inside = frame("0.3");
    let middle = frame("0.5");
    let late = frame("0.8");
    assert!(
        early.iter().all(|v| *v < 20),
        "zero scale/opacity frame was visible"
    );
    assert!(
        held_inside.iter().all(|v| *v < 20),
        "clip-local hold zero was visible inside the clip"
    );
    assert!(middle.iter().any(|v| *v > 100));
    assert_ne!(
        middle, late,
        "linear transforms did not change rendered pixels"
    );
    let pcm_peak = |at: &str| {
        let bytes = Command::new(ffmpeg)
            .args(["-v", "error", "-i"])
            .arg(&output)
            .args(["-ss", at, "-t", "0.1", "-map", "0:a:0", "-f", "s16le", "-"])
            .output()
            .unwrap()
            .stdout;
        bytes
            .chunks_exact(2)
            .map(|s| i16::from_le_bytes([s[0], s[1]]).unsigned_abs())
            .max()
            .unwrap_or(0)
    };
    let muted_peak = pcm_peak("0.25");
    let audible_peak = pcm_peak("0.6");
    let louder_peak = pcm_peak("0.85");
    assert!(muted_peak < 50, "hold-muted audio was audible");
    assert!(audible_peak > 100, "animated gain did not restore audio");
    assert!(
        louder_peak > audible_peak,
        "linear gain did not increase PCM amplitude"
    );
}

#[test]
fn independently_renders_visual_parameter_curves() {
    let Ok(backend) = FfmpegBackend::discover() else {
        return;
    };
    let ffmpeg = if Path::new("/usr/local/bin/ffmpeg").is_file() {
        Path::new("/usr/local/bin/ffmpeg")
    } else {
        Path::new("ffmpeg")
    };
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("rectangle.png");
    assert!(
        Command::new(ffmpeg)
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=lime:s=40x10",
                "-frames:v",
                "1"
            ])
            .arg(&image)
            .status()
            .unwrap()
            .success()
    );
    let make_channel =
        |parameter: &str, a: f64, b: f64, interpolation: KeyframeInterpolation| KeyframeChannel {
            parameter: parameter.into(),
            keyframes: vec![
                Keyframe {
                    id: format!("{parameter}-a"),
                    time: t(0, 10),
                    value: serde_json::json!(a),
                    interpolation,
                },
                Keyframe {
                    id: format!("{parameter}-b"),
                    time: t(10, 10),
                    value: serde_json::json!(b),
                    interpolation: KeyframeInterpolation::Linear,
                },
            ],
        };
    let graphic = |id: &str, start: i64, channels: Vec<KeyframeChannel>| Clip {
        id: id.into(),
        asset_id: Some("image".into()),
        timeline_range: range(start, 10),
        source_range: None,
        params: BTreeMap::new(),
        channels,
    };
    let clips = vec![
        graphic(
            "opacity",
            0,
            vec![make_channel(
                "opacity",
                0.2,
                1.0,
                KeyframeInterpolation::Linear,
            )],
        ),
        graphic(
            "scale",
            10,
            vec![
                make_channel("scale_x", 0.25, 0.75, KeyframeInterpolation::Linear),
                make_channel("scale_y", 0.25, 0.75, KeyframeInterpolation::Linear),
            ],
        ),
        graphic(
            "rotation",
            20,
            vec![KeyframeChannel {
                parameter: "rotation_degrees".into(),
                keyframes: vec![
                    Keyframe {
                        id: "r0".into(),
                        time: t(0, 10),
                        value: serde_json::json!(0.0),
                        interpolation: KeyframeInterpolation::Hold,
                    },
                    Keyframe {
                        id: "r90".into(),
                        time: t(5, 10),
                        value: serde_json::json!(90.0),
                        interpolation: KeyframeInterpolation::Hold,
                    },
                ],
            }],
        ),
        graphic(
            "position-y",
            30,
            vec![make_channel(
                "position_y",
                -20.0,
                20.0,
                KeyframeInterpolation::Linear,
            )],
        ),
        graphic(
            "split-left",
            40,
            vec![make_channel(
                "position_x",
                -20.0,
                0.0,
                KeyframeInterpolation::Linear,
            )],
        ),
        graphic(
            "split-right",
            50,
            vec![make_channel(
                "position_x",
                0.0,
                20.0,
                KeyframeInterpolation::Linear,
            )],
        ),
    ];
    let snapshot = ProjectSnapshot {
        project: ProjectDocument {
            id: "p".into(),
            name: "p".into(),
            schema_version: 1,
            default_sequence_id: "s".into(),
            sequences: vec![Sequence {
                id: "s".into(),
                name: "s".into(),
                width: 100,
                height: 100,
                fps: FrameRate {
                    numerator: 10,
                    denominator: 1,
                },
                tracks: vec![track("g", TrackKind::Graphic, clips, None)],
                caption_cues: vec![],
            }],
            assets: vec![Asset {
                id: "image".into(),
                name: "image".into(),
                kind: AssetKind::Image,
                uri: image.to_string_lossy().into(),
                duration: None,
                version: None,
                metadata: BTreeMap::new(),
            }],
        },
        revision: "r".into(),
    };
    let output = dir.path().join("visual-curves.mp4");
    let plan = ProjectRenderAdapter::plan(&ProjectRenderRequest {
        snapshot,
        sequence_id: "s".into(),
        output: output.clone(),
        container: Container::Mp4,
        video_codec: VideoCodec::H264,
        overwrite: false,
    })
    .unwrap();
    ProjectRenderAdapter::execute(&plan, &backend, &CancellationToken::new(), &mut |_| {}).unwrap();
    let frame = |at: &str| {
        Command::new(ffmpeg)
            .args(["-v", "error", "-ss", at, "-i"])
            .arg(&output)
            .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .output()
            .unwrap()
            .stdout
    };
    let stats = |raw: Vec<u8>| {
        let mut points = Vec::new();
        let mut green_sum = 0u64;
        for (i, p) in raw.chunks_exact(3).enumerate() {
            green_sum += p[1] as u64;
            if p[1] as i16 > p[0] as i16 + 30 && p[1] as i16 > p[2] as i16 + 30 {
                points.push((i % 100, i / 100));
            }
        }
        let min_x = points.iter().map(|p| p.0).min().unwrap();
        let max_x = points.iter().map(|p| p.0).max().unwrap();
        let min_y = points.iter().map(|p| p.1).min().unwrap();
        let max_y = points.iter().map(|p| p.1).max().unwrap();
        (
            green_sum,
            max_x - min_x + 1,
            max_y - min_y + 1,
            (min_y + max_y) as f64 / 2.0,
            (min_x + max_x) as f64 / 2.0,
        )
    };
    let opacity_low = stats(frame("0.1"));
    let opacity_high = stats(frame("0.9"));
    assert!(
        opacity_high.0 > opacity_low.0 * 2,
        "opacity intensity did not rise"
    );
    let scale_small = stats(frame("1.1"));
    let scale_large = stats(frame("1.9"));
    assert!(
        scale_large.1 > scale_small.1 && scale_large.2 > scale_small.2,
        "scale bbox did not grow"
    );
    let rotation_0 = stats(frame("2.2"));
    let rotation_90 = stats(frame("2.7"));
    assert!(
        rotation_0.1 > rotation_0.2 * 2 && rotation_90.2 > rotation_90.1 * 2,
        "0/90 rotation bbox is wrong"
    );
    let position_top = stats(frame("3.1"));
    let position_bottom = stats(frame("3.9"));
    assert!(
        position_bottom.3 > position_top.3 + 20.0,
        "position_y center did not move down"
    );
    let before_split = stats(frame("4.9"));
    let at_split = stats(frame("5.0"));
    let after_split = stats(frame("5.1"));
    assert!((at_split.4 - before_split.4).abs() <= 3.0);
    assert!((after_split.4 - at_split.4).abs() <= 3.0);
    assert!(before_split.4 < at_split.4 && at_split.4 < after_split.4);
}
