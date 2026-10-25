use opencut_media::*;

fn seconds(ticks: i64, base: u32) -> MediaTime {
    MediaTime {
        ticks,
        timebase: base,
    }
}
fn request(output: std::path::PathBuf) -> HistoricalVisualTimelineExportRequest {
    HistoricalVisualTimelineExportRequest {
        base: TimelineExportRequest {
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
            duration: seconds(1, 1),
            video_clips: vec![],
            image_clips: vec![],
            audio_clips: vec![],
            captions: vec![],
            overwrite: false,
        },
        visual: HistoricalVisualCompiledVisualGraph {
            targets: vec![HistoricalVisualVisualTargetBinding {
                target_id: "shape-1".into(),
                kind: HistoricalVisualVisualTargetKind::Shape,
                index: 0,
                z_index: 0,
                layer_order: 0,
            }],
            shapes: vec![HistoricalVisualCompiledShape {
                id: "shape-1".into(),
                start: seconds(0, 1),
                duration: seconds(1, 1),
                primitive: HistoricalVisualShapePrimitive::Rectangle {
                    width_px: 80.0,
                    height_px: 40.0,
                },
                fill: HistoricalVisualRgba {
                    red: 255,
                    green: 0,
                    blue: 0,
                    alpha: 255,
                },
                position_x: 0.0,
                position_y: 0.0,
                rotation_degrees: 0.0,
                opacity: 1.0,
            }],
            effects: vec![HistoricalVisualEffectNode {
                target_id: "shape-1".into(),
                effect: HistoricalVisualVisualEffect::ColorAdjust {
                    exposure_ev: 0.0,
                    contrast: 1.0,
                    saturation: 1.0,
                },
            }],
            masks: vec![HistoricalVisualMaskNode {
                target_id: "shape-1".into(),
                shape: HistoricalVisualMaskShape::Rectangle {
                    x: 0.0,
                    y: 0.0,
                    width: 0.5,
                    height: 1.0,
                },
                inverted: false,
                feather: 0.0,
            }],
            transitions: vec![],
        },
    }
}

#[test]
fn shape_effect_mask_render_in_real_frame() {
    let Some(backend) = FfmpegBackend::discover().ok() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("frame.png");
    let result = backend
        .render_timeline_frame_historical_visual(
            &HistoricalVisualTimelineFrameRenderRequest {
                timeline: request(output.clone()),
                sequence_time: seconds(1, 2),
            },
            &CancellationToken::new(),
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(result.actual_sequence_time, seconds(1, 2));
    let frame = image::open(output).unwrap().to_rgb8();
    let left = frame.get_pixel(60, 45).0;
    let right = frame.get_pixel(100, 45).0;
    assert!(
        left[0] > 180 && left[1] < 50 && left[2] < 50,
        "left={left:?}"
    );
    assert!(right.iter().all(|v| *v < 20), "right={right:?}");
}

#[test]
fn blur_and_feather_change_real_pixels() {
    let Some(backend) = FfmpegBackend::discover().ok() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("soft.png");
    let mut timeline = request(output.clone());
    timeline.visual.effects[0].effect =
        HistoricalVisualVisualEffect::GaussianBlur { radius_px: 4.0 };
    timeline.visual.masks[0].feather = 0.2;
    backend
        .render_timeline_frame_historical_visual(
            &HistoricalVisualTimelineFrameRenderRequest {
                timeline,
                sequence_time: seconds(1, 2),
            },
            &CancellationToken::new(),
            &mut |_| {},
        )
        .unwrap();
    let frame = image::open(output).unwrap().to_rgb8();
    let core = frame.get_pixel(65, 45).0[0];
    let edge = frame.get_pixel(80, 45).0[0];
    let outer = frame.get_pixel(110, 45).0[0];
    assert!(
        core > edge && edge > outer,
        "core={core} edge={edge} outer={outer}"
    );
    assert!(edge > 10, "feather did not leave intermediate coverage");
}

#[test]
fn desaturation_keeps_transparent_area_black() {
    let Some(backend) = FfmpegBackend::discover().ok() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("gray.png");
    let mut timeline = request(output.clone());
    timeline.visual.masks.clear();
    timeline.visual.effects[0].effect = HistoricalVisualVisualEffect::ColorAdjust {
        exposure_ev: 0.0,
        contrast: 1.0,
        saturation: 0.0,
    };
    backend
        .render_timeline_frame_historical_visual(
            &HistoricalVisualTimelineFrameRenderRequest {
                timeline,
                sequence_time: seconds(1, 2),
            },
            &CancellationToken::new(),
            &mut |_| {},
        )
        .unwrap();
    let image = image::open(output).unwrap().to_rgb8();
    let center = image.get_pixel(80, 45).0;
    let outside = image.get_pixel(10, 10).0;
    assert!(
        (i16::from(center[0]) - i16::from(center[1])).abs() < 10,
        "center={center:?}"
    );
    assert!(
        (i16::from(center[1]) - i16::from(center[2])).abs() < 10,
        "center={center:?}"
    );
    assert!(outside.iter().all(|v| *v < 20), "outside={outside:?}");
}

#[test]
fn source_handle_crossfade_renders_red_blue_mix() {
    check_source_handle_crossfade(false);
}

#[test]
fn color_adjust_preserves_crossfade_alpha() {
    check_source_handle_crossfade(true);
}

fn check_source_handle_crossfade(graded: bool) {
    let Some(backend) = FfmpegBackend::discover().ok() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let ffmpeg = if std::path::Path::new("/usr/local/bin/ffmpeg").is_file() {
        "/usr/local/bin/ffmpeg"
    } else {
        "ffmpeg"
    };
    let mut paths = Vec::new();
    for (name, color) in [("red", "red"), ("blue", "blue")] {
        let path = dir.path().join(format!("{name}.mp4"));
        let status = std::process::Command::new(ffmpeg)
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                &format!("color=c={color}:s=160x90:r=10:d=2"),
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-y",
            ])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        paths.push(path);
    }
    let mut graph = request(dir.path().join("mix.png"));
    graph.base.duration = seconds(2, 1);
    graph.base.video_clips = vec![
        TimelineVideoClip {
            input: paths[0].clone(),
            source_start: seconds(0, 1),
            source_duration: seconds(1, 1),
            timeline_start: seconds(0, 1),
            timeline_duration: seconds(1, 1),
            stream: None,
            opacity: 1.0,
            position_x: 0.0,
            position_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
            z_index: 0,
            curves: Default::default(),
        },
        TimelineVideoClip {
            input: paths[1].clone(),
            source_start: seconds(1, 2),
            source_duration: seconds(1, 1),
            timeline_start: seconds(1, 1),
            timeline_duration: seconds(1, 1),
            stream: None,
            opacity: 1.0,
            position_x: 0.0,
            position_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
            z_index: 0,
            curves: Default::default(),
        },
    ];
    graph.visual.targets = vec![
        HistoricalVisualVisualTargetBinding {
            target_id: "out".into(),
            kind: HistoricalVisualVisualTargetKind::VideoClip,
            index: 0,
            z_index: 0,
            layer_order: 0,
        },
        HistoricalVisualVisualTargetBinding {
            target_id: "in".into(),
            kind: HistoricalVisualVisualTargetKind::VideoClip,
            index: 1,
            z_index: 0,
            layer_order: 1,
        },
    ];
    graph.visual.shapes.clear();
    graph.visual.effects.clear();
    graph.visual.masks.clear();
    graph
        .visual
        .transitions
        .push(HistoricalVisualTransitionNode {
            outgoing_clip_id: "out".into(),
            incoming_clip_id: "in".into(),
            duration: seconds(3, 5),
            transition: HistoricalVisualTransition::Crossfade,
        });
    if graded {
        for target in ["out", "in"] {
            graph.visual.effects.push(HistoricalVisualEffectNode {
                target_id: target.into(),
                effect: HistoricalVisualVisualEffect::ColorAdjust {
                    exposure_ev: 0.0,
                    contrast: 1.025,
                    saturation: 1.025,
                },
            });
        }
    }
    for (step, red_dominant, blue_dominant) in
        [(8, true, false), (10, false, false), (12, false, true)]
    {
        let output = dir.path().join(format!("step{step}.png"));
        graph.base.output = output.clone();
        backend
            .render_timeline_frame_historical_visual(
                &HistoricalVisualTimelineFrameRenderRequest {
                    timeline: graph.clone(),
                    sequence_time: seconds(step, 10),
                },
                &CancellationToken::new(),
                &mut |_| {},
            )
            .unwrap();
        let pixel = image::open(output).unwrap().to_rgb8().get_pixel(80, 45).0;
        if red_dominant {
            assert!(pixel[0] > pixel[2] * 2, "early={pixel:?}");
        } else if blue_dominant {
            assert!(pixel[2] > pixel[0] * 2, "late={pixel:?}");
        } else {
            assert!(pixel[0] > 30 && pixel[2] > 30, "middle={pixel:?}");
        }
    }
    let range = HistoricalVisualTimelineRangeRenderRequest {
        timeline: graph.clone(),
        sequence_start: seconds(1, 2),
        sequence_duration: seconds(1, 1),
    };
    let mut range = range;
    range.timeline.base.output = dir.path().join("range.mp4");
    backend
        .render_timeline_range_historical_visual(&range, &CancellationToken::new(), &mut |_| {})
        .unwrap();
    let producer = backend
        .start_timeline_stream_historical_visual(
            &range,
            CancellationToken::new(),
            PreviewStreamLimits::default(),
        )
        .unwrap();
    let mut bytes = Vec::new();
    loop {
        match producer
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap()
        {
            PreviewStreamRead::Chunk(chunk) => bytes.extend(chunk),
            PreviewStreamRead::Timeout => panic!("V3 producer stalled"),
            PreviewStreamRead::End => break,
        }
    }
    assert!(bytes.windows(4).any(|chunk| chunk == b"moof"));
    let stream_path = dir.path().join("stream.mp4");
    std::fs::write(&stream_path, bytes).unwrap();
    let mut full = graph.clone();
    full.base.output = dir.path().join("full.mp4");
    backend
        .render_timeline_historical_visual(&full, &CancellationToken::new(), &mut |_| {})
        .unwrap();
    for (name, input, seek) in [
        ("range", &range.timeline.base.output, "0.5"),
        ("stream", &stream_path, "0.5"),
        ("full", &full.base.output, "1.0"),
    ] {
        let output = dir.path().join(format!("{name}-middle.png"));
        let status = std::process::Command::new(ffmpeg)
            .args(["-v", "error", "-ss", seek, "-i"])
            .arg(input)
            .args(["-frames:v", "1", "-y"])
            .arg(&output)
            .status()
            .unwrap();
        assert!(status.success());
        let pixel = image::open(output).unwrap().to_rgb8().get_pixel(80, 45).0;
        assert!(pixel[0] > 25 && pixel[2] > 25, "{name} middle={pixel:?}");
    }
    // Source handles extend the visible range, not the clip-local animation
    // clock. At sequence 1.3 the incoming curve is 0.3 seconds into the clip.
    graph.base.video_clips[1]
        .curves
        .opacity
        .segments
        .push(EvaluatedSegment {
            start: seconds(0, 1),
            end: seconds(1, 1),
            start_value: 0.25,
            end_value: 0.75,
            cubic_bezier: None,
        });
    graph.base.output = dir.path().join("transition-animation-clock.png");
    backend
        .render_timeline_frame_historical_visual(
            &HistoricalVisualTimelineFrameRenderRequest {
                timeline: graph.clone(),
                sequence_time: seconds(13, 10),
            },
            &CancellationToken::new(),
            &mut |_| {},
        )
        .unwrap();
    let pixel = image::open(&graph.base.output)
        .unwrap()
        .to_rgb8()
        .get_pixel(80, 45)
        .0;
    assert!((i16::from(pixel[2]) - 102).abs() <= 10, "curve={pixel:?}");
}

#[test]
fn overlapping_adjacent_crossfades_keep_all_three_real_sources() {
    let Some(backend) = FfmpegBackend::discover().ok() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let ffmpeg = if std::path::Path::new("/usr/local/bin/ffmpeg").is_file() {
        "/usr/local/bin/ffmpeg"
    } else {
        "ffmpeg"
    };
    let mut paths = Vec::new();
    for (name, color) in [("red", "red"), ("green", "green"), ("blue", "blue")] {
        let path = dir.path().join(format!("{name}.mp4"));
        let status = std::process::Command::new(ffmpeg)
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                &format!("color=c={color}:s=160x90:r=10:d=2"),
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-y",
            ])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        paths.push(path);
    }
    let mut graph = request(dir.path().join("triple.png"));
    graph.base.duration = seconds(12, 5);
    graph.base.video_clips = [
        (0, seconds(0, 1), seconds(0, 1), seconds(1, 1)),
        (1, seconds(1, 2), seconds(1, 1), seconds(2, 5)),
        (2, seconds(1, 2), seconds(7, 5), seconds(1, 1)),
    ]
    .into_iter()
    .map(
        |(index, source_start, timeline_start, duration)| TimelineVideoClip {
            input: paths[index].clone(),
            source_start,
            source_duration: duration,
            timeline_start,
            timeline_duration: duration,
            stream: None,
            opacity: 1.0,
            position_x: 0.0,
            position_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
            z_index: 0,
            curves: Default::default(),
        },
    )
    .collect();
    graph.visual.targets = (0..3)
        .map(|index| HistoricalVisualVisualTargetBinding {
            target_id: format!("clip-{index}"),
            kind: HistoricalVisualVisualTargetKind::VideoClip,
            index,
            z_index: 0,
            layer_order: index as u32,
        })
        .collect();
    graph.visual.shapes.clear();
    graph.visual.effects.clear();
    graph.visual.masks.clear();
    graph.visual.transitions = vec![
        HistoricalVisualTransitionNode {
            outgoing_clip_id: "clip-0".into(),
            incoming_clip_id: "clip-1".into(),
            duration: seconds(3, 5),
            transition: HistoricalVisualTransition::Crossfade,
        },
        HistoricalVisualTransitionNode {
            outgoing_clip_id: "clip-1".into(),
            incoming_clip_id: "clip-2".into(),
            duration: seconds(3, 5),
            transition: HistoricalVisualTransition::Crossfade,
        },
    ];
    backend
        .render_timeline_frame_historical_visual(
            &HistoricalVisualTimelineFrameRenderRequest {
                timeline: graph,
                sequence_time: seconds(6, 5),
            },
            &CancellationToken::new(),
            &mut |_| {},
        )
        .unwrap();
    let pixel = image::open(dir.path().join("triple.png"))
        .unwrap()
        .to_rgb8()
        .get_pixel(80, 45)
        .0;
    assert!(
        pixel[0] > 12 && pixel[1] > 40 && pixel[2] > 12,
        "triple mix={pixel:?}"
    );
}

#[test]
fn identity_alpha_paths_and_half_open_interval_preserve_real_pixels() {
    let backend = FfmpegBackend::discover().expect("FFmpeg required for media acceptance");
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("alpha.png");
    let mut picture = image::RgbaImage::new(80, 40);
    for (x, y, pixel) in picture.enumerate_pixels_mut() {
        *pixel = image::Rgba([
            220,
            (x * 2) as u8,
            (y * 4) as u8,
            if x < 40 { 255 } else { 128 },
        ]);
    }
    picture.save(&input).unwrap();
    let mut graph = request(dir.path().join("identity.png"));
    graph.base.duration = seconds(3, 1);
    graph.base.image_clips = vec![TimelineImageClip {
        input,
        timeline_start: seconds(1, 2),
        timeline_duration: seconds(1, 1),
        stream: None,
        opacity: 0.5,
        position_x: 0.0,
        position_y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        rotation_degrees: 0.0,
        z_index: 0,
        curves: Default::default(),
    }];
    graph.visual.targets = vec![HistoricalVisualVisualTargetBinding {
        target_id: "image".into(),
        kind: HistoricalVisualVisualTargetKind::ImageClip,
        index: 0,
        z_index: 0,
        layer_order: 0,
    }];
    graph.visual.shapes.clear();
    graph.visual.effects.clear();
    graph.visual.masks.clear();
    let render = |graph: &HistoricalVisualTimelineExportRequest, time: MediaTime, name: &str| {
        let mut graph = graph.clone();
        graph.base.output = dir.path().join(name);
        backend
            .render_timeline_frame_historical_visual(
                &HistoricalVisualTimelineFrameRenderRequest {
                    timeline: graph.clone(),
                    sequence_time: time,
                },
                &CancellationToken::new(),
                &mut |_| {},
            )
            .unwrap();
        image::open(&graph.base.output).unwrap().to_rgb8()
    };
    let static_frame = render(&graph, seconds(1, 1), "static.png");
    // The animated path with a constant evaluated value must preserve the same
    // source alpha and contain padding as the fast constant-alpha path.
    graph.base.image_clips[0]
        .curves
        .opacity
        .segments
        .push(EvaluatedSegment {
            start: seconds(0, 1),
            end: seconds(1, 1),
            start_value: 0.5,
            end_value: 0.5,
            cubic_bezier: None,
        });
    let animated_frame = render(&graph, seconds(1, 1), "animated.png");
    for (index, (a, b)) in static_frame
        .pixels()
        .zip(animated_frame.pixels())
        .enumerate()
    {
        for channel in 0..3 {
            assert!(
                (i16::from(a[channel]) - i16::from(b[channel])).abs() <= 2,
                "pixel ({}, {}): static={a:?}, animated={b:?}",
                index % 160,
                index / 160
            );
        }
    }
    assert!(static_frame.get_pixel(35, 45)[0] > 70);
    assert!(static_frame.get_pixel(125, 45)[0] < static_frame.get_pixel(35, 45)[0]);
    let mut fading = graph.clone();
    fading.base.image_clips[0].curves.opacity.segments[0].start_value = 0.0;
    fading.base.image_clips[0].curves.opacity.segments[0].end_value = 1.0;
    for (time, expected_red, name) in [
        (seconds(7, 10), 44_i16, "fade-early.png"),
        (seconds(13, 10), 176_i16, "fade-late.png"),
    ] {
        let frame = render(&fading, time, name);
        assert!((i16::from(frame.get_pixel(35, 45)[0]) - expected_red).abs() <= 8);
        assert!((i16::from(frame.get_pixel(125, 45)[0]) - expected_red / 2).abs() <= 8);
    }
    // Independent RGB expectations for source-over black, including the
    // source image's half-alpha region. This also catches a mislabeled YUV
    // output matrix instead of accepting matching tags alone.
    for (x, expected) in [(35, [110_i16, 18, 40]), (125, [55_i16, 31, 20])] {
        let actual = static_frame.get_pixel(x, 45);
        for channel in 0..3 {
            assert!(
                (i16::from(actual[channel]) - expected[channel]).abs() <= 8,
                "RGB composition at {x}: actual={actual:?}, expected={expected:?}"
            );
        }
    }
    assert!(
        render(&graph, seconds(15, 10), "end.png")
            .pixels()
            .all(|p| p.0.iter().all(|v| *v < 10))
    );
    let mut ranged = graph.clone();
    ranged.base.output = dir.path().join("boundary.mp4");
    backend
        .render_timeline_range_historical_visual(
            &HistoricalVisualTimelineRangeRenderRequest {
                timeline: ranged.clone(),
                sequence_start: seconds(14, 10),
                sequence_duration: seconds(3, 10),
            },
            &CancellationToken::new(),
            &mut |_| {},
        )
        .unwrap();
    let decoded = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(ranged.base.output)
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .unwrap();
    assert!(decoded.status.success());
    let size = 160 * 90 * 3;
    assert_eq!(decoded.stdout.len(), size * 3);
    assert!(decoded.stdout[(45 * 160 + 35) * 3] > 60);
    assert!(decoded.stdout[size..].iter().all(|v| *v < 10));
    let probe = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=color_space,color_transfer,color_primaries,color_range",
            "-of",
            "json",
        ])
        .arg(dir.path().join("boundary.mp4"))
        .output()
        .unwrap();
    assert!(probe.status.success());
    let probe: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
    let stream = &probe["streams"][0];
    assert_eq!(stream["color_space"], "bt709");
    assert_eq!(stream["color_transfer"], "bt709");
    assert_eq!(stream["color_primaries"], "bt709");
    assert_eq!(stream["color_range"], "tv");
}

#[test]
fn caption_still_input_keeps_subsecond_boundaries_and_sdr_metadata() {
    let backend = FfmpegBackend::discover().expect("FFmpeg required for media acceptance");
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("caption.mp4");
    let mut graph = request(output.clone());
    graph.base.duration = seconds(2, 1);
    graph.visual.targets.clear();
    graph.visual.shapes.clear();
    graph.visual.effects.clear();
    graph.visual.masks.clear();
    graph.base.captions = vec![TimelineCaption {
        start: seconds(2, 10),
        duration: seconds(12, 10),
        text: "Boundary".into(),
        font_family: "Arial".into(),
        font_size: 22.0,
        color: "#ffffff".into(),
        background_color: None,
        font_policy: FontResolutionPolicy::Strict {},
        max_width: None,
        line_height: None,
        text_align: TextAlign::Center,
    }];
    backend
        .render_timeline_historical_visual(&graph, &CancellationToken::new(), &mut |_| {})
        .unwrap();
    let decoded = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&output)
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .unwrap();
    assert!(decoded.status.success());
    let size = 160 * 90 * 3;
    assert_eq!(decoded.stdout.len(), size * 20);
    for (index, frame) in decoded.stdout.chunks_exact(size).enumerate() {
        let visible = frame.iter().filter(|value| **value > 180).count() > 20;
        assert_eq!(visible, (2..14).contains(&index), "frame {index}");
    }
    let probe = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-show_streams", "-of", "json"])
        .arg(output)
        .output()
        .unwrap();
    assert!(probe.status.success());
    let probe: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
    let video = probe["streams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|stream| stream["codec_type"] == "video")
        .unwrap();
    for key in ["color_space", "color_transfer", "color_primaries"] {
        assert_eq!(video[key], "bt709", "{key}");
    }
    assert_eq!(video["color_range"], "tv");
}
