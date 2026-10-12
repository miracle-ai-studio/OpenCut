use super::*;

fn spans(intervals: &[(u64, u64)]) -> ProjectSnapshotV4 {
    let mut s = snapshot();
    let original = s.project.sequences[0].tracks[1].clone();
    s.project.sequences[0].tracks.truncate(1);
    for (index, &(start, length)) in intervals.iter().enumerate() {
        let mut track = original.clone();
        track.id = format!("sound-{index}");
        let element = &mut track.elements[0];
        element.id = format!("voice-{index}");
        element.timeline_range = TimeRange {
            start: t(start as i64, 48_000),
            duration: t(length as i64, 48_000),
        };
        s.project.sequences[0].tracks.push(track);
    }
    s
}

fn preview(s: &ProjectSnapshotV4) -> Result<PreparedPreviewAudioV4, CompositionError> {
    prepare_preview_audio_v4(
        s,
        "scene",
        PcmFormatV4 {
            sample_rate: 48_000,
            channels: 2,
        },
    )
}

#[test]
fn hundred_sequential_voices_bind_and_partition_without_dropping_samples() {
    let s = spans(&(0..100).map(|i| (i * 100, 100)).collect::<Vec<_>>());
    let p = prepare(&s, 48_000);
    let realtime = preview(&s).unwrap();
    assert_eq!(p.voices().count(), 100);
    assert_eq!(p.block_frame_limit(0, 4096), 3200);
    assert_eq!(realtime.block_frame_limit(0), 3200);
    assert_eq!(p.block_frame_limit(0, 20), 20);
    assert_eq!(p.block_frame_limit(0, 0), 0);
    assert_eq!(p.block_frame_limit(p.duration_samples(), 1), 0);
    assert_eq!(p.block_frame_limit(u64::MAX, 4096), 0);
    let formats = p
        .voices()
        .map(|v| RuntimeSourceFormatV4 {
            element_id: v.element_id.into(),
            sample_rate: 48_000,
            frame_count: 96_000,
            channels: 1,
            mode: ClassicAudioPreparationModeV4::UnpreparedLinear,
        })
        .collect::<Vec<_>>();
    let bound = bind_classic_audio_pcm_v4(&p, &formats).unwrap();
    let preview_formats = formats
        .iter()
        .map(|f| PreviewSourceFormatV4 {
            element_id: f.element_id.clone(),
            sample_rate: f.sample_rate,
            frame_count: f.frame_count,
            channels: 1,
            prepared: false,
            segments: vec![PreviewAudioSegmentV4 {
                segment_id: 1,
                timestamp_seconds: 0.0,
                frame_count: f.frame_count,
            }],
        })
        .collect::<Vec<_>>();
    let preview_bound = bind_preview_audio_v4(&realtime, &preview_formats).unwrap();
    let mut cursor = 0;
    let mut counts = std::collections::BTreeMap::<String, u64>::new();
    while cursor < p.duration_samples() {
        let count = p.block_frame_limit(cursor, 4096);
        assert!(count > 0);
        let block = compile_bound_audio_block_v4(&bound, cursor, count).unwrap();
        let live = compile_preview_audio_block_v4(&preview_bound, cursor, count, 0, None).unwrap();
        assert_eq!(block.identity, *p.identity());
        assert_eq!(live.voices.len(), block.voices.len());
        assert!(block.voices.len() <= 32);
        for voice in block.voices {
            *counts.entry(voice.element_id).or_default() += u64::from(voice.frame_count);
            assert_eq!(voice.positions.len(), voice.frame_count as usize);
            assert_eq!(voice.amplitudes.len(), voice.frame_count as usize);
        }
        cursor += u64::from(count);
    }
    assert_eq!(cursor, 10_000);
    assert_eq!(counts.len(), 100);
    assert!(counts.values().all(|&count| count == 100));
}

#[test]
fn half_open_admission_ends_before_starts_and_rejects_true_thirty_third_voice() {
    let thirty_two = vec![(0, 1); 32];
    let mut touching = thirty_two.clone();
    touching.extend(vec![(1, 1); 32]);
    let s = spans(&touching);
    let p = prepare(&s, 48_000);
    assert!(preview(&s).is_ok());
    assert_eq!(p.block_frame_limit(0, 2), 1);
    assert_eq!(p.block_frame_limit(1, 2), 1);
    assert_eq!(compile_audio_block_v4(&p, 0, 1).unwrap().voices.len(), 32);
    assert_eq!(compile_audio_block_v4(&p, 1, 1).unwrap().voices.len(), 32);
    let rejected = spans(&[(0, 1); 33]);
    assert!(
        prepare_audio_v4(
            &rejected,
            "scene",
            AudioExecutionProfileV4::ClassicV31,
            PcmFormatV4 {
                sample_rate: 48_000,
                channels: 2
            }
        )
        .is_err()
    );
    assert!(preview(&rejected).is_err());
}

#[test]
fn block_union_counts_short_sequential_voices_even_without_overlap() {
    let s = spans(&(0..33).map(|i| (i * 2, 1)).collect::<Vec<_>>());
    let p = prepare(&s, 48_000);
    // Voice 33 begins at sample 64. Prefix [0,64) includes exactly 32
    // voices, only 32 actual samples, and the inter-voice silence as well.
    assert_eq!(p.block_frame_limit(0, 65), 64);
    assert_eq!(compile_audio_block_v4(&p, 0, 64).unwrap().voices.len(), 32);
    assert!(compile_audio_block_v4(&p, 0, 65).is_err());
    assert_eq!(
        compile_audio_block_v4(&p, 64, 1).unwrap().voices[0].element_id,
        "voice-32"
    );
}

#[test]
fn thirty_two_sustained_voices_choose_exact_2048_sample_budget() {
    let s = spans(&[(0, 4096); 32]);
    let p = prepare(&s, 48_000);
    assert_eq!(p.block_frame_limit(0, 4096), 2048);
    assert_eq!(preview(&s).unwrap().block_frame_limit(0), 2048);
    let block = compile_audio_block_v4(&p, 0, 2048).unwrap();
    assert_eq!(
        block
            .voices
            .iter()
            .map(|v| v.frame_count as usize)
            .sum::<usize>(),
        65_536
    );
    assert!(compile_audio_block_v4(&p, 0, 2049).is_err());
    assert_eq!(p.block_frame_limit(2048, 4096), 2048);
    assert_eq!(
        compile_audio_block_v4(&p, 2048, 2048).unwrap().voices.len(),
        32
    );
}

#[test]
fn metadata_cap_is_independent_of_concurrency() {
    let s = spans(&(0..256).map(|i| (i * 2, 1)).collect::<Vec<_>>());
    assert_eq!(prepare(&s, 48_000).voices().count(), 256);
    assert!(preview(&s).is_ok());
    let excessive = spans(&(0..257).map(|i| (i * 2, 1)).collect::<Vec<_>>());
    assert!(
        prepare_audio_v4(
            &excessive,
            "scene",
            AudioExecutionProfileV4::ClassicV31,
            PcmFormatV4 {
                sample_rate: 48_000,
                channels: 2
            }
        )
        .is_err()
    );
    assert!(preview(&excessive).is_err());
}
