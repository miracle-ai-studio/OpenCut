use crate::{
    CancellationToken, MediaError, MediaResult, MediaTime, TranscribeRequest, TranscriptionResult,
    TranscriptionSegment, TranscriptionWord, WordTimingStatus,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::Duration,
};

const MAX_WAV_BYTES: u64 = 120_000_000;
const MAX_JSON_BYTES: u64 = 16_000_000;

/// Host-configured native whisper.cpp backend. Executable and model paths never
/// come from the portable request or a client-supplied field.
#[derive(Clone, Debug)]
pub struct WhisperBackend {
    executable: PathBuf,
    model: PathBuf,
    ffmpeg: PathBuf,
    admission: Option<Arc<dyn crate::OutputAdmission>>,
    work_directory: Option<PathBuf>,
}

struct WorkFiles {
    wav: PathBuf,
    json: PathBuf,
    words_json: PathBuf,
    model: PathBuf,
    _marker: tempfile::NamedTempFile,
}

impl WorkFiles {
    fn new(directory: &Path) -> MediaResult<Self> {
        let marker = tempfile::Builder::new()
            .prefix(".opencut-whisper-")
            .tempfile_in(directory)?;
        let base = marker.path().to_string_lossy();
        Ok(Self {
            wav: PathBuf::from(format!("{base}.wav")),
            json: PathBuf::from(format!("{base}.json")),
            words_json: PathBuf::from(format!("{base}.words.json")),
            model: PathBuf::from(format!("{base}.model.bin")),
            _marker: marker,
        })
    }
}

impl Drop for WorkFiles {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.wav);
        let _ = fs::remove_file(&self.json);
        let _ = fs::remove_file(&self.words_json);
        let _ = fs::remove_file(&self.model);
    }
}

impl WhisperBackend {
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
        backend.check_model()?;
        Ok(backend)
    }

    pub fn with_output_admission(
        mut self,
        admission: Arc<dyn crate::OutputAdmission>,
        attempt_directory: PathBuf,
    ) -> Self {
        self.admission = Some(admission);
        self.work_directory = Some(attempt_directory);
        self
    }

    fn check_model(&self) -> MediaResult<()> {
        let metadata = fs::metadata(&self.model).map_err(|error| MediaError::ToolUnavailable {
            tool: "whisper model".into(),
            detail: format!(
                "configured model {} is unavailable: {error}",
                self.model.display()
            ),
        })?;
        if !metadata.is_file() || metadata.len() < 1_000_000 {
            return Err(MediaError::ToolUnavailable {
                tool: "whisper model".into(),
                detail: format!(
                    "configured model {} is not a complete inference model",
                    self.model.display()
                ),
            });
        }
        Ok(())
    }

    pub fn transcribe(
        &self,
        request: &TranscribeRequest,
        cancel: &CancellationToken,
    ) -> MediaResult<TranscriptionResult> {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        self.check_model()?;
        if !request.input.is_file() {
            return Err(MediaError::NotFound(request.input.display().to_string()));
        }
        let language = request.language.as_deref().unwrap_or("auto");
        if !validate_transcription_language(language) {
            return Err(MediaError::InvalidRequest(
                "language must be `auto` or a two/three letter language code".into(),
            ));
        }
        let temporary_directory = if self.work_directory.is_none() {
            Some(tempfile::tempdir()?)
        } else {
            None
        };
        let work_directory = self.work_directory.as_deref().unwrap_or_else(|| {
            temporary_directory
                .as_ref()
                .expect("temporary directory")
                .path()
        });
        let work = WorkFiles::new(work_directory)?;
        let model_bytes = fs::metadata(&self.model)?.len();
        let _model_reservation = self
            .admission
            .as_ref()
            .map(|admission| admission.reserve_private_copy(&work.model, model_bytes))
            .transpose()?;
        snapshot_model(&self.model, &work.model, model_bytes, cancel)?;
        let model_hash = hash_model(&work.model, cancel)?;
        let wav = &work.wav;
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
            .arg(wav);
        crate::process::checked_output_file_limited(
            &mut convert,
            "ffmpeg",
            cancel,
            Duration::from_secs(600),
            self.admission.as_deref(),
            Some((wav, MAX_WAV_BYTES)),
        )?;
        let wav_size = fs::metadata(wav)?.len();
        if wav_size >= MAX_WAV_BYTES {
            return Err(MediaError::OutputLimit {
                limit_bytes: MAX_WAV_BYTES,
            });
        }

        let output_base = work.json.with_extension("");
        let output_json = &work.json;
        let mut whisper = Command::new(&self.executable);
        whisper
            .args(["-m"])
            .arg(&work.model)
            .args(["-f"])
            .arg(wav)
            .args(["-l", language, "-oj", "-of"])
            .arg(&output_base)
            .arg("-np");
        crate::process::checked_output_file_limited(
            &mut whisper,
            "whisper-cli",
            cancel,
            Duration::from_secs(3600),
            self.admission.as_deref(),
            Some((output_json, MAX_JSON_BYTES)),
        )?;
        let metadata =
            fs::metadata(output_json).map_err(|error| MediaError::InvalidToolOutput {
                tool: "whisper-cli".into(),
                detail: format!("JSON transcript missing: {error}"),
            })?;
        if metadata.len() > MAX_JSON_BYTES {
            return Err(MediaError::OutputLimit {
                limit_bytes: MAX_JSON_BYTES,
            });
        }
        let bytes = fs::read(output_json)?;
        if hash_model(&work.model, cancel)? != model_hash {
            return Err(MediaError::InvalidToolOutput {
                tool: "whisper-cli".into(),
                detail: "model changed during inference".into(),
            });
        }
        let mut result = parse_output(&bytes, language, model_label(&self.model))?;
        if request.word_timestamps {
            result.word_timing_status = WordTimingStatus::Unavailable;
            if result.language.as_deref() == Some("en") && !result.segments.is_empty() {
                let word_base = work.words_json.with_extension("");
                let mut word_command = Command::new(&self.executable);
                word_command
                    .arg("-m")
                    .arg(&work.model)
                    .arg("-f")
                    .arg(wav)
                    .args(["-l", "en", "-ml", "1", "-sow", "-oj", "-of"])
                    .arg(&word_base)
                    .arg("-np");
                crate::process::checked_output_file_limited(
                    &mut word_command,
                    "whisper-cli",
                    cancel,
                    Duration::from_secs(3600),
                    self.admission.as_deref(),
                    Some((&work.words_json, MAX_JSON_BYTES)),
                )?;
                let word_bytes = fs::read(&work.words_json)?;
                if let Some(words) = parse_word_output(&word_bytes, &result.segments)? {
                    for (segment, words) in result.segments.iter_mut().zip(words) {
                        segment.words = words;
                    }
                    result.word_timing_status = WordTimingStatus::Available;
                }
            }
        }
        if hash_model(&work.model, cancel)? != model_hash {
            return Err(MediaError::InvalidToolOutput {
                tool: "whisper-cli".into(),
                detail: "model changed during inference".into(),
            });
        }
        result.model_sha256 = Some(model_hash);
        if !crate::validate_public_transcription_result(&result) {
            return Err(MediaError::InvalidToolOutput {
                tool: "whisper-cli".into(),
                detail: "transcript exceeds public result limits".into(),
            });
        }
        Ok(result)
    }
}

pub(crate) fn snapshot_model(
    source: &Path,
    destination: &Path,
    expected_bytes: u64,
    cancel: &CancellationToken,
) -> MediaResult<()> {
    let mut input = fs::File::open(source)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let mut buffer = [0u8; 65_536];
    let mut copied = 0u64;
    loop {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        copied = copied
            .checked_add(count as u64)
            .ok_or(MediaError::OutputLimit {
                limit_bytes: expected_bytes,
            })?;
        if copied > expected_bytes {
            return Err(MediaError::OutputLimit {
                limit_bytes: expected_bytes,
            });
        }
        output.write_all(&buffer[..count])?;
    }
    if copied != expected_bytes {
        return Err(MediaError::InvalidToolOutput {
            tool: "whisper model".into(),
            detail: "model changed while being snapshotted".into(),
        });
    }
    output.sync_all()?;
    let mut permissions = output.metadata()?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(destination, permissions)?;
    Ok(())
}

pub(crate) fn hash_model(path: &Path, cancel: &CancellationToken) -> MediaResult<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65_536];
    loop {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn model_label(path: &Path) -> String {
    let Some(name) = path.file_name() else {
        return "local-model".into();
    };
    let label: String = name
        .to_string_lossy()
        .chars()
        .take(128)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect();
    if label.is_empty() {
        "local-model".into()
    } else {
        label
    }
}

pub fn validate_transcription_language(value: &str) -> bool {
    value == "auto"
        || ((2..=3).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_lowercase()))
}

#[derive(Deserialize)]
struct RawResult {
    language: Option<String>,
}
#[derive(Deserialize)]
struct RawOffsets {
    from: i64,
    to: i64,
}
#[derive(Deserialize)]
struct RawSegment {
    offsets: RawOffsets,
    text: String,
}
#[derive(Deserialize)]
struct RawOutput {
    result: RawResult,
    transcription: Vec<RawSegment>,
}

fn parse_output(
    bytes: &[u8],
    requested_language: &str,
    model: String,
) -> MediaResult<TranscriptionResult> {
    let raw: RawOutput =
        serde_json::from_slice(bytes).map_err(|error| MediaError::InvalidToolOutput {
            tool: "whisper-cli".into(),
            detail: error.to_string(),
        })?;
    let detected = raw
        .result
        .language
        .filter(|value| validate_transcription_language(value) && value != "auto");
    let language = if requested_language == "auto" {
        detected.ok_or_else(|| MediaError::InvalidToolOutput {
            tool: "whisper-cli".into(),
            detail: "automatic language detection missing a valid language".into(),
        })?
    } else {
        requested_language.into()
    };
    let mut previous_end = 0;
    let mut segments = Vec::with_capacity(raw.transcription.len());
    for segment in raw.transcription {
        let start = segment.offsets.from;
        let end = segment.offsets.to;
        if start < 0
            || end <= start
            || start < previous_end
            || end > 3_600_000
            || segment.text.trim().is_empty()
        {
            return Err(MediaError::InvalidToolOutput {
                tool: "whisper-cli".into(),
                detail: "invalid transcription segment interval or text".into(),
            });
        }
        previous_end = end;
        segments.push(TranscriptionSegment {
            start: MediaTime {
                ticks: start,
                timebase: 1000,
            },
            end: MediaTime {
                ticks: end,
                timebase: 1000,
            },
            text: segment.text.trim().into(),
            words: Vec::new(),
        });
    }
    let result = TranscriptionResult {
        language: Some(language),
        segments,
        provider: "whisper.cpp".into(),
        model,
        model_sha256: None,
        word_timing_status: WordTimingStatus::NotRequested,
    };
    if !crate::validate_public_transcription_result(&result) {
        return Err(MediaError::InvalidToolOutput {
            tool: "whisper-cli".into(),
            detail: "transcript exceeds public result limits".into(),
        });
    }
    Ok(result)
}

/// Accept only actual word-split segments whose text and timing match the first pass.
/// A token, phrase, changed decoding, or unaligned segment makes words unavailable.
fn parse_word_output(
    bytes: &[u8],
    segments: &[TranscriptionSegment],
) -> MediaResult<Option<Vec<Vec<TranscriptionWord>>>> {
    let raw: RawOutput =
        serde_json::from_slice(bytes).map_err(|error| MediaError::InvalidToolOutput {
            tool: "whisper-cli".into(),
            detail: error.to_string(),
        })?;
    let mut grouped = vec![Vec::new(); segments.len()];
    let mut index = 0usize;
    for raw_word in raw.transcription {
        let text = raw_word.text.trim();
        if text.is_empty() {
            continue;
        }
        if text.len() > 256
            || text.chars().any(char::is_whitespace)
            || !text.bytes().any(|byte| byte.is_ascii_alphabetic())
            || raw_word.offsets.to <= raw_word.offsets.from
        {
            return Ok(None);
        }
        while index < segments.len() && raw_word.offsets.from >= segments[index].end.ticks {
            index += 1;
        }
        let Some(segment) = segments.get(index) else {
            return Ok(None);
        };
        if raw_word.offsets.from < segment.start.ticks || raw_word.offsets.to > segment.end.ticks {
            return Ok(None);
        }
        grouped[index].push(TranscriptionWord {
            start: MediaTime {
                ticks: raw_word.offsets.from,
                timebase: 1000,
            },
            end: MediaTime {
                ticks: raw_word.offsets.to,
                timebase: 1000,
            },
            text: text.into(),
        });
    }
    if grouped.iter().zip(segments).any(|(words, segment)| {
        words.is_empty()
            || words
                .iter()
                .map(|word| word.text.as_str())
                .collect::<String>()
                != segment
                    .text
                    .chars()
                    .filter(|ch| !ch.is_whitespace())
                    .collect::<String>()
    }) {
        return Ok(None);
    }
    Ok(Some(grouped))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_valid_segments_and_ignores_upstream_metadata() {
        let result = parse_output(br#"{"result":{"language":"en","future":true},"transcription":[{"offsets":{"from":120,"to":780},"text":" Hello ","tokens":[1]}],"systeminfo":"x"}"#, "auto", "ggml-tiny.bin".into()).unwrap();
        assert_eq!(result.language.as_deref(), Some("en"));
        assert_eq!(result.segments[0].start.ticks, 120);
        assert_eq!(result.segments[0].text, "Hello");
        assert!(result.segments[0].words.is_empty());
    }
    #[test]
    fn accepts_empty_transcription() {
        let result = parse_output(
            br#"{"result":{"language":"en"},"transcription":[]}"#,
            "auto",
            "tiny".into(),
        )
        .unwrap();
        assert!(result.segments.is_empty());
    }
    #[test]
    fn rejects_corrupt_and_invalid_intervals() {
        for data in [
            &br#"{"result":{"language":"en"},"transcription":[{"offsets":{"from":4,"to":3},"text":"x"}]}"#[..],
            &br#"{"result":{"language":"en"},"transcription":[{"offsets":{"from":-1,"to":3},"text":"x"}]}"#[..],
            &br#"{"result":{"language":"en"},"transcription":[{"offsets":{"from":0,"to":3},"text":" "}]}"#[..],
            &br#"{"result":{"language":"en"},"transcription":[{"text":"x"}]}"#[..],
            &br#"not json"#[..],
            &br#"{"result":{"language":"en"},"transcription":[{"offsets":{"from":0,"to":10},"text":"a"},{"offsets":{"from":9,"to":12},"text":"b"}]}"#[..],
        ] {
            assert!(matches!(parse_output(data, "auto", "tiny".into()), Err(MediaError::InvalidToolOutput { .. })));
        }
    }
    #[test]
    fn model_label_is_safe_for_public_projection() {
        let label = model_label(Path::new("/host/private/中文 tiny model.bin"));
        assert_eq!(label, "___tiny_model.bin");
        assert!(label.len() <= 128);
    }
    #[test]
    fn missing_model_is_a_capability_configuration_error() {
        let model = tempfile::tempdir().unwrap().path().join("missing.bin");
        let error = WhisperBackend::new("whisper-cli", model, "ffmpeg").unwrap_err();
        assert!(matches!(error, MediaError::ToolUnavailable { .. }));
        assert_eq!(error.code(), "CAPABILITY_UNAVAILABLE");
    }
    #[test]
    fn word_pass_accepts_only_matching_real_word_segments() {
        let baseline = parse_output(br#"{"result":{"language":"en"},"transcription":[{"offsets":{"from":0,"to":1000},"text":" Hello world!"}]}"#, "en", "tiny".into()).unwrap();
        let words = parse_word_output(br#"{"result":{"language":"en"},"transcription":[{"offsets":{"from":0,"to":400},"text":" Hello"},{"offsets":{"from":400,"to":1000},"text":" world!"}]}"#, &baseline.segments).unwrap().unwrap();
        assert_eq!(words[0].len(), 2);
        assert_eq!(words[0][1].start.ticks, 400);
        assert!(parse_word_output(br#"{"result":{"language":"en"},"transcription":[{"offsets":{"from":0,"to":1000},"text":" Hello world!"}]}"#, &baseline.segments).unwrap().is_none());
        assert!(parse_word_output(br#"{"result":{"language":"en"},"transcription":[{"offsets":{"from":0,"to":400},"text":" Hello"},{"offsets":{"from":400,"to":1000},"text":" earth!"}]}"#, &baseline.segments).unwrap().is_none());
    }
    #[test]
    fn real_english_words_when_configured() {
        let (Ok(executable), Ok(model), Ok(input)) = (
            std::env::var("OPENCUT_TEST_WHISPER_CLI"),
            std::env::var("OPENCUT_TEST_WHISPER_MODEL"),
            std::env::var("OPENCUT_TEST_WHISPER_JFK"),
        ) else {
            return;
        };
        let backend = WhisperBackend::new(executable, model, "ffmpeg").unwrap();
        let result = backend
            .transcribe(
                &TranscribeRequest {
                    input: input.into(),
                    language: Some("en".into()),
                    word_timestamps: true,
                },
                &CancellationToken::new(),
            )
            .unwrap();
        assert_eq!(result.word_timing_status, WordTimingStatus::Available);
        assert!(
            result
                .segments
                .iter()
                .map(|segment| segment.words.len())
                .sum::<usize>()
                > 10
        );
        assert!(crate::validate_public_transcription_result(&result));
    }
    #[test]
    fn real_whisper_when_configured() {
        let (Ok(executable), Ok(model), Ok(input)) = (
            std::env::var("OPENCUT_TEST_WHISPER_CLI"),
            std::env::var("OPENCUT_TEST_WHISPER_MODEL"),
            std::env::var("OPENCUT_TEST_WHISPER_AUDIO"),
        ) else {
            return;
        };
        let backend = WhisperBackend::new(executable, model, "ffmpeg").unwrap();
        let result = backend
            .transcribe(
                &TranscribeRequest {
                    input: input.into(),
                    language: Some("zh".into()),
                    word_timestamps: false,
                },
                &CancellationToken::new(),
            )
            .unwrap();
        assert_eq!(result.provider, "whisper.cpp");
        assert_eq!(result.model_sha256.as_ref().map(String::len), Some(64));
        assert!(!result.segments.is_empty());
        assert!(
            result
                .segments
                .iter()
                .all(|segment| segment.end.ticks > segment.start.ticks)
        );
    }
}
