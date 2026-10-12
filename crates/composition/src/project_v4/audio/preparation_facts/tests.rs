use super::*;

fn t(n: i64, d: u32) -> Time {
    RationalTime::new(n, d).unwrap().into()
}
fn input() -> ClassicAudioPreparationInputV4 {
    ClassicAudioPreparationInputV4 {
        source_start: t(1, 3),
        timeline_duration: t(1, 7),
        rate: RationalRateV4 {
            numerator: 3,
            denominator: 2,
        },
        maintain_pitch: true,
        decoded_sample_rate: 44100,
        decoded_frames: 15000,
        decoded_channels: 3,
        target_sample_rate: 48000,
    }
}
fn budget() -> ClassicAudioPreparationBudgetV4 {
    ClassicAudioPreparationBudgetV4 {
        max_prefix_frames: 100000,
        max_source_frames: 20000,
        max_output_frames: 20000,
        max_total_channel_frames: 100000,
    }
}

#[test]
fn actual_timeline_duration_controls_native_and_resampled_counts_without_placement_roundtrip() {
    let input = input();
    let facts = classic_audio_preparation_facts_v4(&input, &budget()).unwrap();
    assert_eq!(
        facts.mode,
        ClassicAudioPreparationModeV4::PreparedSoundtouch
    );
    assert_eq!(facts.source_slice_start_sample, 14700);
    assert_eq!(facts.requested_source_frames, 9450);
    assert_eq!(facts.actual_source_frames, 300);
    assert_eq!(facts.resampled_frames, 10286);
    assert_eq!(facts.output_frames, 6858);
    assert_eq!(facts.prefix_frames, 24150);
    assert_eq!(facts.num_channels, 2);
    assert_eq!(facts.total_channel_frames, 79888);
    assert_eq!(facts.decoded_bytes, 180000);
    // Three prepared buffers, two channels, four bytes per Float32 sample.
    assert_eq!(facts.prepared_bytes, (300 + 10286 + 6858) * 2 * 4);
    assert_eq!(facts.total_bytes, 180000 + 139552);
    let mut rounded = input.clone();
    rounded.timeline_duration = t(6858, 48000);
    let different = classic_audio_preparation_facts_v4(&rounded, &budget()).unwrap();
    assert_eq!(different.requested_source_frames, 9452);
    assert_eq!(different.resampled_frames, 10287);
    let wire = serde_json::to_value(&facts).unwrap();
    assert_eq!(wire["mode"], "prepared-soundtouch");
    assert_eq!(
        wire["tempo"],
        serde_json::json!({"numerator":"3","denominator":"2"})
    );
    assert_eq!(wire["sourceSliceStartSample"], "14700");
    assert_eq!(
        serde_json::from_value::<ClassicAudioPreparationFactsV4>(wire).unwrap(),
        facts
    );
    let input_wire = serde_json::to_value(&input).unwrap();
    assert_eq!(input_wire["rate"]["numerator"], "3");
    assert_eq!(
        serde_json::from_value::<ClassicAudioPreparationInputV4>(input_wire).unwrap(),
        input
    );
}

#[test]
fn pitch_preparation_has_three_modes_and_exact_epsilon_and_clamp_boundaries() {
    for (rate, expected) in [
        (
            RationalRateV4 {
                numerator: 1,
                denominator: 1,
            },
            ClassicAudioPreparationModeV4::PreparedLinear,
        ),
        (
            RationalRateV4 {
                numerator: 1000001,
                denominator: 1000000,
            },
            ClassicAudioPreparationModeV4::PreparedLinear,
        ),
        (
            RationalRateV4 {
                numerator: 999999,
                denominator: 1000000,
            },
            // Independent Classic JS predicate: abs(0.999999-1)>1e-6.
            ClassicAudioPreparationModeV4::PreparedSoundtouch,
        ),
        (
            RationalRateV4 {
                numerator: 1000002,
                denominator: 1000000,
            },
            ClassicAudioPreparationModeV4::PreparedSoundtouch,
        ),
        (
            RationalRateV4 {
                numerator: 999998,
                denominator: 1000000,
            },
            ClassicAudioPreparationModeV4::PreparedSoundtouch,
        ),
    ] {
        assert_eq!(
            classic_pitch_processing_mode_v4(rate, true).unwrap(),
            expected
        );
        assert_eq!(
            classic_pitch_processing_mode_v4(rate, false).unwrap(),
            ClassicAudioPreparationModeV4::UnpreparedLinear
        );
        let mut sample = input();
        sample.rate = rate;
        sample.decoded_channels = 1;
        sample.decoded_frames = 50000;
        let facts = classic_audio_preparation_facts_v4(&sample, &budget()).unwrap();
        assert_eq!(facts.mode, expected);
        assert!(facts.output_frames > 0);
    }
    assert_eq!(
        classic_effective_rate_v4(RationalRateV4 {
            numerator: 1,
            denominator: 1000
        })
        .unwrap(),
        RationalRateV4 {
            numerator: 1,
            denominator: 100
        }
    );
    assert_eq!(
        classic_effective_rate_v4(RationalRateV4 {
            numerator: 600,
            denominator: 100
        })
        .unwrap(),
        RationalRateV4 {
            numerator: 5,
            denominator: 1
        }
    );
    assert_eq!(
        classic_effective_rate_v4(RationalRateV4 {
            numerator: u32::MAX,
            denominator: u32::MAX
        })
        .unwrap(),
        RationalRateV4 {
            numerator: 1,
            denominator: 1
        }
    );
}

#[test]
fn empty_available_source_is_one_zero_frame_fact_and_negative_trim_clamps_to_zero() {
    let mut sample = input();
    sample.source_start = t(1, 1);
    sample.decoded_frames = 10;
    let facts = classic_audio_preparation_facts_v4(&sample, &budget()).unwrap();
    assert!(facts.source_slice_start_sample > sample.decoded_frames);
    assert_eq!(facts.actual_source_frames, 1);
    sample.source_start = t(-1, 2);
    sample.decoded_frames = 0;
    let facts = classic_audio_preparation_facts_v4(&sample, &budget()).unwrap();
    assert_eq!(facts.source_slice_start_sample, 0);
    assert_eq!(facts.actual_source_frames, 1);
    assert_eq!(facts.decoded_bytes, 0);
}

#[test]
fn every_budget_and_integer_overflow_fail_before_buffer_allocation() {
    let sample = input();
    for bounded in [
        ClassicAudioPreparationBudgetV4 {
            max_prefix_frames: 24149,
            ..budget()
        },
        ClassicAudioPreparationBudgetV4 {
            max_source_frames: 10285,
            ..budget()
        },
        ClassicAudioPreparationBudgetV4 {
            max_output_frames: 6857,
            ..budget()
        },
        ClassicAudioPreparationBudgetV4 {
            max_total_channel_frames: 79887,
            ..budget()
        },
    ] {
        assert!(classic_audio_preparation_facts_v4(&sample, &bounded).is_err());
    }
    let mut huge = sample.clone();
    huge.source_start = t(i64::MAX, 1);
    huge.decoded_sample_rate = u32::MAX;
    assert!(classic_audio_preparation_facts_v4(&huge, &budget()).is_err());
    huge = sample.clone();
    huge.timeline_duration = t(i64::MAX, 1);
    huge.target_sample_rate = u32::MAX;
    assert!(classic_audio_preparation_facts_v4(&huge, &budget()).is_err());
    huge = sample.clone();
    huge.decoded_frames = u64::MAX;
    huge.decoded_channels = u32::MAX;
    assert!(classic_audio_preparation_facts_v4(&huge, &budget()).is_err());
    let mut invalid_input = sample;
    invalid_input.rate.denominator = 0;
    assert!(classic_audio_preparation_facts_v4(&invalid_input, &budget()).is_err());
}
