use super::*;
mod budget;
mod fixture;
mod preview;
mod runtime_pcm;
use fixture::channel;
use fixture::*;

#[test]
fn classic_audio_uses_effective_tempo_and_preserves_authored_duration_and_request() {
    let mut s = snapshot();
    s.project.assets[0].duration = Some(t(10, 1));
    let e = &mut s.project.sequences[0].tracks[1].elements[0];
    e.source_span = ElementSourceSpanV4::ClassicTrimmed {
        trim_start: t(1, 4),
        trim_end: t(25, 4),
        source_duration: Some(t(10, 1)),
    };
    let ElementContentV4::Audio { retime, .. } = &mut e.content else {
        panic!()
    };
    retime.rate = RationalRateV4 {
        numerator: 7,
        denominator: 1,
    };
    let p = prepare(&s, 4);
    let info = p.voices().next().unwrap();
    assert_eq!(*info.timeline_duration, t(1, 2));
    assert_eq!(info.source.clock.rate.numerator, 7);
    assert_eq!(
        info.source.effective_rate,
        RationalRateV4 {
            numerator: 5,
            denominator: 1
        }
    );
    // .25s trim plus one .25s output sample at effective5x -> source8Hz index12.
    assert_eq!(info.source.position_at(1, 8).unwrap().lower, 12);
}

#[test]
fn classic_unit_rate_keep_pitch_requires_prepared_float32_roundtrip() {
    let mut s = snapshot();
    let e = &mut s.project.sequences[0].tracks[1].elements[0];
    e.source_span = ElementSourceSpanV4::ClassicTrimmed {
        trim_start: t(1, 4),
        trim_end: t(5, 4),
        source_duration: Some(t(2, 1)),
    };
    let ElementContentV4::Audio { retime, .. } = &mut e.content else {
        panic!()
    };
    retime.rate = RationalRateV4 {
        numerator: 1,
        denominator: 1,
    };
    retime.maintain_pitch = true;
    let p = prepare(&s, 4);
    assert!(p.voices().next().unwrap().maintain_pitch);
    assert_eq!(
        p.voices().next().unwrap().processing,
        AudioProcessingV4::ClassicPreparedLinear
    );
}

#[test]
fn fractional_placement_rate_and_nonzero_trim_have_independent_exact_positions() {
    let s = snapshot();
    let p = prepare(&s, 4);
    let b = compile_audio_block_v4(&p, 0, 4).unwrap();
    assert_eq!(b.identity.revision, "fixed-audio-r1");
    assert_eq!(b.voices.len(), 1);
    let v = &b.voices[0];
    // start=3/8s -> floor(1.5)=1; duration=1/2s -> ceil(2)=2.
    assert_eq!(
        (
            v.placement_start_sample,
            v.placement_frame_count,
            v.block_offset,
            v.frame_count
        ),
        (1, 2, 1, 2)
    );
    assert_eq!(v.local_start_sample, 0);
    // trim=1/4s, rate=3/2, source8Hz: positions 2 and 5 exactly.
    assert_eq!(
        v.source.position_at(0, 8).unwrap(),
        ExactSourceSampleV4 {
            lower: 2,
            fraction_numerator: 0,
            fraction_denominator: 1
        }
    );
    assert_eq!(
        v.source.position_at(1, 8).unwrap(),
        ExactSourceSampleV4 {
            lower: 5,
            fraction_numerator: 0,
            fraction_denominator: 1
        }
    );
    assert_eq!(
        v.source.position_at(1, 10).unwrap(),
        ExactSourceSampleV4 {
            lower: 6,
            fraction_numerator: 1,
            fraction_denominator: 4
        }
    );
    assert_eq!(v.processing, AudioProcessingV4::LinearInterpolationZeroTail);
    assert!(!v.probe_required);
}

#[test]
fn sample_gain_uses_local_quantized_origin_and_is_invariant_across_blocks() {
    let mut s = snapshot();
    s.project.sequences[0].tracks[1].elements[0]
        .channels
        .push(channel(
            false,
            vec![key("a", t(0, 1), 0.0), key("b", t(1, 2), 20.0)],
        ));
    s.project.sequences[0].tracks[1].gain = GainV4::Decibels { value: -20.0 };
    let p = prepare(&s, 4);
    let whole = compile_audio_block_v4(&p, 0, 4).unwrap();
    let left = compile_audio_block_v4(&p, 1, 1).unwrap();
    let right = compile_audio_block_v4(&p, 2, 1).unwrap();
    let want = vec![0.1, 10.0_f64.sqrt() * 0.1];
    let SampleGainV4::Dense { amplitudes } = &whole.voices[0].gain else {
        panic!()
    };
    for (a, b) in amplitudes.iter().zip(&want) {
        close(*a, *b);
    }
    let SampleGainV4::Dense { amplitudes: a } = &left.voices[0].gain else {
        panic!()
    };
    let SampleGainV4::Dense { amplitudes: b } = &right.voices[0].gain else {
        panic!()
    };
    assert_eq!([a.clone(), b.clone()].concat(), *amplitudes);
    assert_eq!(right.voices[0].local_start_sample, 1);
}

#[test]
fn track_gain_uses_sequence_clock_and_db_zero_is_distinct_from_linear_zero() {
    let mut s = snapshot();
    s.project.sequences[0].tracks[1].channels.push(channel(
        true,
        vec![key("a", t(0, 1), 0.0), key("b", t(1, 2), 20.0)],
    ));
    let b = compile_audio_block_v4(&prepare(&s, 4), 1, 1).unwrap();
    let SampleGainV4::Dense { amplitudes } = &b.voices[0].gain else {
        panic!()
    };
    close(amplitudes[0], 10.0_f64.sqrt()); // absolute1/4s, not local0.
    s.project.sequences[0].tracks[1].channels.clear();
    let b = compile_audio_block_v4(&prepare(&s, 4), 1, 1).unwrap();
    assert_eq!(b.voices[0].gain, SampleGainV4::Constant { amplitude: 1.0 });
    audio_mut(&mut s).gain = GainV4::LinearLegacy { value: 0.0 };
    let b = compile_audio_block_v4(&prepare(&s, 4), 1, 1).unwrap();
    assert_eq!(b.voices[0].gain, SampleGainV4::Constant { amplitude: 0.0 });
}

#[test]
fn classic_animation_half_tick_rounds_up_and_gain_physical_bounds_are_shared() {
    let mut s = snapshot();
    s.project.sequences[0].tracks[1].elements[0]
        .channels
        .push(channel(
            false,
            vec![key("a", t(0, 120000), 0.0), key("b", t(1, 120000), 20.0)],
        ));
    let p = prepare(&s, 240000);
    let b = compile_audio_block_v4(&p, 90001, 1).unwrap();
    let SampleGainV4::Dense { amplitudes } = &b.voices[0].gain else {
        panic!()
    };
    close(amplitudes[0], 10.0); // local1/240000 -> round(.5 tick)=1.
    s.project.sequences[0].tracks[1].elements[0]
        .channels
        .clear();
    audio_mut(&mut s).gain = GainV4::Decibels { value: 50.0 };
    s.project.sequences[0].tracks[1].gain = GainV4::Decibels { value: -100.0 };
    let b = compile_audio_block_v4(&prepare(&s, 4), 1, 1).unwrap();
    assert_eq!(b.voices[0].gain, SampleGainV4::Constant { amplitude: 0.01 });
}

#[test]
fn mute_and_disabled_precede_probe_and_unknown_is_never_confirmed_absence() {
    let mut s = snapshot();
    s.project.assets[0].metadata.audio = AudioPresenceV4::Unknown {};
    s.project.assets[0].duration = None;
    audio_mut(&mut s).muted = true;
    let p = prepare(&s, 4);
    assert_eq!(p.voices().count(), 0);
    assert_eq!(p.receipts()[0].decision, AudioParticipationV4::Muted);
    audio_mut(&mut s).muted = false;
    let p = prepare(&s, 4);
    assert_eq!(
        p.receipts()[0].decision,
        AudioParticipationV4::RequiresProbe
    );
    let b = compile_audio_block_v4(&p, 1, 1).unwrap();
    assert!(b.voices[0].probe_required);
    assert_eq!(
        b.voices[0].source.clock.read_window,
        BoundedReadWindowV4::UnknownResourceExtent
    );
    // Known-absent audio-only nodes are structurally invalid; the valid
    // participation case is a video with a confirmed silent source.
    as_video(&mut s);
    s.project.assets[0].metadata.audio = AudioPresenceV4::Absent {};
    assert_eq!(
        prepare(&s, 4).receipts()[0].decision,
        AudioParticipationV4::KnownAbsent
    );
    s.project.sequences[0].tracks[1].enabled = false;
    assert_eq!(
        prepare(&s, 4).receipts()[0].decision,
        AudioParticipationV4::DisabledTrack
    );
}

#[test]
fn inverse_rounded_requested_extent_is_not_a_bounded_decoder_window() {
    let mut s = snapshot();
    s.project.assets[0].duration = Some(t(120001, 120000));
    let e = &mut s.project.sequences[0].tracks[1].elements[0];
    e.timeline_range.duration = t(60001, 120000);
    e.source_span = ElementSourceSpanV4::ClassicTrimmed {
        trim_start: t(0, 1),
        trim_end: t(0, 1),
        source_duration: Some(t(120001, 120000)),
    };
    if let ElementContentV4::Audio { retime, .. } = &mut e.content {
        retime.rate = RationalRateV4 {
            numerator: 2,
            denominator: 1,
        };
    }
    let p = prepare(&s, 240000);
    let info = p.voices().next().unwrap();
    assert_eq!(info.placement_frame_count, 120002);
    assert_eq!(
        time(&info.source.clock.nominal_request.duration)
            .unwrap()
            .cmp_exact(time(&t(120002, 120000)).unwrap()),
        Ordering::Equal
    );
    let BoundedReadWindowV4::Known {
        range: Some(window),
        ..
    } = &info.source.clock.read_window
    else {
        panic!()
    };
    assert_eq!(
        end(window)
            .unwrap()
            .cmp_exact(time(&t(120001, 120000)).unwrap()),
        Ordering::Equal
    );
    // Last actual output sample reaches the decoded length (120001 frames at
    // 120000 Hz). The explicit native linear profile emits zero there, rather
    // than reading the longer nominal request or holding the previous sample.
    assert_eq!(
        info.source.position_at(120001, 120000).unwrap().lower,
        120001
    );
}

#[test]
fn pitch_requirement_and_legacy_profile_are_explicit_and_never_silently_substituted() {
    let mut s = snapshot();
    if let ElementContentV4::Audio { retime, .. } =
        &mut s.project.sequences[0].tracks[1].elements[0].content
    {
        retime.maintain_pitch = true;
    }
    assert_eq!(
        prepare(&s, 4).voices().next().unwrap().processing,
        AudioProcessingV4::NativeMaintainPitch
    );
    if let ElementContentV4::Audio { retime, .. } =
        &mut s.project.sequences[0].tracks[1].elements[0].content
    {
        retime.maintain_pitch = false;
    }
    let f = PcmFormatV4 {
        sample_rate: 48000,
        channels: 2,
    };
    let p = prepare_audio_v4(&s, "scene", AudioExecutionProfileV4::LegacyFfmpegV3, f).unwrap();
    let info = p.voices().next().unwrap();
    assert_eq!(info.source.legacy_retime_sample_rate, Some(72000));
    assert_eq!(info.processing, AudioProcessingV4::LegacyFfmpegResample);
    assert!(info.source.position_at(0, 48000).is_err());
    s.project.sequences[0].tracks[1].elements[0]
        .channels
        .push(channel(false, vec![key("a", t(0, 1), 0.0)]));
    assert_eq!(
        prepare_audio_v4(&s, "scene", AudioExecutionProfileV4::LegacyFfmpegV3, f)
            .err()
            .unwrap()
            .code,
        "UNSUPPORTED_V4"
    );
}

#[test]
fn block_admission_and_exact_u64_source_boundaries_do_not_wrap() {
    let p = prepare(&snapshot(), 4);
    assert!(compile_audio_block_v4(&p, 0, 0).is_err());
    assert!(compile_audio_block_v4(&p, 0, 4097).is_err());
    assert!(compile_audio_block_v4(&p, u64::MAX - 1, 2).is_err());
    assert!(
        compile_audio_block_v4(&p, u64::MAX - 1, 1)
            .unwrap()
            .voices
            .is_empty()
    );
    let mut mapping = p.voices().next().unwrap().source.clone();
    mapping.clock.nominal_request.start = t(0, 1);
    mapping.clock.rate = RationalRateV4 {
        numerator: 1,
        denominator: 1,
    };
    mapping.effective_rate = mapping.clock.rate;
    mapping.output_sample_rate = 1;
    assert_eq!(mapping.position_at(u64::MAX, 1).unwrap().lower, u64::MAX);
    mapping.clock.nominal_request.start = t(1, 1);
    assert!(mapping.position_at(u64::MAX, 1).is_err());
}

#[test]
fn overlapping_voice_budget_is_admitted_before_dense_allocation_and_order_is_stable() {
    let mut s = snapshot();
    let original = s.project.sequences[0].tracks[1].clone();
    for index in 0..16 {
        let mut track = original.clone();
        track.id = format!("sound-{index}");
        track.elements[0].id = format!("voice-{index}");
        s.project.sequences[0].tracks.push(track);
    }
    let p = prepare(&s, 48000);
    let b = compile_audio_block_v4(&p, 18000, 1).unwrap();
    assert_eq!(b.voices[0].element_id, "voice");
    assert_eq!(b.voices[1].element_id, "voice-0");
    assert_eq!(b.voices[16].element_id, "voice-15");
    // 17 simultaneous voices * 4096 samples exceeds the 65536 sample budget.
    assert!(compile_audio_block_v4(&p, 18000, 4096).is_err());
}

#[test]
fn legacy_linear_unit_interpolates_amplitude_without_a_db_reinterpretation() {
    let mut s = snapshot();
    audio_mut(&mut s).gain = GainV4::LinearLegacy { value: 0.0 };
    let mut ch = channel(false, vec![key("a", t(0, 1), 0.0), key("b", t(1, 2), 1.0)]);
    if let AnimationChannelV4::Scalar { binding, .. } = &mut ch {
        *binding = ScalarBindingV4::Property {
            parameter: ScalarParameterV4::GainLinearLegacy,
        };
    }
    s.project.sequences[0].tracks[1].elements[0]
        .channels
        .push(ch);
    let b = compile_audio_block_v4(&prepare(&s, 4), 1, 2).unwrap();
    assert_eq!(
        b.voices[0].gain,
        SampleGainV4::Dense {
            amplitudes: vec![0.0, 0.5]
        }
    );
}

#[test]
fn independent_longer_captions_preserve_scene_silence_even_without_audio_elements() {
    let mut s = snapshot();
    let mut caption_track = s.project.sequences[0].tracks[1].clone();
    caption_track.id = "captions".into();
    caption_track.kind = TrackKindV4::Caption;
    caption_track.elements.clear();
    s.project.sequences[0].tracks.push(caption_track);
    s.project.sequences[0].captions.push(CaptionV4 {
        id: "caption".into(),
        track_id: "captions".into(),
        timeline_range: TimeRange {
            start: t(5, 4),
            duration: t(1, 2),
        },
        text: "silent tail".into(),
        source_anchor: None,
        style: CaptionStyleV4 {
            font: FontBindingV4::UnresolvedFamily {
                family: "sans-serif".into(),
                policy: opencut_contracts::FontResolutionPolicy::Strict {},
            },
            font_size: FontSizeV4::CanvasPixels { value: 18.0 },
            color: ColorV4 {
                encoding: ColorEncodingV4::Srgb,
                red: 1.0,
                green: 1.0,
                blue: 1.0,
                alpha: 1.0,
            },
            background_color: None,
            max_width_px: None,
            line_height_px: None,
            text_align: TextAlignV4::Center,
            layout: CaptionLayoutProfileV4::LegacyCaptionV2,
        },
    });
    // Caption end7/4s is later than voice end7/8s: retain7 outputframes at4Hz.
    let p = prepare(&s, 4);
    assert_eq!(p.duration_samples(), 7);
    assert!(compile_audio_block_v4(&p, 5, 2).unwrap().voices.is_empty());
    s.project.sequences[0].tracks[1].elements.clear();
    let p = prepare(&s, 4);
    assert_eq!(p.duration_samples(), 7);
    assert_eq!(p.voices().count(), 0);
}
