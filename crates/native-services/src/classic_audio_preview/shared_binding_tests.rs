use super::*;
use serde_json::json;

fn fixture() -> (Value, Value, Value) {
    let segments = json!([
        {"segmentId":9,"timestampSeconds":-1.0/48_000.0,"frameCount":"1"},
        {"segmentId":2,"timestampSeconds":0.0,"frameCount":"2"},
    ]);
    let voices = json!([
        {"elementId":"ordinary","assetId":"asset","sourceMode":"preserve-pts"},
        {"elementId":"prepared","assetId":"asset","sourceMode":"prepared-concat"},
    ]);
    let formats = json!([
        {"elementId":"ordinary","sampleRate":48_000,"frameCount":"3","channels":1,"prepared":false,"sourceRef":"asset"},
        {"elementId":"prepared","sampleRate":48_000,"frameCount":"2","channels":1,"prepared":true,"sourceRef":null},
    ]);
    let binding = json!({"ordinarySources":[{"assetId":"asset","segments":segments}],"sourceFormats":formats});
    let expanded = json!([
        {"elementId":"ordinary","sampleRate":48_000,"frameCount":"3","channels":1,"prepared":false,"segments":segments},
        {"elementId":"prepared","sampleRate":48_000,"frameCount":"2","channels":1,"prepared":true,"segments":[]},
    ]);
    (binding, voices, expanded)
}

#[test]
fn shared_binding_preserves_voice_segment_order_decimal_and_original_pts_hash() {
    let (binding, voices, expected) = fixture();
    let expanded = expand_source_binding(&binding, &voices).unwrap();
    assert_eq!(expanded, expected);
    let typed: Vec<PreviewSourceFormat> = serde_json::from_value(expanded.clone()).unwrap();
    assert_eq!(typed[0].segments[0].segment_id, 9);
    assert_eq!(typed[0].segments[1].segment_id, 2);
    assert_eq!(
        typed[0].segments[0].timestamp_seconds.to_bits(),
        (-1.0f64 / 48_000.0).to_bits()
    );
    assert_eq!(expanded[0]["frameCount"], "3");
    assert_eq!(expanded[1]["segments"], json!([]));
    // Existing receipt hashing sees the identical expanded facts. Dictionary
    // key order and representation never replace that accepted digest input.
    assert_eq!(
        super::super::canonical_binding(&expanded).unwrap(),
        super::super::canonical_binding(&expected).unwrap()
    );
    assert_eq!(
        expand_source_binding(
            &json!({"ordinarySources":[],"sourceFormats":[]}),
            &json!([])
        )
        .unwrap(),
        json!([])
    );
}

#[test]
fn shared_binding_rejects_duplicate_missing_orphan_changed_mode_and_order() {
    let (binding, voices, _) = fixture();
    let mut bad = Vec::new();
    let mut candidate = binding.clone();
    candidate["schemaVersion"] = json!(2);
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["sourceFormats"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["sourceFormats"][1]["elementId"] = json!("ordinary");
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["sourceFormats"].as_array_mut().unwrap().pop();
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["ordinarySources"] = json!([]);
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["ordinarySources"]
        .as_array_mut()
        .unwrap()
        .push(binding["ordinarySources"][0].clone());
    bad.push(candidate);
    let mut candidate = binding.clone();
    let mut orphan = binding["ordinarySources"][0].clone();
    orphan["assetId"] = json!("orphan");
    candidate["ordinarySources"]
        .as_array_mut()
        .unwrap()
        .push(orphan);
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["sourceFormats"][0]["sourceRef"] = json!("other");
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["sourceFormats"][0]["sourceRef"] = Value::Null;
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["sourceFormats"][1]["sourceRef"] = json!("asset");
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["sourceFormats"][0]["prepared"] = json!(true);
    candidate["sourceFormats"][0]["sourceRef"] = Value::Null;
    bad.push(candidate);
    for candidate in bad {
        assert_eq!(
            expand_source_binding(&candidate, &voices).unwrap_err().code,
            "AUDIO_PROTOCOL_ERROR"
        );
    }
    let mut duplicate = voices.clone();
    duplicate[1] = voices[0].clone();
    assert!(expand_source_binding(&binding, &duplicate).is_err());
}

#[test]
fn shared_binding_rejects_noncanonical_frames_unknown_fields_and_invalid_segments() {
    let (binding, voices, _) = fixture();
    let mut bad = Vec::new();
    let mut candidate = binding.clone();
    candidate["extra"] = json!(true);
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["sourceFormats"][0]["extra"] = json!(true);
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["sourceFormats"][1]
        .as_object_mut()
        .unwrap()
        .remove("sourceRef");
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["ordinarySources"][0]["segments"][0]["extra"] = json!(true);
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["sourceFormats"][0]["frameCount"] = json!("03");
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["sourceFormats"][0]["frameCount"] = json!("4");
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["sourceFormats"][0]["channels"] = json!(3);
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["sourceFormats"][0]["sampleRate"] = json!(0);
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["ordinarySources"][0]["segments"] = json!([]);
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["ordinarySources"][0]["segments"][0]["frameCount"] = json!("0");
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["ordinarySources"][0]["segments"][0]["frameCount"] = json!("01");
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["ordinarySources"][0]["segments"][1]["segmentId"] = json!(9);
    bad.push(candidate);
    let mut candidate = binding.clone();
    candidate["ordinarySources"][0]["segments"][0]["timestampSeconds"] = Value::Null;
    bad.push(candidate);
    for candidate in bad {
        assert_eq!(
            expand_source_binding(&candidate, &voices).unwrap_err().code,
            "AUDIO_PROTOCOL_ERROR"
        );
    }
}

fn dense_binding(voices: usize, segments: usize) -> (Value, Value) {
    let dictionary = (0..segments).map(|index| json!({"segmentId":index,"timestampSeconds":index as f64 * 1024.0 / 48_000.0,"frameCount":"1024"})).collect::<Vec<_>>();
    let fixed = (0..voices).map(|index| json!({"elementId":format!("voice-{index}"),"assetId":"asset","sourceMode":"preserve-pts"})).collect::<Vec<_>>();
    let formats = (0..voices).map(|index| json!({"elementId":format!("voice-{index}"),"sampleRate":48_000,"frameCount":(segments*1024).to_string(),"channels":1,"prepared":false,"sourceRef":"asset"})).collect::<Vec<_>>();
    (
        json!({"ordinarySources":[{"assetId":"asset","segments":dictionary}],"sourceFormats":formats}),
        Value::Array(fixed),
    )
}

#[test]
fn hundred_voices_share_188_segments_with_bounded_expansion() {
    let (binding, fixed) = dense_binding(100, 188);
    let expanded = expand_source_binding(&binding, &fixed).unwrap();
    let compressed_bytes = serde_json::to_vec(&binding).unwrap().len();
    let expanded_bytes = serde_json::to_vec(&expanded).unwrap().len();
    assert!(compressed_bytes < 1024 * 1024);
    assert!(expanded_bytes > 1024 * 1024);
    assert!(expanded_bytes <= MAX_EXPANDED_BYTES);
    assert_eq!(expanded.as_array().unwrap().len(), 100);
    for format in expanded.as_array().unwrap() {
        assert_eq!(
            format["segments"],
            binding["ordinarySources"][0]["segments"]
        );
    }
    // Only the small shared input is constructed. Expansion is refused during
    // preflight before cloning thousands of segments for 256 participants.
    let (amplified, fixed) = dense_binding(256, 2000);
    assert_eq!(
        expand_source_binding(&amplified, &fixed).unwrap_err().code,
        "AUDIO_BUDGET_EXCEEDED"
    );
}
