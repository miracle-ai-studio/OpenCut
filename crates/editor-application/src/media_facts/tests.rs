use super::*;
use opencut_contracts::AssetKind;
use serde_json::json;
#[test]
fn published_rate_is_typed_video_metadata_without_relaxing_resource_identity() {
    use crate::uploads::{PreparedMediaImportInput, PreparedUploadChunk, validate_import_result};
    use opencut_job_contracts::MediaImportJobResult;
    let hash = "a".repeat(64);
    let input = PreparedMediaImportInput {
        upload_id: format!("upload_{}", "b".repeat(32)),
        project_id: "p".into(),
        media_id: format!("media_{}", "c".repeat(32)),
        name: "video.mp4".into(),
        size_bytes: 1,
        content_hash: hash.clone(),
        authority: None,
        chunks: vec![PreparedUploadChunk {
            offset: 0,
            size_bytes: 1,
            content_hash: hash.clone(),
        }],
    };
    let mut resource:MediaImportJobResult=serde_json::from_value(json!({"media_id":input.media_id,"project_id":"p","upload_id":input.upload_id,"job_id":"job","asset":{"id":input.media_id,"name":"video.mp4","kind":"video","uri":format!("opencut-media://{}",input.media_id),"duration":{"ticks":"4","timebase":1},"version":{"id":format!("av_{hash}"),"content_hash":hash,"size":1},"metadata":{"content_hash":hash,"size":1,"mime_type":"video/mp4","has_audio":false,"frame_rate":{"numerator":60000,"denominator":1001}}}})).unwrap();
    validate_import_result(&input, "job", &resource).unwrap();
    let mut with_audio = resource.clone();
    with_audio
        .asset
        .metadata
        .insert("has_audio".into(), json!(true));
    with_audio.asset.metadata.insert(
        "audio_metadata".into(),
        json!({"sample_rate":48000,"channels":1,"channel_layout":"mono"}),
    );
    validate_import_result(&input, "job", &with_audio).unwrap();
    for format in [
        json!({"sample_rate":0,"channels":1,"channel_layout":null}),
        json!({"sample_rate":48000,"channels":1,"channel_layout":null,"native_path":"private"}),
    ] {
        let mut bad = with_audio.clone();
        bad.asset.metadata.insert("audio_metadata".into(), format);
        assert_eq!(
            validate_import_result(&input, "job", &bad)
                .unwrap_err()
                .code,
            "INVALID_JOB_RESULT"
        );
    }
    with_audio
        .asset
        .metadata
        .insert("has_audio".into(), json!(false));
    assert_eq!(
        validate_import_result(&input, "job", &with_audio)
            .unwrap_err()
            .code,
        "INVALID_JOB_RESULT"
    );
    for rate in [
        json!({"numerator":60,"denominator":0}),
        json!({"numerator":0,"denominator":1}),
        json!({"numerator":60,"denominator":1,"path":"private"}),
        json!("59.94"),
    ] {
        let mut bad = resource.clone();
        bad.asset.metadata.insert("frame_rate".into(), rate);
        assert_eq!(
            validate_import_result(&input, "job", &bad)
                .unwrap_err()
                .code,
            "INVALID_JOB_RESULT"
        );
    }
    let packet_key = opencut_media_contracts::CLASSIC_PACKET_RATE_METADATA_KEY;
    for rate in [Some(30.006), None] {
        resource.asset.metadata.insert(
            packet_key.into(),
            json!({
                "schema_version":1, "packet_count":100, "average_packet_rate":rate,
            }),
        );
        validate_import_result(&input, "job", &resource).unwrap();
    }
    for observation in [
        json!({"schema_version":2,"packet_count":100,"average_packet_rate":30.0}),
        json!({"schema_version":1,"packet_count":0,"average_packet_rate":30.0}),
        json!({"schema_version":1,"packet_count":100,"average_packet_rate":-1.0}),
        json!({"schema_version":1,"packet_count":100,"average_packet_rate":30.0,"path":"private"}),
    ] {
        let mut bad = resource.clone();
        bad.asset.metadata.insert(packet_key.into(), observation);
        assert_eq!(
            validate_import_result(&input, "job", &bad)
                .unwrap_err()
                .code,
            "INVALID_JOB_RESULT"
        );
    }
    resource.asset.kind = AssetKind::Image;
    assert_eq!(
        validate_import_result(&input, "job", &resource)
            .unwrap_err()
            .code,
        "INVALID_JOB_RESULT"
    );
    resource.asset.metadata.remove("frame_rate");
    assert_eq!(
        validate_import_result(&input, "job", &resource)
            .unwrap_err()
            .code,
        "INVALID_JOB_RESULT"
    );
    resource.asset.metadata.remove(packet_key);
    validate_import_result(&input, "job", &resource).unwrap();
    resource
        .asset
        .metadata
        .insert("native_path".into(), json!("private"));
    assert_eq!(
        validate_import_result(&input, "job", &resource)
            .unwrap_err()
            .code,
        "INVALID_JOB_RESULT"
    );
}

#[test]
fn raw_packet_normalization_preserves_classic_floats_and_rejects_unrepresentable_rates() {
    for (raw, expected) in [
        (30.006, (30, 1)),
        (30000.0 / 1001.0, (30000, 1001)),
        (24.01, (2401, 100)), // JS subtraction exceeds .01 at this floating boundary.
        (31.4159, (314159, 10000)),
        (31.0000005, (31000001, 1000000)),
        (f64::from(u32::MAX), (u32::MAX, 1)),
    ] {
        assert_eq!(
            normalize_classic_packet_fps(raw).unwrap(),
            FrameRate {
                numerator: expected.0,
                denominator: expected.1
            }
        );
    }
    for raw in [
        f64::NAN,
        f64::INFINITY,
        0.0,
        -1.0,
        0.0000001,
        f64::from(u32::MAX) + 1.0,
    ] {
        assert!(normalize_classic_packet_fps(raw).is_err());
    }
}
