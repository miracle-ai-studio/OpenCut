use super::*;
use opencut_contracts::{CreateProject, media_import::MediaBindingV1};
use serde_json::json;
fn asset(id: &str, kind: AssetKind, n: u32, d: u32) -> Asset {
    let mut a:Asset=serde_json::from_value(json!({"id":id,"name":id,"kind":kind,"uri":format!("opencut-media://{id}"),"duration":{"ticks":"4","timebase":1},"metadata":{"frame_rate":{"numerator":n,"denominator":d}}})).unwrap();
    if n == 0 {
        a.metadata.clear();
    }
    a
}
fn snapshot() -> ProjectSnapshotV3 {
    let p = crate::project_v3::create(&CreateProject {
        id: "p".into(),
        name: "P".into(),
        sequence_id: "s".into(),
        width: 160,
        height: 90,
        fps: FrameRate {
            numerator: 24,
            denominator: 1,
        },
    })
    .unwrap();
    ProjectSnapshotV3 {
        project: p,
        revision: "rev".into(),
    }
}
fn input() -> MediaImportRequestV1 {
    MediaImportRequestV1 {
        project_id: "p".into(),
        expected_revision: "rev".into(),
        sequence_id: "s".into(),
        idempotency_key: "import".into(),
        label: "Import".into(),
        media: vec![MediaBindingV1 {
            media_id: "video".into(),
            replace_asset_id: None,
        }],
        operations: vec![],
        ripple: false,
    }
}
#[test]
fn classic_rates_mixed_inputs_and_invalid_or_nonvideo_metadata() {
    let current = FrameRate {
        numerator: 24,
        denominator: 1,
    };
    for (n, d, expected) in [
        (25, 1, (25, 1)),
        (2997, 100, (30000, 1001)),
        (5994, 100, (60000, 1001)),
        (48, 1, (48, 1)),
        (120, 1, (120, 1)),
        (314159, 10000, (314159, 10000)),
    ] {
        assert_eq!(
            raised_fps(&current, &[asset("v", AssetKind::Video, n, d)]).unwrap(),
            Some(FrameRate {
                numerator: expected.0,
                denominator: expected.1
            })
        );
    }
    let videos = vec![
        asset("low", AssetKind::Video, 25, 1),
        asset("high", AssetKind::Video, 60000, 1001),
        asset("a", AssetKind::Audio, 120, 1),
        asset("i", AssetKind::Image, 200, 1),
    ];
    assert_eq!(
        raised_fps(&current, &videos).unwrap(),
        Some(FrameRate {
            numerator: 60000,
            denominator: 1001
        })
    );
    assert_eq!(
        raised_fps(
            &FrameRate {
                numerator: 60,
                denominator: 1
            },
            &videos
        )
        .unwrap(),
        None
    );
    for (kind, n, d) in [
        (AssetKind::Video, 0, 1),
        (AssetKind::Video, 60, 0),
        (AssetKind::Video, 23, 1),
        (AssetKind::Audio, 60, 1),
        (AssetKind::Image, 60, 1),
        (AssetKind::Video, 240005, 10000),
    ] {
        assert_eq!(
            raised_fps(&current, &[asset("v", kind, n, d)]).unwrap(),
            None
        );
    }
    // Normalization cannot lower a custom setting through the tolerance edge.
    assert_eq!(
        raised_fps(
            &FrameRate {
                numerator: 24005,
                denominator: 1000
            },
            &[asset("v", AssetKind::Video, 24009, 1000)]
        )
        .unwrap(),
        None
    );
}
#[test]
fn binding_and_fps_preserve_existing_exact_timing_and_other_scene() {
    let mut s = snapshot();
    let a = asset("old", AssetKind::Video, 24, 1);
    s.project.base.assets.push(a);
    s.project.base.sequences[0].tracks.push(serde_json::from_value(json!({"id":"v","name":"V","kind":"video","enabled":true,"muted":false,"locked":false,"clips":[{"id":"old-clip","asset_id":"old","timeline_range":{"start":{"ticks":"1001","timebase":30000},"duration":{"ticks":"1001","timebase":1000}},"source_range":{"start":{"ticks":"1001","timebase":24000},"duration":{"ticks":"1001","timebase":1000}},"params":{},"channels":[{"parameter":"opacity","keyframes":[{"id":"k","time":{"ticks":"1001","timebase":30000},"value":0.5,"interpolation":"linear"}]}]}]})).unwrap());
    let mut other = s.project.base.sequences[0].clone();
    other.id = "other".into();
    other.tracks.clear();
    s.project.base.sequences.push(other.clone());
    let before = s.clone();
    let mut observed = asset("video", AssetKind::Video, 60000, 1001);
    let packet_key = opencut_media_contracts::CLASSIC_PACKET_RATE_METADATA_KEY_V1;
    observed.metadata.insert(
        packet_key.into(),
        json!({
            "schema_version":1, "packet_count":100, "average_packet_rate":30.006,
        }),
    );
    let plan = plan(&s, &input(), std::slice::from_ref(&observed)).unwrap();
    let next = crate::project_v3::plan_apply(&s.project, &plan.transaction)
        .unwrap()
        .proposed_project;
    assert_eq!(
        next.base.sequences[0].fps,
        FrameRate {
            numerator: 60000,
            denominator: 1001
        }
    );
    assert_eq!(
        next.base.sequences[0].tracks,
        s.project.base.sequences[0].tracks
    );
    assert_eq!(next.base.sequences[1], other);
    assert!(observed.metadata.contains_key(packet_key));
    assert!(
        !next
            .base
            .assets
            .iter()
            .find(|a| a.id == "video")
            .unwrap()
            .metadata
            .contains_key(packet_key)
    );
    assert_eq!(
        next.base
            .assets
            .iter()
            .find(|a| a.id == "video")
            .unwrap()
            .metadata
            .get("frame_rate"),
        observed.metadata.get("frame_rate")
    );
    assert_eq!(s, before);
    assert_eq!(plan.transaction.operations.len(), 2);
}
#[test]
fn fixed_scope_replacements_and_atomic_validation() {
    let mut s = snapshot();
    s.project
        .base
        .assets
        .push(asset("old", AssetKind::Video, 24, 1));
    let mut req = input();
    req.media[0].replace_asset_id = Some("old".into());
    let p = plan(&s, &req, &[asset("video", AssetKind::Video, 30, 1)]).unwrap();
    assert!(
        matches!(&p.transaction.operations[0],VisualOperation::Base{operation:Operation::ReplaceAsset{asset}} if asset.id=="old")
    );
    assert_eq!(
        plan(&s, &req, &[asset("video", AssetKind::Audio, 60, 1)])
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
    req.media.push(req.media[0].clone());
    assert_eq!(validate(&req).unwrap_err().code, "INVALID_ARGUMENT");
    req.media.pop();
    req.expected_revision = "stale".into();
    assert_eq!(
        plan(&s, &req, &[asset("video", AssetKind::Video, 30, 1)])
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
    req.expected_revision = "rev".into();
    assert_eq!(
        plan(&s, &req, &[asset("wrong", AssetKind::Video, 30, 1)])
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
    req.operations.push(VisualOperation::Base {
        operation: Operation::BindAsset {
            asset: asset("untrusted", AssetKind::Video, 120, 1),
        },
    });
    assert_eq!(validate(&req).unwrap_err().code, "INVALID_ARGUMENT");
    req.operations.clear();
    req.media[0].replace_asset_id = None;
    req.operations.push(serde_json::from_value(json!({"type":"base.apply","operation":{"type":"track.create","sequence_id":"other","track":{"id":"v","name":"V","kind":"video","enabled":true,"muted":false,"locked":false,"clips":[]}}})).unwrap());
    assert_eq!(
        plan(&s, &req, &[asset("video", AssetKind::Video, 30, 1)])
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
}

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
    let packet_key = opencut_media_contracts::CLASSIC_PACKET_RATE_METADATA_KEY_V1;
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
