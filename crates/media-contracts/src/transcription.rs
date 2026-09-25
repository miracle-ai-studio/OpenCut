use crate::MediaTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The input is resolved to a fixed asset version by the caller before native execution.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "TranscribeRequest")]
pub struct TranscribeRequest<P = String> {
    pub input: P,
    /// ISO 639-1 code or `auto`; omitted means automatic detection.
    #[serde(default)]
    pub language: Option<String>,
    /// Request a second whisper.cpp pass split on English words.
    #[serde(default)]
    pub word_timestamps: bool,
}

impl<P> TranscribeRequest<P> {
    pub fn try_map_paths<Q, E>(
        self,
        mut map: impl FnMut(P) -> Result<Q, E>,
    ) -> Result<TranscribeRequest<Q>, E> {
        Ok(TranscribeRequest {
            input: map(self.input)?,
            language: self.language,
            word_timestamps: self.word_timestamps,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TranscriptionResult {
    pub language: Option<String>,
    pub segments: Vec<TranscriptionSegment>,
    pub provider: String,
    pub model: String,
    /// SHA-256 of the exact host model bytes used for inference, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_sha256: Option<String>,
    #[serde(default)]
    pub word_timing_status: WordTimingStatus,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WordTimingStatus {
    #[default]
    NotRequested,
    Available,
    Unavailable,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TranscriptionSegment {
    pub start: MediaTime,
    pub end: MediaTime,
    pub text: String,
    /// Empty unless the configured engine actually returns validated word intervals.
    #[serde(default)]
    pub words: Vec<TranscriptionWord>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TranscriptionWord {
    pub start: MediaTime,
    pub end: MediaTime,
    pub text: String,
}

/// Shared publication boundary for native output and durable Job projections.
pub fn validate_public_transcription_result(value: &TranscriptionResult) -> bool {
    if value.provider != "whisper.cpp"
        || value.model.is_empty()
        || value.model.len() > 128
        || value.model_sha256.as_ref().is_some_and(|hash| {
            hash.len() != 64
                || !hash
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
        || !value
            .model
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        || value.language.as_ref().is_none_or(|language| {
            !(2..=3).contains(&language.len())
                || !language.bytes().all(|byte| byte.is_ascii_lowercase())
        })
        || value.segments.len() > 10_000
    {
        return false;
    }
    let mut previous_end = 0;
    let mut total_text = 0usize;
    let mut total_words = 0usize;
    for segment in &value.segments {
        total_text = total_text.saturating_add(segment.text.len());
        if segment.text.trim().is_empty()
            || segment.text.len() > 4000
            || total_text > 1_000_000
            || segment.start.timebase != 1000
            || segment.end.timebase != 1000
            || segment.start.ticks < previous_end
            || segment.end.ticks <= segment.start.ticks
            || segment.end.ticks > 3_600_000
        {
            return false;
        }
        total_words = total_words.saturating_add(segment.words.len());
        if total_words > 50_000
            || (value.word_timing_status != WordTimingStatus::Available
                && !segment.words.is_empty())
        {
            return false;
        }
        let mut previous_word_end = segment.start.ticks;
        let mut joined = String::new();
        for word in &segment.words {
            if word.start.timebase != 1000
                || word.end.timebase != 1000
                || word.start.ticks < previous_word_end
                || word.end.ticks <= word.start.ticks
                || word.end.ticks > segment.end.ticks
                || word.text.is_empty()
                || word.text.len() > 256
                || word.text.chars().any(char::is_whitespace)
                || !word.text.bytes().any(|byte| byte.is_ascii_alphabetic())
            {
                return false;
            }
            previous_word_end = word.end.ticks;
            joined.push_str(&word.text);
        }
        if !segment.words.is_empty()
            && joined
                != segment
                    .text
                    .chars()
                    .filter(|ch| !ch.is_whitespace())
                    .collect::<String>()
        {
            return false;
        }
        previous_end = segment.end.ticks;
    }
    if value.word_timing_status == WordTimingStatus::Available
        && value
            .segments
            .iter()
            .any(|segment| segment.words.is_empty())
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(start: i64, end: i64, text: String) -> TranscriptionSegment {
        TranscriptionSegment {
            start: MediaTime {
                ticks: start,
                timebase: 1000,
            },
            end: MediaTime {
                ticks: end,
                timebase: 1000,
            },
            text,
            words: Vec::new(),
        }
    }

    fn result(segments: Vec<TranscriptionSegment>) -> TranscriptionResult {
        TranscriptionResult {
            language: Some("zh".into()),
            segments,
            provider: "whisper.cpp".into(),
            model: "ggml-tiny.bin".into(),
            model_sha256: None,
            word_timing_status: WordTimingStatus::NotRequested,
        }
    }

    #[test]
    fn rejects_unpublishable_overlap_and_text_bounds() {
        assert!(validate_public_transcription_result(&result(vec![
            segment(0, 10, "a".into())
        ])));
        assert!(!validate_public_transcription_result(&result(vec![
            segment(0, 10, "a".into()),
            segment(9, 20, "b".into())
        ])));
        assert!(!validate_public_transcription_result(&result(vec![
            segment(0, 10, "x".repeat(4001))
        ])));
        let segments = (0..251)
            .map(|index| segment(index, index + 1, "x".repeat(4000)))
            .collect();
        assert!(!validate_public_transcription_result(&result(segments)));
        let segments = (0..10_001)
            .map(|index| segment(index, index + 1, "x".into()))
            .collect();
        assert!(!validate_public_transcription_result(&result(segments)));
    }
}
