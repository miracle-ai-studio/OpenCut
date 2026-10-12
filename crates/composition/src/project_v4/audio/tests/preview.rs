use super::*;
fn preview(s: &ProjectSnapshotV4) -> PreparedPreviewAudioV4 {
    prepare_preview_audio_v4(
        s,
        "scene",
        PcmFormatV4 {
            sample_rate: 48000,
            channels: 2,
        },
    )
    .unwrap()
}
fn ordinary() -> PreviewSourceFormatV4 {
    PreviewSourceFormatV4 {
        element_id: "voice".into(),
        sample_rate: 120,
        frame_count: 40,
        channels: 1,
        prepared: false,
        segments: vec![
            PreviewAudioSegmentV4 {
                segment_id: 7,
                timestamp_seconds: 0.25,
                frame_count: 10,
            },
            PreviewAudioSegmentV4 {
                segment_id: 9,
                timestamp_seconds: 0.5,
                frame_count: 30,
            },
        ],
    }
}
#[test]
fn retained_binding_preserves_input_receipt_order_and_compiled_voice_order() {
    let mut s = snapshot();
    let mut second = s.project.sequences[0].tracks[1].clone();
    second.id = "second-track".into();
    second.elements[0].id = "second".into();
    s.project.sequences[0].tracks.push(second);
    let p = preview(&s);
    let first_format = ordinary();
    let second_format = PreviewSourceFormatV4 {
        element_id: "second".into(),
        ..ordinary()
    };
    let formats = [second_format, first_format];
    let bound = bind_preview_audio_v4(&p, &formats).unwrap();
    assert!(bound.source_formats().eq(formats.iter()));
    let block = compile_preview_audio_block_v4(&bound, 18000, 4096, 0, None).unwrap();
    assert_eq!(
        block
            .voices
            .iter()
            .map(|v| v.element_id.as_str())
            .collect::<Vec<_>>(),
        ["voice", "second"]
    );
    assert_eq!(block.voices[0].positions, block.voices[1].positions);
}
#[test]
fn constant_gain_has_independent_full_block_oracles_across_overlay_and_anchor_changes() {
    for db in [-60.0, -6.0, 0.0, 20.0] {
        let mut s = snapshot();
        audio_mut(&mut s).gain = GainV4::Decibels { value: db };
        let track = &mut s.project.sequences[0].tracks[1];
        track.gain = GainV4::Decibels { value: -6.0 };
        let p = preview(&s);
        let bound = bind_preview_audio_v4(&p, &[ordinary()]).unwrap();
        let expected = (10f64.powf(db / 20.0) * 10f64.powf(-6.0 / 20.0)).to_bits();
        for anchor in [0, 18000] {
            let block = compile_preview_audio_block_v4(&bound, 18000, 4096, anchor, None).unwrap();
            assert!(
                block.voices[0]
                    .amplitudes
                    .iter()
                    .all(|a| a.to_bits() == expected)
            );
            let overlay = p.validate_temporary_gain("sound", "voice", -60).unwrap();
            let changed =
                compile_preview_audio_block_v4(&bound, 18000, 4096, anchor, Some(&overlay))
                    .unwrap();
            let expected = (10f64.powf(-6.0 / 20.0) * 10f64.powf(-6.0 / 20.0)).to_bits();
            assert!(
                changed.voices[0]
                    .amplitudes
                    .iter()
                    .all(|a| a.to_bits() == expected)
            );
            assert_eq!(changed.voices[0].positions, block.voices[0].positions);
        }
    }
}
#[test]
fn pcm_packet_pts_accumulation_survives_without_rewriting_observed_timestamps() {
    // Independent of the producer: WAVE packets have 2048 frames and the
    // pinned PCM decoder accumulates their duration. Packet 5 differs from
    // division of its integer source index by one f64 bit.
    let p = preview(&snapshot());
    let mut timestamp = 0.0;
    let mut segments = vec![];
    for segment_id in 0..94 {
        let frame_count = if segment_id == 93 { 1536 } else { 2048 };
        segments.push(PreviewAudioSegmentV4 {
            segment_id,
            timestamp_seconds: timestamp,
            frame_count,
        });
        timestamp += frame_count as f64 / 48_000.0;
    }
    assert_ne!(segments[5].timestamp_seconds, 10_240.0 / 48_000.0);
    let mut format = PreviewSourceFormatV4 {
        element_id: "voice".into(),
        sample_rate: 48_000,
        frame_count: 192_000,
        channels: 1,
        prepared: false,
        segments,
    };
    let observed = serde_json::to_string(&format).unwrap();
    let transported: PreviewSourceFormatV4 = serde_json::from_str(&observed).unwrap();
    assert!(bind_preview_audio_v4(&p, &[transported]).is_ok());
    let formats = [format.clone()];
    let bound = bind_preview_audio_v4(&p, &formats).unwrap();
    // Fixed fixture: trim=.25s, rate=3/2, placement=18000/48000.
    // At output22288 native source18432 starts packet9. Accumulated PTS must
    // not create a one-sample false gap at this exact packet boundary.
    let before = compile_preview_audio_block_v4(&bound, 22287, 1, 0, None).unwrap();
    let point = before.voices[0].positions[0].as_ref().unwrap();
    assert_eq!((point.segment_id, point.lower), (8, 2046));
    close(point.fraction, 0.5);
    let after = compile_preview_audio_block_v4(&bound, 22288, 1, 0, None).unwrap();
    let point = after.voices[0].positions[0].as_ref().unwrap();
    assert_eq!((point.segment_id, point.lower, point.fraction), (9, 0, 0.0));
    assert_eq!(serde_json::to_string(&format).unwrap(), observed);
    // A thousandth of a sample is a real off-grid observation, not rounding.
    format.segments[5].timestamp_seconds += 0.001 / 48_000.0;
    assert!(bind_preview_audio_v4(&p, &[format]).is_err());
}
#[test]
fn adjacent_native_sample_grid_pts_survive_ieee_rounding_without_accepting_overlap() {
    // Independent IEEE counterexample: the decoder emits n / nativeSR for each
    // chunk, whereas adding the preceding chunk duration rounds differently.
    let p = preview(&snapshot());
    let mut format = PreviewSourceFormatV4 {
        element_id: "voice".into(),
        sample_rate: 10,
        frame_count: 2,
        channels: 1,
        prepared: false,
        segments: vec![
            PreviewAudioSegmentV4 {
                segment_id: 1,
                timestamp_seconds: 0.2,
                frame_count: 1,
            },
            PreviewAudioSegmentV4 {
                segment_id: 2,
                timestamp_seconds: 0.3,
                frame_count: 1,
            },
        ],
    };
    assert!(
        format.segments[0].timestamp_seconds
            + format.segments[0].frame_count as f64 / f64::from(format.sample_rate)
            > format.segments[1].timestamp_seconds
    );
    let original = serde_json::to_value(&format).unwrap();
    let bound = bind_preview_audio_v4(&p, &[format.clone()]).unwrap();
    assert_eq!(serde_json::to_value(&format).unwrap(), original);
    let block = compile_preview_audio_block_v4(&bound, 18000, 1, 0, None).unwrap();
    let point = block.voices[0].positions[0].as_ref().unwrap();
    assert_eq!((point.segment_id, point.lower), (1, 0));
    close(point.fraction, 0.5);

    format.segments[1].timestamp_seconds = 0.2;
    assert_eq!(
        bind_preview_audio_v4(&p, &[format.clone()])
            .err()
            .unwrap()
            .code,
        "UNSUPPORTED_V4"
    );
    format.segments[1].timestamp_seconds = 0.31;
    assert!(bind_preview_audio_v4(&p, &[format.clone()]).is_err());
    // Negative native PTS are legitimate observations, not unsigned clocks.
    format.segments[0].timestamp_seconds = -0.2;
    format.segments[1].timestamp_seconds = -0.1;
    assert!(bind_preview_audio_v4(&p, &[format.clone()]).is_ok());
    format.segments[0].timestamp_seconds = f64::MAX;
    assert!(bind_preview_audio_v4(&p, &[format.clone()]).is_err());
    format.segments[0].timestamp_seconds = 0.2;
    format.segments[0].frame_count = u64::MAX;
    assert!(bind_preview_audio_v4(&p, &[format.clone()]).is_err());
    format.segments[0].frame_count = 9_007_199_254_740_991;
    assert!(bind_preview_audio_v4(&p, &[format]).is_err());
}
#[test]
fn ordinary_native_pts_gap_and_fractional_trim_have_independent_sample_oracles() {
    let s = snapshot();
    let before = serde_json::to_value(&s).unwrap();
    let p = preview(&s);
    let bound = bind_preview_audio_v4(&p, &[ordinary()]).unwrap();
    let first = compile_preview_audio_block_v4(&bound, 18000, 1, 0, None).unwrap();
    let point = first.voices[0].positions[0].as_ref().unwrap();
    assert_eq!((point.segment_id, point.lower, point.upper), (7, 0, 1));
    let half = compile_preview_audio_block_v4(&bound, 18200, 1, 0, None).unwrap();
    let point = half.voices[0].positions[0].as_ref().unwrap();
    assert_eq!(point.lower, 0);
    close(point.fraction, 0.75);
    assert!(
        compile_preview_audio_block_v4(&bound, 22000, 1, 0, None)
            .unwrap()
            .voices[0]
            .positions[0]
            .is_none()
    );
    let next = compile_preview_audio_block_v4(&bound, 26000, 1, 0, None).unwrap();
    assert_eq!(next.voices[0].positions[0].as_ref().unwrap().segment_id, 9);
    let gain = p.validate_temporary_gain("sound", "voice", -60).unwrap();
    let changed = compile_preview_audio_block_v4(&bound, 18000, 1, 0, Some(&gain)).unwrap();
    close(changed.voices[0].amplitudes[0], 10f64.powf(-6.0 / 20.0));
    assert_eq!(changed.voices[0].positions, first.voices[0].positions);
    assert_eq!(serde_json::to_value(&s).unwrap(), before);
    let json = serde_json::to_value(changed).unwrap();
    assert_eq!(json["startSample"], "18000");
    assert_eq!(
        json["identity"]["profile"],
        CLASSIC_REALTIME_AUDIO_PROFILE_V4
    );
}
#[test]
fn sixty_hz_interpolates_amplitude_and_keeps_sequence_and_element_clocks_across_blocks() {
    let mut s = snapshot();
    let track = &mut s.project.sequences[0].tracks[1];
    track.elements[0].channels = vec![channel(
        false,
        vec![
            key("e0", t(0, 120000), -20.0),
            key("e1", t(2000, 120000), 0.0),
        ],
    )];
    track.channels = vec![channel(
        true,
        vec![
            key("t0", t(18000, 48000), 0.0),
            key("t1", t(18800, 48000), -20.0),
        ],
    )];
    let p = preview(&s);
    assert_eq!(
        p.audio().voices().next().unwrap().processing,
        AudioProcessingV4::ClassicPreparedLinear
    );
    let f = PreviewSourceFormatV4 {
        element_id: "voice".into(),
        sample_rate: 48000,
        frame_count: 24000,
        channels: 1,
        prepared: true,
        segments: vec![],
    };
    let bound = bind_preview_audio_v4(&p, &[f]).unwrap();
    let a = compile_preview_audio_block_v4(&bound, 18000, 401, 18000, None).unwrap();
    close(a.voices[0].amplitudes[0], 0.1);
    close(a.voices[0].amplitudes[400], 0.55 * 0.55);
    // Independent oracle: linear interpolation of 0.1 and 1 gives 0.55,
    // whereas dB interpolation would give sqrt(0.1), a different waveform.
    assert!((a.voices[0].amplitudes[400] - 0.1).abs() > 0.1);
    let b = compile_preview_audio_block_v4(&bound, 18400, 1, 18000, None).unwrap();
    close(b.voices[0].amplitudes[0], a.voices[0].amplitudes[400]);
    assert_eq!(b.voices[0].positions[0].as_ref().unwrap().lower, 400);
    assert!(p.validate_temporary_gain("sound", "voice", -60).is_err());
    let info = p.audio().voices().next().unwrap();
    assert_eq!(info.source.clock.nominal_request.start, t(1, 4));
}
#[test]
fn static_overlay_never_enables_muted_or_source_disabled_voices_and_binding_refuses_ambiguity() {
    let mut s = snapshot();
    s.project.sequences[0].tracks[1].muted = true;
    let p = preview(&s);
    assert_eq!(p.audio().voices().count(), 0);
    let gain = p.validate_temporary_gain("sound", "voice", 200).unwrap();
    let bound = bind_preview_audio_v4(&p, &[]).unwrap();
    assert!(
        compile_preview_audio_block_v4(&bound, 18000, 1, 0, Some(&gain))
            .unwrap()
            .voices
            .is_empty()
    );
    s.project.sequences[0].tracks[1].muted = false;
    s.project.sequences[0].tracks[1].locked = true;
    assert!(
        preview(&s)
            .validate_temporary_gain("sound", "voice", 0)
            .is_err()
    );
    s.project.sequences[0].tracks[1].locked = false;
    let p = preview(&s);
    let mut invalid = ordinary();
    invalid.segments[1].timestamp_seconds = 0.3;
    assert!(bind_preview_audio_v4(&p, &[invalid]).is_err());
    assert!(bind_preview_audio_v4(&p, &[ordinary(), ordinary()]).is_err());
    assert!(p.validate_temporary_gain("sound", "voice", 201).is_err());
    as_video(&mut s);
    let ElementContentV4::Video {
        source_audio_enabled,
        ..
    } = &mut s.project.sequences[0].tracks[1].elements[0].content
    else {
        panic!()
    };
    *source_audio_enabled = false;
    assert_eq!(preview(&s).audio().voices().count(), 0);
}
