use crate::MediaTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// English-only local speaker-turn detection; the host selects source and model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "SpeakerTurnsRequest")]
pub struct SpeakerTurnsRequest<P = String> {
    pub input: P,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpeakerTurn {
    /// A detected change after the model segment ending at this source time.
    pub at: MediaTime,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpeakerTurnsResult {
    pub turns: Vec<SpeakerTurn>,
    pub language: String,
    pub provider: String,
    pub method: String,
    pub model: String,
    pub model_sha256: String,
    pub confidence_known: bool,
    /// The model marks local turn boundaries, not stable speaker identities.
    pub speaker_identity_status: String,
}

pub fn validate_public_speaker_turns_result(value: &SpeakerTurnsResult) -> bool {
    if value.language != "en"
        || value.provider != "whisper.cpp"
        || value.method != "tinydiarize_speaker_turn_next"
        || value.model != "ggml-small.en-tdrz.bin"
        || value.model_sha256 != "ceac3ec06d1d98ef71aec665283564631055fd6129b79d8e1be4f9cc33cc54b4"
        || value.confidence_known
        || value.speaker_identity_status != "unavailable"
        || value.turns.len() > 10_000
    {
        return false;
    }
    let mut previous = 0;
    for turn in &value.turns {
        if turn.at.timebase != 1000 || turn.at.ticks <= previous || turn.at.ticks > 3_600_000 {
            return false;
        }
        previous = turn.at.ticks;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_turns_reject_fake_identity_and_unordered_times() {
        let mut value = SpeakerTurnsResult {
            turns: vec![SpeakerTurn {
                at: MediaTime {
                    ticks: 100,
                    timebase: 1000,
                },
            }],
            language: "en".into(),
            provider: "whisper.cpp".into(),
            method: "tinydiarize_speaker_turn_next".into(),
            model: "ggml-small.en-tdrz.bin".into(),
            model_sha256: "ceac3ec06d1d98ef71aec665283564631055fd6129b79d8e1be4f9cc33cc54b4".into(),
            confidence_known: false,
            speaker_identity_status: "unavailable".into(),
        };
        assert!(validate_public_speaker_turns_result(&value));
        value.speaker_identity_status = "speaker_1".into();
        assert!(!validate_public_speaker_turns_result(&value));
        value.speaker_identity_status = "unavailable".into();
        value.turns.push(SpeakerTurn {
            at: MediaTime {
                ticks: 99,
                timebase: 1000,
            },
        });
        assert!(!validate_public_speaker_turns_result(&value));
    }
}
