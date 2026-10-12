use super::*;

pub(super) struct PreparedVoice {
    pub track_id: String,
    pub element_id: String,
    pub resource: CasResourceV4,
    pub stream: Option<u32>,
    pub probe_required: bool,
    pub processing: AudioProcessingV4,
    pub timeline_duration: Time,
    pub maintain_pitch: bool,
    pub source: AudioSourceMappingV4,
    pub start: u64,
    pub end: u64,
    pub receipt_index: usize,
    pub element_gain: GainV4,
    pub track_gain: GainV4,
    pub element_channels: Vec<AnimationChannelV4>,
    pub track_channels: Vec<AnimationChannelV4>,
}
pub(super) fn prepare(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    profile: AudioExecutionProfileV4,
    format: PcmFormatV4,
) -> Result<PreparedAudioV4, CompositionError> {
    if snapshot.revision.trim().is_empty() {
        return Err(invalid("Audio preparation requires a fixed revision"));
    }
    if format.sample_rate == 0
        || format.sample_rate > 384_000
        || !(1..=2).contains(&format.channels)
    {
        return Err(unsupported(
            "Audio PCM format requires 1..384000 Hz and mono/stereo",
        ));
    }
    if profile == AudioExecutionProfileV4::LegacyFfmpegV3
        && format
            != (PcmFormatV4 {
                sample_rate: 48_000,
                channels: 2,
            })
    {
        return Err(unsupported(
            "Frozen legacy FFmpeg profile requires 48000 Hz stereo",
        ));
    }
    validate_project_v4(&snapshot.project).map_err(error)?;
    let sequence = snapshot
        .project
        .sequences
        .iter()
        .find(|s| s.id == sequence_id)
        .ok_or_else(|| invalid("Audio sequence does not exist"))?;
    let mut output = PreparedAudioV4 {
        identity: AudioIdentityV4 {
            project_id: snapshot.project.id.clone(),
            revision: snapshot.revision.clone(),
            sequence_id: sequence_id.into(),
            profile,
            format,
        },
        voices: vec![],
        receipts: vec![],
        duration_samples: 0,
    };
    let zero = RationalTime::new(0, 1).map_err(error)?;
    for track in &sequence.tracks {
        for element in &track.elements {
            output.duration_samples = output.duration_samples.max(clocks::sample_index(
                end(&element.timeline_range)?,
                format.sample_rate,
                clocks::Rounding::Ceil,
            )?);
            let media = match &element.content {
                ElementContentV4::Video {
                    asset_id,
                    source_audio_enabled,
                    audio,
                    retime,
                    ..
                } => Some((asset_id, *source_audio_enabled, audio, retime)),
                ElementContentV4::Audio {
                    asset_id,
                    audio,
                    retime,
                } => Some((asset_id, true, audio, retime)),
                _ => None,
            };
            let asset =
                media.and_then(|(id, ..)| snapshot.project.assets.iter().find(|a| a.id == *id));
            let decision = if !track.enabled {
                AudioParticipationV4::DisabledTrack
            } else if !element.enabled {
                AudioParticipationV4::DisabledElement
            } else if let Some((_, enabled, audio, _)) = media {
                if !enabled {
                    AudioParticipationV4::SourceAudioDisabled
                } else if track.muted || audio.muted {
                    AudioParticipationV4::Muted
                } else if asset
                    .is_some_and(|a| matches!(a.metadata.audio, AudioPresenceV4::Absent {}))
                {
                    AudioParticipationV4::KnownAbsent
                } else if asset.is_some_and(|a| {
                    a.duration.is_none()
                        || !matches!(
                            a.metadata.audio,
                            AudioPresenceV4::Present { metadata: Some(_) }
                        )
                }) {
                    AudioParticipationV4::RequiresProbe
                } else {
                    AudioParticipationV4::Ready
                }
            } else {
                AudioParticipationV4::NonAudio
            };
            let receipt_index = output.receipts.len();
            output.receipts.push(AudioReceiptV4 {
                track_id: track.id.clone(),
                element_id: element.id.clone(),
                decision,
            });
            if !matches!(
                decision,
                AudioParticipationV4::Ready | AudioParticipationV4::RequiresProbe
            ) {
                continue;
            }
            let (_, _, audio, retime) =
                media.ok_or_else(|| invalid("Validated audio node missing"))?;
            let asset = asset.ok_or_else(|| invalid("Validated audio asset missing"))?;
            if profile == AudioExecutionProfileV4::ClassicV31
                && output.voices.len() == MAX_PREPARED_AUDIO_VOICES_V4
            {
                return Err(unsupported(
                    "Classic audio admits at most 256 participating voice records",
                ));
            }
            let element_channels = gains::channels(&element.channels);
            let track_channels = gains::channels(&track.channels);
            if profile == AudioExecutionProfileV4::LegacyFfmpegV3
                && (!element_channels.is_empty() || !track_channels.is_empty())
            {
                return Err(unsupported(
                    "Frozen legacy frame-evaluated gain cannot be replaced with per-sample gain",
                ));
            }
            // Muted/nonparticipating nodes never project physical gain curves.
            let element_gain = animation::gain(audio.gain)?.physical;
            let track_gain = animation::gain(track.gain)?.physical;
            let start = clocks::sample_index(
                time(&element.timeline_range.start)?,
                format.sample_rate,
                if profile == AudioExecutionProfileV4::ClassicV31 {
                    clocks::Rounding::Floor
                } else {
                    clocks::Rounding::Nearest
                },
            )?;
            let length = clocks::sample_index(
                time(&element.timeline_range.duration)?,
                format.sample_rate,
                if profile == AudioExecutionProfileV4::ClassicV31 {
                    clocks::Rounding::Ceil
                } else {
                    clocks::Rounding::Nearest
                },
            )?;
            let voice_end = start
                .checked_add(length)
                .ok_or_else(|| invalid("Audio placement end overflows u64"))?;
            output.duration_samples = output.duration_samples.max(voice_end);
            let legacy_rate = if profile == AudioExecutionProfileV4::LegacyFfmpegV3 {
                if retime.maintain_pitch && retime.rate.numerator != retime.rate.denominator {
                    return Err(unsupported(
                        "Frozen legacy profile has no maintain-pitch processing",
                    ));
                }
                Some(clocks::legacy_rate(retime.rate)?)
            } else {
                None
            };
            let processing = if profile == AudioExecutionProfileV4::LegacyFfmpegV3 {
                AudioProcessingV4::LegacyFfmpegResample
            } else {
                match classic_pitch_processing_mode_v4(retime.rate, retime.maintain_pitch)? {
                    ClassicAudioPreparationModeV4::UnpreparedLinear => {
                        AudioProcessingV4::LinearInterpolationZeroTail
                    }
                    ClassicAudioPreparationModeV4::PreparedLinear => {
                        AudioProcessingV4::ClassicPreparedLinear
                    }
                    ClassicAudioPreparationModeV4::PreparedSoundtouch => {
                        AudioProcessingV4::NativeMaintainPitch
                    }
                }
            };
            output.voices.push(PreparedVoice {
                track_id: track.id.clone(),
                element_id: element.id.clone(),
                resource: CasResourceV4 {
                    asset_id: asset.id.clone(),
                    version: asset.version.clone(),
                    kind: asset.kind.clone(),
                    duration: asset.duration.clone(),
                    metadata: asset.metadata.clone(),
                    license: asset.license.clone(),
                },
                stream: audio.stream,
                probe_required: decision == AudioParticipationV4::RequiresProbe,
                processing,
                timeline_duration: element.timeline_range.duration.clone(),
                maintain_pitch: retime.maintain_pitch,
                source: AudioSourceMappingV4 {
                    clock: source::clock(element, asset, &zero.into(), zero, zero)?,
                    effective_rate: if profile == AudioExecutionProfileV4::ClassicV31 {
                        classic_effective_rate_v4(retime.rate)?
                    } else {
                        retime.rate
                    },
                    output_sample_rate: format.sample_rate,
                    legacy_retime_sample_rate: legacy_rate,
                },
                start,
                end: voice_end,
                receipt_index,
                element_gain,
                track_gain,
                element_channels,
                track_channels,
            });
        }
    }
    for caption in &sequence.captions {
        output.duration_samples = output.duration_samples.max(clocks::sample_index(
            end(&caption.timeline_range)?,
            format.sample_rate,
            clocks::Rounding::Ceil,
        )?);
    }
    if profile == AudioExecutionProfileV4::ClassicV31 {
        budget::admit(&output.voices)?;
    }
    Ok(output)
}
