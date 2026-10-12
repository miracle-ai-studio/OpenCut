use super::*;
use std::fs;

fn backend() -> FfmpegBackend {
    FfmpegBackend::discover().expect("Explicit native acceptance requires FFmpeg and ffprobe")
}
fn execute(
    backend: &FfmpegBackend,
    graph: CompositionV4,
    input: PathBuf,
    dir: &std::path::Path,
) -> FrameResultV4 {
    let resolved = ProjectFrameAdapterV4::resolve(graph, |_| Ok(input.clone())).unwrap();
    ProjectFrameAdapterV4::render_resolved(
        backend,
        &resolved,
        &FrameExecutionV4 {
            output: dir.join("frame.png"),
            scratch_root: dir.join("scratch"),
            overwrite: false,
            source_alignment: FrameAlignment::Floor,
        },
        &CancellationToken::new(),
        &mut |_| {},
    )
    .unwrap()
}
fn pixel(output: &std::path::Path, x: u32, y: u32, want: [u8; 4]) {
    let rgba = image::open(output).unwrap().to_rgba8();
    let actual = rgba.get_pixel(x, y).0;
    // Existing native V3 filter path goes through YUV420 BT.709. Coordinates
    // are well inside uniform regions; tolerance covers quantization/matrix
    // conversion, not geometry edges or a second renderer as reference.
    for (a, b) in actual.into_iter().zip(want) {
        assert!(
            a.abs_diff(b) <= 12,
            "pixel({x},{y}) expected {want:?}, got {actual:?}"
        );
    }
}

#[test]
#[ignore = "root runs real native pixel acceptance explicitly"]
fn native_pixels_prove_sampled_bump_mask_shape_opacity_and_background() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("red.ppm");
    let mut bytes = b"P6\n64 48\n255\n".to_vec();
    bytes.extend([255, 0, 0].repeat(64 * 48));
    fs::write(&input, bytes).unwrap();
    let graph = compile(&snapshot());
    let result = execute(&backend(), graph, input, dir.path());
    assert_eq!(result.actual_sequence_time, t(3, 2));
    assert_eq!(
        result.frame.actual_sequence_time,
        MediaTime {
            ticks: 3,
            timebase: 2
        }
    );
    assert_eq!(result.revision, "native-fixed-r1");
    // Blue background + red media alpha .5*.25=.125 → (32,0,223).
    // Equal-endpoint Bezier shape alpha .5 bypasses track .25 →
    // white over that base = (144,128,239), only inside the left mask.
    pixel(&result.frame.output, 8, 24, [32, 0, 223, 255]);
    pixel(&result.frame.output, 24, 24, [144, 128, 239, 255]);
    pixel(&result.frame.output, 40, 24, [32, 0, 223, 255]);
    assert_eq!(fs::read_dir(dir.path().join("scratch")).unwrap().count(), 0);
}

#[test]
#[ignore = "root runs real native pixel acceptance explicitly"]
fn native_color_adjust_executes_on_shape_before_mask_composite() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("red.ppm");
    let mut bytes = b"P6\n64 48\n255\n".to_vec();
    bytes.extend([255, 0, 0].repeat(64 * 48));
    fs::write(&input, bytes).unwrap();
    let mut s = snapshot();
    s.project.sequences[0].tracks[1].elements[0]
        .visual
        .as_mut()
        .unwrap()
        .effects[0]
        .effect = EffectKindV4::LegacyColorAdjust {
        exposure_ev: 0.0,
        contrast: 0.0,
        saturation: 1.0,
    };
    let result = execute(&backend(), compile(&s), input, dir.path());
    // Contrast zero in the legacy eq profile puts luminance at midpoint;
    // neutral gray .5 over (32,0,223) with alpha .5 → (80,64,176).
    pixel(&result.frame.output, 24, 24, [80, 64, 176, 255]);
    pixel(&result.frame.output, 40, 24, [32, 0, 223, 255]);
}

fn y4m(path: &std::path::Path) {
    let mut bytes = b"YUV4MPEG2 W64 H48 F2:1 Ip A1:1 C444 XCOLORRANGE=LIMITED\n".to_vec();
    // Independent raw planar oracle: black, white, black, white at 0,.5,1,1.5s.
    for y in [16, 235, 16, 235] {
        bytes.extend_from_slice(b"FRAME\n");
        bytes.extend(vec![y; 64 * 48]);
        bytes.extend(vec![128; 64 * 48 * 2]);
    }
    fs::write(path, bytes).unwrap();
}
fn video_snapshot() -> ProjectSnapshotV4 {
    let mut s = snapshot();
    s.project.sequences[0].tracks.truncate(1);
    let asset = &mut s.project.assets[0];
    asset.kind = opencut_contracts::AssetKind::Video;
    asset.duration = Some(t(2, 1));
    asset.metadata.mime_type = Some("video/x-yuv4mpeg".into());
    let e = &mut s.project.sequences[0].tracks[0].elements[0];
    e.timeline_range = opencut_contracts::TimeRange {
        start: t(1, 4),
        duration: t(1, 1),
    };
    e.source_span = ElementSourceSpanV4::LegacySourceRange {
        range: opencut_contracts::TimeRange {
            start: t(1, 4),
            duration: t(1, 1),
        },
    };
    e.content = ElementContentV4::Video {
        asset_id: "asset".into(),
        video_stream: None,
        source_audio_enabled: false,
        audio: AudioStyleV4 {
            stream: None,
            gain: GainV4::Decibels { value: 0.0 },
            muted: false,
        },
        retime: RetimeV4 {
            rate: RationalRateV4 {
                numerator: 1,
                denominator: 1,
            },
            maintain_pitch: true,
            mapping: RetimeMappingV4::ExactRational,
        },
    };
    e.visual.as_mut().unwrap().opacity = 1.0;
    s.project.sequences[0].tracks[0].opacity = 1.0;
    s
}
#[test]
#[ignore = "root runs real native pixel acceptance explicitly"]
fn native_decode_uses_exact_source_point_and_records_floor_pts() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("oracle.y4m");
    y4m(&input);
    let s = video_snapshot();
    let graph = opencut_composition::project_v4::compile_frame_v4(
        &s,
        "scene",
        &t(3, 4),
        opencut_composition::CompositionScope::Visual,
    )
    .unwrap();
    let result = execute(&backend(), graph, input, dir.path());
    assert_eq!(result.decoded_sources.len(), 1);
    let source = &result.decoded_sources[0];
    assert_eq!(source.requested_time, t(3, 4));
    assert_eq!(source.actual_time.seconds(), 0.5);
    assert!(!source.exact);
    pixel(&result.frame.output, 32, 24, [255, 255, 255, 255]);
}

#[test]
#[ignore = "root runs real native pixel acceptance explicitly"]
fn native_half_tick_request_over_extent_does_not_become_decode_window() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("oracle.y4m");
    y4m(&input);
    let mut s = video_snapshot();
    s.project.assets[0].duration = Some(t(120001, 120000));
    let e = &mut s.project.sequences[0].tracks[0].elements[0];
    e.timeline_range = opencut_contracts::TimeRange {
        start: t(0, 1),
        duration: t(60001, 120000),
    };
    e.source_span = ElementSourceSpanV4::ClassicTrimmed {
        trim_start: t(0, 1),
        trim_end: t(0, 1),
        source_duration: Some(t(120001, 120000)),
    };
    if let ElementContentV4::Video { retime, .. } = &mut e.content {
        retime.rate.numerator = 2;
        retime.mapping = RetimeMappingV4::ClassicRoundedTicks { timebase: 120000 };
    }
    let graph = opencut_composition::project_v4::compile_frame_v4(
        &s,
        "scene",
        &t(60000, 120000),
        opencut_composition::CompositionScope::Visual,
    )
    .unwrap();
    let SampledContentV4::Video { source, .. } = &graph.layers[0].content else {
        panic!()
    };
    assert_eq!(
        media_time(&source.nominal_request.duration)
            .unwrap()
            .seconds(),
        120002.0 / 120000.0
    );
    let result = execute(&backend(), graph, input, dir.path());
    assert_eq!(result.decoded_sources[0].actual_time.seconds(), 1.0);
    assert!(result.decoded_sources[0].exact);
    pixel(&result.frame.output, 32, 24, [0, 0, 0, 255]);
}

#[test]
#[ignore = "root runs real native pixel acceptance explicitly"]
fn native_canvas_fraction_feather_uses_min_canvas_dimension() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("red.ppm");
    let mut bytes = b"P6\n64 48\n255\n".to_vec();
    bytes.extend([255, 0, 0].repeat(64 * 48));
    fs::write(&input, bytes).unwrap();
    let mut s = snapshot();
    s.project.sequences[0].tracks[1].elements[0]
        .visual
        .as_mut()
        .unwrap()
        .masks[0]
        .feather = MaskFeatherV4::LegacyCanvasFraction { value: 0.25 };
    let result = execute(&backend(), compile(&s), input, dir.path());
    // Feather width .25*min(64,48)=12px. Four pixels inside/outside the
    // right edge yield coverage 5/6 and 1/6; shape opacity .5 still applies.
    pixel(&result.frame.output, 28, 24, [125, 106, 236, 255]);
    pixel(&result.frame.output, 36, 24, [51, 21, 226, 255]);
}

#[test]
#[ignore = "root runs real native pixel acceptance explicitly"]
fn native_crossfade_composites_incoming_half_over_full_outgoing() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("oracle.y4m");
    y4m(&input);
    let mut s = video_snapshot();
    let out = &mut s.project.sequences[0].tracks[0].elements[0];
    out.id = "out".into();
    out.timeline_range.start = t(0, 1);
    out.source_span = ElementSourceSpanV4::LegacySourceRange {
        range: opencut_contracts::TimeRange {
            start: t(0, 1),
            duration: t(1, 1),
        },
    };
    let mut inc = out.clone();
    inc.id = "in".into();
    inc.timeline_range.start = t(1, 1);
    inc.source_span = ElementSourceSpanV4::LegacySourceRange {
        range: opencut_contracts::TimeRange {
            start: t(1, 2),
            duration: t(1, 1),
        },
    };
    s.project.sequences[0].tracks[0].elements.push(inc);
    s.project.sequences[0].transitions.push(TransitionV4 {
        id: "fade".into(),
        outgoing_element_id: "out".into(),
        incoming_element_id: "in".into(),
        duration: t(1, 2),
        transition: TransitionKindV4::LegacyCrossfadeV3,
    });
    let graph = opencut_composition::project_v4::compile_frame_v4(
        &s,
        "scene",
        &t(1, 1),
        opencut_composition::CompositionScope::Visual,
    )
    .unwrap();
    let result = execute(&backend(), graph, input, dir.path());
    // Raw source at 1s is black, incoming at .5s is white. A .5 incoming
    // alpha over opaque outgoing gives gray; fading both would expose blue.
    pixel(&result.frame.output, 32, 24, [128, 128, 128, 255]);
    assert_eq!(result.decoded_sources.len(), 2);
}
