use opencut_media::*;
use std::{path::Path, process::Command};

fn backend() -> Option<FfmpegBackend> {
    FfmpegBackend::discover().ok()
}

#[test]
fn media_time_json_and_schema_use_decimal_string_ticks() {
    let value = mt(9_007_199_254_740_993, 30_000);
    let json = serde_json::to_value(value).unwrap();
    assert_eq!(json["ticks"], "9007199254740993");
    let schema = schemars::schema_for!(MediaTime);
    let schema_json = serde_json::to_value(schema).unwrap();
    assert_eq!(schema_json["properties"]["ticks"]["type"], "string");
}
fn mt(ticks: i64, timebase: u32) -> MediaTime {
    MediaTime::new(ticks, timebase).unwrap()
}

fn make_fixture(ffmpeg: &Path, output: &Path, color: &str, frequency: u32) {
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
            "-bf",
            "2",
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

fn make_offset_vfr_fixture(ffmpeg: &Path, output: &Path) {
    let status = Command::new(ffmpeg)
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x90:rate=10:duration=0.5",
            "-vf",
            "setpts=PTS+if(gte(N\\,2)\\,0.15/TB\\,0)+5/TB",
            "-fps_mode",
            "vfr",
            "-c:v",
            "libx264",
            "-bf",
            "2",
            "-pix_fmt",
            "yuv420p",
            "-output_ts_offset",
            "3",
        ])
        .arg(output)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn directly_renders_aligned_timeline_png_without_audio_dependencies() {
    let Some(backend) = backend() else {
        eprintln!("skipping: ffmpeg/ffprobe unavailable");
        return;
    };
    let ffmpeg = if Path::new("/usr/local/bin/ffmpeg").is_file() {
        Path::new("/usr/local/bin/ffmpeg")
    } else {
        Path::new("ffmpeg")
    };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("red.mp4");
    make_fixture(ffmpeg, &source, "red", 440);
    let output = dir.path().join("direct-frame.png");
    let timeline = TimelineExportRequest {
        output: output.clone(),
        container: Container::Mp4,
        video: VideoOutput {
            width: 160,
            height: 90,
            frame_rate: FrameRate {
                numerator: 10,
                denominator: 1,
            },
            aspect_mode: AspectMode::Fit,
            codec: VideoCodec::H264,
            crf: None,
        },
        duration: mt(10, 10),
        video_clips: vec![TimelineVideoClip {
            input: source,
            source_start: mt(0, 10),
            source_duration: mt(10, 10),
            timeline_start: mt(0, 10),
            timeline_duration: mt(10, 10),
            stream: None,
            opacity: 1.0,
            position_x: 0.0,
            position_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
            z_index: 0,
            curves: TimelineVisualCurves::default(),
        }],
        image_clips: vec![],
        audio_clips: vec![TimelineAudioClip {
            input: dir.path().join("missing-audio.wav"),
            source_start: mt(0, 1),
            source_duration: mt(1, 1),
            timeline_start: mt(0, 1),
            timeline_duration: mt(1, 1),
            gain: 1.0,
            optional: false,
            stream: None,
            gain_curve: EvaluatedCurve::default(),
        }],
        captions: vec![TimelineCaption {
            start: mt(0, 1),
            duration: mt(1, 1),
            text: "Frame".into(),
            font_family: "Arial".into(),
            font_size: 18.0,
            color: "#ffffff".into(),
            background_color: None,
            font_policy: FontResolutionPolicy::Strict {},
            max_width: None,
            line_height: None,
            text_align: TextAlign::Center,
        }],
        overwrite: false,
    };
    let mut progress = Vec::new();
    let rendered = backend
        .render_timeline_frame(
            &TimelineFrameRenderRequest {
                timeline: timeline.clone(),
                sequence_time: mt(5, 10),
            },
            &CancellationToken::new(),
            &mut |update| progress.push(update),
        )
        .unwrap();
    assert_eq!(rendered.output, output);
    assert_eq!(rendered.actual_sequence_time, mt(5, 10));
    assert_eq!((rendered.width, rendered.height), (160, 90));
    assert_eq!(rendered.text_layouts.len(), 1);
    let pixels = image::open(&rendered.output).unwrap().to_rgb8();
    let pixel = pixels.get_pixel(4, 4).0;
    assert!(
        pixel[0] > 180 && pixel[1] < 60 && pixel[2] < 60,
        "{pixel:?}"
    );
    assert!(
        progress
            .iter()
            .any(|update| update.stage == ProgressStage::Encoding)
    );
    assert!(!std::fs::read_dir(dir.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".caption-")
    }));

    for (name, sequence_time) in [("off-grid.png", mt(1, 20)), ("at-end.png", mt(1, 1))] {
        let mut invalid = timeline.clone();
        invalid.output = dir.path().join(name);
        assert!(matches!(
            backend.render_timeline_frame(
                &TimelineFrameRenderRequest {
                    timeline: invalid.clone(),
                    sequence_time,
                },
                &CancellationToken::new(),
                &mut |_| {},
            ),
            Err(MediaError::InvalidRequest(_))
        ));
        assert!(!invalid.output.exists());
    }

    let mut cancelled = timeline;
    cancelled.output = dir.path().join("cancelled.png");
    let token = CancellationToken::new();
    token.cancel();
    assert!(matches!(
        backend.render_timeline_frame(
            &TimelineFrameRenderRequest {
                timeline: cancelled.clone(),
                sequence_time: mt(0, 10),
            },
            &token,
            &mut |_| {},
        ),
        Err(MediaError::Cancelled)
    ));
    assert!(!cancelled.output.exists());
}

#[test]
fn renders_timeline_range_with_absolute_visual_and_audio_time() {
    let Some(backend) = backend() else {
        eprintln!("skipping: ffmpeg/ffprobe unavailable");
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
    make_fixture(ffmpeg, &red, "red", 440);
    make_fixture(ffmpeg, &blue, "blue", 880);
    let output = dir.path().join("range.mp4");
    let clip = |input: std::path::PathBuf, start: i64| TimelineVideoClip {
        input,
        source_start: mt(0, 10),
        source_duration: mt(10, 10),
        timeline_start: mt(start, 10),
        timeline_duration: mt(10, 10),
        stream: None,
        opacity: 1.0,
        position_x: 0.0,
        position_y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        rotation_degrees: 0.0,
        z_index: 0,
        curves: TimelineVisualCurves::default(),
    };
    let timeline = TimelineExportRequest {
        output: output.clone(),
        container: Container::Mp4,
        video: VideoOutput {
            width: 160,
            height: 90,
            frame_rate: FrameRate {
                numerator: 10,
                denominator: 1,
            },
            aspect_mode: AspectMode::Fit,
            codec: VideoCodec::H264,
            crf: None,
        },
        duration: mt(20, 10),
        video_clips: vec![clip(red.clone(), 0), clip(blue, 10)],
        image_clips: vec![],
        audio_clips: vec![TimelineAudioClip {
            input: red,
            source_start: mt(0, 10),
            source_duration: mt(10, 10),
            timeline_start: mt(0, 10),
            // The source second is stretched to two timeline seconds.
            timeline_duration: mt(20, 10),
            gain: 1.0,
            optional: false,
            stream: None,
            gain_curve: EvaluatedCurve {
                segments: vec![EvaluatedSegment {
                    start: mt(0, 10),
                    end: mt(20, 10),
                    start_value: 0.2,
                    end_value: 1.0,
                    cubic_bezier: None,
                }],
            },
        }],
        captions: vec![TimelineCaption {
            start: mt(5, 10),
            duration: mt(4, 10),
            text: "RANGE".into(),
            font_family: "Arial".into(),
            font_size: 28.0,
            color: "#ffffff".into(),
            background_color: None,
            font_policy: FontResolutionPolicy::Strict {},
            max_width: None,
            line_height: None,
            text_align: TextAlign::Center,
        }],
        overwrite: false,
    };
    let mut progress = Vec::new();
    let result = backend
        .render_timeline_range(
            &TimelineRangeRenderRequest {
                timeline: timeline.clone(),
                sequence_start: mt(5, 10),
                sequence_duration: mt(10, 10),
            },
            &CancellationToken::new(),
            &mut |update| progress.push(update),
        )
        .unwrap();
    assert!(result.verification.passed);
    assert_eq!(result.text_layouts.len(), 1);
    assert!((result.metadata.duration.unwrap().seconds() - 1.0).abs() < 0.12);
    assert!(
        progress
            .iter()
            .any(|update| update.stage == ProgressStage::Encoding)
    );
    assert!(
        progress
            .iter()
            .any(|update| update.stage == ProgressStage::Verifying)
    );

    for (name, time, expected) in [
        ("range-start.png", mt(0, 10), [200, 0, 0]),
        ("range-after-boundary.png", mt(6, 10), [0, 0, 200]),
    ] {
        let frame = dir.path().join(name);
        backend
            .extract_frame(
                &FrameExtractRequest {
                    input: output.clone(),
                    output: frame.clone(),
                    requested_time: time,
                    video_stream: None,
                    alignment: FrameAlignment::Exact,
                    overwrite: false,
                },
                &CancellationToken::new(),
                &mut |_| {},
            )
            .unwrap();
        let pixel = image::open(frame).unwrap().to_rgb8().get_pixel(4, 4).0;
        assert!(
            pixel[0] >= expected[0] && pixel[1] >= expected[1] && pixel[2] >= expected[2],
            "unexpected range pixel at {time:?}: {pixel:?}"
        );
    }

    let white_pixel_count = |time: MediaTime, name: &str| {
        let frame = dir.path().join(name);
        backend
            .extract_frame(
                &FrameExtractRequest {
                    input: output.clone(),
                    output: frame.clone(),
                    requested_time: time,
                    video_stream: None,
                    alignment: FrameAlignment::Exact,
                    overwrite: false,
                },
                &CancellationToken::new(),
                &mut |_| {},
            )
            .unwrap();
        image::open(frame)
            .unwrap()
            .to_rgb8()
            .pixels()
            .filter(|pixel| pixel[0] > 220 && pixel[1] > 220 && pixel[2] > 220)
            .count()
    };
    let caption_inside = white_pixel_count(mt(3, 10), "caption-inside.png");
    let caption_at_end = white_pixel_count(mt(4, 10), "caption-at-end.png");
    assert!(
        caption_inside > 20,
        "caption was not visible inside its range"
    );
    assert_eq!(
        caption_at_end, 0,
        "caption must be absent at its half-open absolute end"
    );

    let pcm = Command::new(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(&output)
        .args([
            "-map", "0:a:0", "-f", "s16le", "-ac", "1", "-ar", "48000", "-",
        ])
        .output()
        .unwrap();
    assert!(pcm.status.success());
    let samples = pcm
        .stdout
        .chunks_exact(2)
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]) as f64)
        .collect::<Vec<_>>();
    let rms = |slice: &[f64]| {
        (slice.iter().map(|sample| sample * sample).sum::<f64>() / slice.len() as f64).sqrt()
    };
    let quarter = samples.len() / 4;
    let early_rms = rms(&samples[..quarter]);
    let late_rms = rms(&samples[samples.len() - quarter..]);
    assert!(
        late_rms > early_rms * 1.25,
        "absolute-time linear gain must rise across the selected range: {early_rms} -> {late_rms}"
    );

    let invalid = |start, duration| TimelineRangeRenderRequest {
        timeline: TimelineExportRequest {
            output: dir.path().join(format!("invalid-{start}-{duration}.mp4")),
            ..timeline.clone()
        },
        sequence_start: mt(start, 20),
        sequence_duration: mt(duration, 20),
    };
    for request in [invalid(1, 10), invalid(10, 0), invalid(30, 20)] {
        assert!(matches!(
            backend.render_timeline_range(&request, &CancellationToken::new(), &mut |_| {}),
            Err(MediaError::InvalidRequest(_))
        ));
        assert!(!request.timeline.output.exists());
    }

    let cancelled_output = dir.path().join("cancelled-range.mp4");
    let cancel = CancellationToken::new();
    let callback_token = cancel.clone();
    let error = backend
        .render_timeline_range(
            &TimelineRangeRenderRequest {
                timeline: TimelineExportRequest {
                    output: cancelled_output.clone(),
                    ..timeline
                },
                sequence_start: mt(0, 10),
                sequence_duration: mt(10, 10),
            },
            &cancel,
            &mut |update| {
                if update.stage == ProgressStage::Encoding {
                    callback_token.cancel();
                }
            },
        )
        .unwrap_err();
    assert!(matches!(error, MediaError::Cancelled));
    assert!(!cancelled_output.exists());
    assert!(!std::fs::read_dir(dir.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("cancelled-range.mp4.opencut")
    }));
}

#[test]
fn ntsc_range_has_exact_half_open_frames_and_absolute_audio_window() {
    let Some(backend) = backend() else {
        eprintln!("skipping: ffmpeg/ffprobe unavailable");
        return;
    };
    let ffmpeg = if Path::new("/usr/local/bin/ffmpeg").is_file() {
        Path::new("/usr/local/bin/ffmpeg")
    } else {
        Path::new("ffmpeg")
    };
    let ffprobe = if Path::new("/usr/local/bin/ffprobe").is_file() {
        Path::new("/usr/local/bin/ffprobe")
    } else {
        Path::new("ffprobe")
    };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("ntsc-source.mp4");
    assert!(
        Command::new(ffmpeg)
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg("testsrc2=s=160x90:r=30000/1001:d=0.5")
            .args(["-f", "lavfi", "-i"])
            .arg("sine=frequency=700:sample_rate=48000:duration=0.5")
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
    let output = dir.path().join("ntsc-three-frames.mp4");
    let frame = |count: i64| mt(count * 1001, 30_000);
    let timeline = TimelineExportRequest {
        output: output.clone(),
        container: Container::Mp4,
        video: VideoOutput {
            width: 160,
            height: 90,
            frame_rate: FrameRate {
                numerator: 30_000,
                denominator: 1_001,
            },
            aspect_mode: AspectMode::Fit,
            codec: VideoCodec::H264,
            crf: None,
        },
        duration: frame(10),
        video_clips: vec![TimelineVideoClip {
            input: source.clone(),
            source_start: mt(0, 1),
            source_duration: frame(10),
            timeline_start: mt(0, 1),
            timeline_duration: frame(10),
            stream: None,
            opacity: 1.0,
            position_x: 0.0,
            position_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
            z_index: 0,
            curves: TimelineVisualCurves::default(),
        }],
        image_clips: vec![],
        audio_clips: vec![TimelineAudioClip {
            input: source,
            source_start: mt(0, 1),
            source_duration: frame(10),
            timeline_start: mt(0, 1),
            timeline_duration: frame(10),
            gain: 1.0,
            optional: false,
            stream: None,
            gain_curve: EvaluatedCurve {
                segments: vec![
                    EvaluatedSegment {
                        start: frame(0),
                        end: frame(3),
                        start_value: 0.0,
                        end_value: 0.0,
                        cubic_bezier: None,
                    },
                    EvaluatedSegment {
                        start: frame(3),
                        end: frame(10),
                        start_value: 1.0,
                        end_value: 1.0,
                        cubic_bezier: None,
                    },
                ],
            },
        }],
        captions: vec![],
        overwrite: false,
    };
    backend
        .render_timeline_range(
            &TimelineRangeRenderRequest {
                timeline,
                sequence_start: frame(2),
                sequence_duration: frame(3),
            },
            &CancellationToken::new(),
            &mut |_| {},
        )
        .unwrap();

    let probe = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=time_base:frame=best_effort_timestamp",
            "-of",
            "json",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(probe.status.success());
    let probe: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
    assert_eq!(probe["streams"][0]["time_base"], "1/30000");
    let pts = probe["frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|frame| frame["best_effort_timestamp"].as_i64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        pts,
        [0, 1001, 2002],
        "range must be exactly three half-open NTSC frames"
    );

    let pcm = Command::new(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(&output)
        .args([
            "-map", "0:a:0", "-f", "s16le", "-ac", "1", "-ar", "48000", "-",
        ])
        .output()
        .unwrap();
    assert!(pcm.status.success());
    let samples = pcm
        .stdout
        .chunks_exact(2)
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]).unsigned_abs())
        .collect::<Vec<_>>();
    assert!(samples.len() > 3_000);
    let early_peak = samples[..1_000].iter().copied().max().unwrap();
    let late_peak = samples[samples.len() - 1_000..]
        .iter()
        .copied()
        .max()
        .unwrap();
    assert!(
        early_peak < 50,
        "absolute muted window leaked: {early_peak}"
    );
    assert!(
        late_peak > 100,
        "absolute audible window was not selected: {late_peak}"
    );
}

#[test]
fn cubic_bezier_keyframe_curve_changes_real_rendered_frame_pixels() {
    let Some(backend) = backend() else {
        eprintln!("skipping: ffmpeg/ffprobe unavailable");
        return;
    };
    let ffmpeg = if Path::new("/usr/local/bin/ffmpeg").is_file() {
        Path::new("/usr/local/bin/ffmpeg")
    } else {
        Path::new("ffmpeg")
    };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("white-source.mp4");
    make_fixture(ffmpeg, &source, "white", 440);
    let frame = |path: &Path, easing| {
        let output = dir.path().join(path);
        backend
            .render_timeline(
                &TimelineExportRequest {
                    output: output.clone(),
                    container: Container::Mp4,
                    video: VideoOutput {
                        width: 160,
                        height: 90,
                        frame_rate: FrameRate {
                            numerator: 10,
                            denominator: 1,
                        },
                        aspect_mode: AspectMode::Fit,
                        codec: VideoCodec::H264,
                        crf: None,
                    },
                    duration: mt(1, 1),
                    video_clips: vec![TimelineVideoClip {
                        input: source.clone(),
                        source_start: mt(0, 1),
                        source_duration: mt(1, 1),
                        timeline_start: mt(0, 1),
                        timeline_duration: mt(1, 1),
                        stream: None,
                        opacity: 1.0,
                        position_x: 0.0,
                        position_y: 0.0,
                        scale_x: 1.0,
                        scale_y: 1.0,
                        rotation_degrees: 0.0,
                        z_index: 0,
                        curves: TimelineVisualCurves {
                            opacity: EvaluatedCurve {
                                segments: vec![EvaluatedSegment {
                                    start: mt(0, 1),
                                    end: mt(1, 1),
                                    start_value: 0.0,
                                    end_value: 1.0,
                                    cubic_bezier: easing,
                                }],
                            },
                            ..Default::default()
                        },
                    }],
                    image_clips: vec![],
                    audio_clips: vec![],
                    captions: vec![],
                    overwrite: false,
                },
                &CancellationToken::new(),
                &mut |_| {},
            )
            .unwrap();
        let decoded = Command::new(ffmpeg)
            .args(["-v", "error", "-ss", "0.5", "-i"])
            .arg(output)
            .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .output()
            .unwrap();
        assert!(decoded.status.success());
        let center = (45 * 160 + 80) * 3;
        decoded.stdout[center..center + 3]
            .iter()
            .map(|value| u16::from(*value))
            .sum::<u16>()
            / 3
    };
    let linear = frame(Path::new("linear.mp4"), None);
    let cubic = frame(
        Path::new("cubic.mp4"),
        Some(opencut_media_contracts::composition::CubicBezierCurve {
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 0.0,
        }),
    );
    assert!(linear > 35, "linear mid-frame should be visible: {linear}");
    assert!(
        cubic + 20 < linear,
        "cubic easing should change pixels: cubic={cubic}, linear={linear}"
    );
}

#[test]
fn probes_extracts_exports_and_verifies_real_media() {
    let Some(backend) = backend() else {
        eprintln!("skipping: ffmpeg/ffprobe unavailable");
        return;
    };
    let ffmpeg = Path::new("/usr/local/bin/ffmpeg");
    let ffmpeg = if ffmpeg.is_file() {
        ffmpeg
    } else {
        Path::new("ffmpeg")
    };
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.mp4");
    let second = dir.path().join("second.mp4");
    make_fixture(ffmpeg, &first, "red", 440);
    make_fixture(ffmpeg, &second, "blue", 880);

    let metadata = backend
        .probe(&ProbeRequest {
            input: first.clone(),
        })
        .unwrap();
    assert_eq!(
        metadata
            .streams
            .iter()
            .filter(|s| s.codec_type == StreamKind::Video)
            .count(),
        1
    );
    assert_eq!(
        metadata
            .streams
            .iter()
            .filter(|s| s.codec_type == StreamKind::Audio)
            .count(),
        1
    );
    assert_eq!(
        metadata
            .streams
            .iter()
            .find(|s| s.codec_type == StreamKind::Video)
            .unwrap()
            .width,
        Some(160)
    );

    let first_frame = backend
        .extract_frame(
            &FrameExtractRequest {
                input: first.clone(),
                output: dir.path().join("first-frame.png"),
                requested_time: mt(0, 10),
                video_stream: None,
                alignment: FrameAlignment::Exact,
                overwrite: false,
            },
            &CancellationToken::new(),
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(first_frame.source_pts, 0);
    assert_eq!(first_frame.actual_time.ticks, 0);

    let frame_path = dir.path().join("frame.png");
    let frame = backend
        .extract_frame(
            &FrameExtractRequest {
                input: first.clone(),
                output: frame_path.clone(),
                requested_time: mt(25, 100),
                video_stream: None,
                alignment: FrameAlignment::Nearest,
                overwrite: false,
            },
            &CancellationToken::new(),
            &mut |_| {},
        )
        .unwrap();
    assert!(frame_path.is_file());
    assert!((frame.actual_time.seconds() - 0.2).abs() < 0.000_001);
    assert_eq!(frame.source_pts, 2048);
    assert_eq!(
        frame.source_time_base,
        FrameRate {
            numerator: 1,
            denominator: 10240
        }
    );
    assert!(!frame.exact);

    let exact_error = backend
        .extract_frame(
            &FrameExtractRequest {
                input: first.clone(),
                output: dir.path().join("never.png"),
                requested_time: mt(25, 100),
                video_stream: None,
                alignment: FrameAlignment::Exact,
                overwrite: false,
            },
            &CancellationToken::new(),
            &mut |_| {},
        )
        .unwrap_err();
    assert!(matches!(exact_error, MediaError::Unsupported(_)));

    let output = dir.path().join("joined.mp4");
    let mut export_progress = Vec::new();
    let result = backend
        .export(
            &ExportRequest {
                segments: vec![
                    ExportSegment {
                        input: first,
                        source_start: mt(2, 10),
                        source_duration: mt(5, 10),
                        video_stream: None,
                        audio_stream: None,
                    },
                    ExportSegment {
                        input: second,
                        source_start: mt(1, 10),
                        source_duration: mt(4, 10),
                        video_stream: None,
                        audio_stream: None,
                    },
                ],
                output: output.clone(),
                container: Container::Mp4,
                video: VideoOutput {
                    width: 120,
                    height: 120,
                    frame_rate: FrameRate {
                        numerator: 10,
                        denominator: 1,
                    },
                    aspect_mode: AspectMode::Fit,
                    codec: VideoCodec::H264,
                    crf: Some(28),
                },
                audio: AudioOutput::Preserve { gain_db: -3.0 },
                requested_features: vec![],
                overwrite: false,
            },
            &CancellationToken::new(),
            &mut |update| export_progress.push(update),
        )
        .unwrap();
    assert!(output.is_file());
    assert!(result.verification.passed);
    assert_eq!(
        result
            .metadata
            .streams
            .iter()
            .find(|s| s.codec_type == StreamKind::Video)
            .unwrap()
            .width,
        Some(120)
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
    let verifying = export_progress
        .iter()
        .position(|update| update.stage == ProgressStage::Verifying)
        .expect("export must announce verification stage");
    assert_eq!(export_progress[verifying].fraction, None);
    assert!(
        export_progress[..verifying]
            .iter()
            .any(|update| update.stage == ProgressStage::Encoding)
    );
}

#[test]
fn refuses_existing_outputs_and_unknown_features() {
    let Some(backend) = backend() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.mp4");
    let ffmpeg = Path::new("/usr/local/bin/ffmpeg");
    let ffmpeg = if ffmpeg.is_file() {
        ffmpeg
    } else {
        Path::new("ffmpeg")
    };
    make_fixture(ffmpeg, &input, "black", 440);
    let request = ExportRequest {
        segments: vec![ExportSegment {
            input: input.clone(),
            source_start: mt(0, 1),
            source_duration: mt(1, 2),
            video_stream: None,
            audio_stream: None,
        }],
        output: input,
        container: Container::Mp4,
        video: VideoOutput {
            width: 160,
            height: 90,
            frame_rate: FrameRate {
                numerator: 10,
                denominator: 1,
            },
            aspect_mode: AspectMode::Fit,
            codec: VideoCodec::H264,
            crf: None,
        },
        audio: AudioOutput::Drop,
        requested_features: vec!["transition.crossfade".into()],
        overwrite: true,
    };
    assert!(matches!(
        backend.export(&request, &CancellationToken::new(), &mut |_| {}),
        Err(MediaError::Unsupported(_))
    ));
}

#[test]
fn observes_pre_requested_cancellation_without_creating_output() {
    let Some(backend) = backend() else {
        return;
    };
    let token = CancellationToken::new();
    token.cancel();
    let output = std::env::temp_dir().join(format!(
        "opencut-cancelled-export-{}.mp4",
        std::process::id()
    ));
    let request = ExportRequest {
        segments: vec![],
        output: output.clone(),
        container: Container::Mp4,
        video: VideoOutput {
            width: 16,
            height: 16,
            frame_rate: FrameRate {
                numerator: 1,
                denominator: 1,
            },
            aspect_mode: AspectMode::Fit,
            codec: VideoCodec::H264,
            crf: None,
        },
        audio: AudioOutput::Drop,
        requested_features: vec![],
        overwrite: false,
    };
    assert!(matches!(
        backend.export(&request, &token, &mut |_| {}),
        Err(MediaError::Cancelled)
    ));
    assert!(!output.exists());
}

#[test]
fn extracts_from_nonzero_start_vfr_using_actual_source_pts() {
    let Some(backend) = backend() else {
        return;
    };
    let ffmpeg = Path::new("/usr/local/bin/ffmpeg");
    let ffmpeg = if ffmpeg.is_file() {
        ffmpeg
    } else {
        Path::new("ffmpeg")
    };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("offset-vfr.mp4");
    let output = dir.path().join("offset-frame.png");
    make_offset_vfr_fixture(ffmpeg, &input);
    let frame = backend
        .extract_frame(
            &FrameExtractRequest {
                input: input.clone(),
                output: output.clone(),
                requested_time: mt(827, 100),
                video_stream: None,
                alignment: FrameAlignment::Nearest,
                overwrite: false,
            },
            &CancellationToken::new(),
            &mut |_| {},
        )
        .unwrap();
    assert!(output.is_file());
    assert!((frame.actual_time.seconds() - 8.3).abs() < 0.000_001);
    assert_eq!(frame.source_pts, 84_992);
    assert!(!frame.exact);
}

#[test]
fn direct_frame_preserves_nonzero_vfr_pts_bframes_and_retime_mapping() {
    let Some(backend) = backend() else {
        return;
    };
    let ffmpeg = if Path::new("/usr/local/bin/ffmpeg").is_file() {
        Path::new("/usr/local/bin/ffmpeg")
    } else {
        Path::new("ffmpeg")
    };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("offset-vfr-bframes.mp4");
    make_offset_vfr_fixture(ffmpeg, &input);
    let ffprobe = if Path::new("/usr/local/bin/ffprobe").is_file() {
        Path::new("/usr/local/bin/ffprobe")
    } else {
        Path::new("ffprobe")
    };
    let frame_probe = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "frame=best_effort_timestamp_time,pict_type",
            "-of",
            "json",
        ])
        .arg(&input)
        .output()
        .unwrap();
    assert!(frame_probe.status.success());
    let frame_probe: serde_json::Value = serde_json::from_slice(&frame_probe.stdout).unwrap();
    let frames = frame_probe["frames"].as_array().unwrap();
    assert!(
        frames
            .iter()
            .any(|frame| frame["pict_type"].as_str() == Some("B")),
        "fixture must contain a decoded B frame: {frames:?}"
    );
    let pts = frames
        .iter()
        .map(|frame| {
            frame["best_effort_timestamp_time"]
                .as_str()
                .unwrap()
                .parse::<f64>()
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(
        pts[0] >= 7.9,
        "fixture must retain a non-zero PTS origin: {pts:?}"
    );
    let deltas = pts
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .collect::<Vec<_>>();
    assert!(
        deltas
            .windows(2)
            .any(|pair| (pair[0] - pair[1]).abs() > 0.05),
        "fixture must be VFR with unequal neighboring PTS deltas: {pts:?}"
    );
    assert!(pts.iter().any(|pts| (*pts - 8.0).abs() < 0.000_001));
    assert!(pts.iter().any(|pts| (*pts - 8.1).abs() < 0.000_001));
    assert!(pts.iter().any(|pts| (*pts - 8.3).abs() < 0.000_001));
    let direct_output = dir.path().join("direct-vfr.png");
    let timeline = TimelineExportRequest {
        output: direct_output.clone(),
        container: Container::Mp4,
        video: VideoOutput {
            width: 160,
            height: 90,
            frame_rate: FrameRate {
                numerator: 10,
                denominator: 1,
            },
            aspect_mode: AspectMode::Fit,
            codec: VideoCodec::H264,
            crf: None,
        },
        duration: mt(10, 10),
        video_clips: vec![TimelineVideoClip {
            input: input.clone(),
            // Source ranges are relative to the media presentation origin;
            // this file's first decoded PTS is 8 seconds.
            source_start: mt(0, 1),
            source_duration: mt(5, 10),
            timeline_start: mt(0, 1),
            timeline_duration: mt(10, 10),
            stream: None,
            opacity: 1.0,
            position_x: 0.0,
            position_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
            z_index: 0,
            curves: TimelineVisualCurves::default(),
        }],
        image_clips: vec![],
        audio_clips: vec![],
        captions: vec![],
        overwrite: false,
    };
    let rendered = backend
        .render_timeline_frame(
            &TimelineFrameRenderRequest {
                timeline: timeline.clone(),
                sequence_time: mt(6, 10),
            },
            &CancellationToken::new(),
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(rendered.actual_sequence_time, mt(6, 10));

    let source_output = dir.path().join("source-vfr.png");
    let source_frame = backend
        .extract_frame(
            &FrameExtractRequest {
                input: input.clone(),
                output: source_output.clone(),
                requested_time: mt(83, 10),
                video_stream: None,
                alignment: FrameAlignment::Exact,
                overwrite: false,
            },
            &CancellationToken::new(),
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(source_frame.source_pts, 84_992);
    let direct = image::open(direct_output).unwrap().to_rgb8();
    let source = image::open(source_output).unwrap().to_rgb8();
    let mut total_difference = 0u64;
    let mut maximum_difference = 0u8;
    let mut samples = 0u64;
    for (actual, expected) in direct.pixels().zip(source.pixels()) {
        for channel in 0..3 {
            let difference = actual[channel].abs_diff(expected[channel]);
            total_difference += u64::from(difference);
            maximum_difference = maximum_difference.max(difference);
            samples += 1;
        }
    }
    let target_mean = total_difference as f64 / samples as f64;
    eprintln!("direct vs target PTS 8.3: mean={target_mean:.4}, max={maximum_difference}");
    assert!(target_mean <= 3.0);

    let mean_against = |requested: MediaTime, name: &str| {
        let output = dir.path().join(name);
        backend
            .extract_frame(
                &FrameExtractRequest {
                    input: input.clone(),
                    output: output.clone(),
                    requested_time: requested,
                    video_stream: None,
                    alignment: FrameAlignment::Exact,
                    overwrite: false,
                },
                &CancellationToken::new(),
                &mut |_| {},
            )
            .unwrap();
        let candidate = image::open(output).unwrap().to_rgb8();
        let difference: u64 = direct
            .pixels()
            .zip(candidate.pixels())
            .flat_map(|(actual, expected)| {
                (0..3).map(move |channel| u64::from(actual[channel].abs_diff(expected[channel])))
            })
            .sum();
        difference as f64 / samples as f64
    };
    let previous_mean = mean_against(mt(81, 10), "source-previous.png");
    let next_mean = mean_against(mt(84, 10), "source-next.png");
    eprintln!(
        "direct temporal comparison: target={target_mean:.4}, previous={previous_mean:.4}, next={next_mean:.4}"
    );
    assert!(target_mean + 0.5 < previous_mean);
    assert!(target_mean + 0.5 < next_mean);

    let full_output = dir.path().join("full-vfr.mp4");
    let mut full_timeline = timeline;
    full_timeline.output = full_output.clone();
    backend
        .render_timeline(&full_timeline, &CancellationToken::new(), &mut |_| {})
        .unwrap();
    let full_frame_output = dir.path().join("full-vfr-frame.png");
    backend
        .extract_frame(
            &FrameExtractRequest {
                input: full_output,
                output: full_frame_output.clone(),
                requested_time: mt(6, 10),
                video_stream: None,
                alignment: FrameAlignment::Exact,
                overwrite: false,
            },
            &CancellationToken::new(),
            &mut |_| {},
        )
        .unwrap();
    let full = image::open(full_frame_output).unwrap().to_rgb8();
    let encoded_difference: u64 = direct
        .pixels()
        .zip(full.pixels())
        .flat_map(|(actual, expected)| {
            (0..3).map(move |channel| u64::from(actual[channel].abs_diff(expected[channel])))
        })
        .sum();
    assert!(encoded_difference as f64 / (160 * 90 * 3) as f64 <= 4.0);
}

#[test]
fn direct_frame_caption_interval_is_half_open_and_panic_cleans_temporaries() {
    let Some(backend) = backend() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let make_timeline = |output: std::path::PathBuf| TimelineExportRequest {
        output,
        container: Container::Mp4,
        video: VideoOutput {
            width: 160,
            height: 90,
            frame_rate: FrameRate {
                numerator: 10,
                denominator: 1,
            },
            aspect_mode: AspectMode::Fit,
            codec: VideoCodec::H264,
            crf: None,
        },
        duration: mt(1, 1),
        video_clips: vec![],
        image_clips: vec![],
        audio_clips: vec![],
        captions: vec![TimelineCaption {
            start: mt(2, 10),
            duration: mt(2, 10),
            text: "Boundary".into(),
            font_family: "Arial".into(),
            font_size: 22.0,
            color: "#ffffff".into(),
            background_color: None,
            font_policy: FontResolutionPolicy::Strict {},
            max_width: None,
            line_height: None,
            text_align: TextAlign::Center,
        }],
        overwrite: false,
    };
    let active = dir.path().join("caption-active.png");
    backend
        .render_timeline_frame(
            &TimelineFrameRenderRequest {
                timeline: make_timeline(active.clone()),
                sequence_time: mt(2, 10),
            },
            &CancellationToken::new(),
            &mut |_| {},
        )
        .unwrap();
    let ended = dir.path().join("caption-ended.png");
    backend
        .render_timeline_frame(
            &TimelineFrameRenderRequest {
                timeline: make_timeline(ended.clone()),
                sequence_time: mt(4, 10),
            },
            &CancellationToken::new(),
            &mut |_| {},
        )
        .unwrap();
    let active_pixels = image::open(active).unwrap().to_rgb8();
    let ended_pixels = image::open(ended).unwrap().to_rgb8();
    assert!(active_pixels.pixels().any(|pixel| pixel.0 != [0, 0, 0]));
    assert!(ended_pixels.pixels().all(|pixel| pixel.0 == [0, 0, 0]));

    let panic_output = dir.path().join("panic.png");
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut encoding_updates = 0;
        let _ = backend.render_timeline_frame(
            &TimelineFrameRenderRequest {
                timeline: make_timeline(panic_output.clone()),
                sequence_time: mt(2, 10),
            },
            &CancellationToken::new(),
            &mut |update| {
                if update.stage == ProgressStage::Encoding {
                    encoding_updates += 1;
                    if encoding_updates == 2 {
                        panic!("test progress panic");
                    }
                }
            },
        );
    }));
    assert!(panic.is_err());
    assert!(!panic_output.exists());
    assert!(!std::fs::read_dir(dir.path()).unwrap().any(|entry| {
        let name = entry.unwrap().file_name();
        let name = name.to_string_lossy();
        name.contains(".staging") || name.contains(".caption-")
    }));
}
