use crate::{
    CancellationToken, MediaError, MediaResult, MediaTime, OutputAdmission, ShotBoundary,
    ShotsRequest, ShotsResult, SilenceInterval, SilenceRequest, SilenceResult, WaveformBucket,
    WaveformRequest, WaveformResult,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::Duration,
};

const MAX_DURATION_SECONDS: f64 = 3600.0;
const MAX_META_BYTES: u64 = 16_000_000;
const MAX_EVENTS: usize = 10_000;

#[derive(Clone, Debug)]
pub struct FfmpegAnalysisBackend {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    admission: Option<Arc<dyn OutputAdmission>>,
    work_directory: Option<PathBuf>,
}

struct MetadataFile {
    path: PathBuf,
    _marker: tempfile::NamedTempFile,
}
impl MetadataFile {
    fn new(directory: &Path) -> MediaResult<Self> {
        let marker = tempfile::Builder::new()
            .prefix(".opencut-analysis-")
            .tempfile_in(directory)?;
        Ok(Self {
            path: PathBuf::from(format!("{}.meta", marker.path().display())),
            _marker: marker,
        })
    }
}
impl Drop for MetadataFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl FfmpegAnalysisBackend {
    pub fn new(ffmpeg: impl Into<PathBuf>, ffprobe: impl Into<PathBuf>) -> Self {
        Self {
            ffmpeg: ffmpeg.into(),
            ffprobe: ffprobe.into(),
            admission: None,
            work_directory: None,
        }
    }
    pub fn with_output_admission(
        mut self,
        admission: Arc<dyn OutputAdmission>,
        attempt_directory: PathBuf,
    ) -> Self {
        self.admission = Some(admission);
        self.work_directory = Some(attempt_directory);
        self
    }
    fn duration(&self, input: &Path, cancel: &CancellationToken) -> MediaResult<f64> {
        if !input.is_file() {
            return Err(MediaError::NotFound(input.display().to_string()));
        }
        let bytes = crate::process::checked_output_limited(
            Command::new(&self.ffprobe)
                .args([
                    "-v",
                    "error",
                    "-show_entries",
                    "format=duration",
                    "-of",
                    "default=nokey=1:noprint_wrappers=1",
                ])
                .arg(input),
            "ffprobe",
            cancel,
            Duration::from_secs(30),
            self.admission.as_deref(),
        )?;
        let duration = String::from_utf8_lossy(&bytes)
            .trim()
            .parse::<f64>()
            .map_err(|_| MediaError::InvalidToolOutput {
                tool: "ffprobe".into(),
                detail: "finite duration unavailable".into(),
            })?;
        if !duration.is_finite() || duration <= 0.0 || duration > MAX_DURATION_SECONDS {
            return Err(MediaError::InvalidRequest(
                "analysis requires media duration in (0, 3600] seconds".into(),
            ));
        }
        Ok(duration)
    }
    fn run_metadata(
        &self,
        input: &Path,
        filter: &str,
        video: bool,
        cancel: &CancellationToken,
    ) -> MediaResult<(String, f64)> {
        let duration = self.duration(input, cancel)?;
        let temporary_directory = if self.work_directory.is_none() {
            Some(tempfile::tempdir()?)
        } else {
            None
        };
        let directory = self.work_directory.as_deref().unwrap_or_else(|| {
            temporary_directory
                .as_ref()
                .expect("temporary directory")
                .path()
        });
        let metadata = MetadataFile::new(directory)?;
        let filter = filter.replace("{OUTPUT}", &metadata.path.to_string_lossy());
        let mut command = Command::new(&self.ffmpeg);
        command
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-protocol_whitelist",
                "file",
                "-i",
            ])
            .arg(input);
        if video {
            command.args(["-map", "0:v:0", "-an", "-vf"]);
        } else {
            command.args(["-map", "0:a:0", "-vn", "-af"]);
        }
        command.arg(filter).args(["-f", "null", "-"]);
        crate::process::checked_output_file_limited(
            &mut command,
            "ffmpeg",
            cancel,
            Duration::from_secs(900),
            self.admission.as_deref(),
            Some((&metadata.path, MAX_META_BYTES)),
        )?;
        let size = fs::metadata(&metadata.path)?.len();
        if size >= MAX_META_BYTES {
            return Err(MediaError::OutputLimit {
                limit_bytes: MAX_META_BYTES,
            });
        }
        Ok((
            fs::read_to_string(&metadata.path).map_err(MediaError::Io)?,
            duration,
        ))
    }
    pub fn silence(
        &self,
        request: &SilenceRequest,
        cancel: &CancellationToken,
    ) -> MediaResult<SilenceResult> {
        if !(-80..=-5).contains(&request.noise_db)
            || !(100..=10_000).contains(&request.min_duration_ms)
        {
            return Err(MediaError::InvalidRequest(
                "silence parameters outside supported bounds".into(),
            ));
        }
        let filter = format!(
            "asetpts=PTS-STARTPTS,silencedetect=noise={}dB:d={:.3},ametadata=mode=print:file={{OUTPUT}}",
            request.noise_db,
            request.min_duration_ms as f64 / 1000.0
        );
        let (raw, duration) = self.run_metadata(&request.input, &filter, false, cancel)?;
        let mut start: Option<f64> = None;
        let mut intervals = Vec::new();
        for line in raw.lines() {
            if let Some(value) = line.strip_prefix("lavfi.silence_start=") {
                start = Some(parse_seconds(value)?);
            }
            if let Some(value) = line.strip_prefix("lavfi.silence_end=") {
                let end = parse_seconds(value)?;
                if let Some(begin) = start.take() {
                    push_silence(&mut intervals, begin, end, duration)?;
                }
            }
        }
        if let Some(begin) = start {
            push_silence(&mut intervals, begin, duration, duration)?;
        }
        let result = SilenceResult {
            intervals,
            provider: "ffmpeg".into(),
            method: "silencedetect".into(),
            confidence_known: false,
        };
        if !opencut_media_contracts::validate_public_silence_result(&result) {
            return Err(invalid_metadata());
        }
        Ok(result)
    }
    pub fn shots(
        &self,
        request: &ShotsRequest,
        cancel: &CancellationToken,
    ) -> MediaResult<ShotsResult> {
        if !request.threshold.is_finite() || !(0.05..=0.95).contains(&request.threshold) {
            return Err(MediaError::InvalidRequest(
                "scene threshold must be in [0.05, 0.95]".into(),
            ));
        }
        let filter = format!(
            "setpts=PTS-STARTPTS,select='gt(scene,{:.6})',metadata=mode=print:file={{OUTPUT}}",
            request.threshold
        );
        let (raw, duration) = self.run_metadata(&request.input, &filter, true, cancel)?;
        let mut boundaries = Vec::new();
        let mut at = None;
        for line in raw.lines() {
            if line.starts_with("frame:") {
                at = frame_time(line);
            }
            if let Some(value) = line.strip_prefix("lavfi.scene_score=") {
                let score = value.parse::<f32>().map_err(|_| invalid_metadata())?;
                let time = at.ok_or_else(invalid_metadata)?;
                if !score.is_finite()
                    || !(0.0..=1.0).contains(&score)
                    || time > duration + 0.1
                    || boundaries
                        .last()
                        .is_some_and(|prev: &ShotBoundary| prev.at.ticks >= millis(time))
                {
                    return Err(invalid_metadata());
                }
                if boundaries.len() >= MAX_EVENTS {
                    return Err(MediaError::InvalidToolOutput {
                        tool: "ffmpeg".into(),
                        detail: "too many scene boundaries".into(),
                    });
                }
                boundaries.push(ShotBoundary {
                    at: MediaTime {
                        ticks: millis(time),
                        timebase: 1000,
                    },
                    scene_score: score,
                });
            }
        }
        let result = ShotsResult {
            boundaries,
            provider: "ffmpeg".into(),
            method: "scene_score".into(),
            confidence_known: false,
        };
        if !opencut_media_contracts::validate_public_shots_result(&result) {
            return Err(invalid_metadata());
        }
        Ok(result)
    }
    pub fn waveform(
        &self,
        request: &WaveformRequest,
        cancel: &CancellationToken,
    ) -> MediaResult<WaveformResult> {
        if !(16..=2048).contains(&request.buckets) {
            return Err(MediaError::InvalidRequest(
                "waveform buckets must be in 16..=2048".into(),
            ));
        }
        let duration = self.duration(&request.input, cancel)?;
        let samples = ((duration * 16_000.0 / request.buckets as f64).ceil() as u32).max(1);
        let filter = format!(
            "aresample=16000,asetpts=PTS-STARTPTS,atrim=end={duration:.9},asetnsamples=n={samples}:p=0,astats=metadata=1:reset=1,ametadata=mode=print:file={{OUTPUT}}"
        );
        let (raw, _) = self.run_metadata(&request.input, &filter, false, cancel)?;
        let mut buckets = Vec::new();
        let mut start: Option<f64> = None;
        let mut peak = None;
        let mut rms = None;
        for line in raw.lines().chain(std::iter::once("frame:END")) {
            if line.starts_with("frame:") {
                if let Some(begin) = start.take() {
                    let begin_sample = (begin * 16_000.0).round() as i64;
                    let end_sample =
                        (begin_sample + samples as i64).min((duration * 16_000.0).round() as i64);
                    if end_sample <= begin_sample || buckets.len() >= request.buckets as usize {
                        return Err(invalid_metadata());
                    }
                    buckets.push(WaveformBucket {
                        start: MediaTime {
                            ticks: begin_sample,
                            timebase: 16_000,
                        },
                        end: MediaTime {
                            ticks: end_sample,
                            timebase: 16_000,
                        },
                        rms: db_to_amplitude(rms.ok_or_else(invalid_metadata)?)?,
                        peak: db_to_amplitude(peak.ok_or_else(invalid_metadata)?)?,
                    });
                    peak = None;
                    rms = None;
                }
                start = if line == "frame:END" {
                    None
                } else {
                    Some(frame_time(line).ok_or_else(invalid_metadata)?)
                };
            } else if let Some(value) = line.strip_prefix("lavfi.astats.Overall.Peak_level=") {
                peak = Some(value.to_owned());
            } else if let Some(value) = line.strip_prefix("lavfi.astats.Overall.RMS_level=") {
                rms = Some(value.to_owned());
            }
        }
        let result = WaveformResult {
            buckets,
            provider: "ffmpeg".into(),
            method: "astats_rms_peak".into(),
            confidence_known: false,
        };
        if !opencut_media_contracts::validate_public_waveform_result(&result) {
            return Err(invalid_metadata());
        }
        Ok(result)
    }
}

fn invalid_metadata() -> MediaError {
    MediaError::InvalidToolOutput {
        tool: "ffmpeg".into(),
        detail: "invalid analysis metadata".into(),
    }
}
fn parse_seconds(value: &str) -> MediaResult<f64> {
    let number = value
        .trim()
        .parse::<f64>()
        .map_err(|_| invalid_metadata())?;
    if !number.is_finite() || !(0.0..=MAX_DURATION_SECONDS + 0.1).contains(&number) {
        return Err(invalid_metadata());
    }
    Ok(number)
}
fn millis(seconds: f64) -> i64 {
    (seconds * 1000.0).round() as i64
}
fn frame_time(line: &str) -> Option<f64> {
    line.split_whitespace().find_map(|field| {
        field
            .strip_prefix("pts_time:")
            .and_then(|value| parse_seconds(value).ok())
    })
}
fn push_silence(
    intervals: &mut Vec<SilenceInterval>,
    begin: f64,
    end: f64,
    duration: f64,
) -> MediaResult<()> {
    if end <= begin
        || end > duration + 0.1
        || intervals.len() >= MAX_EVENTS
        || intervals
            .last()
            .is_some_and(|prev| prev.end.ticks > millis(begin))
    {
        return Err(invalid_metadata());
    }
    intervals.push(SilenceInterval {
        start: MediaTime {
            ticks: millis(begin),
            timebase: 1000,
        },
        end: MediaTime {
            ticks: millis(end.min(duration)),
            timebase: 1000,
        },
    });
    Ok(())
}
fn db_to_amplitude(value: String) -> MediaResult<f32> {
    if value == "-inf" {
        return Ok(0.0);
    }
    let db = value.parse::<f32>().map_err(|_| invalid_metadata())?;
    if !db.is_finite() {
        return Err(invalid_metadata());
    }
    Ok((10f32.powf(db / 20.0)).clamp(0.0, 1.0))
}

mod quality;
