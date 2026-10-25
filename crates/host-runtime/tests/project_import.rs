//! Real upload, native Worker, scoped binding and independently decoded pixels.
use opencut_asset_store::uploads::UploadChunks;
use opencut_contracts::*;
use opencut_editor_application::{access::*, uploads::*};
use opencut_sqlite_store::{EditorApplication, JobState};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    process::Command,
    sync::{Arc, atomic::AtomicBool},
};
fn ffmpeg(args: &[&str], output: &Path) {
    let result = Command::new("ffmpeg")
        .args(["-v", "error", "-y"])
        .args(args)
        .arg(output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
fn project(app: &mut EditorApplication, id: &str) -> ProjectSnapshot {
    app.create_project(
        CreateProject {
            id: id.into(),
            name: id.into(),
            sequence_id: "main".into(),
            width: 64,
            height: 48,
            fps: FrameRate {
                numerator: 10,
                denominator: 1,
            },
        },
        &format!("create-{id}"),
    )
    .unwrap()
}
fn upload(
    app: &mut EditorApplication,
    root: &Path,
    bytes: &[u8],
    key: &str,
    wrong_hash: bool,
) -> UploadView {
    let view = app
        .begin_upload(&BeginUpload {
            project_id: "a".into(),
            idempotency_key: key.into(),
            name: "display-name.mp4".into(),
            size_bytes: bytes.len().to_string(),
            content_hash: Some(if wrong_hash {
                "0".repeat(64)
            } else {
                format!("{:x}", Sha256::digest(bytes))
            }),
        })
        .unwrap();
    let mut offset = 0;
    for part in bytes.chunks(71) {
        let chunk = ChunkDescriptor {
            upload_id: view.upload_id.clone(),
            offset: offset.to_string(),
            size_bytes: part.len().to_string(),
            content_hash: format!("{:x}", Sha256::digest(part)),
        };
        UploadChunks::new(root.join("uploads"))
            .put(app, &chunk, part)
            .unwrap();
        offset += part.len();
    }
    app.complete_upload(&CompleteUpload {
        upload_id: view.upload_id,
        content_hash: view.content_hash.unwrap(),
    })
    .unwrap()
}
fn run(database: &Path) {
    opencut_host_runtime::worker::run(database, true, Arc::new(AtomicBool::new(false))).unwrap();
}
#[test]
fn uploaded_image_survives_source_removal_and_renders_after_scoped_binding() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let database = root.join("db");
    let source = root.join("source.png");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "color=red:s=64x48:r=10",
            "-frames:v",
            "1",
        ],
        &source,
    );
    let bytes = std::fs::read(&source).unwrap();
    let mut app = EditorApplication::open(&database).unwrap();
    let original = project(&mut app, "a");
    project(&mut app, "b");
    app.set_principal_policy(PrincipalPolicy {
        principal_id: "agent".into(),
        expected_revision: "0".into(),
        enabled: true,
        grants: ["a", "b"]
            .into_iter()
            .map(|id| ProjectGrant {
                project_id: id.into(),
                actions: vec![
                    ProjectAction::Read,
                    ProjectAction::Edit,
                    ProjectAction::RunJobs,
                ],
            })
            .collect(),
    })
    .unwrap();
    let credential = app.issue_credential("agent").unwrap();
    let caller = app.authenticate(&credential.secret).unwrap();
    app.set_caller(caller.clone());
    let accepted = upload(&mut app, root, &bytes, "red", false);
    std::fs::remove_file(source).unwrap();
    run(&database);
    let view = app.job_view(accepted.job_id.as_deref().unwrap()).unwrap();
    assert_eq!(view.state, JobState::Succeeded, "{view:?}");
    let Some(opencut_api_contracts::job_results::JobResultView::MediaImport(imported)) =
        view.result
    else {
        panic!("Expected typed media import result")
    };
    assert_eq!(imported.asset.kind, AssetKind::Image);
    assert_eq!(app.snapshot("a").unwrap(), original);
    assert_eq!(
        app.project_media(&ProjectMediaInput {
            project_id: "a".into(),
            media_id: imported.media_id.clone()
        })
        .unwrap(),
        imported
    );
    assert_eq!(
        app.project_media(&ProjectMediaInput {
            project_id: "b".into(),
            media_id: imported.media_id.clone()
        })
        .unwrap_err()
        .code,
        "NOT_AUTHORIZED"
    );
    let mut asset = imported.asset.clone();
    asset.id = "image".into();
    asset.name = "Renamed".into();
    for mode in ["uri", "hash", "metadata", "kind"] {
        let mut forged = asset.clone();
        match mode {
            "uri" => forged.uri = "/private/local-file".into(),
            "hash" => forged.version.as_mut().unwrap().content_hash = "0".repeat(64),
            "metadata" => {
                forged
                    .metadata
                    .insert("private_path".into(), json!("/secret"));
            }
            _ => forged.kind = AssetKind::Video,
        }
        let request = TransactionRequest {
            expected_revision: original.revision.clone(),
            idempotency_key: mode.into(),
            label: mode.into(),
            operations: vec![Operation::BindAsset { asset: forged }],
        };
        assert_eq!(app.apply("a", &request).unwrap_err().code, "NOT_AUTHORIZED");
    }
    let foreign = TransactionRequest {
        expected_revision: app.snapshot("b").unwrap().revision,
        idempotency_key: "cross-project-bind".into(),
        label: "cross-project".into(),
        operations: vec![Operation::BindAsset {
            asset: asset.clone(),
        }],
    };
    assert_eq!(app.apply("b", &foreign).unwrap_err().code, "NOT_AUTHORIZED");
    let request: TransactionRequest=serde_json::from_value(json!({"expected_revision":original.revision,"idempotency_key":"bind","label":"Imported image","operations":[
        {"type":"asset.bind","asset":asset},
        {"type":"track.create","sequence_id":"main","track":{"id":"v","name":"Image","kind":"graphic","enabled":true,"muted":false,"locked":false,"clips":[],"params":{}}},
        {"type":"clip.insert","sequence_id":"main","track_id":"v","clip":{"id":"red","asset_id":"image","timeline_range":{"start":{"ticks":"0","timebase":10},"duration":{"ticks":"10","timebase":10}},"source_range":null,"params":{},"channels":[]}}
    ]})).unwrap();
    let edited = app.apply("a", &request).unwrap();
    assert_eq!(app.apply("a", &request).unwrap(), edited);
    let asset_root = app.asset_store_root().to_path_buf();
    let preview=opencut_sqlite_store::jobs::start(&mut app,&asset_root,serde_json::from_value(json!({"kind":"preview.composited_frame","idempotency_key":"preview","input":{"project_id":"a","revision":edited.revision,"sequence_id":"main","requested_time":{"ticks":"0","timebase":10},"alignment":"exact"}})).unwrap()).unwrap();
    drop(app);
    run(&database);
    let app = EditorApplication::open(&database).unwrap();
    let done = app.get_job(&preview.job_id).unwrap();
    assert_eq!(done.state, JobState::Succeeded, "{done:?}");
    let artifact = done.result.unwrap()["artifact"]["path"]
        .as_str()
        .unwrap()
        .to_owned();
    let rgb = Command::new("ffmpeg")
        .args([
            "-v", "error", "-i", &artifact, "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(rgb.status.success());
    assert_eq!(rgb.stdout.len(), 64 * 48 * 3);
    let pixel = &rgb.stdout[(24 * 64 + 32) * 3..][..3];
    assert!(
        pixel[0] > 240 && pixel[1] < 10 && pixel[2] < 10,
        "{pixel:?}"
    );
    assert!(
        !root
            .join("artifacts")
            .join(accepted.job_id.unwrap())
            .join("attempt-1/inputs")
            .exists()
    );
}
#[test]
fn corrupt_bytes_wrong_full_hash_and_playlists_never_register_media() {
    for mode in ["corrupt", "hash", "playlist"] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let database = root.join("db");
        let source = root.join("source.png");
        ffmpeg(
            &["-f", "lavfi", "-i", "color=red:s=64x48", "-frames:v", "1"],
            &source,
        );
        let bytes = if mode == "playlist" {
            format!("ffconcat version 1.0\nfile '{}'\n", source.display()).into_bytes()
        } else {
            std::fs::read(source).unwrap()
        };
        let mut app = EditorApplication::open(&database).unwrap();
        let original = project(&mut app, "a");
        let accepted = upload(&mut app, root, &bytes, mode, mode == "hash");
        if mode == "corrupt" {
            std::fs::write(
                root.join("uploads")
                    .join(&accepted.upload_id)
                    .join("0.chunk"),
                b"bad",
            )
            .unwrap();
        }
        run(&database);
        let job = app.get_job(accepted.job_id.as_deref().unwrap()).unwrap();
        assert_eq!(job.state, JobState::Failed, "{job:?}");
        if mode != "playlist" {
            assert_eq!(
                job.error.as_ref().unwrap()["code"],
                "UPLOAD_INTEGRITY_ERROR"
            );
        } else {
            assert_eq!(job.error.as_ref().unwrap()["code"], "MEDIA_PROCESS_FAILED");
        }
        assert_eq!(app.snapshot("a").unwrap(), original);
        let media_id = job.input["media_id"].as_str().unwrap().to_owned();
        assert!(
            app.project_media(&ProjectMediaInput {
                project_id: "a".into(),
                media_id
            })
            .is_err()
        );
        assert!(
            !root
                .join("artifacts")
                .join(&job.job_id)
                .join("attempt-1/inputs")
                .exists()
        );
        let public = app.job_view(&job.job_id).unwrap();
        assert!(
            !serde_json::to_string(&public)
                .unwrap()
                .contains(root.to_str().unwrap())
        );
    }
}

#[test]
fn video_and_audio_are_classified_from_uploaded_bytes() {
    for (extension, kind) in [("mp4", AssetKind::Video), ("wav", AssetKind::Audio)] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let database = root.join("db");
        let source = root.join(format!("source.{extension}"));
        if extension == "mp4" {
            ffmpeg(
                &[
                    "-f",
                    "lavfi",
                    "-i",
                    "color=blue:s=64x48:r=10",
                    "-t",
                    "0.4",
                    "-c:v",
                    "libx264",
                    "-pix_fmt",
                    "yuv420p",
                ],
                &source,
            );
        } else {
            ffmpeg(
                &[
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:sample_rate=48000",
                    "-t",
                    "0.1",
                    "-c:a",
                    "pcm_s16le",
                ],
                &source,
            );
        }
        let bytes = std::fs::read(&source).unwrap();
        let mut app = EditorApplication::open(&database).unwrap();
        let original = project(&mut app, "a");
        let accepted = upload(&mut app, root, &bytes, extension, false);
        std::fs::remove_file(source).unwrap();
        run(&database);
        let job = app.get_job(accepted.job_id.as_deref().unwrap()).unwrap();
        assert_eq!(job.state, JobState::Succeeded, "{job:?}");
        let result: MediaImportJobResult = serde_json::from_value(job.result.unwrap()).unwrap();
        assert_eq!(result.asset.kind, kind);
        assert!(result.asset.duration.is_some());
        let resolved = opencut_asset_store::AssetStore::read_only(root.join("asset-store"))
            .unwrap()
            .resolve_asset(&result.asset)
            .unwrap();
        assert_eq!(std::fs::read(resolved.uri).unwrap(), bytes);
        assert_eq!(app.snapshot("a").unwrap(), original);
        let public = app.job_view(&job.job_id).unwrap();
        assert!(public.result.is_some());
        assert!(
            !serde_json::to_string(&public)
                .unwrap()
                .contains(root.to_str().unwrap())
        );
    }
}
