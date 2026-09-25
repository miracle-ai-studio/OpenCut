use crate::{
    CancellationToken, MediaError, MediaResult, MediaTime, OutputAdmission, SpeakerTurn,
    SpeakerTurnsRequest, SpeakerTurnsResult,
};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::Duration,
};

const MODEL_SHA256: &str = "ceac3ec06d1d98ef71aec665283564631055fd6129b79d8e1be4f9cc33cc54b4";
const MAX_WAV_BYTES: u64 = 120_000_000;
const MAX_JSON_BYTES: u64 = 16_000_000;

#[derive(Clone, Debug)]
pub struct TinydiarizeBackend {
    executable: PathBuf,
    model: PathBuf,
    ffmpeg: PathBuf,
    admission: Option<Arc<dyn OutputAdmission>>,
    work_directory: Option<PathBuf>,
}
struct WorkFiles {
    wav: PathBuf,
    json: PathBuf,
    model: PathBuf,
    _marker: tempfile::NamedTempFile,
}
impl WorkFiles {
    fn new(directory: &Path) -> MediaResult<Self> {
        let marker = tempfile::Builder::new()
            .prefix(".opencut-tdrz-")
            .tempfile_in(directory)?;
        let base = marker.path().to_string_lossy();
        Ok(Self {
            wav: PathBuf::from(format!("{base}.wav")),
            json: PathBuf::from(format!("{base}.json")),
            model: PathBuf::from(format!("{base}.model.bin")),
            _marker: marker,
        })
    }
}
impl Drop for WorkFiles {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.wav);
        let _ = fs::remove_file(&self.json);
        let _ = fs::remove_file(&self.model);
    }
}
impl TinydiarizeBackend {
    pub fn new(
        executable: impl Into<PathBuf>,
        model: impl Into<PathBuf>,
        ffmpeg: impl Into<PathBuf>,
    ) -> MediaResult<Self> {
        let backend = Self {
            executable: executable.into(),
            model: model.into(),
            ffmpeg: ffmpeg.into(),
            admission: None,
            work_directory: None,
        };
        let metadata = fs::metadata(&backend.model).map_err(|_| MediaError::ToolUnavailable {
            tool: "tinydiarize model".into(),
            detail: "configured English speaker-turn model is unavailable".into(),
        })?;
        if !metadata.is_file() || metadata.len() < 400_000_000 {
            return Err(MediaError::ToolUnavailable {
                tool: "tinydiarize model".into(),
                detail: "configured model is not a complete tinydiarize inference model".into(),
            });
        }
        Ok(backend)
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
    pub fn speaker_turns(
        &self,
        request: &SpeakerTurnsRequest,
        cancel: &CancellationToken,
    ) -> MediaResult<SpeakerTurnsResult> {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        if !request.input.is_file() {
            return Err(MediaError::NotFound(request.input.display().to_string()));
        }
        let temporary = if self.work_directory.is_none() {
            Some(tempfile::tempdir()?)
        } else {
            None
        };
        let directory = self
            .work_directory
            .as_deref()
            .unwrap_or_else(|| temporary.as_ref().expect("temporary directory").path());
        let work = WorkFiles::new(directory)?;
        let model_bytes = fs::metadata(&self.model)?.len();
        let _reservation = self
            .admission
            .as_ref()
            .map(|admission| admission.reserve_private_copy(&work.model, model_bytes))
            .transpose()?;
        crate::transcription::snapshot_model(&self.model, &work.model, model_bytes, cancel)?;
        let hash = crate::transcription::hash_model(&work.model, cancel)?;
        if hash != MODEL_SHA256 {
            return Err(MediaError::ToolUnavailable {
                tool: "tinydiarize model".into(),
                detail: "configured model is not the verified English speaker-turn model".into(),
            });
        }
        let mut convert = Command::new(&self.ffmpeg);
        convert
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-y",
                "-protocol_whitelist",
                "file",
                "-i",
            ])
            .arg(&request.input)
            .args([
                "-map",
                "0:a:0",
                "-vn",
                "-ac",
                "1",
                "-ar",
                "16000",
                "-sample_fmt",
                "s16",
                "-f",
                "wav",
            ])
            .arg(&work.wav);
        crate::process::checked_output_file_limited(
            &mut convert,
            "ffmpeg",
            cancel,
            Duration::from_secs(600),
            self.admission.as_deref(),
            Some((&work.wav, MAX_WAV_BYTES)),
        )?;
        if fs::metadata(&work.wav)?.len() >= MAX_WAV_BYTES {
            return Err(MediaError::OutputLimit {
                limit_bytes: MAX_WAV_BYTES,
            });
        }
        let mut command = Command::new(&self.executable);
        command
            .arg("-m")
            .arg(&work.model)
            .arg("-f")
            .arg(&work.wav)
            .args(["-l", "en", "-tdrz", "-oj", "-of"])
            .arg(work.json.with_extension(""))
            .arg("-np");
        crate::process::checked_output_file_limited(
            &mut command,
            "whisper-cli",
            cancel,
            Duration::from_secs(3600),
            self.admission.as_deref(),
            Some((&work.json, MAX_JSON_BYTES)),
        )?;
        if fs::metadata(&work.json)?.len() > MAX_JSON_BYTES {
            return Err(MediaError::OutputLimit {
                limit_bytes: MAX_JSON_BYTES,
            });
        }
        let result = parse_output(&fs::read(&work.json)?)?;
        if crate::transcription::hash_model(&work.model, cancel)? != hash {
            return Err(MediaError::InvalidToolOutput {
                tool: "whisper-cli".into(),
                detail: "model snapshot changed during inference".into(),
            });
        }
        Ok(result)
    }
}
#[derive(Deserialize)]
struct RawOffsets {
    from: i64,
    to: i64,
}
#[derive(Deserialize)]
struct RawSegment {
    offsets: RawOffsets,
    speaker_turn_next: bool,
}
#[derive(Deserialize)]
struct RawResult {
    language: String,
}
#[derive(Deserialize)]
struct RawOutput {
    result: RawResult,
    transcription: Vec<RawSegment>,
}
fn parse_output(bytes: &[u8]) -> MediaResult<SpeakerTurnsResult> {
    let raw: RawOutput =
        serde_json::from_slice(bytes).map_err(|e| MediaError::InvalidToolOutput {
            tool: "whisper-cli".into(),
            detail: e.to_string(),
        })?;
    if raw.result.language != "en" || raw.transcription.len() > 10_000 {
        return Err(invalid());
    }
    let mut previous_end = 0;
    let mut turns = Vec::new();
    let segment_count = raw.transcription.len();
    for (index, segment) in raw.transcription.into_iter().enumerate() {
        if segment.offsets.from < previous_end
            || segment.offsets.to <= segment.offsets.from
            || segment.offsets.to > 3_600_000
        {
            return Err(invalid());
        }
        previous_end = segment.offsets.to;
        // `speaker_turn_next` describes a transition after this segment. A
        // final marker without a following decoded segment is outside the
        // observed media and must not be published as an in-clip boundary.
        if segment.speaker_turn_next && index + 1 < segment_count {
            turns.push(SpeakerTurn {
                at: MediaTime {
                    ticks: segment.offsets.to,
                    timebase: 1000,
                },
            });
        }
    }
    let result = SpeakerTurnsResult {
        turns,
        language: "en".into(),
        provider: "whisper.cpp".into(),
        method: "tinydiarize_speaker_turn_next".into(),
        model: "ggml-small.en-tdrz.bin".into(),
        model_sha256: MODEL_SHA256.into(),
        confidence_known: false,
        speaker_identity_status: "unavailable".into(),
    };
    if !opencut_media_contracts::validate_public_speaker_turns_result(&result) {
        return Err(invalid());
    }
    Ok(result)
}
fn invalid() -> MediaError {
    MediaError::InvalidToolOutput {
        tool: "whisper-cli".into(),
        detail: "invalid English speaker-turn output".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_missing_flags_wrong_language_and_intervals() {
        let parsed = parse_output(br#"{"result":{"language":"en"},"transcription":[{"offsets":{"from":0,"to":100},"speaker_turn_next":true},{"offsets":{"from":100,"to":200},"speaker_turn_next":false}]}"#).unwrap();
        assert_eq!(parsed.turns.len(), 1);
        assert_eq!(parsed.turns[0].at.ticks, 100);
        let terminal_marker = parse_output(br#"{"result":{"language":"en"},"transcription":[{"offsets":{"from":0,"to":100},"speaker_turn_next":true}]}"#).unwrap();
        assert!(terminal_marker.turns.is_empty());
        for bytes in [
            &br#"{"result":{"language":"en"},"transcription":[{"offsets":{"from":0,"to":100}}]}"#[..],
            &br#"{"result":{"language":"zh"},"transcription":[]}"#[..],
            &br#"{"result":{"language":"en"},"transcription":[{"offsets":{"from":10,"to":10},"speaker_turn_next":false}]}"#[..],
            &br#"{"result":{"language":"en"},"transcription":[{"offsets":{"from":0,"to":100},"speaker_turn_next":true},{"offsets":{"from":99,"to":200},"speaker_turn_next":true}]}"#[..],
        ] { assert!(parse_output(bytes).is_err()); }
    }
    #[test]
    fn real_model_marks_only_observed_turns_when_configured() {
        let (Ok(model), Ok(audio)) = (
            std::env::var("OPENCUT_TEST_TDRZ_MODEL"),
            std::env::var("OPENCUT_TEST_TDRZ_AUDIO"),
        ) else {
            return;
        };
        let engine = TinydiarizeBackend::new("whisper-cli", model, "ffmpeg").unwrap();
        let result = engine
            .speaker_turns(
                &SpeakerTurnsRequest {
                    input: audio.into(),
                },
                &CancellationToken::new(),
            )
            .unwrap();
        assert!(opencut_media_contracts::validate_public_speaker_turns_result(&result));
        assert_eq!(result.speaker_identity_status, "unavailable");
    }
}
