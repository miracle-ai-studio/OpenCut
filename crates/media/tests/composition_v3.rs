use opencut_media::*;

fn seconds(ticks: i64, base: u32) -> MediaTime {
    MediaTime {
        ticks,
        timebase: base,
    }
}
fn request(output: std::path::PathBuf) -> TimelineExportRequestV3 {
    TimelineExportRequestV3 {
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
        visual: CompiledVisualGraphV3 {
            targets: vec![VisualTargetBindingV3 {
                target_id: "shape-1".into(),
                kind: VisualTargetKindV3::Shape,
                index: 0,
                z_index: 0,
                layer_order: 0,
            }],
            shapes: vec![CompiledShapeV3 {
                id: "shape-1".into(),
                start: seconds(0, 1),
                duration: seconds(1, 1),
                primitive: ShapePrimitiveV3::Rectangle {
                    width_px: 80.0,
                    height_px: 40.0,
                },
                fill: RgbaV3 {
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
            effects: vec![EffectNodeV3 {
                target_id: "shape-1".into(),
                effect: VisualEffectV3::ColorAdjust {
                    exposure_ev: 0.0,
                    contrast: 1.0,
                    saturation: 1.0,
                },
            }],
            masks: vec![MaskNodeV3 {
                target_id: "shape-1".into(),
                shape: MaskShapeV3::Rectangle {
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
        .render_timeline_frame_v3(
            &TimelineFrameRenderRequestV3 {
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
    timeline.visual.effects[0].effect = VisualEffectV3::GaussianBlur { radius_px: 4.0 };
    timeline.visual.masks[0].feather = 0.2;
    backend
        .render_timeline_frame_v3(
            &TimelineFrameRenderRequestV3 {
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
    timeline.visual.effects[0].effect = VisualEffectV3::ColorAdjust {
        exposure_ev: 0.0,
        contrast: 1.0,
        saturation: 0.0,
    };
    backend
        .render_timeline_frame_v3(
            &TimelineFrameRenderRequestV3 {
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
        VisualTargetBindingV3 {
            target_id: "out".into(),
            kind: VisualTargetKindV3::VideoClip,
            index: 0,
            z_index: 0,
            layer_order: 0,
        },
        VisualTargetBindingV3 {
            target_id: "in".into(),
            kind: VisualTargetKindV3::VideoClip,
            index: 1,
            z_index: 0,
            layer_order: 1,
        },
    ];
    graph.visual.shapes.clear();
    graph.visual.effects.clear();
    graph.visual.masks.clear();
    graph.visual.transitions.push(TransitionNodeV3 {
        outgoing_clip_id: "out".into(),
        incoming_clip_id: "in".into(),
        duration: seconds(3, 5),
        transition: TransitionV3::Crossfade,
    });
    for (step, red_dominant, blue_dominant) in
        [(8, true, false), (10, false, false), (12, false, true)]
    {
        let output = dir.path().join(format!("step{step}.png"));
        graph.base.output = output.clone();
        backend
            .render_timeline_frame_v3(
                &TimelineFrameRenderRequestV3 {
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
    let range = TimelineRangeRenderRequestV3 {
        timeline: graph.clone(),
        sequence_start: seconds(1, 2),
        sequence_duration: seconds(1, 1),
    };
    let mut range = range;
    range.timeline.base.output = dir.path().join("range.mp4");
    backend
        .render_timeline_range_v3(&range, &CancellationToken::new(), &mut |_| {})
        .unwrap();
    let producer = backend
        .start_timeline_stream_v3(
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
    for (name, input) in [
        ("range", &range.timeline.base.output),
        ("stream", &stream_path),
    ] {
        let output = dir.path().join(format!("{name}-middle.png"));
        let status = std::process::Command::new(ffmpeg)
            .args(["-v", "error", "-ss", "0.5", "-i"])
            .arg(input)
            .args(["-frames:v", "1", "-y"])
            .arg(&output)
            .status()
            .unwrap();
        assert!(status.success());
        let pixel = image::open(output).unwrap().to_rgb8().get_pixel(80, 45).0;
        assert!(pixel[0] > 25 && pixel[2] > 25, "{name} middle={pixel:?}");
    }
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
        .map(|index| VisualTargetBindingV3 {
            target_id: format!("clip-{index}"),
            kind: VisualTargetKindV3::VideoClip,
            index,
            z_index: 0,
            layer_order: index as u32,
        })
        .collect();
    graph.visual.shapes.clear();
    graph.visual.effects.clear();
    graph.visual.masks.clear();
    graph.visual.transitions = vec![
        TransitionNodeV3 {
            outgoing_clip_id: "clip-0".into(),
            incoming_clip_id: "clip-1".into(),
            duration: seconds(3, 5),
            transition: TransitionV3::Crossfade,
        },
        TransitionNodeV3 {
            outgoing_clip_id: "clip-1".into(),
            incoming_clip_id: "clip-2".into(),
            duration: seconds(3, 5),
            transition: TransitionV3::Crossfade,
        },
    ];
    backend
        .render_timeline_frame_v3(
            &TimelineFrameRenderRequestV3 {
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
