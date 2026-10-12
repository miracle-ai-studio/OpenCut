//! Internal native PCM decoding. No project document, portable host path or
//! published render artifact is introduced by this adapter.
use crate::{CancellationToken, FfmpegBackend, MediaError, MediaResult};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

mod stream;
pub use stream::NativePcmStream;

#[derive(Clone, Copy, Debug)]
pub struct PcmDecodeLimits {
    /// Budget includes skipped leading samples, not just returned samples.
    pub max_decoded_bytes: u64,
    pub max_buffered_blocks: usize,
    pub stall_timeout: Duration,
}
impl PcmDecodeLimits {
    pub(crate) fn validate(self) -> MediaResult<()> {
        if self.max_decoded_bytes == 0
            || !(1..=8).contains(&self.max_buffered_blocks)
            || self.stall_timeout.is_zero()
            || self.stall_timeout > Duration::from_secs(60)
        {
            return Err(MediaError::InvalidRequest(
                "PCM decode limits outside supported bounds".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeChannelMap {
    MonoDuplicate,
    FirstTwo,
}

#[derive(Clone, Debug)]
pub struct NativePcmSource {
    pub(crate) input: PathBuf,
    /// FFprobe's actual demuxer identity; input options never follow a filename
    /// extension or caller-supplied MIME type.
    detected_format: String,
    pub stream_index: u32,
    pub sample_rate: u32,
    pub source_channels: u32,
    pub channel_layout: Option<String>,
    pub channel_map: NativeChannelMap,
    /// First decoded frame, before resetting any timestamps. Sample offsets in
    /// the stream are relative to this decoded origin, not container time zero.
    pub first_frame_pts: i64,
    pub time_base_numerator: u32,
    pub time_base_denominator: u32,
}

#[derive(Debug)]
pub struct NativePcmBlock {
    pub start_sample: u64,
    /// Interleaved stereo; mono has already been explicitly duplicated.
    pub samples: Vec<f32>,
}
impl NativePcmBlock {
    pub fn frame_count(&self) -> usize {
        self.samples.len() / 2
    }
}

#[derive(Deserialize)]
struct Probe {
    format: Option<ProbeFormat>,
    #[serde(default)]
    streams: Vec<ProbeStream>,
    #[serde(default)]
    frames: Vec<ProbeFrame>,
}
#[derive(Deserialize)]
struct ProbeFormat {
    format_name: Option<String>,
}
#[derive(Deserialize)]
struct ProbeStream {
    index: u32,
    codec_type: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u32>,
    channel_layout: Option<String>,
    time_base: Option<String>,
}
#[derive(Deserialize)]
struct ProbeFrame {
    pts: Option<i64>,
    best_effort_timestamp: Option<i64>,
    nb_samples: Option<u32>,
}

impl FfmpegBackend {
    /// Called only with a host-authorized, immutable local resource. This probe
    /// does not grant access or infer silence from missing metadata.
    pub fn prepare_pcm_source(
        &self,
        input: &Path,
        stream: Option<u32>,
        cancel: &CancellationToken,
    ) -> MediaResult<NativePcmSource> {
        if !input.is_absolute() || !input.is_file() {
            return Err(MediaError::InvalidRequest(
                "PCM source must be an absolute existing file".into(),
            ));
        }
        let selector = stream.map_or_else(|| "a:0".into(), |index| index.to_string());
        let mut command = self.audio_ffprobe_command();
        local_input_policy(&mut command);
        // MOV's default enable_drefs/use_absolute_path are both false. Format
        // detection retains those closed defaults; private MOV options cannot
        // be bound to an as-yet unknown input demuxer.
        command.args(["-v","error","-select_streams", &selector,"-read_intervals","%+#64","-show_format","-show_streams","-show_frames","-show_entries","format=format_name:stream=index,codec_type,sample_rate,channels,channel_layout,time_base:frame=pts,best_effort_timestamp,nb_samples","-of","json"]).arg(input);
        let bytes = crate::process::checked_output_limited(
            &mut command,
            "ffprobe",
            cancel,
            Duration::from_secs(30),
            self.output_admission().as_deref(),
        )?;
        let probe: Probe = serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
        let detected_format = probe
            .format
            .and_then(|format| format.format_name)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| invalid("Missing actual PCM input demuxer identity"))?;
        let stream =
            probe.streams.into_iter().next().ok_or_else(|| {
                MediaError::Unsupported("Selected source has no audio stream".into())
            })?;
        if stream.codec_type.as_deref() != Some("audio") {
            return Err(MediaError::Unsupported(
                "Selected stream is not audio".into(),
            ));
        }
        let rate = stream
            .sample_rate
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|v| (8000..=384000).contains(v))
            .ok_or_else(|| invalid("Missing or unsupported native PCM sample rate"))?;
        let channels = stream
            .channels
            .filter(|v| (1..=64).contains(v))
            .ok_or_else(|| invalid("Missing or unsupported PCM channel count"))?;
        let (top, bottom) = stream
            .time_base
            .as_deref()
            .and_then(|v| v.split_once('/'))
            .and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))
            .filter(|(a, b)| *a > 0 && *b > 0)
            .ok_or_else(|| invalid("Missing decoded audio time base"))?;
        let first = probe
            .frames
            .iter()
            .find(|f| f.nb_samples.is_some_and(|n| n > 0))
            .and_then(|f| f.pts.or(f.best_effort_timestamp))
            .ok_or_else(|| {
                MediaError::Unsupported(
                    "Audio decode origin PTS unavailable; cannot claim an exact sample origin"
                        .into(),
                )
            })?;
        Ok(NativePcmSource {
            input: input.to_owned(),
            detected_format,
            stream_index: stream.index,
            sample_rate: rate,
            source_channels: channels,
            channel_layout: stream.channel_layout,
            channel_map: if channels == 1 {
                NativeChannelMap::MonoDuplicate
            } else {
                NativeChannelMap::FirstTwo
            },
            first_frame_pts: first,
            time_base_numerator: top,
            time_base_denominator: bottom,
        })
    }

    pub fn open_pcm_stream(
        &self,
        source: &NativePcmSource,
        limits: PcmDecodeLimits,
        cancel: &CancellationToken,
    ) -> MediaResult<NativePcmStream> {
        limits.validate()?;
        let pan = match source.channel_map {
            NativeChannelMap::MonoDuplicate => "pan=stereo|c0=c0|c1=c0",
            NativeChannelMap::FirstTwo => "pan=stereo|c0=c0|c1=c1",
        };
        let mut command = self.audio_ffmpeg_command();
        local_input_policy(&mut command);
        if source
            .detected_format
            .split(',')
            .any(|format| format == "mov")
        {
            command.args(["-enable_drefs", "0", "-use_absolute_path", "0"]);
        }
        command
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
            .arg(&source.input)
            .args([
                "-map",
                &format!("0:{}", source.stream_index),
                "-vn",
                "-sn",
                "-dn",
                "-af",
                pan,
                "-ar",
                &source.sample_rate.to_string(),
                "-c:a",
                "pcm_f32le",
                "-f",
                "f32le",
                "pipe:1",
            ]);
        stream::spawn(command, limits, self.output_admission(), cancel.clone())
    }
}
/// Same self-contained local-input boundary as upload probing. These options
/// are placed before the input; stdout's pipe protocol is an output context.
fn local_input_policy(command: &mut std::process::Command) {
    command.args([
        "-protocol_whitelist",
        "file",
        "-format_whitelist",
        "mov,matroska,avi,wav,mp3,flac,ogg,aac,mpeg,mpegts",
    ]);
}
fn invalid(detail: impl Into<String>) -> MediaError {
    MediaError::InvalidToolOutput {
        tool: "ffmpeg-pcm".into(),
        detail: detail.into(),
    }
}
