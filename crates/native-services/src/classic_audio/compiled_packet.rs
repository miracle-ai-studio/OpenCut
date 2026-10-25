//! Lossless transport projection of authoritative compiled export sample facts.
//! Sharing is exact equality, never a new clock, gain or mixing implementation.
use super::transport::error;
use opencut_composition::scene::{
    AudioExecutionProfile, ExactSourceSample, MAX_AUDIO_BLOCK_FRAMES, MAX_AUDIO_BLOCK_GAIN_SAMPLES,
    MAX_RUNTIME_AUDIO_VOICES, RuntimeAudioBlock,
};
use opencut_contracts::EditorError;
use serde::{
    Serialize, Serializer,
    ser::{SerializeSeq, SerializeTuple},
};
use std::collections::{BTreeMap, BTreeSet};

fn invalid() -> EditorError {
    error(
        "AUDIO_PROTOCOL_ERROR",
        "Invalid compiled export sample facts",
    )
}
fn budget() -> EditorError {
    error(
        "AUDIO_BUDGET_EXCEEDED",
        "Compiled export block exceeds its work budget",
    )
}
fn valid_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= 1024 && !id.chars().any(char::is_control)
}
fn validate(block: &RuntimeAudioBlock) -> Result<(), EditorError> {
    let identity = &block.identity;
    if ![
        &identity.project_id,
        &identity.revision,
        &identity.sequence_id,
    ]
    .into_iter()
    .all(|id| valid_id(id))
        || identity.profile != AudioExecutionProfile::ClassicV31
        || !(8000..=192000).contains(&identity.format.sample_rate)
        || identity.format.channels != 2
        || !(1..=MAX_AUDIO_BLOCK_FRAMES).contains(&block.frame_count)
        || block
            .start_sample
            .checked_add(u64::from(block.frame_count))
            .is_none()
    {
        return Err(invalid());
    }
    if block.voices.len() > MAX_RUNTIME_AUDIO_VOICES {
        return Err(budget());
    }
    let mut receipts = BTreeMap::new();
    for (ordinal, receipt) in block.receipts.iter().enumerate() {
        if !valid_id(&receipt.track_id)
            || !valid_id(&receipt.element_id)
            || receipts
                .insert(receipt.element_id.as_str(), ordinal)
                .is_some()
        {
            return Err(invalid());
        }
    }
    let mut seen = BTreeSet::new();
    let mut last = None;
    let mut work = 0usize;
    for voice in &block.voices {
        let ordinal = *receipts
            .get(voice.element_id.as_str())
            .ok_or_else(invalid)?;
        if !seen.insert(voice.element_id.as_str())
            || last.is_some_and(|old| ordinal <= old)
            || voice.frame_count == 0
            || voice
                .block_offset
                .checked_add(voice.frame_count)
                .is_none_or(|n| n > block.frame_count)
            || voice.positions.len() != voice.frame_count as usize
            || voice.amplitudes.len() != voice.frame_count as usize
        {
            return Err(invalid());
        }
        last = Some(ordinal);
        // Count every original voice even when its transport tables are shared.
        work = work
            .checked_add(voice.frame_count as usize)
            .filter(|n| *n <= MAX_AUDIO_BLOCK_GAIN_SAMPLES)
            .ok_or_else(budget)?;
        if voice
            .positions
            .iter()
            .any(|p| p.fraction_denominator == 0 || p.fraction_numerator >= p.fraction_denominator)
            || voice.amplitudes.iter().any(|a| !a.is_finite() || *a < 0.0)
        {
            return Err(invalid());
        }
    }
    Ok(())
}
struct Positions<'a>(&'a [ExactSourceSample]);
struct Point<'a>(&'a ExactSourceSample);
impl Serialize for Point<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut tuple = serializer.serialize_tuple(3)?;
        tuple.serialize_element(&self.0.lower.to_string())?;
        tuple.serialize_element(&self.0.fraction_numerator.to_string())?;
        tuple.serialize_element(&self.0.fraction_denominator.to_string())?;
        tuple.end()
    }
}
impl Serialize for Positions<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for point in self.0 {
            sequence.serialize_element(&Point(point))?;
        }
        sequence.end()
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Voice<'a> {
    element_id: &'a str,
    block_offset: u32,
    frame_count: u32,
    position_ref: usize,
    amplitude_ref: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Packet<'a> {
    block: serde_json::Value,
    position_tables: Vec<Positions<'a>>,
    amplitude_tables: Vec<&'a [f64]>,
}
pub(super) fn pack_export_block(block: RuntimeAudioBlock) -> Result<Vec<u8>, EditorError> {
    validate(&block)?;
    let mut positions: Vec<Positions<'_>> = Vec::new();
    let mut amplitudes: Vec<&[f64]> = Vec::new();
    let mut voices = Vec::with_capacity(block.voices.len());
    for voice in &block.voices {
        let position_ref = positions
            .iter()
            .position(|table| table.0 == voice.positions.as_slice())
            .unwrap_or_else(|| {
                positions.push(Positions(&voice.positions));
                positions.len() - 1
            });
        let amplitude_ref = amplitudes
            .iter()
            .position(|table| {
                table.len() == voice.amplitudes.len()
                    && table
                        .iter()
                        .zip(&voice.amplitudes)
                        .all(|(a, b)| a.to_bits() == b.to_bits())
            })
            .unwrap_or_else(|| {
                amplitudes.push(&voice.amplitudes);
                amplitudes.len() - 1
            });
        voices.push(Voice {
            element_id: &voice.element_id,
            block_offset: voice.block_offset,
            frame_count: voice.frame_count,
            position_ref,
            amplitude_ref,
        });
    }
    // The existing Rust serializer remains the sole declaration of identity,
    // decimal source clocks and participation metadata. Only the small header
    // uses Value; dense sample tables stay borrowed and are encoded once.
    let mut header = serde_json::to_value(RuntimeAudioBlock {
        identity: block.identity.clone(),
        start_sample: block.start_sample,
        frame_count: block.frame_count,
        voices: Vec::new(),
        receipts: block.receipts.clone(),
    })
    .map_err(|_| invalid())?;
    header["voices"] = serde_json::to_value(voices).map_err(|_| invalid())?;
    serde_json::to_vec(&Packet {
        block: header,
        position_tables: positions,
        amplitude_tables: amplitudes,
    })
    .map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_composition::scene::{
        AudioIdentity, AudioParticipation, AudioReceipt, PcmFormat, RuntimeAudioVoiceBlock,
    };
    fn block(voices: usize, frames: u32) -> RuntimeAudioBlock {
        RuntimeAudioBlock {
            identity: AudioIdentity {
                project_id: "project".into(),
                revision: "fixed".into(),
                sequence_id: "scene".into(),
                profile: AudioExecutionProfile::ClassicV31,
                format: PcmFormat {
                    sample_rate: 48000,
                    channels: 2,
                },
            },
            start_sample: 0,
            frame_count: frames,
            receipts: (0..voices)
                .map(|i| AudioReceipt {
                    track_id: "track".into(),
                    element_id: format!("e{i}"),
                    decision: AudioParticipation::Ready,
                })
                .collect(),
            voices: (0..voices)
                .map(|i| RuntimeAudioVoiceBlock {
                    element_id: format!("e{i}"),
                    block_offset: 0,
                    frame_count: frames,
                    positions: (0..frames)
                        .map(|n| ExactSourceSample {
                            lower: u64::from(n),
                            fraction_numerator: 0,
                            fraction_denominator: 1,
                        })
                        .collect(),
                    amplitudes: vec![0.5; frames as usize],
                })
                .collect(),
        }
    }
    #[test]
    fn exact_tables_share_without_losing_negative_zero_or_wide_fractions() {
        let mut value = block(3, 2);
        for voice in &mut value.voices {
            voice.positions[0] = ExactSourceSample {
                lower: u64::MAX,
                fraction_numerator: u128::MAX - 1,
                fraction_denominator: u128::MAX,
            };
        }
        value.voices[1].amplitudes[0] = -0.0;
        value.voices[2].amplitudes[0] = 0.0;
        let bytes = pack_export_block(value).unwrap();
        assert!(std::str::from_utf8(&bytes).unwrap().contains("-0.0"));
        let p: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(p["positionTables"].as_array().unwrap().len(), 1);
        assert_eq!(p["amplitudeTables"].as_array().unwrap().len(), 3);
        assert_eq!(p["positionTables"][0][0][0], u64::MAX.to_string());
        assert_eq!(p["positionTables"][0][0][2], u128::MAX.to_string());
        assert_eq!(
            p["block"]["identity"]["profile"],
            "classic-export-v31-web-audio"
        );
    }
    #[test]
    fn original_work_and_order_are_admitted_before_table_sharing() {
        assert!(pack_export_block(block(32, 2048)).is_ok());
        assert_eq!(
            pack_export_block(block(33, 1)).unwrap_err().code,
            "AUDIO_BUDGET_EXCEEDED"
        );
        assert_eq!(
            pack_export_block(block(32, 4096)).unwrap_err().code,
            "AUDIO_BUDGET_EXCEEDED"
        );
        let mut invalid_block = block(2, 4);
        invalid_block.voices.reverse();
        assert_eq!(
            pack_export_block(invalid_block).unwrap_err().code,
            "AUDIO_PROTOCOL_ERROR"
        );
    }
}
