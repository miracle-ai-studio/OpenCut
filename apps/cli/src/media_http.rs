use crate::blocking_owned::BlockingOwned;
use axum::{
    body::Body,
    extract::{Path, Query},
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use opencut_contracts::{Asset, EditorError};
use opencut_media::{FfmpegBackend, ProbeRequest};
use opencut_native_services::budget::AdmittedFile;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::io::Read;
use tokio_util::io::ReaderStream;

use crate::http::{Host, error_response, into_http};
use crate::http_identity::AuthorizedHost;

#[derive(Deserialize)]
pub(super) struct UploadQuery {
    name: String,
}

pub(super) async fn upload(
    AuthorizedHost(host): AuthorizedHost,
    Query(query): Query<UploadQuery>,
    body: Body,
) -> Response {
    if !host.identity.caller.is_owner() {
        return error_response(
            "NOT_AUTHORIZED",
            "Global media upload requires owner authority",
        );
    }
    let staging = host
        .media_root
        .join(format!(".upload-{}", uuid::Uuid::new_v4().simple()));
    let context = opencut_automation::ExecutionContext::for_caller(
        opencut_automation::CallSource::Http,
        host.identity.caller.clone(),
    );
    let request_id = context.request_id().to_owned();
    let result = upload_inner(&host, query, body, &staging, context).await;
    into_http(opencut_automation::Response::from_result_with_id(
        request_id, result,
    ))
}

async fn upload_inner(
    host: &Host,
    query: UploadQuery,
    body: Body,
    staging: &std::path::Path,
    context: opencut_automation::ExecutionContext,
) -> Result<serde_json::Value, EditorError> {
    if query.name.trim().is_empty() || query.name.len() > 512 {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "A filename of 1..512 bytes is required",
        ));
    }
    let database = host.database.clone();
    let temporary = staging.to_path_buf();
    let mut file = BlockingOwned::create(move || {
        AdmittedFile::create(&database, &temporary, "media_upload", 0, false)
    })
    .await?;
    let mut stream = body.into_data_stream();
    let mut hash = Sha256::new();
    let mut size = 0usize;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(io_error)?;
        size = size
            .checked_add(chunk.len())
            .ok_or_else(|| EditorError::new("INVALID_ARGUMENT", "Upload exceeds size limit"))?;
        if size > 1_073_741_824 {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Upload exceeds 1 GiB limit",
            ));
        }
        hash.update(&chunk);
        (file, ()) = file.run(move |file| file.append(&chunk)).await?;
    }
    if size == 0 {
        return Err(EditorError::new("INVALID_ARGUMENT", "Empty upload"));
    }
    let hash = format!("{:x}", hash.finalize());
    let root = host.media_root.clone();
    let database = host.database.clone();
    let staging = staging.to_path_buf();
    let (_file, result) = file
        .run(move |file| {
            file.sync()?;
            let metadata = FfmpegBackend::discover()
                .and_then(|backend| {
                    backend.probe(&ProbeRequest {
                        input: staging.clone(),
                    })
                })
                .map_err(|e| EditorError::new("INVALID_ARGUMENT", e.to_string()))?;
            let canonical = std::fs::canonicalize(root.as_ref())
                .map_err(io_error)?
                .join(&hash);
            let asset = opencut_automation::media::describe(
                &metadata,
                &canonical,
                &query.name,
                &hash,
                size as u64,
            )?;
            let mut app = opencut_sqlite_store::EditorApplication::open(database.as_ref())?;
            app.set_execution_context(context);
            opencut_automation::media::import_observed(&root, &staging, &hash, &asset, &app)?;
            Ok(json!({"asset":asset,"media_url":format!("/v1/media/{hash}")}))
        })
        .await?;
    Ok(result)
}

pub(super) async fn read(
    AuthorizedHost(host): AuthorizedHost,
    Path(hash): Path<String>,
) -> Response {
    if !host.identity.caller.is_owner() {
        return error_response(
            "NOT_AUTHORIZED",
            "Global media reads require owner authority",
        );
    }
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return error_response("NOT_FOUND", "Unknown media ID");
    }
    let metadata = match tokio::fs::read(host.media_root.join(format!("{hash}.json"))).await {
        Ok(bytes) => serde_json::from_slice::<Asset>(&bytes).ok(),
        Err(_) => None,
    };
    let Some(asset) = metadata else {
        return error_response("NOT_FOUND", "Media is unavailable");
    };
    let file = match tokio::fs::File::open(host.media_root.join(&hash)).await {
        Ok(file) => file,
        Err(_) => return error_response("NOT_FOUND", "Media file is missing"),
    };
    let size = file.metadata().await.map(|m| m.len()).unwrap_or(0);
    let mut response = (StatusCode::OK, Body::from_stream(ReaderStream::new(file))).into_response();
    if let Some(mime) = asset
        .metadata
        .get("mime_type")
        .and_then(|v| v.as_str())
        .and_then(|v| HeaderValue::from_str(v).ok())
    {
        response.headers_mut().insert("content-type", mime);
    }
    if let Ok(size) = HeaderValue::from_str(&size.to_string()) {
        response.headers_mut().insert("content-length", size);
    }
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn io_error(error: impl std::fmt::Display) -> EditorError {
    EditorError::new("STORAGE_ERROR", error.to_string())
}

/// Only the committed successful attempt can publish an artifact. The caller
/// supplies a job identity, never a filesystem path.
pub(super) async fn artifact(
    AuthorizedHost(host): AuthorizedHost,
    Path(job): Path<String>,
) -> Response {
    serve_artifact(host, job, None).await
}
pub(super) async fn artifact_reference(
    AuthorizedHost(host): AuthorizedHost,
    Path(id): Path<String>,
) -> Response {
    match opencut_editor_application::job_views::parse_artifact_id(&id) {
        Ok((job, hash)) => serve_artifact(host, job.into(), Some(hash.into())).await,
        Err(error) => error_response(&error.code, &error.message),
    }
}
async fn serve_artifact(host: Host, job: String, requested_hash: Option<String>) -> Response {
    let database = host.database.clone();
    let caller = host.identity.caller.clone();
    let data = match tokio::task::spawn_blocking(move || {
        let mut app = opencut_sqlite_store::EditorApplication::open(database.as_ref())?;
        app.set_caller(caller);
        app.artifact_job(&job).map(|record| json!(record))
    })
    .await
    {
        Ok(Ok(data)) => data,
        Ok(Err(error)) => return into_http(opencut_automation::response(Err(error))),
        Err(_) => return error_response("INTERNAL_ERROR", "Cannot read task state"),
    };
    if data["state"] != "succeeded" {
        return error_response(
            "CONSTRAINT_VIOLATION",
            "Artifact is only available after the task succeeds",
        );
    }
    if requested_hash.as_deref().is_some_and(|hash| {
        data.pointer("/result/artifact/content_hash")
            .and_then(serde_json::Value::as_str)
            != Some(hash)
    }) {
        return error_response(
            "NOT_FOUND",
            "Artifact identity does not match the published task result",
        );
    }
    let Some(path) = data
        .pointer("/result/artifact/path")
        .and_then(|p| p.as_str())
    else {
        return error_response("NOT_FOUND", "Task has no downloadable artifact");
    };
    let (Ok(path), Ok(root)) = (
        tokio::fs::canonicalize(path).await,
        tokio::fs::canonicalize(host.artifact_root.as_ref()).await,
    ) else {
        return error_response("NOT_FOUND", "Artifact is missing");
    };
    if !path.starts_with(root) {
        return error_response("NOT_AUTHORIZED", "Artifact is outside the workspace");
    }
    let Some(expected_size) = data
        .pointer("/result/artifact/size")
        .and_then(|v| v.as_u64())
    else {
        return error_response("ARTIFACT_VERIFICATION_FAILED", "Artifact size is missing");
    };
    let Some(expected_hash) = data
        .pointer("/result/artifact/content_hash")
        .and_then(|v| v.as_str())
    else {
        return error_response("ARTIFACT_VERIFICATION_FAILED", "Artifact hash is missing");
    };
    let expected_hash = expected_hash.to_owned();
    let verified_path = host
        .artifact_root
        .join(format!(".download-{}", uuid::Uuid::new_v4().simple()));
    let database = host.database.clone();
    let cancelled = Cancellation::default();
    let signal = cancelled.0.clone();
    let verified = match BlockingOwned::create(move || {
        let mut source = std::fs::File::open(&path).map_err(io_error)?;
        let metadata = source.metadata().map_err(io_error)?;
        if !metadata.is_file() || metadata.len() != expected_size {
            return Err(EditorError::new(
                "ARTIFACT_VERIFICATION_FAILED",
                "Artifact size does not match the published result",
            ));
        }
        let mut verified = AdmittedFile::create(
            &database,
            &verified_path,
            "artifact_download",
            expected_size,
            true,
        )?;
        let mut digest = Sha256::new();
        let mut copied = 0_u64;
        let mut buffer = [0_u8; 65536];
        loop {
            if signal.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(EditorError::new(
                    "CANCELLED",
                    "Download request was cancelled",
                ));
            }
            let count = source.read(&mut buffer).map_err(io_error)?;
            if count == 0 {
                break;
            }
            copied = copied
                .checked_add(count as u64)
                .ok_or_else(|| io_error("File size overflow"))?;
            if copied > expected_size {
                return Err(EditorError::new(
                    "ARTIFACT_VERIFICATION_FAILED",
                    "Artifact changed during verification",
                ));
            }
            digest.update(&buffer[..count]);
            verified.append(&buffer[..count])?;
        }
        if copied != expected_size || format!("{:x}", digest.finalize()) != expected_hash {
            return Err(EditorError::new(
                "ARTIFACT_VERIFICATION_FAILED",
                "Artifact hash does not match the published result",
            ));
        }
        verified.rewind()?;
        Ok(verified)
    })
    .await
    {
        Ok(file) => file,
        Err(error) => return into_http(opencut_automation::response(Err(error))),
    };
    // The stream owns the anonymous file and its reservation until EOF or drop.
    // Moving ownership into every blocking read also fences cancelled reads.
    let stream = futures_util::stream::try_unfold(verified, |file| async move {
        let (file, bytes) = file
            .run(|file| file.read_chunk())
            .await
            .map_err(|error| std::io::Error::other(error.message))?;
        Ok::<_, std::io::Error>(if bytes.is_empty() {
            None
        } else {
            Some((bytes, file))
        })
    });
    let mut response = (StatusCode::OK, Body::from_stream(stream)).into_response();
    // Artifact authorization must not depend on its declared MIME type; JSON
    // analysis artifacts are still byte streams requiring ongoing authorization.
    response
        .extensions_mut()
        .insert(crate::http_identity::RevalidateStream);
    let mime = data
        .pointer("/result/artifact/mime_type")
        .and_then(|p| p.as_str())
        .unwrap_or("application/octet-stream");
    if let Ok(value) = HeaderValue::from_str(mime) {
        response.headers_mut().insert("content-type", value);
    }
    if let Ok(value) = HeaderValue::from_str(&expected_size.to_string()) {
        response.headers_mut().insert("content-length", value);
    }
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
}

#[derive(Default)]
struct Cancellation(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl Drop for Cancellation {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
