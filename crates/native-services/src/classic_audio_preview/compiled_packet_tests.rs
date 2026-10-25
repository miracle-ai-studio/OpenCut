use super::*;
use opencut_composition::scene::PreviewAudioVoiceBlock;
use opencut_composition::scene::{AudioParticipation, AudioReceipt, PreviewAudioIdentity};
use serde_json::{Value, json};

fn block(voices: usize, frames: u32, metadata: usize) -> PreviewAudioBlock {
    PreviewAudioBlock {
        identity: PreviewAudioIdentity {
            project_id: "project".into(),
            revision: "fixed".into(),
            sequence_id: "scene".into(),
            profile: CLASSIC_REALTIME_AUDIO_PROFILE,
            sample_rate: 48_000,
            channels: 2,
        },
        start_sample: 123,
        frame_count: frames,
        receipts: (0..metadata)
            .map(|i| AudioReceipt {
                track_id: "audio".into(),
                element_id: format!("e{i}"),
                decision: if i < voices {
                    AudioParticipation::Ready
                } else {
                    AudioParticipation::OutsideBlock
                },
            })
            .collect(),
        voices: (0..voices)
            .map(|i| PreviewAudioVoiceBlock {
                element_id: format!("e{i}"),
                block_offset: 0,
                frame_count: frames,
                positions: (0..frames)
                    .map(|n| {
                        Some(PreviewSourceSample {
                            segment_id: 7,
                            lower: u64::from(n),
                            upper: u64::from(n) + 1,
                            fraction: 0.5,
                        })
                    })
                    .collect(),
                amplitudes: vec![0.5; frames as usize],
            })
            .collect(),
    }
}
fn unpack(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}

#[test]
fn mixed_gap_fraction_and_high_decimal_are_lossless() {
    let mut original = block(1, 4, 1);
    original.start_sample = 9_007_199_254_740_993;
    original.voices[0].positions = vec![
        None,
        Some(PreviewSourceSample {
            segment_id: u32::MAX,
            lower: 9_007_199_254_740_993,
            upper: 9_007_199_254_740_994,
            fraction: 0.25,
        }),
        Some(PreviewSourceSample {
            segment_id: 1,
            lower: u64::MAX,
            upper: u64::MAX,
            fraction: -0.0,
        }),
        Some(PreviewSourceSample {
            segment_id: 2,
            lower: 0,
            upper: 1,
            fraction: 0.100_000_000_000_000_02,
        }),
    ];
    original.voices[0].amplitudes = vec![-0.0, 0.100_000_000_000_000_02, 1.5, 0.0];
    let full = serde_json::to_value(&original).unwrap();
    let packed = unpack(&reference_pack_block(original).unwrap());
    assert_eq!(packed["block"]["startSample"], "9007199254740993");
    assert_eq!(
        packed["block"]["voices"][0]["positions"],
        json!([
            null,
            [u32::MAX, "9007199254740993", "9007199254740994", 0.25],
            [1, "18446744073709551615", "18446744073709551615", -0.0],
            [2, "0", "1", 0.100_000_000_000_000_02]
        ])
    );
    for (actual, expected) in [
        (
            packed["block"]["voices"][0]["positions"][2][3]
                .as_f64()
                .unwrap(),
            -0.0f64,
        ),
        (
            packed["block"]["voices"][0]["positions"][3][3]
                .as_f64()
                .unwrap(),
            0.100_000_000_000_000_02,
        ),
        (
            packed["block"]["voices"][0]["amplitudes"][0]
                .as_f64()
                .unwrap(),
            -0.0,
        ),
        (
            packed["block"]["voices"][0]["amplitudes"][1]
                .as_f64()
                .unwrap(),
            0.100_000_000_000_000_02,
        ),
    ] {
        assert_eq!(actual.to_bits(), expected.to_bits());
    }
    let mut roundtrip = packed["block"].clone();
    for point in roundtrip["voices"][0]["positions"].as_array_mut().unwrap() {
        if !point.is_null() {
            *point =
                json!({"segmentId":point[0],"lower":point[1],"upper":point[2],"fraction":point[3]});
        }
    }
    assert_eq!(roundtrip, full);
}

#[test]
fn invalid_identity_nonfinite_gain_and_noncanonical_taps_are_rejected() {
    let original = block(1, 1, 1);
    let mut invalids = Vec::new();
    for (lower, upper, fraction) in [
        (0, 2, 0.5),
        (1, 0, 0.5),
        (u64::MAX, 0, 0.5),
        (0, 1, -0.1),
        (0, 1, 1.1),
        (0, 1, f64::NAN),
        (0, 1, f64::INFINITY),
    ] {
        let mut value = original.clone();
        value.voices[0].positions[0] = Some(PreviewSourceSample {
            segment_id: 0,
            lower,
            upper,
            fraction,
        });
        invalids.push(value);
    }
    for amplitude in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut value = original.clone();
        value.voices[0].amplitudes[0] = amplitude;
        invalids.push(value);
    }
    for bad_id in [
        " ".into(),
        "control\n".into(),
        "a".repeat(1025),
        "é".repeat(513),
    ] {
        let mut value = original.clone();
        value.identity.project_id = bad_id;
        invalids.push(value);
    }
    let mut value = original.clone();
    value.identity.profile = "other";
    invalids.push(value);
    let mut value = original.clone();
    value.identity.channels = 1;
    invalids.push(value);
    let mut value = original.clone();
    value.identity.sample_rate = 96_000;
    invalids.push(value);
    let mut value = original.clone();
    value.identity.revision.clear();
    invalids.push(value);
    let mut value = original.clone();
    value.identity.sequence_id.clear();
    invalids.push(value);
    let mut value = original.clone();
    value.receipts[0].track_id.clear();
    invalids.push(value);
    for value in invalids {
        assert_eq!(
            reference_pack_block(value).unwrap_err().code,
            "AUDIO_PROTOCOL_ERROR"
        );
    }
    let mut valid = original;
    valid.identity.sample_rate = 44_100;
    assert!(reference_pack_block(valid).is_ok());
}

#[test]
fn lengths_coverage_identity_and_work_budget_are_enforced() {
    let original = block(2, 2, 2);
    let mut invalids = Vec::new();
    let mut value = original.clone();
    value.voices[1].element_id = "e0".into();
    invalids.push(value);
    let mut value = original.clone();
    value.voices[1].element_id = "unknown".into();
    invalids.push(value);
    let mut value = original.clone();
    value.voices.swap(0, 1);
    invalids.push(value);
    let mut value = original.clone();
    value.receipts[1].element_id = "e0".into();
    invalids.push(value);
    let mut value = original.clone();
    value.voices[0].positions.clear();
    invalids.push(value);
    let mut value = original.clone();
    value.voices[0].amplitudes.pop();
    invalids.push(value);
    let mut value = original.clone();
    value.voices[0].block_offset = 1;
    invalids.push(value);
    let mut value = original.clone();
    value.voices[0].block_offset = u32::MAX;
    invalids.push(value);
    let mut value = original.clone();
    value.voices[0].frame_count = 0;
    invalids.push(value);
    let mut value = original.clone();
    value.start_sample = u64::MAX;
    invalids.push(value);
    let mut value = original.clone();
    value.frame_count = 0;
    invalids.push(value);
    let mut value = original;
    value.frame_count = 4097;
    invalids.push(value);
    for value in invalids {
        assert_eq!(
            reference_pack_block(value).unwrap_err().code,
            "AUDIO_PROTOCOL_ERROR"
        );
    }
    assert_eq!(
        reference_pack_block(block(33, 1, 33)).unwrap_err().code,
        "AUDIO_BUDGET_EXCEEDED"
    );
    assert_eq!(
        reference_pack_block(block(17, 4096, 100)).unwrap_err().code,
        "AUDIO_BUDGET_EXCEEDED"
    );
    assert!(reference_pack_block(block(32, 2048, 100)).is_ok());
    let mut valid = block(1, 2, 1);
    valid.start_sample = u64::MAX - 2;
    assert!(reference_pack_block(valid).is_ok());
}

#[test]
fn non_audio_receipts_do_not_inherit_the_participating_voice_cap() {
    let mut original = block(2, 3, 514);
    for receipt in original.receipts.iter_mut().skip(2) {
        receipt.decision = AudioParticipation::NonAudio;
    }
    let identity = serde_json::to_value(&original.identity).unwrap();
    let receipts = serde_json::to_value(&original.receipts).unwrap();
    let packed = unpack(&reference_pack_block(original).unwrap());
    assert_eq!(packed["block"]["receipts"], receipts);
    assert_eq!(packed["block"]["identity"], identity);
    assert_eq!(packed["block"]["voices"][1]["elementId"], "e1");
    assert_eq!(
        packed["block"]["voices"][1]["amplitudes"],
        json!([0.5, 0.5, 0.5])
    );
}

#[test]
fn five_dense_voices_with_hundred_receipts_fit_one_mebibyte() {
    let original = block(5, 4096, 100);
    let full_bytes = serde_json::to_vec(&original).unwrap().len();
    let identity = serde_json::to_value(&original.identity).unwrap();
    let receipts = serde_json::to_value(&original.receipts).unwrap();
    let bytes = reference_pack_block(original).unwrap();
    assert!(full_bytes > 1024 * 1024);
    assert!(bytes.len() < 1024 * 1024 - 256);
    assert!(bytes.len() * 2 < full_bytes);
    let packed = unpack(&bytes);
    assert_eq!(packed["block"]["receipts"], receipts);
    assert_eq!(packed["block"]["identity"], identity);
    assert_eq!(
        packed["block"]["voices"][4]["amplitudes"],
        json!(vec![0.5; 4096])
    );
}

// Independent test expansion uses only the wire references, not packer helpers.
fn expand_shared(packet: &Value) -> Value {
    let mut block = packet["block"].clone();
    for voice in block["voices"].as_array_mut().unwrap() {
        let position_ref = voice["positionRef"].as_u64().unwrap() as usize;
        let amplitude_ref = voice["amplitudeRef"].as_u64().unwrap() as usize;
        let fields = voice.as_object_mut().unwrap();
        fields.remove("positionRef");
        fields.remove("amplitudeRef");
        fields.insert(
            "positions".into(),
            packet["positionTables"][position_ref].clone(),
        );
        fields.insert(
            "amplitudes".into(),
            packet["amplitudeTables"][amplitude_ref].clone(),
        );
    }
    block
}

#[test]
fn packed_five_dense_voices_share_exact_facts_below_150_kilobytes() {
    let mut original = block(5, 4096, 100);
    original.start_sample = 9_007_199_254_740_993;
    for voice in &mut original.voices {
        voice.positions[0] = None;
        voice.positions[1] = Some(PreviewSourceSample {
            segment_id: u32::MAX,
            lower: u64::MAX,
            upper: u64::MAX,
            fraction: -0.0,
        });
        voice.amplitudes[0] = -0.0;
    }
    let reference = unpack(&reference_pack_block(original.clone()).unwrap());
    let bytes = pack_preview_block(original).unwrap();
    assert!(bytes.len() < 150_000);
    let packed = unpack(&bytes);
    assert_eq!(packed["positionTables"].as_array().unwrap().len(), 1);
    assert_eq!(packed["amplitudeTables"].as_array().unwrap().len(), 1);
    assert_eq!(expand_shared(&packed), reference["block"]);
    for voice in packed["block"]["voices"].as_array().unwrap() {
        assert_eq!(voice["positionRef"], 0);
        assert_eq!(voice["amplitudeRef"], 0);
    }
    assert_eq!(
        packed["positionTables"][0][1][3]
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(
        packed["amplitudeTables"][0][0].as_f64().unwrap().to_bits(),
        (-0.0f64).to_bits()
    );
}

#[test]
fn packed_preserves_every_fact_bit_and_length_with_first_use_table_order() {
    let mut original = block(10, 2, 10);
    for voice in &mut original.voices {
        voice.positions[0].as_mut().unwrap().fraction = 0.0;
        voice.amplitudes[0] = 0.0;
    }
    // Position changes are independent of source identity: even otherwise
    // equal voices must not share these changed facts or signed zero bits.
    original.voices[1].positions[0].as_mut().unwrap().fraction = -0.0;
    original.voices[2].positions[0].as_mut().unwrap().fraction = 0.100_000_000_000_000_02;
    original.voices[3].positions[0].as_mut().unwrap().segment_id = 8;
    original.voices[4].positions[0].as_mut().unwrap().lower = 1;
    original.voices[5].positions[0].as_mut().unwrap().upper = 0;
    original.voices[6].positions[0] = None;
    original.voices[7].frame_count = 1;
    original.voices[7].positions.truncate(1);
    original.voices[7].amplitudes.truncate(1);
    original.voices[8].amplitudes[0] = -0.0;
    original.voices[9].amplitudes[0] = 0.100_000_000_000_000_02;
    // Varying placement offset does not alter equal sample facts.
    original.voices[7].block_offset = 1;
    let reference = unpack(&reference_pack_block(original.clone()).unwrap());
    let packed = unpack(&pack_preview_block(original).unwrap());
    assert_eq!(expand_shared(&packed), reference["block"]);
    let voices = packed["block"]["voices"].as_array().unwrap();
    let position_refs: Vec<_> = voices
        .iter()
        .map(|v| v["positionRef"].as_u64().unwrap())
        .collect();
    let amplitude_refs: Vec<_> = voices
        .iter()
        .map(|v| v["amplitudeRef"].as_u64().unwrap())
        .collect();
    assert_eq!(position_refs, vec![0, 1, 2, 3, 4, 5, 6, 7, 0, 0]);
    assert_eq!(amplitude_refs, vec![0, 0, 0, 0, 0, 0, 0, 1, 2, 3]);
    assert_eq!(packed["positionTables"].as_array().unwrap().len(), 8);
    assert_eq!(packed["amplitudeTables"].as_array().unwrap().len(), 4);
    assert_eq!(
        packed["positionTables"][0][0][3]
            .as_f64()
            .unwrap()
            .to_bits(),
        0.0f64.to_bits()
    );
    assert_eq!(
        packed["positionTables"][1][0][3]
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(
        packed["amplitudeTables"][0][0].as_f64().unwrap().to_bits(),
        0.0f64.to_bits()
    );
    assert_eq!(
        packed["amplitudeTables"][2][0].as_f64().unwrap().to_bits(),
        (-0.0f64).to_bits()
    );
}

#[test]
fn packed_keeps_original_per_voice_admission_and_complete_non_audio_receipts() {
    assert_eq!(
        pack_preview_block(block(33, 1, 33)).unwrap_err().code,
        "AUDIO_BUDGET_EXCEEDED"
    );
    // One deduplicated table still represents 17*4096 samples of voice work.
    assert_eq!(
        pack_preview_block(block(17, 4096, 100)).unwrap_err().code,
        "AUDIO_BUDGET_EXCEEDED"
    );
    let accepted = unpack(&pack_preview_block(block(32, 2048, 100)).unwrap());
    assert_eq!(accepted["block"]["voices"].as_array().unwrap().len(), 32);
    assert_eq!(accepted["positionTables"].as_array().unwrap().len(), 1);
    let mut distinct = block(32, 1, 32);
    for (index, voice) in distinct.voices.iter_mut().enumerate() {
        voice.positions[0].as_mut().unwrap().fraction = index as f64 / 32.0;
        voice.amplitudes[0] = index as f64;
    }
    let distinct = unpack(&pack_preview_block(distinct).unwrap());
    assert_eq!(distinct["positionTables"].as_array().unwrap().len(), 32);
    assert_eq!(distinct["amplitudeTables"].as_array().unwrap().len(), 32);
    assert_eq!(distinct["block"]["voices"][31]["positionRef"], 31);
    assert_eq!(distinct["block"]["voices"][31]["amplitudeRef"], 31);
    let mut original = block(2, 3, 514);
    for receipt in original.receipts.iter_mut().skip(2) {
        receipt.decision = AudioParticipation::NonAudio;
    }
    let reference = unpack(&reference_pack_block(original.clone()).unwrap());
    let packed = unpack(&pack_preview_block(original).unwrap());
    assert_eq!(expand_shared(&packed), reference["block"]);
    assert_eq!(packed["block"]["receipts"].as_array().unwrap().len(), 514);
    let mut invalid = block(1, 1, 1);
    invalid.voices[0].amplitudes[0] = f64::NAN;
    assert_eq!(
        pack_preview_block(invalid).unwrap_err().code,
        "AUDIO_PROTOCOL_ERROR"
    );
}

// Independent dense reference encoder retained only for exact fact/bit oracles.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReferenceVoice<'a> {
    element_id: &'a str,
    block_offset: u32,
    frame_count: u32,
    positions: PackedPositions<'a>,
    amplitudes: &'a [f64],
}
struct ReferenceVoices<'a>(&'a [PreviewAudioVoiceBlock]);
impl Serialize for ReferenceVoices<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for voice in self.0 {
            sequence.serialize_element(&ReferenceVoice {
                element_id: &voice.element_id,
                block_offset: voice.block_offset,
                frame_count: voice.frame_count,
                positions: PackedPositions(&voice.positions),
                amplitudes: &voice.amplitudes,
            })?;
        }
        sequence.end()
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReferenceBlock<'a> {
    identity: &'a opencut_composition::scene::PreviewAudioIdentity,
    start_sample: String,
    frame_count: u32,
    voices: ReferenceVoices<'a>,
    receipts: &'a [opencut_composition::scene::AudioReceipt],
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReferencePacket<'a> {
    block: ReferenceBlock<'a>,
}

fn reference_pack_block(block: PreviewAudioBlock) -> Result<Vec<u8>, EditorError> {
    validate_preview_block(&block)?;
    // Borrow dense data all the way into the serializer: no intermediate point
    // maps, JSON values, sample vectors or clones are constructed.
    serde_json::to_vec(&ReferencePacket {
        block: ReferenceBlock {
            identity: &block.identity,
            start_sample: block.start_sample.to_string(),
            frame_count: block.frame_count,
            voices: ReferenceVoices(&block.voices),
            receipts: &block.receipts,
        },
    })
    .map_err(|_| invalid())
}
