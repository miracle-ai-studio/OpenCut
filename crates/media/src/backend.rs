use crate::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    env, fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct FfmpegBackend {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    admission: Option<std::sync::Arc<dyn OutputAdmission>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaToolFingerprint {
    pub name: String,
    pub version: String,
    pub binary_sha256: String,
}

#[derive(Default)]
struct TemporaryArtifacts {
    paths: Vec<PathBuf>,
}

impl TemporaryArtifacts {
    fn track(&mut self, path: PathBuf) {
        self.paths.push(path);
    }
}

impl Drop for TemporaryArtifacts {
    fn drop(&mut self) {
        for path in &self.paths {
            let _ = fs::remove_file(path);
        }
    }
}

impl FfmpegBackend {
    pub fn with_output_admission(mut self, admission: std::sync::Arc<dyn OutputAdmission>) -> Self {
        self.admission = Some(admission);
        self
    }
    pub fn discover() -> MediaResult<Self> {
        let backend = Self {
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            admission: None,
        };
        backend.check_tools()?;
        Ok(backend)
    }
    pub fn new(ffmpeg: impl Into<PathBuf>, ffprobe: impl Into<PathBuf>) -> MediaResult<Self> {
        let backend = Self {
            ffmpeg: ffmpeg.into(),
            ffprobe: ffprobe.into(),
            admission: None,
        };
        backend.check_tools()?;
        Ok(backend)
    }
    fn check_tools(&self) -> MediaResult<()> {
        process::checked_output(
            Command::new(&self.ffmpeg).arg("-version"),
            "ffmpeg",
            self.admission.as_deref(),
        )?;
        process::checked_output(
            Command::new(&self.ffprobe).arg("-version"),
            "ffprobe",
            self.admission.as_deref(),
        )?;
        Ok(())
    }

    /// Identifies the exact native binaries used for media operations without
    /// exposing host paths in portable project packages.
    pub fn tool_fingerprints(&self) -> MediaResult<Vec<MediaToolFingerprint>> {
        Ok(vec![
            fingerprint_tool("ffmpeg", &self.ffmpeg)?,
            fingerprint_tool("ffprobe", &self.ffprobe)?,
        ])
    }

    pub fn supports_video_codec(&self, codec: VideoCodec) -> MediaResult<bool> {
        let encoder = match codec {
            VideoCodec::H264 => "libx264",
            VideoCodec::Hevc => "libx265",
            VideoCodec::Vp9 => "libvpx-vp9",
        };
        let output = process::checked_output(
            Command::new(&self.ffmpeg).args(["-hide_banner", "-encoders"]),
            "ffmpeg",
            self.admission.as_deref(),
        )?;
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.split_whitespace().any(|field| field == encoder)))
    }

    /// Host-side capability probe for the fixed filters used by schema 3.
    pub fn missing_v3_filters(&self, needed: &[&str]) -> MediaResult<Vec<String>> {
        if needed.iter().any(|name| {
            !matches!(
                *name,
                "geq" | "eq" | "gblur" | "overlay" | "color" | "fps" | "pad"
            )
        }) {
            return Err(MediaError::InvalidRequest(
                "unknown filter capability probe".into(),
            ));
        }
        let cancel = CancellationToken::new();
        let output = process::checked_output_limited(
            Command::new(&self.ffmpeg).args(["-hide_banner", "-filters"]),
            "ffmpeg",
            &cancel,
            Duration::from_secs(30),
            self.admission.as_deref(),
        )?;
        let installed: std::collections::HashSet<String> = String::from_utf8_lossy(&output)
            .lines()
            .filter_map(|line| line.split_whitespace().nth(1).map(str::to_owned))
            .collect();
        Ok(needed
            .iter()
            .filter(|name| !installed.contains(**name))
            .map(|name| (*name).to_owned())
            .collect())
    }

    pub fn probe(&self, request: &ProbeRequest) -> MediaResult<MediaMetadata> {
        self.probe_cancellable(request, &CancellationToken::new())
    }

    pub fn probe_cancellable(
        &self,
        request: &ProbeRequest,
        cancel: &CancellationToken,
    ) -> MediaResult<MediaMetadata> {
        require_input(&request.input)?;
        let output = process::checked_output_limited(
            Command::new(&self.ffprobe)
                .args([
                    "-v",
                    "error",
                    "-show_format",
                    "-show_streams",
                    "-of",
                    "json",
                ])
                .arg(&request.input),
            "ffprobe",
            cancel,
            Duration::from_secs(30),
            self.admission.as_deref(),
        )?;
        let raw: RawProbe =
            serde_json::from_slice(&output).map_err(|e| MediaError::InvalidToolOutput {
                tool: "ffprobe".into(),
                detail: e.to_string(),
            })?;
        Ok(convert_probe(request.input.clone(), raw))
    }

    /// Import only self-contained binary formats; playlist demuxers and network
    /// protocols are deliberately absent. MOV external data references stay off.
    pub fn probe_upload(
        &self,
        input: &Path,
        cancel: &CancellationToken,
    ) -> MediaResult<MediaMetadata> {
        require_input(input)?;
        let output = process::checked_output_limited(
            Command::new(&self.ffprobe).args([
                "-v", "error", "-protocol_whitelist", "file",
                "-format_whitelist", "mov,matroska,avi,wav,mp3,flac,ogg,aac,mpeg,mpegts,png_pipe,jpeg_pipe,webp_pipe,bmp_pipe,tiff_pipe,gif",
                "-enable_drefs", "0", "-use_absolute_path", "0",
                "-show_entries", "format=format_name,duration,bit_rate:format_tags=major_brand:stream=index,codec_type,codec_name,width,height",
                "-of", "json", "-i",
            ]).arg(input), "ffprobe", cancel, Duration::from_secs(30), self.admission.as_deref(),
        )?;
        let raw: RawProbe =
            serde_json::from_slice(&output).map_err(|e| MediaError::InvalidToolOutput {
                tool: "ffprobe".into(),
                detail: e.to_string(),
            })?;
        Ok(convert_probe(input.into(), raw))
    }

    pub fn extract_frame(
        &self,
        request: &FrameExtractRequest,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(ProgressUpdate),
    ) -> MediaResult<ExtractedFrame> {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        require_input(&request.input)?;
        validate_output(&request.input, &request.output, request.overwrite)?;
        if request.requested_time.ticks < 0 {
            return Err(MediaError::InvalidRequest(
                "requested frame time cannot be negative".into(),
            ));
        }
        progress(ProgressUpdate {
            stage: ProgressStage::SelectingFrame,
            fraction: None,
            media_time: None,
        });
        let selection_alignment = if request.alignment == FrameAlignment::Exact {
            FrameAlignment::Nearest
        } else {
            request.alignment
        };
        let (stream_index, time_base, chosen) = self.select_frame_pts(
            &request.input,
            request.video_stream,
            request.requested_time,
            selection_alignment,
            cancel,
        )?;
        let exact = rational_cmp(
            chosen,
            time_base,
            request.requested_time.ticks,
            request.requested_time.timebase,
        ) == Ordering::Equal;
        if request.alignment == FrameAlignment::Exact && !exact {
            return Err(MediaError::Unsupported(format!(
                "no frame exists at exact requested time {}/{}; nearest source PTS is {chosen} at time base {}/{}",
                request.requested_time.ticks,
                request.requested_time.timebase,
                time_base.numerator,
                time_base.denominator
            )));
        }
        if let Some(parent) = request.output.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut command = Command::new(&self.ffmpeg);
        command.args(["-v", "error"]);
        if request.overwrite {
            command.arg("-y");
        } else {
            command.arg("-n");
        }
        command
            .args(["-copyts", "-i"])
            .arg(&request.input)
            .args([
                "-map",
                &format!("0:{stream_index}"),
                "-vf",
                &format!("select=eq(pts\\,{chosen})"),
                "-frames:v",
                "1",
                "-vsync",
                "0",
                "-progress",
                "pipe:1",
                "-nostats",
            ])
            .arg(&request.output);
        process::run_ffmpeg(
            command,
            cancel,
            None,
            ProgressStage::Decoding,
            progress,
            self.admission
                .as_deref()
                .map(|admission| (Some(request.output.as_path()), admission)),
        )?;
        if !request.output.is_file() {
            return Err(MediaError::InvalidToolOutput {
                tool: "ffmpeg".into(),
                detail: "frame output was not created".into(),
            });
        }
        Ok(ExtractedFrame {
            output: request.output.clone(),
            requested_time: request.requested_time,
            actual_time: MediaTime {
                ticks: i64::try_from(chosen as i128 * time_base.numerator as i128).map_err(
                    |_| MediaError::InvalidToolOutput {
                        tool: "ffprobe".into(),
                        detail: "frame timestamp overflows public time representation".into(),
                    },
                )?,
                timebase: time_base.denominator,
            },
            source_pts: chosen,
            source_time_base: time_base,
            alignment: request.alignment,
            exact,
            quality: FrameQuality::AccurateSourceFrame,
        })
    }

    pub fn export(
        &self,
        request: &ExportRequest,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(ProgressUpdate),
    ) -> MediaResult<ExportResult> {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        progress(ProgressUpdate {
            stage: ProgressStage::Probing,
            fraction: None,
            media_time: None,
        });
        validate_export(request)?;
        for segment in &request.segments {
            require_input(&segment.input)?;
            validate_output(&segment.input, &request.output, request.overwrite)?;
        }
        if let Some(parent) = request.output.parent() {
            fs::create_dir_all(parent)?;
        }
        let total = request
            .segments
            .iter()
            .map(|s| s.source_duration.seconds())
            .sum::<f64>();
        let include_audio = matches!(request.audio, AudioOutput::Preserve { .. });
        for segment in &request.segments {
            let metadata = self.probe(&ProbeRequest {
                input: segment.input.clone(),
            })?;
            if let Some(duration) = metadata.duration
                && segment.source_start.seconds() + segment.source_duration.seconds()
                    > duration.seconds() + 0.000_001
            {
                return Err(MediaError::InvalidRequest(format!(
                    "segment source range ends at {:.6}s but {} is only {:.6}s",
                    segment.source_start.seconds() + segment.source_duration.seconds(),
                    segment.input.display(),
                    duration.seconds()
                )));
            }
            if include_audio {
                let wanted = segment.audio_stream;
                let found = metadata.streams.iter().any(|s| {
                    s.codec_type == StreamKind::Audio && wanted.is_none_or(|i| s.index == i)
                });
                if !found {
                    return Err(MediaError::Unsupported(format!(
                        "preserve audio requested but {} has no selected audio stream",
                        segment.input.display()
                    )));
                }
            }
        }
        let filter = build_filter(request, include_audio);
        let mut command = Command::new(&self.ffmpeg);
        command.args(["-v", "error"]);
        if request.overwrite {
            command.arg("-y");
        } else {
            command.arg("-n");
        }
        for segment in &request.segments {
            command.arg("-i").arg(&segment.input);
        }
        command.args(["-filter_complex", &filter, "-map", "[vout]"]);
        if include_audio {
            command.args(["-map", "[aout]"]);
        } else {
            command.arg("-an");
        }
        let codec = match request.video.codec {
            VideoCodec::H264 => "libx264",
            VideoCodec::Hevc => "libx265",
            VideoCodec::Vp9 => "libvpx-vp9",
        };
        command.args(["-c:v", codec]);
        if let Some(crf) = request.video.crf {
            command.args(["-crf", &crf.to_string()]);
        }
        if include_audio {
            command.args(["-c:a", "aac", "-ar", "48000", "-ac", "2"]);
        }
        match request.container {
            Container::Mp4 => {
                command.args(["-f", "mp4", "-movflags", "+faststart"]);
            }
            Container::Matroska => {
                command.args(["-f", "matroska"]);
            }
        }
        let staging = staging_path(&request.output);
        command
            .args(["-progress", "pipe:1", "-nostats"])
            .arg(&staging);
        if let Err(error) = process::run_ffmpeg(
            command,
            cancel,
            Some(total),
            ProgressStage::Encoding,
            progress,
            self.admission
                .as_deref()
                .map(|admission| (Some(staging.as_path()), admission)),
        ) {
            let _ = fs::remove_file(&staging);
            return Err(error);
        }
        progress(ProgressUpdate {
            stage: ProgressStage::Verifying,
            fraction: None,
            media_time: None,
        });
        let tolerance = MediaTime {
            ticks: i64::from(request.video.frame_rate.denominator) * 2,
            timebase: request.video.frame_rate.numerator.max(1),
        };
        let mut verification = match self.verify(
            &VerifyRequest {
                input: staging.clone(),
                expected_width: Some(request.video.width),
                expected_height: Some(request.video.height),
                expected_duration: Some(MediaTime {
                    ticks: (total * 1_000_000.0).round() as i64,
                    timebase: 1_000_000,
                }),
                duration_tolerance: Some(tolerance),
                expected_video_streams: Some(1),
                expected_audio_streams: Some(u32::from(include_audio)),
                decode_samples: 3,
            },
            cancel,
            progress,
        ) {
            Ok(report) => report,
            Err(error) => {
                let _ = fs::remove_file(&staging);
                return Err(error);
            }
        };
        if !verification.passed {
            let detail = verification
                .checks
                .iter()
                .filter(|c| !c.passed)
                .map(|c| c.detail.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            let _ = fs::remove_file(&staging);
            return Err(MediaError::VerificationFailed(detail));
        }
        fs::rename(&staging, &request.output)?;
        verification.metadata.input = request.output.clone();
        Ok(ExportResult {
            output: request.output.clone(),
            metadata: verification.metadata.clone(),
            verification,
            text_layouts: Vec::new(),
        })
    }

    pub fn render_timeline(
        &self,
        request: &TimelineExportRequest,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(ProgressUpdate),
    ) -> MediaResult<ExportResult> {
        self.render_timeline_inner(request, None, None, cancel, progress)
    }

    pub fn validate_timeline_request(&self, request: &TimelineExportRequest) -> MediaResult<()> {
        validate_timeline_export(request)
    }

    pub fn render_timeline_v3(
        &self,
        request: &TimelineExportRequestV3,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(ProgressUpdate),
    ) -> MediaResult<ExportResult> {
        self.validate_v3_handles(&request.base, &request.visual)?;
        self.render_timeline_inner(&request.base, Some(&request.visual), None, cancel, progress)
    }

    pub fn render_timeline_range(
        &self,
        request: &TimelineRangeRenderRequest,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(ProgressUpdate),
    ) -> MediaResult<ExportResult> {
        validate_timeline_range(request)?;
        self.render_timeline_inner(
            &request.timeline,
            None,
            Some((request.sequence_start, request.sequence_duration)),
            cancel,
            progress,
        )
    }

    pub fn render_timeline_range_v3(
        &self,
        request: &TimelineRangeRenderRequestV3,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(ProgressUpdate),
    ) -> MediaResult<ExportResult> {
        self.validate_v3_handles(&request.timeline.base, &request.timeline.visual)?;
        validate_timeline_range(&TimelineRangeRenderRequest {
            timeline: request.timeline.base.clone(),
            sequence_start: request.sequence_start,
            sequence_duration: request.sequence_duration,
        })?;
        self.render_timeline_inner(
            &request.timeline.base,
            Some(&request.timeline.visual),
            Some((request.sequence_start, request.sequence_duration)),
            cancel,
            progress,
        )
    }

    /// One ongoing FFmpeg process for a fixed project range. The returned
    /// producer owns its stdout stream and is cancelled when dropped.
    pub fn start_timeline_stream(
        &self,
        request: &TimelineRangeRenderRequest,
        cancel: CancellationToken,
        limits: crate::PreviewStreamLimits,
    ) -> MediaResult<crate::ContinuousPreviewProducer> {
        self.start_timeline_stream_inner(request, None, cancel, limits)
    }

    pub fn start_timeline_stream_v3(
        &self,
        request: &TimelineRangeRenderRequestV3,
        cancel: CancellationToken,
        limits: crate::PreviewStreamLimits,
    ) -> MediaResult<crate::ContinuousPreviewProducer> {
        self.validate_v3_handles(&request.timeline.base, &request.timeline.visual)?;
        let base = TimelineRangeRenderRequest {
            timeline: request.timeline.base.clone(),
            sequence_start: request.sequence_start,
            sequence_duration: request.sequence_duration,
        };
        self.start_timeline_stream_inner(&base, Some(&request.timeline.visual), cancel, limits)
    }

    fn start_timeline_stream_inner(
        &self,
        request: &TimelineRangeRenderRequest,
        visual: Option<&CompiledVisualGraphV3>,
        cancel: CancellationToken,
        limits: crate::PreviewStreamLimits,
    ) -> MediaResult<crate::ContinuousPreviewProducer> {
        validate_timeline_export(&request.timeline)?;
        validate_range_times(
            request.sequence_start,
            request.sequence_duration,
            request.timeline.duration,
            request.timeline.video.frame_rate,
            true,
        )?;
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        let mut resolved = request.timeline.clone();
        for path in request
            .timeline
            .video_clips
            .iter()
            .map(|clip| &clip.input)
            .chain(request.timeline.image_clips.iter().map(|clip| &clip.input))
            .chain(request.timeline.audio_clips.iter().map(|clip| &clip.input))
        {
            require_input(path)?;
        }
        for clip in &resolved.video_clips {
            self.validate_source_range(
                &clip.input,
                clip.source_start,
                clip.source_duration,
                StreamKind::Video,
                clip.stream,
            )?;
        }
        for clip in &resolved.image_clips {
            self.validate_source_range(
                &clip.input,
                MediaTime {
                    ticks: 0,
                    timebase: 1,
                },
                MediaTime {
                    ticks: 0,
                    timebase: 1,
                },
                StreamKind::Video,
                clip.stream,
            )?;
        }
        let mut source_error = None;
        resolved.audio_clips.retain(|clip| {
            match self.validate_source_range(
                &clip.input,
                clip.source_start,
                clip.source_duration,
                StreamKind::Audio,
                clip.stream,
            ) {
                Ok(()) => true,
                Err(MediaError::Unsupported(_)) if clip.optional => false,
                Err(error) => {
                    source_error = Some(error);
                    false
                }
            }
        });
        if let Some(error) = source_error {
            return Err(error);
        }
        let base = request.timeline.output.parent().ok_or_else(|| {
            MediaError::InvalidRequest("preview attempt output must have a parent directory".into())
        })?;
        fs::create_dir_all(base)?;
        let mut temporary = TemporaryArtifacts::default();
        let mut command = Command::new(&self.ffmpeg);
        command.args(["-hide_banner", "-loglevel", "error", "-nostdin"]);
        let duration = request.timeline.duration.seconds();
        command.args([
            "-f",
            "lavfi",
            "-t",
            &format!("{duration:.9}"),
            "-i",
            &format!(
                "color=c=black:s={}x{}:r={}/{}",
                request.timeline.video.width,
                request.timeline.video.height,
                request.timeline.video.frame_rate.numerator,
                request.timeline.video.frame_rate.denominator
            ),
        ]);
        for clip in &resolved.video_clips {
            command.arg("-i").arg(&clip.input);
        }
        for clip in &resolved.image_clips {
            command.args(["-loop", "1", "-i"]).arg(&clip.input);
        }
        for clip in &resolved.audio_clips {
            command.arg("-i").arg(&clip.input);
        }
        let name = request
            .timeline
            .output
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        for (index, caption) in resolved.captions.iter().enumerate() {
            let path = base.join(format!(".{name}.stream-caption-{index}.png"));
            temporary.track(path.clone());
            crate::caption::render_admitted(
                caption,
                request.timeline.video.width,
                request.timeline.video.height,
                &path,
                self.admission.as_deref(),
            )?;
            command.args(["-loop", "1", "-i"]).arg(&path);
        }
        let filter = build_timeline_filter_inner(
            &resolved,
            None,
            Some((request.sequence_start, request.sequence_duration)),
            visual,
        )?;
        command.args([
            "-filter_complex",
            &filter,
            "-map",
            "[vout]",
            "-map",
            "[aout]",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-g",
            "25",
            "-c:a",
            "aac",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-f",
            "mp4",
            "-movflags",
            "+frag_keyframe+empty_moov+default_base_moof",
            "-frag_duration",
            "500000",
            "-t",
            &format!("{:.9}", request.sequence_duration.seconds()),
            "-progress",
            "pipe:2",
            "-nostats",
            "pipe:1",
        ]);
        let caption_paths = std::mem::take(&mut temporary.paths);
        let producer = crate::preview_stream::spawn(
            command,
            cancel,
            limits,
            self.admission.clone(),
            caption_paths.clone(),
        );
        if producer.is_err() {
            for path in &caption_paths {
                let _ = fs::remove_file(path);
            }
        }
        producer
    }

    fn render_timeline_inner(
        &self,
        request: &TimelineExportRequest,
        visual: Option<&CompiledVisualGraphV3>,
        range: Option<(MediaTime, MediaTime)>,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(ProgressUpdate),
    ) -> MediaResult<ExportResult> {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        progress(ProgressUpdate {
            stage: ProgressStage::Probing,
            fraction: None,
            media_time: None,
        });
        validate_timeline_export(request)?;
        let mut resolved = request.clone();
        for path in request
            .video_clips
            .iter()
            .map(|c| &c.input)
            .chain(request.image_clips.iter().map(|c| &c.input))
            .chain(request.audio_clips.iter().map(|c| &c.input))
        {
            require_input(path)?;
            validate_output(path, &request.output, request.overwrite)?;
        }
        for clip in &request.video_clips {
            self.validate_source_range(
                &clip.input,
                clip.source_start,
                clip.source_duration,
                StreamKind::Video,
                clip.stream,
            )?;
        }
        for clip in &request.image_clips {
            self.validate_source_range(
                &clip.input,
                MediaTime {
                    ticks: 0,
                    timebase: 1,
                },
                MediaTime {
                    ticks: 0,
                    timebase: 1,
                },
                StreamKind::Video,
                clip.stream,
            )?;
        }
        let mut resolved_error = None;
        resolved.audio_clips.retain(|clip| {
            match self.validate_source_range(
                &clip.input,
                clip.source_start,
                clip.source_duration,
                StreamKind::Audio,
                clip.stream,
            ) {
                Ok(()) => true,
                Err(MediaError::Unsupported(_)) if clip.optional => false,
                Err(error) => {
                    resolved_error.replace(error);
                    false
                }
            }
        });
        if let Some(error) = resolved_error {
            return Err(error);
        }
        if let Some(parent) = request.output.parent() {
            fs::create_dir_all(parent)?;
        }
        let staging = staging_path(&request.output);
        let mut temporary = TemporaryArtifacts::default();
        temporary.track(staging.clone());
        let mut command = Command::new(&self.ffmpeg);
        command.args(["-v", "error"]);
        if request.overwrite {
            command.arg("-y");
        } else {
            command.arg("-n");
        }
        let duration = request.duration.seconds();
        command.args([
            "-f",
            "lavfi",
            "-t",
            &format!("{duration:.9}"),
            "-i",
            &format!(
                "color=c=black:s={}x{}:r={}/{}",
                request.video.width,
                request.video.height,
                request.video.frame_rate.numerator,
                request.video.frame_rate.denominator
            ),
        ]);
        for clip in &request.video_clips {
            command.arg("-i").arg(&clip.input);
        }
        for clip in &request.image_clips {
            command.args(["-loop", "1", "-i"]).arg(&clip.input);
        }
        for clip in &resolved.audio_clips {
            command.arg("-i").arg(&clip.input);
        }
        let staging_name = staging.file_name().unwrap_or_default().to_string_lossy();
        let caption_artifacts = resolved
            .captions
            .iter()
            .enumerate()
            .map(|(index, caption)| {
                let path = staging.with_file_name(format!(".{staging_name}.caption-{index}.png"));
                temporary.track(path.clone());
                let report = crate::caption::render_admitted(
                    caption,
                    request.video.width,
                    request.video.height,
                    &path,
                    self.admission.as_deref(),
                )?;
                command.args(["-loop", "1", "-i"]).arg(&path);
                Ok((path, report))
            })
            .collect::<MediaResult<Vec<_>>>()?;
        let text_layouts = caption_artifacts
            .into_iter()
            .map(|(_, report)| report)
            .collect();
        let filter = build_timeline_filter_inner(&resolved, None, range, visual)?;
        command.args([
            "-filter_complex",
            &filter,
            "-map",
            "[vout]",
            "-map",
            "[aout]",
        ]);
        let codec = match request.video.codec {
            VideoCodec::H264 => "libx264",
            VideoCodec::Hevc => "libx265",
            VideoCodec::Vp9 => "libvpx-vp9",
        };
        command.args(["-c:v", codec]);
        if let Some(crf) = request.video.crf {
            command.args(["-crf", &crf.to_string()]);
        }
        command.args(["-c:a", "aac", "-ar", "48000", "-ac", "2"]);
        match request.container {
            Container::Mp4 => {
                command.args(["-f", "mp4", "-movflags", "+faststart"]);
            }
            Container::Matroska => {
                command.args(["-f", "matroska"]);
            }
        }
        let output_duration = range.map_or(request.duration, |(_, duration)| duration);
        let output_seconds = output_duration.seconds();
        command
            .args(["-t", &format!("{output_seconds:.9}")])
            .args(["-progress", "pipe:1", "-nostats"])
            .arg(&staging);
        if let Err(error) = process::run_ffmpeg(
            command,
            cancel,
            Some(output_seconds),
            ProgressStage::Encoding,
            progress,
            self.admission
                .as_deref()
                .map(|admission| (Some(staging.as_path()), admission)),
        ) {
            let _ = fs::remove_file(&staging);
            return Err(error);
        }
        let tolerance = MediaTime {
            ticks: i64::from(request.video.frame_rate.denominator) * 2,
            timebase: request.video.frame_rate.numerator.max(1),
        };
        let mut verification = match self.verify(
            &VerifyRequest {
                input: staging.clone(),
                expected_width: Some(request.video.width),
                expected_height: Some(request.video.height),
                expected_duration: Some(output_duration),
                duration_tolerance: Some(tolerance),
                expected_video_streams: Some(1),
                expected_audio_streams: Some(1),
                decode_samples: 3,
            },
            cancel,
            progress,
        ) {
            Ok(v) if v.passed => v,
            Ok(v) => {
                let detail = v
                    .checks
                    .iter()
                    .filter(|c| !c.passed)
                    .map(|c| c.detail.as_str())
                    .collect::<Vec<_>>()
                    .join("; ");
                let _ = fs::remove_file(&staging);
                return Err(MediaError::VerificationFailed(detail));
            }
            Err(e) => {
                let _ = fs::remove_file(&staging);
                return Err(e);
            }
        };
        fs::rename(&staging, &request.output)?;
        verification.metadata.input = request.output.clone();
        Ok(ExportResult {
            output: request.output.clone(),
            metadata: verification.metadata.clone(),
            verification,
            text_layouts,
        })
    }

    pub fn render_timeline_frame(
        &self,
        request: &TimelineFrameRenderRequest,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(ProgressUpdate),
    ) -> MediaResult<RenderedTimelineFrame> {
        self.render_timeline_frame_inner(request, None, cancel, progress)
    }

    pub fn render_timeline_frame_v3(
        &self,
        request: &TimelineFrameRenderRequestV3,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(ProgressUpdate),
    ) -> MediaResult<RenderedTimelineFrame> {
        self.validate_v3_handles(&request.timeline.base, &request.timeline.visual)?;
        let base = TimelineFrameRenderRequest {
            timeline: request.timeline.base.clone(),
            sequence_time: request.sequence_time,
        };
        self.render_timeline_frame_inner(&base, Some(&request.timeline.visual), cancel, progress)
    }

    fn render_timeline_frame_inner(
        &self,
        request: &TimelineFrameRenderRequest,
        visual: Option<&CompiledVisualGraphV3>,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(ProgressUpdate),
    ) -> MediaResult<RenderedTimelineFrame> {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        let mut timeline = request.timeline.clone();
        // A frame has visual dependencies only. Audio metadata, files, and
        // filter chains must not affect whether it can be rendered.
        timeline.audio_clips.clear();
        validate_timeline_export(&timeline)?;
        validate_sequence_frame_time(
            request.sequence_time,
            timeline.duration,
            timeline.video.frame_rate,
        )?;
        progress(ProgressUpdate {
            stage: ProgressStage::Probing,
            fraction: None,
            media_time: Some(request.sequence_time),
        });
        if timeline.output.exists() && !timeline.overwrite {
            return Err(MediaError::OutputExists(
                timeline.output.display().to_string(),
            ));
        }
        for path in timeline
            .video_clips
            .iter()
            .map(|clip| &clip.input)
            .chain(timeline.image_clips.iter().map(|clip| &clip.input))
        {
            require_input(path)?;
            validate_output(path, &timeline.output, timeline.overwrite)?;
        }
        for clip in &timeline.video_clips {
            self.validate_source_range(
                &clip.input,
                clip.source_start,
                clip.source_duration,
                StreamKind::Video,
                clip.stream,
            )?;
        }
        for clip in &timeline.image_clips {
            self.validate_source_range(
                &clip.input,
                MediaTime {
                    ticks: 0,
                    timebase: 1,
                },
                MediaTime {
                    ticks: 0,
                    timebase: 1,
                },
                StreamKind::Video,
                clip.stream,
            )?;
        }
        if let Some(parent) = timeline.output.parent() {
            fs::create_dir_all(parent)?;
        }
        let staging = staging_path(&timeline.output);
        let mut temporary = TemporaryArtifacts::default();
        temporary.track(staging.clone());
        let mut command = Command::new(&self.ffmpeg);
        command.args(["-v", "error", "-n"]);
        let duration = timeline.duration.seconds();
        command.args([
            "-f",
            "lavfi",
            "-t",
            &format!("{duration:.9}"),
            "-i",
            &format!(
                "color=c=black:s={}x{}:r={}/{}",
                timeline.video.width,
                timeline.video.height,
                timeline.video.frame_rate.numerator,
                timeline.video.frame_rate.denominator
            ),
        ]);
        for clip in &timeline.video_clips {
            command.arg("-i").arg(&clip.input);
        }
        for clip in &timeline.image_clips {
            command.args(["-loop", "1", "-i"]).arg(&clip.input);
        }
        let mut caption_files = Vec::new();
        let mut text_layouts = Vec::new();
        for (index, caption) in timeline.captions.iter().enumerate() {
            if cancel.is_cancelled() {
                for path in &caption_files {
                    let _ = fs::remove_file(path);
                }
                return Err(MediaError::Cancelled);
            }
            let staging_name = staging.file_name().unwrap_or_default().to_string_lossy();
            let path = staging.with_file_name(format!(".{staging_name}.caption-{index}.png"));
            temporary.track(path.clone());
            match crate::caption::render_admitted(
                caption,
                timeline.video.width,
                timeline.video.height,
                &path,
                self.admission.as_deref(),
            ) {
                Ok(report) => {
                    command.args(["-loop", "1", "-i"]).arg(&path);
                    caption_files.push(path);
                    text_layouts.push(report);
                }
                Err(problem) => {
                    let _ = fs::remove_file(&path);
                    for path in &caption_files {
                        let _ = fs::remove_file(path);
                    }
                    return Err(problem);
                }
            }
        }
        let filter =
            match build_timeline_filter_inner(&timeline, Some(request.sequence_time), None, visual)
            {
                Ok(filter) => filter,
                Err(problem) => {
                    for path in &caption_files {
                        let _ = fs::remove_file(path);
                    }
                    return Err(problem);
                }
            };
        command.args([
            "-filter_complex",
            &filter,
            "-map",
            "[vout]",
            "-frames:v",
            "1",
            "-update",
            "1",
            "-c:v",
            "png",
            "-f",
            "image2",
            "-progress",
            "pipe:1",
            "-nostats",
        ]);
        command.arg(&staging);
        let result = process::run_ffmpeg(
            command,
            cancel,
            None,
            ProgressStage::Encoding,
            progress,
            self.admission
                .as_deref()
                .map(|admission| (Some(staging.as_path()), admission)),
        );
        for path in &caption_files {
            let _ = fs::remove_file(path);
        }
        if let Err(problem) = result {
            let _ = fs::remove_file(&staging);
            return Err(problem);
        }
        let image = match fs::read(&staging)
            .map_err(MediaError::Io)
            .and_then(|bytes| {
                image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
                    .map_err(|error| MediaError::VerificationFailed(error.to_string()))
            }) {
            Ok(image) => image,
            Err(problem) => {
                let _ = fs::remove_file(&staging);
                return Err(problem);
            }
        };
        if image.width() != timeline.video.width || image.height() != timeline.video.height {
            let _ = fs::remove_file(&staging);
            return Err(MediaError::VerificationFailed(format!(
                "rendered frame is {}x{}, expected {}x{}",
                image.width(),
                image.height(),
                timeline.video.width,
                timeline.video.height
            )));
        }
        if cancel.is_cancelled() {
            let _ = fs::remove_file(&staging);
            return Err(MediaError::Cancelled);
        }
        if let Err(problem) = fs::rename(&staging, &timeline.output) {
            let _ = fs::remove_file(&staging);
            return Err(MediaError::Io(problem));
        }
        Ok(RenderedTimelineFrame {
            output: timeline.output,
            actual_sequence_time: request.sequence_time,
            width: image.width(),
            height: image.height(),
            text_layouts,
        })
    }

    fn validate_source_range(
        &self,
        input: &Path,
        start: MediaTime,
        duration: MediaTime,
        kind: StreamKind,
        selected: Option<u32>,
    ) -> MediaResult<()> {
        let metadata = self.probe(&ProbeRequest {
            input: input.into(),
        })?;
        if !metadata
            .streams
            .iter()
            .any(|s| s.codec_type == kind && selected.is_none_or(|i| s.index == i))
        {
            return Err(MediaError::Unsupported(format!(
                "{} has no selected {:?} stream",
                input.display(),
                kind
            )));
        }
        if let Some(actual) = metadata.duration
            && start.seconds() + duration.seconds() > actual.seconds() + 0.000_001
        {
            return Err(MediaError::InvalidRequest(format!(
                "requested source range ends at {:.6}s but {} is only {:.6}s",
                start.seconds() + duration.seconds(),
                input.display(),
                actual.seconds()
            )));
        }
        Ok(())
    }

    fn validate_v3_handles(
        &self,
        timeline: &TimelineExportRequest,
        visual: &CompiledVisualGraphV3,
    ) -> MediaResult<()> {
        for (index, source_start, source_duration) in
            crate::composition_v3::transition_source_handles(timeline, visual)?
        {
            let clip = &timeline.video_clips[index];
            let metadata = self.probe(&ProbeRequest {
                input: clip.input.clone(),
            })?;
            let actual = metadata.duration.ok_or_else(|| {
                MediaError::InvalidRequest("crossfade requires a source with known duration".into())
            })?;
            if source_start < 0.0 || source_start + source_duration > actual.seconds() + 0.000_001 {
                return Err(MediaError::InvalidRequest(
                    "crossfade source handle exceeds media duration".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn verify(
        &self,
        request: &VerifyRequest,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(ProgressUpdate),
    ) -> MediaResult<VerificationReport> {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        progress(ProgressUpdate {
            stage: ProgressStage::Verifying,
            fraction: None,
            media_time: None,
        });
        let metadata = self.probe(&ProbeRequest {
            input: request.input.clone(),
        })?;
        let videos: Vec<_> = metadata
            .streams
            .iter()
            .filter(|s| s.codec_type == StreamKind::Video)
            .collect();
        let audios = metadata
            .streams
            .iter()
            .filter(|s| s.codec_type == StreamKind::Audio)
            .count() as u32;
        let mut checks = Vec::new();
        check_opt(
            &mut checks,
            "video_streams",
            request.expected_video_streams,
            videos.len() as u32,
        );
        check_opt(
            &mut checks,
            "audio_streams",
            request.expected_audio_streams,
            audios,
        );
        if let Some(expected) = request.expected_width {
            check_opt(
                &mut checks,
                "width",
                Some(expected),
                videos.first().and_then(|v| v.width).unwrap_or(0),
            );
        }
        if let Some(expected) = request.expected_height {
            check_opt(
                &mut checks,
                "height",
                Some(expected),
                videos.first().and_then(|v| v.height).unwrap_or(0),
            );
        }
        if let Some(expected) = request.expected_duration {
            let actual = metadata.duration.map(MediaTime::seconds).unwrap_or(-1.0);
            let tolerance = request
                .duration_tolerance
                .map(MediaTime::seconds)
                .unwrap_or(0.05);
            let passed = (actual - expected.seconds()).abs() <= tolerance;
            checks.push(VerificationCheck {
                name: "duration".into(),
                passed,
                detail: format!(
                    "expected {:.6}s ± {:.6}s, got {:.6}s",
                    expected.seconds(),
                    tolerance,
                    actual
                ),
            });
        }
        let duration = metadata.duration.map(MediaTime::seconds).unwrap_or(0.0);
        let sample_count = request.decode_samples.max(1);
        let mut decoded_samples = Vec::new();
        for index in 0..sample_count {
            if cancel.is_cancelled() {
                return Err(MediaError::Cancelled);
            }
            let seconds = if duration > 0.0 {
                duration * (index + 1) as f64 / (sample_count + 1) as f64
            } else {
                0.0
            };
            let requested_time = MediaTime {
                ticks: (seconds * 1_000_000.0).round() as i64,
                timebase: 1_000_000,
            };
            let mut command = Command::new(&self.ffmpeg);
            command
                .args(["-v", "error", "-ss", &format!("{seconds:.6}"), "-i"])
                .arg(&request.input)
                .args([
                    "-map",
                    "0:v:0",
                    "-frames:v",
                    "1",
                    "-f",
                    "null",
                    "-",
                    "-progress",
                    "pipe:1",
                    "-nostats",
                ]);
            let decoded = process::run_ffmpeg(
                command,
                cancel,
                None,
                ProgressStage::Verifying,
                progress,
                self.admission.as_deref().map(|admission| (None, admission)),
            )
            .is_ok();
            decoded_samples.push(DecodedSample {
                requested_time,
                decoded,
            });
        }
        checks.push(VerificationCheck {
            name: "sample_decode".into(),
            passed: decoded_samples.iter().all(|s| s.decoded),
            detail: format!(
                "decoded {}/{} requested samples",
                decoded_samples.iter().filter(|s| s.decoded).count(),
                decoded_samples.len()
            ),
        });
        Ok(VerificationReport {
            passed: checks.iter().all(|c| c.passed),
            checks,
            metadata,
            decoded_samples,
        })
    }

    fn select_frame_pts(
        &self,
        input: &Path,
        selected: Option<u32>,
        requested: MediaTime,
        alignment: FrameAlignment,
        cancel: &CancellationToken,
    ) -> MediaResult<(u32, FrameRate, i64)> {
        let metadata = self.probe(&ProbeRequest {
            input: input.into(),
        })?;
        let stream = metadata
            .streams
            .iter()
            .find(|s| s.codec_type == StreamKind::Video && selected.is_none_or(|i| s.index == i))
            .ok_or_else(|| {
                MediaError::InvalidRequest("selected video stream does not exist".into())
            })?;
        let tb = stream
            .time_base
            .ok_or_else(|| MediaError::InvalidToolOutput {
                tool: "ffprobe".into(),
                detail: "video stream lacks time_base".into(),
            })?;
        let mut command = Command::new(&self.ffprobe);
        command
            .args([
                "-v",
                "error",
                "-select_streams",
                &stream.index.to_string(),
                "-show_entries",
                "frame=best_effort_timestamp",
                "-of",
                "csv=p=0",
            ])
            .arg(input)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut owned =
            process::spawn_command(&mut command, "ffprobe", self.admission.as_deref(), None)?;
        let child = owned.child();
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let (tx, rx) = mpsc::channel();
        let stdout_reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
        let stderr_reader = thread::spawn(move || {
            let mut value = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut value);
            value
        });
        let mut candidate: Option<i64> = None;
        loop {
            if cancel.is_cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(MediaError::Cancelled);
            }
            while let Ok(line) = rx.try_recv() {
                if let Some(pts) = parse_frame_pts(&line) {
                    update_candidate(&mut candidate, pts, requested, tb, alignment);
                }
            }
            if let Some(status) = child.try_wait()? {
                let _ = stdout_reader.join();
                while let Ok(line) = rx.try_recv() {
                    if let Some(pts) = parse_frame_pts(&line) {
                        update_candidate(&mut candidate, pts, requested, tb, alignment);
                    }
                }
                let stderr = stderr_reader.join().unwrap_or_default();
                if !status.success() {
                    return Err(MediaError::ProcessFailed {
                        tool: "ffprobe".into(),
                        status: status.code(),
                        stderr,
                    });
                }
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let chosen = candidate.ok_or_else(|| {
            MediaError::InvalidRequest("no frame satisfies the requested alignment".into())
        })?;
        Ok((stream.index, tb, chosen))
    }
}

fn fingerprint_tool(name: &str, configured_path: &Path) -> MediaResult<MediaToolFingerprint> {
    let executable =
        resolve_executable(configured_path).map_err(|error| MediaError::ToolUnavailable {
            tool: name.into(),
            detail: error.to_string(),
        })?;
    let cancel = CancellationToken::new();
    let output = process::checked_output_limited(
        Command::new(&executable).arg("-version"),
        name,
        &cancel,
        Duration::from_secs(10),
        None,
    )?;
    let version = String::from_utf8_lossy(&output)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    if version.is_empty() {
        return Err(MediaError::InvalidToolOutput {
            tool: name.into(),
            detail: "version output is empty".into(),
        });
    }

    let mut file = fs::File::open(&executable)?;
    let mut hasher = Sha256::new();
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        hasher.update(&chunk[..count]);
    }
    let binary_sha256 = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("");
    Ok(MediaToolFingerprint {
        name: name.into(),
        version,
        binary_sha256,
    })
}

fn resolve_executable(configured_path: &Path) -> std::io::Result<PathBuf> {
    let has_directory = configured_path.components().any(|component| {
        matches!(
            component,
            std::path::Component::RootDir
                | std::path::Component::Prefix(_)
                | std::path::Component::CurDir
                | std::path::Component::ParentDir
        )
    });
    if has_directory {
        return fs::canonicalize(configured_path);
    }
    let mut candidates = vec![configured_path.to_path_buf()];
    if !std::env::consts::EXE_SUFFIX.is_empty() && configured_path.extension().is_none() {
        candidates.push(PathBuf::from(format!(
            "{}{suffix}",
            configured_path.to_string_lossy(),
            suffix = std::env::consts::EXE_SUFFIX
        )));
    }
    for directory in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
        for candidate in &candidates {
            let path = directory.join(candidate);
            if path.is_file() {
                return fs::canonicalize(path);
            }
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("{} was not found on PATH", configured_path.display()),
    ))
}

fn parse_frame_pts(line: &str) -> Option<i64> {
    line.split(',').next()?.trim().parse().ok()
}

fn require_input(path: &Path) -> MediaResult<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(MediaError::NotFound(path.display().to_string()))
    }
}
fn validate_output(input: &Path, output: &Path, overwrite: bool) -> MediaResult<()> {
    if input == output
        || (input.exists()
            && output.exists()
            && fs::canonicalize(input).ok() == fs::canonicalize(output).ok())
    {
        return Err(MediaError::InvalidRequest(
            "output must not overwrite an input".into(),
        ));
    }
    if output.exists() && !overwrite {
        return Err(MediaError::OutputExists(output.display().to_string()));
    }
    Ok(())
}
fn staging_path(output: &Path) -> PathBuf {
    let mut name = output.file_name().unwrap_or_default().to_os_string();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |v| v.as_nanos());
    name.push(format!(".opencut-{}-{nonce}.staging", std::process::id()));
    output.with_file_name(name)
}
fn validate_export(r: &ExportRequest) -> MediaResult<()> {
    if r.segments.is_empty() {
        return Err(MediaError::InvalidRequest(
            "export requires at least one segment".into(),
        ));
    }
    if !r.requested_features.is_empty() {
        return Err(MediaError::Unsupported(format!(
            "unimplemented requested features: {}",
            r.requested_features.join(", ")
        )));
    }
    if r.video.width == 0
        || r.video.height == 0
        || r.video.frame_rate.numerator == 0
        || r.video.frame_rate.denominator == 0
    {
        return Err(MediaError::InvalidRequest(
            "output dimensions and frame rate must be positive".into(),
        ));
    }
    if r.container == Container::Mp4 && r.video.codec == VideoCodec::Vp9 {
        return Err(MediaError::Unsupported(
            "VP9 in MP4 is not an enabled export combination".into(),
        ));
    }
    for s in &r.segments {
        if s.source_start.ticks < 0 || s.source_duration.ticks <= 0 {
            return Err(MediaError::InvalidRequest(
                "segment start must be non-negative and duration positive".into(),
            ));
        }
    }
    Ok(())
}

fn validate_timeline_export(r: &TimelineExportRequest) -> MediaResult<()> {
    if r.duration.ticks <= 0
        || r.video.width == 0
        || r.video.height == 0
        || r.video.frame_rate.numerator == 0
        || r.video.frame_rate.denominator == 0
    {
        return Err(MediaError::InvalidRequest(
            "timeline duration, dimensions, and frame rate must be positive".into(),
        ));
    }
    if r.container == Container::Mp4 && r.video.codec == VideoCodec::Vp9 {
        return Err(MediaError::Unsupported(
            "VP9 in MP4 is not an enabled export combination".into(),
        ));
    }
    for (start, duration) in r
        .video_clips
        .iter()
        .map(|c| (c.timeline_start, c.timeline_duration))
        .chain(
            r.image_clips
                .iter()
                .map(|c| (c.timeline_start, c.timeline_duration)),
        )
        .chain(
            r.audio_clips
                .iter()
                .map(|c| (c.timeline_start, c.timeline_duration)),
        )
    {
        if start.ticks < 0
            || duration.ticks <= 0
            || start.seconds() + duration.seconds() > r.duration.seconds() + 0.000_001
        {
            return Err(MediaError::InvalidRequest(
                "clip lies outside timeline duration".into(),
            ));
        }
    }
    for clip in &r.audio_clips {
        retime_sample_rate(clip.source_duration, clip.timeline_duration)?;
    }
    for (opacity, sx, sy) in r
        .video_clips
        .iter()
        .map(|c| (c.opacity, c.scale_x, c.scale_y))
        .chain(
            r.image_clips
                .iter()
                .map(|c| (c.opacity, c.scale_x, c.scale_y)),
        )
    {
        if !(0.0..=1.0).contains(&opacity)
            || sx < 0.0
            || sy < 0.0
            || !sx.is_finite()
            || !sy.is_finite()
        {
            return Err(MediaError::InvalidRequest(
                "visual opacity must be 0..1 and scales finite/non-negative".into(),
            ));
        }
    }
    Ok(())
}

fn validate_sequence_frame_time(
    time: MediaTime,
    duration: MediaTime,
    fps: FrameRate,
) -> MediaResult<()> {
    if time.timebase == 0 || duration.timebase == 0 {
        return Err(MediaError::InvalidRequest(
            "frame and timeline timebase must be positive".into(),
        ));
    }
    if time.ticks < 0
        || rational_cmp(
            time.ticks,
            FrameRate {
                numerator: 1,
                denominator: time.timebase,
            },
            duration.ticks,
            duration.timebase,
        ) != Ordering::Less
    {
        return Err(MediaError::InvalidRequest(
            "sequence_time must be inside the half-open timeline duration".into(),
        ));
    }
    let grid_numerator = time.ticks as i128 * fps.numerator as i128;
    let grid_denominator = time.timebase as i128 * fps.denominator as i128;
    if grid_denominator <= 0 || grid_numerator.rem_euclid(grid_denominator) != 0 {
        return Err(MediaError::InvalidRequest(
            "sequence_time must be aligned to the sequence frame rate".into(),
        ));
    }
    Ok(())
}

fn validate_timeline_range(request: &TimelineRangeRenderRequest) -> MediaResult<()> {
    validate_timeline_export(&request.timeline)?;
    validate_range_times(
        request.sequence_start,
        request.sequence_duration,
        request.timeline.duration,
        request.timeline.video.frame_rate,
        false,
    )
}

fn validate_range_times(
    start: MediaTime,
    duration: MediaTime,
    timeline_duration: MediaTime,
    frame_rate: FrameRate,
    allow_final_tail: bool,
) -> MediaResult<()> {
    if duration.timebase == 0 || duration.ticks <= 0 {
        return Err(MediaError::InvalidRequest(
            "sequence_duration must be positive".into(),
        ));
    }
    validate_frame_grid_time(start, frame_rate, "sequence_start")?;
    let overflow = || MediaError::InvalidRequest("timeline range arithmetic overflow".into());
    let end_ticks = (start.ticks as i128)
        .checked_mul(duration.timebase as i128)
        .and_then(|left| {
            (duration.ticks as i128)
                .checked_mul(start.timebase as i128)
                .and_then(|right| left.checked_add(right))
        })
        .ok_or_else(overflow)?;
    let end_timebase = (start.timebase as i128)
        .checked_mul(duration.timebase as i128)
        .ok_or_else(overflow)?;
    let frame_grid_numerator = end_ticks
        .checked_mul(frame_rate.numerator as i128)
        .ok_or_else(overflow)?;
    let frame_grid_denominator = end_timebase
        .checked_mul(frame_rate.denominator as i128)
        .ok_or_else(overflow)?;
    let range_end_scaled = end_ticks
        .checked_mul(timeline_duration.timebase as i128)
        .ok_or_else(overflow)?;
    let timeline_end_scaled = (timeline_duration.ticks as i128)
        .checked_mul(end_timebase)
        .ok_or_else(overflow)?;
    if frame_grid_denominator <= 0
        || (frame_grid_numerator.rem_euclid(frame_grid_denominator) != 0
            && !(allow_final_tail && range_end_scaled == timeline_end_scaled))
    {
        return Err(MediaError::InvalidRequest(
            "sequence range end must be aligned to the sequence frame rate".into(),
        ));
    }
    if range_end_scaled > timeline_end_scaled {
        return Err(MediaError::InvalidRequest(
            "sequence range ends after the timeline duration".into(),
        ));
    }
    Ok(())
}

fn validate_frame_grid_time(time: MediaTime, fps: FrameRate, name: &str) -> MediaResult<()> {
    if time.timebase == 0 || time.ticks < 0 {
        return Err(MediaError::InvalidRequest(format!(
            "{name} must be non-negative with a positive timebase"
        )));
    }
    let numerator = time.ticks as i128 * fps.numerator as i128;
    let denominator = time.timebase as i128 * fps.denominator as i128;
    if denominator <= 0 || numerator.rem_euclid(denominator) != 0 {
        return Err(MediaError::InvalidRequest(format!(
            "{name} must be aligned to the sequence frame rate"
        )));
    }
    Ok(())
}

fn build_timeline_filter_inner(
    r: &TimelineExportRequest,
    frame_time: Option<MediaTime>,
    range: Option<(MediaTime, MediaTime)>,
    visual: Option<&CompiledVisualGraphV3>,
) -> MediaResult<String> {
    if let Some(visual) = visual {
        return crate::composition_v3::build_filter(r, visual, frame_time, range);
    }
    let mut chains = vec![format!("[0:v]format=yuv420p[base0]")];
    let mut base = "base0".to_string();
    let mut visuals: Vec<(i32, bool, usize)> = r
        .video_clips
        .iter()
        .enumerate()
        .map(|(i, c)| (c.z_index, false, i))
        .chain(
            r.image_clips
                .iter()
                .enumerate()
                .map(|(i, c)| (c.z_index, true, i)),
        )
        .collect();
    visuals.sort_by_key(|(z, is_image, index)| (*z, *is_image, *index));
    for (layer, (_, is_image, index)) in visuals.into_iter().enumerate() {
        let (
            input,
            stream,
            timeline_start,
            timeline_duration,
            opacity,
            px,
            py,
            sx,
            sy,
            rotation,
            curves,
            prefix,
        ) = if is_image {
            let c = &r.image_clips[index];
            (
                1 + r.video_clips.len() + index,
                c.stream.map_or_else(|| "v:0".into(), |n| n.to_string()),
                c.timeline_start,
                c.timeline_duration,
                c.opacity,
                c.position_x,
                c.position_y,
                c.scale_x,
                c.scale_y,
                c.rotation_degrees,
                &c.curves,
                format!("setpts=PTS-STARTPTS+{:.9}/TB", c.timeline_start.seconds()),
            )
        } else {
            let c = &r.video_clips[index];
            let rate = c.source_duration.seconds() / c.timeline_duration.seconds();
            if !(0.01..=100.0).contains(&rate) {
                return Err(MediaError::Unsupported(format!(
                    "video rate {rate} is outside supported range"
                )));
            }
            let start = c.source_start.seconds();
            let end = start + c.source_duration.seconds();
            (
                1 + index,
                c.stream.map_or_else(|| "v:0".into(), |n| n.to_string()),
                c.timeline_start,
                c.timeline_duration,
                c.opacity,
                c.position_x,
                c.position_y,
                c.scale_x,
                c.scale_y,
                c.rotation_degrees,
                &c.curves,
                format!(
                    "trim=start={start:.9}:end={end:.9},setpts=(PTS-STARTPTS)/{rate:.12}+{:.9}/TB",
                    c.timeline_start.seconds()
                ),
            )
        };
        let animated = !curves.opacity.segments.is_empty()
            || !curves.position_x.segments.is_empty()
            || !curves.position_y.segments.is_empty()
            || !curves.scale_x.segments.is_empty()
            || !curves.scale_y.segments.is_empty()
            || !curves.rotation_degrees.segments.is_empty();
        if !animated && (opacity == 0.0 || sx == 0.0 || sy == 0.0) {
            continue;
        }
        let label = format!("visual{layer}");
        let timeline_start_seconds = timeline_start.seconds();
        let local_t = format!("t-{timeline_start_seconds:.9}");
        let px_expr = curve_expr(&curves.position_x, px, &local_t)?;
        let py_expr = curve_expr(&curves.position_y, py, &local_t)?;
        if animated {
            // Stabilize the absolute presentation grid before subtracting the
            // clip start: the clip's own clock may have a sub-frame offset.
            let geq_t = format!(
                "(round(T*{}/{})*{}/{}-({}/{}))",
                r.video.frame_rate.numerator,
                r.video.frame_rate.denominator,
                r.video.frame_rate.denominator,
                r.video.frame_rate.numerator,
                timeline_start.ticks,
                timeline_start.timebase,
            );
            let quantize = |value: String| format!("round(({value})*1000000)/1000000");
            let opacity_value = quantize(curve_expr(&curves.opacity, opacity as f64, &geq_t)?);
            let x_value = quantize(curve_expr(&curves.position_x, px, &geq_t)?);
            let y_value = quantize(curve_expr(&curves.position_y, py, &geq_t)?);
            let sx_value = quantize(curve_expr(&curves.scale_x, sx, &geq_t)?);
            let sy_value = quantize(curve_expr(&curves.scale_y, sy, &geq_t)?);
            let rotation_value = quantize(curve_expr(&curves.rotation_degrees, rotation, &geq_t)?);
            let radians = format!("(({rotation_value})*PI/180)");
            let dx = format!("(X-W/2-({x_value}))");
            let dy = format!("(Y-H/2-({y_value}))");
            let source_x = quantize(format!(
                "W/2+(({dx})*cos({radians})+({dy})*sin({radians}))/max(({sx_value}),0.000001)"
            ));
            let source_y = quantize(format!(
                "H/2+(-({dx})*sin({radians})+({dy})*cos({radians}))/max(({sy_value}),0.000001)"
            ));
            let visible = format!(
                "gt(({sx_value}),0)*gt(({sy_value}),0)*gte(({source_x}),0)*lt(({source_x}),W)*gte(({source_y}),0)*lt(({source_y}),H)"
            );
            chains.push(format!("[{input}:{stream}]{prefix},fps={}/{},tpad=stop_mode=clone:stop_duration={:.9},scale={}:{}:force_original_aspect_ratio=decrease,format=rgba,pad={}:{}:(ow-iw)/2:(oh-ih)/2:color=black@0,geq=r='r({source_x},{source_y})':g='g({source_x},{source_y})':b='b({source_x},{source_y})':a='alpha({source_x},{source_y})*({opacity_value})*({visible})'[{label}]", r.video.frame_rate.numerator, r.video.frame_rate.denominator,timeline_duration.seconds(), r.video.width, r.video.height, r.video.width, r.video.height));
        } else {
            let radians = rotation * std::f64::consts::PI / 180.0;
            chains.push(format!("[{input}:{stream}]{prefix},fps={}/{},tpad=stop_mode=clone:stop_duration={:.9},scale={}:{}:force_original_aspect_ratio=decrease,scale=iw*{sx:.9}:ih*{sy:.9},rotate={radians:.12}:ow='ceil(rotw({radians:.12}))':oh='ceil(roth({radians:.12}))':c=none,format=rgba,colorchannelmixer=aa={opacity:.6}[{label}]",r.video.frame_rate.numerator,r.video.frame_rate.denominator,timeline_duration.seconds(),r.video.width,r.video.height));
        }
        let next = format!("base{}", layer + 1);
        let (overlay_x, overlay_y) = if animated {
            ("0".to_string(), "0".to_string())
        } else {
            (
                format!("(W-w)/2+({px_expr})"),
                format!("(H-h)/2+({py_expr})"),
            )
        };
        chains.push(format!("[{base}][{label}]overlay=x='{overlay_x}':y='{overlay_y}':eval=frame:eof_action=pass:enable='gte(t,{:.9})*lt(t,{:.9})'[{next}]",timeline_start.seconds(),timeline_start.seconds()+timeline_duration.seconds()));
        base = next;
    }
    finish_timeline_filter(r, frame_time, range, chains, base)
}

pub(crate) fn finish_timeline_filter(
    r: &TimelineExportRequest,
    frame_time: Option<MediaTime>,
    range: Option<(MediaTime, MediaTime)>,
    mut chains: Vec<String>,
    base: String,
) -> MediaResult<String> {
    let mut caption_base = base;
    let caption_offset = 1
        + r.video_clips.len()
        + r.image_clips.len()
        + if frame_time.is_some() {
            0
        } else {
            r.audio_clips.len()
        };
    for (i, caption) in r.captions.iter().enumerate() {
        let next = format!("caption{i}");
        chains.push(format!(
            "[{caption_base}][{}:v]overlay=eof_action=pass:enable='gte(t,{:.9})*lt(t,{:.9})'[{next}]",
            caption_offset + i,
            caption.start.seconds(),
            caption.start.seconds() + caption.duration.seconds()
        ));
        caption_base = next;
    }
    if let Some(frame_time) = frame_time {
        let frame_index = frame_time.ticks as i128 * r.video.frame_rate.numerator as i128
            / (frame_time.timebase as i128 * r.video.frame_rate.denominator as i128);
        if frame_index > 9_007_199_254_740_992 {
            return Err(MediaError::Unsupported(
                "sequence frame index exceeds FFmpeg's exact expression range".into(),
            ));
        }
        chains.push(format!(
            "[{caption_base}]select='eq(n\\,{frame_index})',setpts=PTS-STARTPTS[vout]"
        ));
        return Ok(chains.join(";"));
    }
    let (output_start, output_duration) = range
        .map_or((0.0, r.duration.seconds()), |(start, duration)| {
            (start.seconds(), duration.seconds())
        });
    let padded_end = output_start + output_duration;
    let audio_reset = if range.is_some() {
        ",asetpts=PTS-STARTPTS"
    } else {
        ""
    };
    chains.push(format!(
        "[{caption_base}]trim=start={output_start:.9}:duration={output_duration:.9},setpts=PTS-STARTPTS[vout]"
    ));
    if r.audio_clips.is_empty() {
        chains.push(format!(
        "anullsrc=r=48000:cl=stereo:d={padded_end:.9},atrim=start={output_start:.9}:duration={output_duration:.9}{audio_reset}[aout]"
    ));
    } else {
        let offset = 1 + r.video_clips.len() + r.image_clips.len();
        let mut labels = String::new();
        for (i, clip) in r.audio_clips.iter().enumerate() {
            let input = offset + i;
            let start = clip.source_start.seconds();
            let end = start + clip.source_duration.seconds();
            let rate = clip.source_duration.seconds() / clip.timeline_duration.seconds();
            if !(0.5..=2.0).contains(&rate) {
                return Err(MediaError::Unsupported(format!(
                    "audio constant rate {rate} requires unsupported chained atempo"
                )));
            }
            let stream = clip.stream.map_or_else(|| "a:0".into(), |n| n.to_string());
            let gain = curve_expr(&clip.gain_curve, clip.gain as f64, "t")?;
            let volume = if clip.gain_curve.segments.is_empty() {
                format!("volume={:.6}", clip.gain)
            } else {
                format!("volume='{gain}':eval=frame")
            };
            let delay_samples = time_to_sample_index(clip.timeline_start, 48_000)?;
            // Preserve the exact rational identity test before converting to
            // floating point. The project model does not carry a maintain-pitch
            // flag, so non-identity retiming follows Classic's default behavior:
            // change pitch with the clip rate. Pitch-preserving WSOLA can move
            // transients and does not meet the source-to-timeline mapping here.
            let identity_rate = i128::from(clip.source_duration.ticks)
                * i128::from(clip.timeline_duration.timebase)
                == i128::from(clip.timeline_duration.ticks)
                    * i128::from(clip.source_duration.timebase);
            let retime = if identity_rate {
                String::new()
            } else {
                let sample_rate = retime_sample_rate(clip.source_duration, clip.timeline_duration)?;
                format!("asetrate={sample_rate},aresample=48000,")
            };
            // Normalize the delayed samples to a zero-based sample clock before
            // downstream absolute-range trimming. Rate-changing filters may
            // retain a source-clock offset; without this reset, a retimed clip
            // can lose its leading delay despite adelay's exact sample count.
            chains.push(format!("[{input}:{stream}]atrim=start={start:.9}:end={end:.9},asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,{retime}{volume},adelay={delay_samples}S|{delay_samples}S,asetpts=N/SR/TB[a{i}]"));
            labels.push_str(&format!("[a{i}]"));
        }
        if r.audio_clips.len() == 1 {
            chains.push(format!(
                "[a0]apad=whole_dur={padded_end:.9},atrim=start={output_start:.9}:duration={output_duration:.9}{audio_reset}[aout]"
            ));
        } else {
            chains.push(format!(
                "{labels}amix=inputs={}:duration=longest:normalize=0,apad=whole_dur={padded_end:.9},atrim=start={output_start:.9}:duration={output_duration:.9}{audio_reset}[aout]",
                r.audio_clips.len()
            ));
        }
    }
    Ok(chains.join(";"))
}

/// Maps an exact rational time to the nearest output sample. Half-sample ties
/// select the later sample, matching timeline frame-alignment semantics.
pub(crate) fn time_to_sample_index(time: MediaTime, sample_rate: u32) -> MediaResult<u64> {
    if time.timebase == 0 {
        return Err(MediaError::InvalidRequest(
            "audio timeline timebase must be greater than zero".into(),
        ));
    }
    if time.ticks < 0 {
        return Err(MediaError::InvalidRequest(
            "audio timeline start cannot be negative".into(),
        ));
    }
    let numerator = i128::from(time.ticks) * i128::from(sample_rate);
    let denominator = i128::from(time.timebase);
    let samples = (numerator + denominator / 2) / denominator;
    u64::try_from(samples).map_err(|_| {
        MediaError::InvalidRequest("audio timeline start exceeds the sample index range".into())
    })
}

fn retime_sample_rate(source: MediaTime, timeline: MediaTime) -> MediaResult<u32> {
    if source.ticks <= 0 || source.timebase == 0 || timeline.ticks <= 0 || timeline.timebase == 0 {
        return Err(MediaError::InvalidRequest(
            "audio retime durations and timebases must be positive".into(),
        ));
    }
    let rate_numerator = i128::from(source.ticks) * i128::from(timeline.timebase);
    let rate_denominator = i128::from(source.timebase) * i128::from(timeline.ticks);
    if 2 * rate_numerator < rate_denominator || rate_numerator > 2 * rate_denominator {
        return Err(MediaError::Unsupported(
            "audio retime rate must be between 0.5 and 2.0".into(),
        ));
    }
    let numerator = rate_numerator * 48_000;
    let denominator = rate_denominator;
    let rounded_rate = (numerator + denominator / 2) / denominator;
    let rounded_rate = u32::try_from(rounded_rate).map_err(|_| {
        MediaError::InvalidRequest("audio retime sample rate exceeds its supported range".into())
    })?;
    if !(24_000..=96_000).contains(&rounded_rate) {
        return Err(MediaError::Unsupported(
            "audio retime sample rate must be between 24000 and 96000 Hz".into(),
        ));
    }
    let source_samples = rounded_positive_ratio(
        i128::from(source.ticks) * 48_000,
        i128::from(source.timebase),
    );
    let mapped_samples = rounded_positive_ratio(source_samples * 48_000, i128::from(rounded_rate));
    let timeline_samples = rounded_positive_ratio(
        i128::from(timeline.ticks) * 48_000,
        i128::from(timeline.timebase),
    );
    if mapped_samples.abs_diff(timeline_samples) > 1 {
        return Err(MediaError::Unsupported(
            "audio retime rate cannot be represented within one 48 kHz sample".into(),
        ));
    }
    Ok(rounded_rate)
}

fn rounded_positive_ratio(numerator: i128, denominator: i128) -> i128 {
    (numerator + denominator / 2) / denominator
}

pub(crate) fn curve_expr(curve: &EvaluatedCurve, fallback: f64, time: &str) -> MediaResult<String> {
    if curve.segments.is_empty() {
        return Ok(format!("{fallback:.12}"));
    }
    let mut expression = format!("{fallback:.12}");
    for segment in curve.segments.iter().rev() {
        let start = segment.start.seconds();
        let end = segment.end.seconds();
        if !start.is_finite()
            || !end.is_finite()
            || end <= start
            || !segment.start_value.is_finite()
            || !segment.end_value.is_finite()
        {
            return Err(MediaError::InvalidRequest(
                "animation segments must be finite, ordered, and nonempty".into(),
            ));
        }
        if segment.cubic_bezier.is_some_and(|curve| {
            ![curve.x1, curve.y1, curve.x2, curve.y2]
                .into_iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
        }) {
            return Err(MediaError::InvalidRequest(
                "cubic Bezier control points must be finite and within 0..1".into(),
            ));
        }
        let value = if segment.start_value == segment.end_value {
            format!("{:.12}", segment.start_value)
        } else if let Some(control) = segment.cubic_bezier {
            let progress = format!("(({time})-{start:.12})/{:.12}", end - start);
            let eased = cubic_bezier_lookup(control, &progress);
            format!(
                "{:.12}+({:.12})*({eased})",
                segment.start_value,
                segment.end_value - segment.start_value
            )
        } else {
            format!(
                "{:.12}+({:.12})*(({time})-{start:.12})/{:.12}",
                segment.start_value,
                segment.end_value - segment.start_value,
                end - start
            )
        };
        expression = format!("if(lt(({time}),{end:.12}),({value}),({expression}))");
    }
    Ok(expression)
}

fn cubic_bezier_lookup(
    curve: opencut_media_contracts::composition::CubicBezierCurve,
    progress: &str,
) -> String {
    const SUBDIVISIONS: usize = 32;
    let mut points = Vec::with_capacity(SUBDIVISIONS + 1);
    for index in 0..=SUBDIVISIONS {
        let x = index as f64 / SUBDIVISIONS as f64;
        let mut low = 0.0;
        let mut high = 1.0;
        for _ in 0..40 {
            let middle = (low + high) * 0.5;
            if bezier_component(middle, curve.x1, curve.x2) < x {
                low = middle;
            } else {
                high = middle;
            }
        }
        let t = (low + high) * 0.5;
        points.push((x, bezier_component(t, curve.y1, curve.y2)));
    }
    fn linear(left: (f64, f64), right: (f64, f64), progress: &str) -> String {
        let width = right.0 - left.0;
        if width <= f64::EPSILON {
            return format!("{:.12}", right.1);
        }
        format!(
            "{:.12}+({:.12})*((({progress})-{:.12})/{:.12})",
            left.1,
            right.1 - left.1,
            left.0,
            width
        )
    }
    fn lookup(points: &[(f64, f64)], low: usize, high: usize, progress: &str) -> String {
        if high - low == 1 {
            return linear(points[low], points[high], progress);
        }
        let middle = low + (high - low) / 2;
        let before = lookup(points, low, middle, progress);
        let after = lookup(points, middle, high, progress);
        format!(
            "if(lt(({progress}),{:.12}),({before}),({after}))",
            points[middle].0
        )
    }
    lookup(&points, 0, SUBDIVISIONS, progress)
}

fn bezier_component(t: f64, first: f64, second: f64) -> f64 {
    let inverse = 1.0 - t;
    3.0 * inverse * inverse * t * first + 3.0 * inverse * t * t * second + t * t * t
}

fn build_filter(r: &ExportRequest, audio: bool) -> String {
    let fps = format!(
        "{}/{}",
        r.video.frame_rate.numerator, r.video.frame_rate.denominator
    );
    let mut chains = Vec::new();
    let mut inputs = Vec::new();
    for (i, s) in r.segments.iter().enumerate() {
        let start = s.source_start.seconds();
        let end = start + s.source_duration.seconds();
        let vstream = s
            .video_stream
            .map_or_else(|| "v:0".into(), |n| n.to_string());
        let scale = match r.video.aspect_mode {
            AspectMode::Fit => format!(
                "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2",
                r.video.width, r.video.height, r.video.width, r.video.height
            ),
            AspectMode::Fill => format!(
                "scale={}:{}:force_original_aspect_ratio=increase,crop={}:{}",
                r.video.width, r.video.height, r.video.width, r.video.height
            ),
            AspectMode::Stretch => format!("scale={}:{}", r.video.width, r.video.height),
        };
        chains.push(format!("[{i}:{vstream}]trim=start={start:.9}:end={end:.9},setpts=PTS-STARTPTS,{scale},fps={fps},setsar=1,format=yuv420p[v{i}]"));
        inputs.push(format!("[v{i}]"));
        if audio {
            let astream = s
                .audio_stream
                .map_or_else(|| "a:0".into(), |n| n.to_string());
            let gain = match r.audio {
                AudioOutput::Preserve { gain_db } => gain_db,
                AudioOutput::Drop => 0.0,
            };
            chains.push(format!("[{i}:{astream}]atrim=start={start:.9}:end={end:.9},asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,volume={gain:.3}dB[a{i}]"));
            inputs.push(format!("[a{i}]"));
        }
    }
    chains.push(format!(
        "{}concat=n={}:v=1:a={}[vout]{}",
        inputs.join(""),
        r.segments.len(),
        u8::from(audio),
        if audio { "[aout]" } else { "" }
    ));
    chains.join(";")
}

fn update_candidate(
    candidate: &mut Option<i64>,
    pts: i64,
    requested: MediaTime,
    tb: FrameRate,
    alignment: FrameAlignment,
) {
    let side = rational_cmp(pts, tb, requested.ticks, requested.timebase);
    let eligible = match alignment {
        FrameAlignment::Exact => side == Ordering::Equal,
        FrameAlignment::Floor => side != Ordering::Greater,
        FrameAlignment::Ceil => side != Ordering::Less,
        FrameAlignment::Nearest => true,
    };
    if !eligible {
        return;
    }
    let replace = candidate.is_none_or(|old| match alignment {
        FrameAlignment::Exact => false,
        FrameAlignment::Floor => pts > old,
        FrameAlignment::Ceil => pts < old,
        FrameAlignment::Nearest => {
            pts_distance(pts, requested, tb) < pts_distance(old, requested, tb)
        }
    });
    if replace {
        *candidate = Some(pts);
    }
}
fn pts_distance(pts: i64, requested: MediaTime, tb: FrameRate) -> i128 {
    (pts as i128 * tb.numerator as i128 * requested.timebase as i128
        - requested.ticks as i128 * tb.denominator as i128)
        .abs()
}
fn rational_cmp(a: i64, a_time_base: FrameRate, b: i64, b_timebase: u32) -> Ordering {
    (a as i128 * a_time_base.numerator as i128 * b_timebase as i128)
        .cmp(&(b as i128 * a_time_base.denominator as i128))
}
fn check_opt<T: PartialEq + std::fmt::Display>(
    checks: &mut Vec<VerificationCheck>,
    name: &str,
    expected: Option<T>,
    actual: T,
) {
    if let Some(e) = expected {
        checks.push(VerificationCheck {
            name: name.into(),
            passed: e == actual,
            detail: format!("expected {e}, got {actual}"),
        });
    }
}

#[derive(Deserialize)]
struct RawProbe {
    streams: Vec<RawStream>,
    format: Option<RawFormat>,
}
#[derive(Deserialize)]
struct RawFormat {
    format_name: Option<String>,
    duration: Option<String>,
    bit_rate: Option<String>,
    tags: Option<RawFormatTags>,
}
#[derive(Deserialize)]
struct RawFormatTags {
    major_brand: Option<String>,
}
#[derive(Deserialize)]
struct RawStream {
    index: u32,
    codec_type: Option<String>,
    codec_name: Option<String>,
    duration: Option<String>,
    time_base: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    sample_rate: Option<String>,
    channels: Option<u32>,
    avg_frame_rate: Option<String>,
    pix_fmt: Option<String>,
    channel_layout: Option<String>,
    color_space: Option<String>,
    color_transfer: Option<String>,
    color_primaries: Option<String>,
}
fn convert_probe(input: PathBuf, raw: RawProbe) -> MediaMetadata {
    let format = raw.format;
    let format_brand = format
        .as_ref()
        .and_then(|f| f.tags.as_ref()?.major_brand.as_deref())
        .map(str::trim)
        .filter(|brand| !brand.is_empty())
        .map(str::to_owned);
    MediaMetadata {
        input,
        format_name: format.as_ref().and_then(|f| f.format_name.clone()),
        format_brand,
        duration: format
            .as_ref()
            .and_then(|f| parse_decimal_time(f.duration.as_deref()?)),
        bit_rate: format
            .as_ref()
            .and_then(|f| f.bit_rate.as_deref()?.parse().ok()),
        streams: raw
            .streams
            .into_iter()
            .map(|s| MediaStream {
                index: s.index,
                codec_type: match s.codec_type.as_deref() {
                    Some("video") => StreamKind::Video,
                    Some("audio") => StreamKind::Audio,
                    Some("subtitle") => StreamKind::Subtitle,
                    Some("data") => StreamKind::Data,
                    Some("attachment") => StreamKind::Attachment,
                    _ => StreamKind::Unknown,
                },
                codec_name: s.codec_name,
                duration: s.duration.as_deref().and_then(parse_decimal_time),
                time_base: s.time_base.as_deref().and_then(parse_rate),
                width: s.width,
                height: s.height,
                sample_rate: s.sample_rate.and_then(|v| v.parse().ok()),
                channels: s.channels,
                average_frame_rate: s.avg_frame_rate.as_deref().and_then(parse_rate),
                pixel_format: s.pix_fmt,
                channel_layout: s.channel_layout,
                color_space: s.color_space,
                color_transfer: s.color_transfer,
                color_primaries: s.color_primaries,
            })
            .collect(),
    }
}
fn parse_rate(v: &str) -> Option<FrameRate> {
    let (n, d) = v.split_once('/')?;
    Some(FrameRate {
        numerator: n.parse().ok()?,
        denominator: d.parse().ok()?,
    })
}
fn parse_decimal_time(v: &str) -> Option<MediaTime> {
    let seconds: f64 = v.parse().ok()?;
    Some(MediaTime {
        ticks: (seconds * 1_000_000.0).round() as i64,
        timebase: 1_000_000,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn tool_fingerprint_records_version_and_exact_executable_bytes() {
        use sha2::Digest;
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("ffmpeg-test");
        let bytes = b"#!/bin/sh\nprintf 'ffmpeg version fixture-1\\n'\n";
        fs::write(&executable, bytes).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();

        let fingerprint = fingerprint_tool("ffmpeg", &executable).unwrap();
        let expected_hash = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(fingerprint.version, "ffmpeg version fixture-1");
        assert_eq!(fingerprint.binary_sha256, expected_hash);
        assert_eq!(fingerprint.name, "ffmpeg");
    }

    #[cfg(unix)]
    #[test]
    fn worker_probe_honors_cancellation_of_a_blocked_reader() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("media.wav");
        fs::write(&input, b"fixture").unwrap();
        let probe = dir.path().join("ffprobe");
        fs::write(
            &probe,
            "#!/usr/bin/env python3\nimport time\ntime.sleep(30)\n",
        )
        .unwrap();
        fs::set_permissions(&probe, fs::Permissions::from_mode(0o755)).unwrap();
        let backend = FfmpegBackend {
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: probe,
            admission: None,
        };
        let cancel = CancellationToken::new();
        let signal = cancel.clone();
        let timer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            signal.cancel();
        });
        let start = std::time::Instant::now();
        let result = backend.probe_cancellable(&ProbeRequest { input }, &cancel);
        timer.join().unwrap();
        assert!(matches!(result, Err(MediaError::Cancelled)), "{result:?}");
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn audio_start_maps_exact_rational_time_to_output_samples() {
        assert_eq!(
            time_to_sample_index(
                MediaTime {
                    ticks: 1,
                    timebase: 3_000,
                },
                48_000,
            )
            .unwrap(),
            16
        );
        assert_eq!(
            time_to_sample_index(
                MediaTime {
                    ticks: 1,
                    timebase: 96_000,
                },
                48_000,
            )
            .unwrap(),
            1,
            "half-sample ties select the later sample"
        );
    }

    #[test]
    fn audio_retime_maps_rational_rate_to_nearest_output_sample_rate() {
        assert_eq!(
            retime_sample_rate(
                MediaTime {
                    ticks: 1,
                    timebase: 1,
                },
                MediaTime {
                    ticks: 4,
                    timebase: 5,
                },
            )
            .unwrap(),
            60_000,
        );
        assert_eq!(
            retime_sample_rate(
                MediaTime {
                    ticks: 1,
                    timebase: 1,
                },
                MediaTime {
                    ticks: 7,
                    timebase: 9,
                },
            )
            .unwrap(),
            61_714,
            "rate calculation uses exact integer rationals before rounding",
        );
    }

    #[test]
    fn audio_retime_rejects_invalid_duration_and_unsupported_sample_rate() {
        assert!(matches!(
            retime_sample_rate(
                MediaTime {
                    ticks: 1,
                    timebase: 1,
                },
                MediaTime {
                    ticks: 0,
                    timebase: 1,
                },
            ),
            Err(MediaError::InvalidRequest(_))
        ));
        assert!(matches!(
            retime_sample_rate(
                MediaTime {
                    ticks: 1,
                    timebase: 1,
                },
                MediaTime {
                    ticks: 3,
                    timebase: 1,
                },
            ),
            Err(MediaError::Unsupported(_))
        ));
        assert!(
            matches!(
                retime_sample_rate(
                    MediaTime {
                        ticks: 1,
                        timebase: 1,
                    },
                    MediaTime {
                        ticks: 29,
                        timebase: 15,
                    },
                ),
                Err(MediaError::Unsupported(_))
            ),
            "rejects a representable filter rate that would drift by two output samples"
        );
    }

    #[test]
    fn audio_start_rejects_invalid_time_without_float_conversion() {
        assert!(matches!(
            time_to_sample_index(
                MediaTime {
                    ticks: -1,
                    timebase: 48_000,
                },
                48_000,
            ),
            Err(MediaError::InvalidRequest(_))
        ));
        assert!(matches!(
            time_to_sample_index(
                MediaTime {
                    ticks: 1,
                    timebase: 0,
                },
                48_000,
            ),
            Err(MediaError::InvalidRequest(_))
        ));
    }

    #[test]
    fn range_validation_does_not_require_product_timebase_to_fit_dto() {
        let rate = FrameRate {
            numerator: 120_000,
            denominator: 1,
        };
        assert!(
            validate_range_times(
                MediaTime {
                    ticks: 1,
                    timebase: 120_000,
                },
                MediaTime {
                    ticks: 3,
                    timebase: 120_000,
                },
                MediaTime {
                    ticks: 10,
                    timebase: 120_000,
                },
                rate,
                false,
            )
            .is_ok()
        );
    }

    #[test]
    fn range_validation_reports_extreme_arithmetic_overflow() {
        let result = validate_range_times(
            MediaTime {
                ticks: i64::MAX,
                timebase: u32::MAX,
            },
            MediaTime {
                ticks: i64::MAX,
                timebase: u32::MAX,
            },
            MediaTime {
                ticks: i64::MAX,
                timebase: u32::MAX,
            },
            FrameRate {
                numerator: u32::MAX,
                denominator: 1,
            },
            false,
        );
        assert!(
            matches!(&result, Err(MediaError::InvalidRequest(message)) if message.contains("overflow")),
            "unexpected result: {result:?}"
        );
    }
}
