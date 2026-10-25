//! Lossless private transport projection of already compiled Rust sample facts.
//! Tuples shorten the JSON packet; this adapter evaluates no project clocks.
use super::error;
use opencut_composition::scene::{
    CLASSIC_REALTIME_AUDIO_PROFILE, MAX_AUDIO_BLOCK_FRAMES, MAX_AUDIO_BLOCK_GAIN_SAMPLES,
    MAX_RUNTIME_AUDIO_VOICES, PreviewAudioBlock, PreviewSourceSample,
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
        "Invalid compiled preview block facts",
    )
}
fn budget() -> EditorError {
    error(
        "AUDIO_BUDGET_EXCEEDED",
        "Compiled preview block exceeds its work budget",
    )
}
fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn validate_preview_block(block: &PreviewAudioBlock) -> Result<(), EditorError> {
    let identity = &block.identity;
    if ![
        &identity.project_id,
        &identity.revision,
        &identity.sequence_id,
    ]
    .into_iter()
    .all(|value| valid_id(value))
        || identity.profile != CLASSIC_REALTIME_AUDIO_PROFILE
        || !matches!(identity.sample_rate, 44_100 | 48_000)
        || identity.channels != 2
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
    // Receipts include non-audio/disabled elements. Serialized reply bytes are
    // transport-bounded; the participating-voice cap does not bound this ledger.
    let mut receipt_ids = BTreeMap::new();
    for (ordinal, receipt) in block.receipts.iter().enumerate() {
        if !valid_id(&receipt.track_id)
            || !valid_id(&receipt.element_id)
            || receipt_ids
                .insert(receipt.element_id.as_str(), ordinal)
                .is_some()
        {
            return Err(invalid());
        }
    }
    let mut seen = BTreeSet::new();
    let mut last_ordinal = None;
    let mut work = 0usize;
    // Application/source binding still own the actual decoded tap bounds.
    for voice in &block.voices {
        let element = voice.element_id.as_str();
        let ordinal = *receipt_ids.get(element).ok_or_else(invalid)?;
        if !seen.insert(element) || last_ordinal.is_some_and(|previous| ordinal <= previous) {
            return Err(invalid());
        }
        last_ordinal = Some(ordinal);
        if voice.frame_count == 0
            || voice
                .block_offset
                .checked_add(voice.frame_count)
                .is_none_or(|end| end > block.frame_count)
            || voice.positions.len() != voice.frame_count as usize
            || voice.amplitudes.len() != voice.frame_count as usize
        {
            return Err(invalid());
        }
        work = work
            .checked_add(voice.frame_count as usize)
            .filter(|n| *n <= MAX_AUDIO_BLOCK_GAIN_SAMPLES)
            .ok_or_else(budget)?;
        if voice.amplitudes.iter().any(|value| !value.is_finite()) {
            return Err(invalid());
        }
        for point in voice.positions.iter().flatten() {
            if (point.upper != point.lower && point.lower.checked_add(1) != Some(point.upper))
                || !point.fraction.is_finite()
                || !(0.0..=1.0).contains(&point.fraction)
            {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

struct PackedPoint<'a>(&'a Option<PreviewSourceSample>);
impl Serialize for PackedPoint<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let Some(point) = self.0 else {
            return serializer.serialize_none();
        };
        let mut tuple = serializer.serialize_tuple(4)?;
        tuple.serialize_element(&point.segment_id)?;
        tuple.serialize_element(&point.lower.to_string())?;
        tuple.serialize_element(&point.upper.to_string())?;
        tuple.serialize_element(&point.fraction)?;
        tuple.end()
    }
}
struct PackedPositions<'a>(&'a [Option<PreviewSourceSample>]);
impl Serialize for PackedPositions<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for point in self.0 {
            sequence.serialize_element(&PackedPoint(point))?;
        }
        sequence.end()
    }
}

fn same_positions(
    left: &[Option<PreviewSourceSample>],
    right: &[Option<PreviewSourceSample>],
) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| match (left, right) {
                (None, None) => true,
                (Some(left), Some(right)) => {
                    left.segment_id == right.segment_id
                        && left.lower == right.lower
                        && left.upper == right.upper
                        && left.fraction.to_bits() == right.fraction.to_bits()
                }
                _ => false,
            })
}
fn same_amplitudes(left: &[f64], right: &[f64]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.to_bits() == right.to_bits())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PackedVoice<'a> {
    element_id: &'a str,
    block_offset: u32,
    frame_count: u32,
    position_ref: u32,
    amplitude_ref: u32,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PackedBlock<'a> {
    identity: &'a opencut_composition::scene::PreviewAudioIdentity,
    start_sample: String,
    frame_count: u32,
    voices: Vec<PackedVoice<'a>>,
    receipts: &'a [opencut_composition::scene::AudioReceipt],
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PackedPacket<'a> {
    block: PackedBlock<'a>,
    position_tables: Vec<PackedPositions<'a>>,
    amplitude_tables: Vec<&'a [f64]>,
}

pub(super) fn pack_preview_block(block: PreviewAudioBlock) -> Result<Vec<u8>, EditorError> {
    // Admission counts the original per-voice work, even when all tables share.
    validate_preview_block(&block)?;
    let mut position_tables: Vec<PackedPositions<'_>> = Vec::new();
    let mut amplitude_tables: Vec<&[f64]> = Vec::new();
    let mut voices = Vec::with_capacity(block.voices.len());
    for voice in &block.voices {
        let position_ref = position_tables
            .iter()
            .position(|table| same_positions(table.0, &voice.positions))
            .unwrap_or_else(|| {
                position_tables.push(PackedPositions(&voice.positions));
                position_tables.len() - 1
            });
        let amplitude_ref = amplitude_tables
            .iter()
            .position(|table| same_amplitudes(table, &voice.amplitudes))
            .unwrap_or_else(|| {
                amplitude_tables.push(&voice.amplitudes);
                amplitude_tables.len() - 1
            });
        voices.push(PackedVoice {
            element_id: &voice.element_id,
            block_offset: voice.block_offset,
            frame_count: voice.frame_count,
            // Both first-use tables are bounded by the validated 32 voices.
            position_ref: position_ref as u32,
            amplitude_ref: amplitude_ref as u32,
        });
    }
    // Only references and at most 32 voice records are allocated. Source maps
    // remain per voice; sharing is exact compiled fact equality, not clock math.
    serde_json::to_vec(&PackedPacket {
        block: PackedBlock {
            identity: &block.identity,
            start_sample: block.start_sample.to_string(),
            frame_count: block.frame_count,
            voices,
            receipts: &block.receipts,
        },
        position_tables,
        amplitude_tables,
    })
    .map_err(|_| invalid())
}

#[cfg(test)]
#[path = "compiled_packet_tests.rs"]
mod tests;
