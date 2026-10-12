//! Pure, fixed-revision audio preparation. No decoder, locator or PCM I/O.
//! Classic samples use the quantized placement origin; legacy FFmpeg processing
//! remains an explicit native requirement, not an interchangeable resampler.
use super::*;
mod budget;
mod clocks;
mod gains;
mod preparation_facts;
mod prepare;
mod preview;
mod runtime_pcm;
pub use preparation_facts::*;
pub use preview::*;
pub use runtime_pcm::*;
#[cfg(test)]
mod tests;
use prepare::PreparedVoice;

pub const MAX_AUDIO_BLOCK_FRAMES_V4: u32 = 4096;
pub const MAX_AUDIO_BLOCK_GAIN_SAMPLES_V4: usize = 65_536;
/// Bounded fixed-scene metadata, independent of simultaneous participation.
pub const MAX_PREPARED_AUDIO_VOICES_V4: usize = 256;
/// Maximum simultaneous and per-block union voices for Classic execution.
pub const MAX_RUNTIME_AUDIO_VOICES_V4: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioExecutionProfileV4 {
    ClassicV31,
    /// Frozen native V3 uses nearest placement, 48 kHz FFmpeg resampling and
    /// frame-evaluated gain. Animated legacy gain is explicitly unsupported by
    /// this sample compiler; it must not be silently replaced with dense gain.
    LegacyFfmpegV3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcmFormatV4 {
    pub sample_rate: u32,
    pub channels: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioIdentityV4 {
    pub project_id: String,
    pub revision: String,
    pub sequence_id: String,
    pub profile: AudioExecutionProfileV4,
    pub format: PcmFormatV4,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioProcessingV4 {
    /// Native channel selection is min(output channel, last source channel).
    /// Lower sample at/after decoded length is zero; upper clamps to last sample.
    LinearInterpolationZeroTail,
    /// Classic export prepares an intermediate Float32 buffer whenever the
    /// maintain-pitch flag is on, even at unit or epsilon-close rates.
    ClassicPreparedLinear,
    /// Requires a continuous, explicitly identified native pitch processor.
    NativeMaintainPitch,
    LegacyFfmpegResample,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioParticipationV4 {
    NonAudio,
    DisabledTrack,
    DisabledElement,
    SourceAudioDisabled,
    Muted,
    KnownAbsent,
    RequiresProbe,
    Ready,
    OutsideBlock,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioReceiptV4 {
    pub track_id: String,
    pub element_id: String,
    pub decision: AudioParticipationV4,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AudioSourceMappingV4 {
    /// Preserved request and its intersection with the declared extent are
    /// planning facts. Classic interpolation taps are bounded by actual decoded
    /// PCM length, rather than an inverse/forward-rounded nominal endpoint.
    pub clock: SourceClockV4,
    /// Actual processor tempo. Classic clamps at .01..5 without rewriting the
    /// authored request/resize clock or the stored project.
    pub effective_rate: RationalRateV4,
    pub output_sample_rate: u32,
    /// Legacy native rate rounding, after decoding/resampling at 48 kHz.
    pub legacy_retime_sample_rate: Option<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExactSourceSampleV4 {
    #[serde(serialize_with = "runtime_pcm::serialize_u64")]
    pub lower: u64,
    #[serde(serialize_with = "runtime_pcm::serialize_u128")]
    pub fraction_numerator: u128,
    #[serde(serialize_with = "runtime_pcm::serialize_u128")]
    pub fraction_denominator: u128,
}
/// Borrowed execution facts for native decoder admission, without exposing the
/// private preparation state or retaining resource locator provenance.
pub struct AudioVoiceInfoV4<'a> {
    pub track_id: &'a str,
    pub element_id: &'a str,
    pub resource: &'a CasResourceV4,
    pub stream: Option<u32>,
    pub probe_required: bool,
    pub processing: AudioProcessingV4,
    pub placement_start_sample: u64,
    pub placement_frame_count: u64,
    /// Authored duration, before placement ceil quantization. Pitch preparation
    /// must not recover this by dividing the quantized frame count by PCM rate.
    pub timeline_duration: &'a Time,
    pub maintain_pitch: bool,
    pub source: &'a AudioSourceMappingV4,
}
impl AudioSourceMappingV4 {
    /// Exact nominal source position. This does not authorize reading beyond
    /// the bounded window or decoded PCM. `local_sample` is relative to the
    /// placement origin, independent of how callers partition output blocks.
    pub fn position_at(
        &self,
        local_sample: u64,
        source_sample_rate: u32,
    ) -> Result<ExactSourceSampleV4, CompositionError> {
        clocks::position(self, local_sample, source_sample_rate)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum SampleGainV4 {
    Constant {
        amplitude: f64,
    },
    /// One finite amplitude for each output frame, shared across channels.
    Dense {
        amplitudes: Vec<f64>,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct AudioVoiceBlockV4 {
    pub track_id: String,
    pub element_id: String,
    pub resource: CasResourceV4,
    pub stream: Option<u32>,
    pub presence: AudioPresenceV4,
    pub probe_required: bool,
    pub processing: AudioProcessingV4,
    pub placement_start_sample: u64,
    pub placement_frame_count: u64,
    pub block_offset: u32,
    pub frame_count: u32,
    pub local_start_sample: u64,
    pub source: AudioSourceMappingV4,
    pub gain: SampleGainV4,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AudioBlockV4 {
    pub identity: AudioIdentityV4,
    pub start_sample: u64,
    pub frame_count: u32,
    /// Stable sequence track order, then stored element order.
    pub voices: Vec<AudioVoiceBlockV4>,
    pub receipts: Vec<AudioReceiptV4>,
}
/// Construction is private; preparation validates the complete aggregate once.
pub struct PreparedAudioV4 {
    identity: AudioIdentityV4,
    voices: Vec<PreparedVoice>,
    receipts: Vec<AudioReceiptV4>,
    duration_samples: u64,
}
impl PreparedAudioV4 {
    pub fn identity(&self) -> &AudioIdentityV4 {
        &self.identity
    }
    pub fn duration_samples(&self) -> u64 {
        self.duration_samples
    }
    /// Largest admitted prefix within the requested span, 4096-frame cap and
    /// scene extent. Zero denotes EOF or a zero request, not a compilable block.
    /// Callers must still validate their own requested range before shortening.
    pub fn block_frame_limit(&self, start_sample: u64, requested: u32) -> u32 {
        budget::block_frame_limit(self, start_sample, requested)
    }
    pub fn receipts(&self) -> &[AudioReceiptV4] {
        &self.receipts
    }
    /// Resources in stable voice order; duplicate references are intentional.
    pub fn resources(&self) -> impl Iterator<Item = &CasResourceV4> {
        self.voices.iter().map(|voice| &voice.resource)
    }
    pub fn voices(&self) -> impl Iterator<Item = AudioVoiceInfoV4<'_>> {
        self.voices.iter().map(|voice| AudioVoiceInfoV4 {
            track_id: &voice.track_id,
            element_id: &voice.element_id,
            resource: &voice.resource,
            stream: voice.stream,
            probe_required: voice.probe_required,
            processing: voice.processing,
            placement_start_sample: voice.start,
            placement_frame_count: voice.end - voice.start,
            timeline_duration: &voice.timeline_duration,
            maintain_pitch: voice.maintain_pitch,
            source: &voice.source,
        })
    }
}
pub fn prepare_audio_v4(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    profile: AudioExecutionProfileV4,
    format: PcmFormatV4,
) -> Result<PreparedAudioV4, CompositionError> {
    prepare::prepare(snapshot, sequence_id, profile, format)
}
pub fn compile_audio_block_v4(
    prepared: &PreparedAudioV4,
    start_sample: u64,
    frame_count: u32,
) -> Result<AudioBlockV4, CompositionError> {
    compile_audio_block_inner(prepared, start_sample, frame_count, true)
}
fn compile_audio_block_inner(
    prepared: &PreparedAudioV4,
    start_sample: u64,
    frame_count: u32,
    sample_gain: bool,
) -> Result<AudioBlockV4, CompositionError> {
    if frame_count == 0 || frame_count > MAX_AUDIO_BLOCK_FRAMES_V4 {
        return Err(invalid("Audio block must contain 1..4096 frames"));
    }
    let block_end = start_sample
        .checked_add(u64::from(frame_count))
        .ok_or_else(|| invalid("Audio block sample boundary overflow"))?;
    // Admit the entire block before allocating dense curves; direct compilation
    // never silently shortens the caller's requested range.
    budget::check_block(prepared, start_sample, block_end)?;
    let mut output = AudioBlockV4 {
        identity: prepared.identity.clone(),
        start_sample,
        frame_count,
        voices: vec![],
        receipts: prepared.receipts.clone(),
    };
    for voice in &prepared.voices {
        let start = start_sample.max(voice.start);
        let end = block_end.min(voice.end);
        if start >= end {
            output.receipts[voice.receipt_index].decision = AudioParticipationV4::OutsideBlock;
            continue;
        }
        let frames =
            u32::try_from(end - start).map_err(|_| invalid("Audio voice size overflow"))?;
        let local = start - voice.start;
        let gain = if sample_gain {
            gains::compile(voice, &prepared.identity, start, local, frames)?
        } else {
            SampleGainV4::Constant { amplitude: 1.0 }
        };
        output.voices.push(AudioVoiceBlockV4 {
            track_id: voice.track_id.clone(),
            element_id: voice.element_id.clone(),
            resource: voice.resource.clone(),
            stream: voice.stream,
            presence: voice.resource.metadata.audio.clone(),
            probe_required: voice.probe_required,
            processing: voice.processing,
            placement_start_sample: voice.start,
            placement_frame_count: voice.end - voice.start,
            block_offset: u32::try_from(start - start_sample)
                .map_err(|_| invalid("Audio offset overflow"))?,
            frame_count: frames,
            local_start_sample: local,
            source: voice.source.clone(),
            gain,
        });
    }
    Ok(output)
}
