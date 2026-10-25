use super::*;
use opencut_contracts::{AssetKind, AssetVersion, FrameRate, Time, TimeRange, scene::*};
use std::{fs, path::Path, time::Duration};
mod fixture;
use fixture::*;

// Native media execution is deliberately selected explicitly by the parent
// verifier. These tests create only tiny synthetic local WAVs and no artifacts.
#[test]
#[ignore = "actual native PCM fixture; run centrally with FFmpeg"]
fn fractional_rate_trim_placement_and_zero_tail_match_hand_calculated_pcm() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mono.wav");
    wav(&path, 1, &[0, 1000, 2000, 3000]);
    let mut s = snapshot(1, 5, 3, 2);
    let e = &mut s.project.sequences[0].tracks[1].elements[0];
    e.timeline_range.start = t(3, 96000); // floor(1.5)=1, own local starts0.
    e.source_span = ElementSourceSpan::ClassicTrimmed {
        trim_start: t(1, 48000),
        trim_end: t(0, 1),
        source_duration: None,
    };
    let backend = FfmpegBackend::discover().unwrap();
    let cancel = CancellationToken::new();
    let mut stage = stage(&backend, s, &path, &cancel);
    let mut got = vec![];
    while let Some(block) = stage.read_next(2, &cancel).unwrap() {
        got.extend(block.samples);
    }
    // source positions1,2.5,4,5.5,7. EOF lower>=4 ->0. Mono duplicates.
    let expected = [0, 1000, 2500, 0, 0, 0, 0];
    assert_eq!(got.len(), expected.len() * 2);
    for (frame, want) in got.chunks_exact(2).zip(expected) {
        close(frame[0], want as f64 / 32768.0);
        assert_eq!(frame[0], frame[1]);
    }
    let receipt = &stage.decode_receipts()[0];
    assert_eq!(receipt.first_decoded_pts, 0);
    assert_eq!(receipt.native_sample_rate, 48000);
    assert_eq!(receipt.channel_map, NativeChannelMap::MonoDuplicate);
    assert_eq!(receipt.decoded_eof_frames, Some(4));
}

#[test]
#[ignore = "actual native PCM fixture; run centrally with FFmpeg"]
fn last_upper_endpoint_clamps_and_first_two_channels_ignore_third() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("three.wav");
    wav(&path, 3, &[1000, -1000, 30000, 2000, -2000, 30000]);
    let backend = FfmpegBackend::discover().unwrap();
    let cancel = CancellationToken::new();
    let mut stage = stage(&backend, snapshot(3, 5, 1, 2), &path, &cancel);
    let got = stage.read_next(5, &cancel).unwrap().unwrap();
    // positions0,.5,1,1.5,2; upper of1.5 clamps to last, lower2 is0.
    for (frame, want) in got.samples.chunks_exact(2).zip([1000, 1500, 2000, 2000, 0]) {
        close(frame[0], want as f64 / 32768.0);
        close(frame[1], -(want as f64) / 32768.0);
    }
    assert_eq!(
        stage.decode_receipts()[0].channel_map,
        NativeChannelMap::FirstTwo
    );
}

#[test]
#[ignore = "actual native PCM fixture; run centrally with FFmpeg"]
fn independent_db_physical_gain_and_stable_multitrack_float_mix() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("constant.wav");
    wav(&path, 1, &[10000; 4]);
    let mut s = snapshot(1, 4, 1, 1);
    s.project.sequences[0].tracks[1].gain = Gain::Decibels { value: -100.0 };
    audio_mut(&mut s).gain = Gain::Decibels { value: 30.0 }; // physical20, amplitude10.
    let mut second = s.project.sequences[0].tracks[1].clone();
    second.id = "second".into();
    second.elements[0].id = "other".into();
    second.gain = Gain::LinearLegacy { value: 0.5 };
    if let ElementContent::Audio { audio, .. } = &mut second.elements[0].content {
        audio.gain = Gain::LinearLegacy { value: 1.0 };
    }
    s.project.sequences[0].tracks.push(second);
    let backend = FfmpegBackend::discover().unwrap();
    let cancel = CancellationToken::new();
    let mut stage = stage(&backend, s, &path, &cancel);
    let got = stage.read_next(4, &cancel).unwrap().unwrap();
    // first voice10000*10*.001=100; second10000*.5=5000; f32
    // accumulation rounds after the first voice, exactly as Float32Array does.
    let first = (100.0 / 32768.0) as f32;
    let expected = (f64::from(first) + 5000.0 / 32768.0) as f32;
    assert!(got.samples.iter().all(|v| *v == expected));
}

#[test]
#[ignore = "actual native PCM fixture; run centrally with FFmpeg"]
fn persistent_decode_cursor_crosses_source_and_output_block_boundaries() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("boundary.wav");
    let mut source = vec![1000i16; 4100];
    source[4095] = 2000;
    source[4096] = 4000;
    wav(&path, 1, &source);
    let backend = FfmpegBackend::discover().unwrap();
    let cancel = CancellationToken::new();
    let mut s = snapshot(1, 8200, 1, 2);
    audio_mut(&mut s).gain = Gain::Decibels { value: 0.0 };
    let key = |id: &str, time, value| ScalarKey {
        id: id.into(),
        segment_id: None,
        time,
        value,
        left_handle: None,
        right_handle: None,
        segment_to_next: ScalarSegment::Step,
        tangent_mode: TangentMode::Auto,
    };
    s.project.sequences[0].tracks[1].elements[0]
        .channels
        .push(AnimationChannel::Scalar {
            id: "step-db".into(),
            target: ChannelTarget::Element {
                element_id: "voice".into(),
            },
            binding: ScalarBinding::Property {
                parameter: ScalarParameter::GainDecibels,
            },
            keys: vec![
                key("before", t(0, 1), 0.0),
                key("after", t(8192, 48000), -20.0),
            ],
            extrapolation: Extrapolation {
                before: ExtrapolationMode::Hold,
                after: ExtrapolationMode::Hold,
            },
        });
    let mut a = stage(&backend, s.clone(), &path, &cancel);
    let mut b = stage(&backend, s, &path, &cancel);
    let mut wide = vec![];
    while let Some(block) = a.read_next(4096, &cancel).unwrap() {
        wide.extend(block.samples);
    }
    let mut split = vec![];
    while let Some(block) = b.read_next(997, &cancel).unwrap() {
        split.extend(block.samples);
    }
    assert_eq!(wide, split);
    for (sample, want) in [(8190, 2000), (8191, 3000), (8192, 400), (8193, 250)] {
        close(wide[sample * 2], want as f64 / 32768.0);
    }
    assert!(!cancel.is_cancelled()); // finished child Drop must not cancel job.
}

#[test]
#[ignore = "actual native PCM fixture; run centrally with FFmpeg"]
fn native_decode_budget_and_cancellation_stop_stage_without_published_output() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("budget.wav");
    wav(&path, 1, &[1000; 64]);
    let backend = FfmpegBackend::discover().unwrap();
    let cancel = CancellationToken::new();
    let p = prepare(snapshot(1, 64, 1, 1));
    let mut budget = limits();
    budget.max_decoded_bytes = 8;
    let mut limited =
        ProjectAudioAdapter::prepare_unmastered(&backend, p, budget, &cancel, |_| Ok(path.clone()))
            .unwrap();
    assert_eq!(
        limited.read_next(8, &cancel).unwrap_err().code,
        "MEDIA_OUTPUT_LIMIT_EXCEEDED"
    );
    assert_eq!(
        limited.read_next(8, &cancel).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    let job = CancellationToken::new();
    let mut cancelled = stage(&backend, snapshot(1, 64, 1, 1), &path, &job);
    job.cancel();
    assert_eq!(
        cancelled.read_next(1, &job).unwrap_err().code,
        "JOB_CANCELLED"
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
#[ignore = "actual native PCM fixture; run centrally with FFmpeg"]
fn maintain_pitch_is_rejected_before_resource_authorization_or_probe() {
    let mut s = snapshot(1, 4, 2, 1);
    if let ElementContent::Audio { retime, .. } =
        &mut s.project.sequences[0].tracks[1].elements[0].content
    {
        retime.maintain_pitch = true;
    }
    let backend = FfmpegBackend::discover().unwrap();
    let problem = match ProjectAudioAdapter::prepare_unmastered(
        &backend,
        prepare(s),
        limits(),
        &CancellationToken::new(),
        |_| panic!("unsupported pitch processor must not authorize resources"),
    ) {
        Err(problem) => problem,
        Ok(_) => panic!("pitch processor was silently replaced"),
    };
    assert_eq!(problem.code, "UNSUPPORTED_V4");
}

#[test]
#[ignore = "actual local-input policy fixture; run centrally with FFmpeg"]
fn remote_playlist_is_rejected_before_any_network_demux() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("external.m3u8");
    fs::write(&path,b"#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1,\nhttp://127.0.0.1:9/never-request.ts\n#EXT-X-ENDLIST\n").unwrap();
    let backend = FfmpegBackend::discover().unwrap();
    let problem = match ProjectAudioAdapter::prepare_unmastered(
        &backend,
        prepare(snapshot(1, 4, 1, 1)),
        limits(),
        &CancellationToken::new(),
        |_| Ok(path.clone()),
    ) {
        Err(p) => p,
        Ok(_) => panic!("playlist was accepted"),
    };
    assert_eq!(problem.code, "MEDIA_PROCESS_FAILED");
    assert!(problem.message.contains("whitelist"), "{problem}");
}

#[test]
#[ignore = "actual local-input policy fixture; run centrally with FFmpeg"]
fn local_manifest_cannot_read_a_second_unresolved_file() {
    let directory = tempfile::tempdir().unwrap();
    let outside = directory.path().join("unresolved.wav");
    wav(&outside, 1, &[1000; 4]);
    let path = directory.path().join("external.ffconcat");
    fs::write(&path, b"ffconcat version 1.0\nfile 'unresolved.wav'\n").unwrap();
    let backend = FfmpegBackend::discover().unwrap();
    let problem = match ProjectAudioAdapter::prepare_unmastered(
        &backend,
        prepare(snapshot(1, 4, 1, 1)),
        limits(),
        &CancellationToken::new(),
        |_| Ok(path.clone()),
    ) {
        Err(p) => p,
        Ok(_) => panic!("external file manifest was accepted"),
    };
    assert_eq!(problem.code, "MEDIA_PROCESS_FAILED");
    assert!(problem.message.contains("whitelist"), "{problem}");
}
