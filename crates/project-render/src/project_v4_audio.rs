//! Internal unmastered IR4 PCM execution. This stage cannot publish a completed
//! render result: Classic whole-scene peak gating and WebAudio mastering are
//! separate, unfinished execution requirements.
use crate::{ProjectRenderError, error};
use opencut_composition::project_v4::*;
use opencut_contracts::{EditorError, project_v4::AudioPresenceV4};
use opencut_media::*;
use std::{collections::HashMap, path::PathBuf};

mod cursor;
mod mixing;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug)]
pub struct AudioStageLimitsV4 {
    pub max_voices: usize,
    /// Reserved total across voices, including samples skipped before trim.
    pub max_decoded_bytes: u64,
    pub decoder_buffered_blocks: usize,
    pub decoder_stall_timeout: std::time::Duration,
}

#[derive(Clone, Debug)]
pub struct AudioDecodeReceiptV4 {
    pub decode_profile: AudioDecodeProfileV4,
    pub element_id: String,
    pub asset_id: String,
    pub stream_index: u32,
    pub native_sample_rate: u32,
    pub source_channels: u32,
    pub channel_map: NativeChannelMap,
    pub first_decoded_pts: i64,
    pub pts_time_base_numerator: u32,
    pub pts_time_base_denominator: u32,
    pub decoded_frames_observed: u64,
    /// Only Some after the decoder actually reaches EOF. Stopping at the
    /// authored clip endpoint never invents a full-source sample count.
    pub decoded_eof_frames: Option<u64>,
}

/// Source decoder identity is separate from Classic placement/gain semantics.
/// This is not a browser decodeAudioData or mastering equivalence claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioDecodeProfileV4 {
    FfmpegFloatPcmZeroOriginV1,
}

#[derive(Debug)]
pub struct UnmasteredPcmBlockV4 {
    pub identity: AudioIdentityV4,
    pub start_sample: u64,
    pub frame_count: u32,
    /// Interleaved stereo, float32 accumulation in stable source order. Values
    /// may exceed [-1,1]; this is deliberately not a mastered render artifact.
    pub samples: Vec<f32>,
}

/// Prepared by the authorized host CAS resolver, never deserialized from wire
/// paths. Output is strictly sequential so source interpolation and future DSP
/// state are independent of block partitioning.
pub struct UnmasteredPcmStageV4 {
    prepared: PreparedAudioV4,
    backend: FfmpegBackend,
    sources: HashMap<String, NativePcmSource>,
    cursors: HashMap<String, cursor::Cursor>,
    receipts: HashMap<String, AudioDecodeReceiptV4>,
    decode_limits: PcmDecodeLimits,
    next_sample: u64,
    failed: bool,
}

pub struct ProjectAudioAdapterV4;
impl ProjectAudioAdapterV4 {
    pub fn prepare_unmastered(
        backend: &FfmpegBackend,
        prepared: PreparedAudioV4,
        limits: AudioStageLimitsV4,
        cancel: &CancellationToken,
        mut resolver: impl FnMut(&CasResourceV4) -> Result<PathBuf, EditorError>,
    ) -> Result<UnmasteredPcmStageV4, ProjectRenderError> {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled.into());
        }
        if prepared.identity().profile != AudioExecutionProfileV4::ClassicV31 {
            return Err(unsupported(
                "Legacy FFmpeg resampling is not implemented by the Classic PCM stage",
            ));
        }
        if prepared.identity().format.channels != 2 {
            return Err(unsupported(
                "Unmastered native stage currently requires stereo output",
            ));
        }
        let count = prepared.voices().count();
        if !(1..=32).contains(&limits.max_voices)
            || count > limits.max_voices
            || limits.max_decoded_bytes == 0
        {
            return Err(error(
                "INVALID_ARGUMENT",
                "Audio stage voice/decode budget invalid or exceeded",
            ));
        }
        // Capability rejection precedes CAS resolution, probes and processes.
        for voice in prepared.voices() {
            if voice.processing != AudioProcessingV4::LinearInterpolationZeroTail {
                return Err(unsupported(
                    "Maintain-pitch requires the identified Classic SoundTouchJS/WebAudio execution profile",
                ));
            }
        }
        let decode_limits = PcmDecodeLimits {
            max_decoded_bytes: limits.max_decoded_bytes / count.max(1) as u64,
            max_buffered_blocks: limits.decoder_buffered_blocks,
            stall_timeout: limits.decoder_stall_timeout,
        };
        if decode_limits.max_decoded_bytes == 0
            || !(1..=8).contains(&decode_limits.max_buffered_blocks)
            || decode_limits.stall_timeout.is_zero()
            || decode_limits.stall_timeout > std::time::Duration::from_secs(60)
        {
            return Err(error(
                "INVALID_ARGUMENT",
                "Audio decoder limits outside supported bounds",
            ));
        }
        let mut paths: HashMap<String, PathBuf> = HashMap::new();
        for resource in prepared.resources() {
            if paths.contains_key(&resource.asset_id) {
                continue;
            }
            let path = resolver(resource).map_err(resolution_error)?;
            if !path.is_absolute() || !path.is_file() {
                return Err(error(
                    "ASSET_RESOLUTION_FAILED",
                    "Audio CAS resolver must return an absolute existing file",
                ));
            }
            paths.insert(resource.asset_id.clone(), path);
        }
        let mut sources = HashMap::new();
        let mut receipts = HashMap::new();
        for voice in prepared.voices() {
            if cancel.is_cancelled() {
                return Err(MediaError::Cancelled.into());
            }
            let source = backend.prepare_pcm_source(
                &paths[&voice.resource.asset_id],
                voice.stream,
                cancel,
            )?;
            // Browser decoded-buffer origin/priming is not inferred from a
            // nonzero container timestamp. Until that profile is implemented,
            // accept only a known zero decoded origin, explicitly.
            if source.first_frame_pts != 0 {
                return Err(unsupported(
                    "Nonzero decoded PTS needs an explicit browser decode-origin/priming profile",
                ));
            }
            if let AudioPresenceV4::Present {
                metadata: Some(metadata),
            } = &voice.resource.metadata.audio
                && (metadata.sample_rate != source.sample_rate
                    || metadata.channels != source.source_channels)
            {
                return Err(error(
                    "ASSET_METADATA_MISMATCH",
                    "Verified PCM format differs from fixed revision metadata",
                ));
            }
            receipts.insert(
                voice.element_id.into(),
                AudioDecodeReceiptV4 {
                    decode_profile: AudioDecodeProfileV4::FfmpegFloatPcmZeroOriginV1,
                    element_id: voice.element_id.into(),
                    asset_id: voice.resource.asset_id.clone(),
                    stream_index: source.stream_index,
                    native_sample_rate: source.sample_rate,
                    source_channels: source.source_channels,
                    channel_map: source.channel_map,
                    first_decoded_pts: source.first_frame_pts,
                    pts_time_base_numerator: source.time_base_numerator,
                    pts_time_base_denominator: source.time_base_denominator,
                    decoded_frames_observed: 0,
                    decoded_eof_frames: None,
                },
            );
            sources.insert(voice.element_id.into(), source);
        }
        Ok(UnmasteredPcmStageV4 {
            prepared,
            backend: backend.clone(),
            sources,
            cursors: HashMap::new(),
            receipts,
            decode_limits,
            next_sample: 0,
            failed: false,
        })
    }
}
impl UnmasteredPcmStageV4 {
    pub fn identity(&self) -> &AudioIdentityV4 {
        self.prepared.identity()
    }
    pub fn duration_samples(&self) -> u64 {
        self.prepared.duration_samples()
    }
    pub fn participation_receipts(&self) -> &[AudioReceiptV4] {
        self.prepared.receipts()
    }
    pub fn decode_receipts(&self) -> Vec<AudioDecodeReceiptV4> {
        self.prepared
            .voices()
            .filter_map(|v| self.receipts.get(v.element_id).cloned())
            .collect()
    }
    pub fn read_next(
        &mut self,
        max_frames: u32,
        cancel: &CancellationToken,
    ) -> Result<Option<UnmasteredPcmBlockV4>, ProjectRenderError> {
        if self.failed {
            return Err(error(
                "INVALID_ARGUMENT",
                "Failed PCM stage cannot be resumed",
            ));
        }
        if max_frames == 0 || max_frames > MAX_AUDIO_BLOCK_FRAMES_V4 {
            return Err(error(
                "INVALID_ARGUMENT",
                "PCM block must contain 1..4096 frames",
            ));
        }
        let result = self.read_inner(max_frames, cancel);
        if result.is_err() {
            self.failed = true;
            self.cursors.clear();
        }
        result
    }
    fn read_inner(
        &mut self,
        max_frames: u32,
        cancel: &CancellationToken,
    ) -> Result<Option<UnmasteredPcmBlockV4>, ProjectRenderError> {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled.into());
        }
        if self.next_sample >= self.duration_samples() {
            self.cursors.clear();
            return Ok(None);
        }
        let frames =
            u32::try_from((self.duration_samples() - self.next_sample).min(u64::from(max_frames)))
                .expect("bounded block");
        let graph = compile_audio_block_v4(&self.prepared, self.next_sample, frames)?;
        let mut samples = vec![0f32; frames as usize * 2];
        for voice in &graph.voices {
            if cancel.is_cancelled() {
                return Err(MediaError::Cancelled.into());
            }
            if !self.cursors.contains_key(&voice.element_id) {
                let source = self.sources.get(&voice.element_id).ok_or_else(|| {
                    error("ASSET_RESOLUTION_FAILED", "Prepared audio source missing")
                })?;
                let stream = self
                    .backend
                    .open_pcm_stream(source, self.decode_limits, cancel)?;
                self.cursors
                    .insert(voice.element_id.clone(), cursor::Cursor::new(stream));
            }
            let cursor = self
                .cursors
                .get_mut(&voice.element_id)
                .expect("inserted cursor");
            let source = &self.sources[&voice.element_id];
            mixing::mix(voice, source.sample_rate, cursor, &mut samples, cancel)?;
            let receipt = self
                .receipts
                .get_mut(&voice.element_id)
                .expect("prepared receipt");
            receipt.decoded_frames_observed = cursor.observed_frames();
            receipt.decoded_eof_frames = cursor.eof_frames();
        }
        self.next_sample += u64::from(frames);
        // Release each completed voice's admitted child/lease, without
        // cancelling the whole job or retaining all source PCM until scene end.
        for voice in self.prepared.voices() {
            if voice.placement_start_sample + voice.placement_frame_count <= self.next_sample {
                self.cursors.remove(voice.element_id);
            }
        }
        Ok(Some(UnmasteredPcmBlockV4 {
            identity: graph.identity,
            start_sample: graph.start_sample,
            frame_count: graph.frame_count,
            samples,
        }))
    }
}
fn unsupported(message: impl Into<String>) -> ProjectRenderError {
    error("UNSUPPORTED_V4", message)
}
fn resolution_error(problem: EditorError) -> ProjectRenderError {
    let code = match problem.code.as_str() {
        "NOT_AUTHORIZED" => "NOT_AUTHORIZED",
        "POLICY_DENIED" => "POLICY_DENIED",
        "ASSET_INTEGRITY_ERROR" => "ASSET_INTEGRITY_ERROR",
        "ASSET_IO_ERROR" => "ASSET_IO_ERROR",
        _ => "ASSET_RESOLUTION_FAILED",
    };
    error(code, problem.message)
}
