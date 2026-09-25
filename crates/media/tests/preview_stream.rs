use opencut_media::*;
use std::{
    fs,
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[test]
fn one_producer_emits_decodable_fragmented_mp4_and_cleans_on_cancel() {
    let Some(backend) = FfmpegBackend::discover().ok() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let request = TimelineRangeRenderRequest {
        timeline: TimelineExportRequest {
            output: dir.path().join("placeholder.mp4"),
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
            duration: MediaTime {
                ticks: 2,
                timebase: 1,
            },
            video_clips: vec![],
            image_clips: vec![],
            audio_clips: vec![],
            captions: vec![],
            overwrite: false,
        },
        sequence_start: MediaTime {
            ticks: 1,
            timebase: 2,
        },
        sequence_duration: MediaTime {
            ticks: 1,
            timebase: 1,
        },
    };
    let producer = backend
        .start_timeline_stream(
            &request,
            CancellationToken::new(),
            PreviewStreamLimits::default(),
        )
        .unwrap();
    let mut bytes = Vec::new();
    loop {
        match producer.recv_timeout(Duration::from_secs(10)).unwrap() {
            PreviewStreamRead::Chunk(chunk) => bytes.extend(chunk),
            PreviewStreamRead::Timeout => panic!("producer stalled"),
            PreviewStreamRead::End => break,
        }
    }
    assert!(bytes.windows(4).any(|value| value == b"ftyp"));
    assert!(bytes.windows(4).any(|value| value == b"moof"));
    assert!(bytes.windows(4).any(|value| value == b"mdat"));
    assert!(producer.status().done);
    assert!(producer.status().produced_through.is_some());
    assert_eq!(producer.status().buffered_bytes, 0);
    let output = dir.path().join("stream.mp4");
    fs::write(&output, &bytes).unwrap();
    let ffprobe = if Path::new("/usr/local/bin/ffprobe").is_file() {
        "/usr/local/bin/ffprobe"
    } else {
        "ffprobe"
    };
    let probe = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "csv=p=0",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(probe.status.success());
    let streams = String::from_utf8_lossy(&probe.stdout);
    assert!(streams.contains("video") && streams.contains("audio"));

    let token = CancellationToken::new();
    token.cancel();
    let cancelled = backend.start_timeline_stream(&request, token, PreviewStreamLimits::default());
    assert!(matches!(cancelled, Err(MediaError::Cancelled)));
}

#[test]
fn slow_consumer_and_external_cancel_stop_the_generation() {
    let Some(backend) = FfmpegBackend::discover().ok() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("busy.mp4");
    let ffmpeg = if Path::new("/usr/local/bin/ffmpeg").is_file() {
        "/usr/local/bin/ffmpeg"
    } else {
        "ffmpeg"
    };
    let status = Command::new(ffmpeg)
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=640x360:r=30:d=6",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=6",
            "-c:v",
            "mpeg4",
            "-q:v",
            "2",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success());
    let mt = |ticks| MediaTime { ticks, timebase: 1 };
    let request = TimelineRangeRenderRequest {
        timeline: TimelineExportRequest {
            output: dir.path().join("preview.mp4"),
            container: Container::Mp4,
            video: VideoOutput {
                width: 640,
                height: 360,
                frame_rate: FrameRate {
                    numerator: 30,
                    denominator: 1,
                },
                aspect_mode: AspectMode::Fit,
                codec: VideoCodec::H264,
                crf: None,
            },
            duration: mt(6),
            video_clips: vec![TimelineVideoClip {
                input: source.clone(),
                source_start: mt(0),
                source_duration: mt(6),
                timeline_start: mt(0),
                timeline_duration: mt(6),
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
        },
        sequence_start: mt(0),
        sequence_duration: mt(6),
    };
    let limits = PreviewStreamLimits {
        max_buffered_bytes: 512 * 1024,
        max_block_duration: Duration::from_millis(150),
        ..PreviewStreamLimits::default()
    };
    let producer = backend
        .start_timeline_stream(&request, CancellationToken::new(), limits)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    while !producer.status().done && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        producer.status().done,
        "producer must not wait forever for an unread stream"
    );
    assert_eq!(
        producer.status().error_code.as_deref(),
        Some("MEDIA_BACKPRESSURE")
    );
    drop(producer);

    let token = CancellationToken::new();
    let producer = backend
        .start_timeline_stream(&request, token.clone(), PreviewStreamLimits::default())
        .unwrap();
    token.cancel();
    let deadline = Instant::now() + Duration::from_secs(4);
    while !producer.status().done && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        producer.status().done,
        "external cancellation must stop a blocked producer"
    );
    assert_eq!(
        producer.status().error_code.as_deref(),
        Some("JOB_CANCELLED")
    );
}

#[test]
fn stream_keeps_non_frame_aligned_final_audio_tail() {
    let Some(backend) = FfmpegBackend::discover().ok() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let request = TimelineRangeRenderRequest {
        timeline: TimelineExportRequest {
            output: dir.path().join("tail-placeholder.mp4"),
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
            duration: MediaTime {
                ticks: 605,
                timebase: 100,
            },
            video_clips: vec![],
            image_clips: vec![],
            audio_clips: vec![],
            captions: vec![],
            overwrite: false,
        },
        sequence_start: MediaTime {
            ticks: 6,
            timebase: 1,
        },
        sequence_duration: MediaTime {
            ticks: 5,
            timebase: 100,
        },
    };
    let producer = backend
        .start_timeline_stream(
            &request,
            CancellationToken::new(),
            PreviewStreamLimits::default(),
        )
        .unwrap();
    let mut bytes = Vec::new();
    loop {
        match producer.recv_timeout(Duration::from_secs(10)).unwrap() {
            PreviewStreamRead::Chunk(chunk) => bytes.extend(chunk),
            PreviewStreamRead::Timeout => panic!("tail producer stalled"),
            PreviewStreamRead::End => break,
        }
    }
    let output = dir.path().join("tail.mp4");
    fs::write(&output, bytes).unwrap();
    let ffmpeg = if Path::new("/usr/local/bin/ffmpeg").is_file() {
        "/usr/local/bin/ffmpeg"
    } else {
        "ffmpeg"
    };
    let decoded = Command::new(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(&output)
        .args([
            "-map", "0:a:0", "-f", "s16le", "-ac", "2", "-ar", "48000", "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(decoded.status.success());
    // AAC priming can add a small number of samples; the 50 ms tail must
    // remain present rather than being dropped by frame-grid alignment.
    assert!(
        decoded.stdout.len() >= 8_000,
        "decoded audio tail {} bytes",
        decoded.stdout.len()
    );
    let mut invalid = request;
    invalid.sequence_start = MediaTime {
        ticks: 59,
        timebase: 10,
    };
    invalid.sequence_duration = MediaTime {
        ticks: 5,
        timebase: 100,
    };
    assert!(matches!(
        backend.start_timeline_stream(
            &invalid,
            CancellationToken::new(),
            PreviewStreamLimits::default()
        ),
        Err(MediaError::InvalidRequest(_))
    ));
}
