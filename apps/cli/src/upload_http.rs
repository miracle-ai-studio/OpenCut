//! Bounded byte transport; upload state and mutation authority live in Application.
use crate::{
    http::{Host, error_response, into_http},
    http_identity::AuthorizedHost,
};
use axum::{
    body::Body,
    extract::{Path, Query, rejection::QueryRejection},
    http::HeaderMap,
    response::Response,
};
use futures_util::StreamExt;
use opencut_automation::{
    Automation, CallSource, ExecutionContext,
    uploads::{self, ChunkDescriptor, UploadStore},
};
use opencut_contracts::EditorError;
use opencut_sqlite_store::EditorApplication;
use serde::Deserialize;
use std::time::Duration;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChunkQuery {
    size_bytes: String,
    content_hash: String,
}
fn internal() -> EditorError {
    EditorError::new(
        "UPLOAD_STORAGE_ERROR",
        "Upload service could not complete this operation",
    )
}
async fn authorize(host: &Host, input: &ChunkDescriptor) -> Result<(), EditorError> {
    let host = host.clone();
    let input = input.clone();
    tokio::task::spawn_blocking(move || {
        let mut app = EditorApplication::open(host.database.as_ref())?;
        host.identity.validate(&mut app)?;
        app.set_caller(host.identity.caller.clone());
        app.authorize_chunk(&input).map(|_| ())
    })
    .await
    .map_err(|_| internal())?
}
pub(super) async fn put_chunk(
    AuthorizedHost(host): AuthorizedHost,
    Path((upload_id, offset)): Path<(String, String)>,
    query: Result<Query<ChunkQuery>, QueryRejection>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let Query(query) = match query {
        Ok(v) => v,
        Err(_) => {
            return error_response(
                "INVALID_ARGUMENT",
                "Expected size_bytes and content_hash query fields",
            );
        }
    };
    if headers
        .get("content-encoding")
        .is_some_and(|v| v != "identity")
    {
        return error_response(
            "INVALID_ARGUMENT",
            "Upload chunks must contain uncompressed raw bytes",
        );
    }
    let input = ChunkDescriptor {
        upload_id,
        offset,
        size_bytes: query.size_bytes,
        content_hash: query.content_hash,
    };
    let context = ExecutionContext::for_caller(CallSource::Http, host.identity.caller.clone());
    let failure = |error| {
        into_http(opencut_automation::Response::from_result_with_id(
            context.request_id().to_owned(),
            Err(uploads::public_error(error)),
        ))
    };
    if let Err(e) = uploads::validate_chunk(&input) {
        return failure(e);
    }
    if let Err(e) = authorize(&host, &input).await {
        return failure(e);
    }
    let permit = match host.upload_slots.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            return failure(EditorError::new(
                "UPLOAD_BUSY",
                "Concurrent chunk limit reached",
            ));
        }
    };
    let expected = uploads::decimal(&input.size_bytes).expect("validated size") as usize;
    let receive = async {
        let mut bytes = Vec::with_capacity(expected);
        let mut stream = body.into_data_stream();
        let mut last_check = tokio::time::Instant::now();
        loop {
            let chunk = tokio::select! {
                value=stream.next()=>Some(value),
                _=tokio::time::sleep(Duration::from_millis(250))=>None,
            };
            if last_check.elapsed() >= Duration::from_millis(250) || matches!(chunk, Some(None)) {
                authorize(&host, &input).await?;
                last_check = tokio::time::Instant::now();
            }
            match chunk {
                None => continue,
                Some(None) => break,
                Some(Some(Err(_))) => {
                    return Err(EditorError::new(
                        "UPLOAD_CONTENT_MISMATCH",
                        "Chunk stream ended before completion",
                    ));
                }
                Some(Some(Ok(part))) => {
                    if part.len() > expected - bytes.len() {
                        return Err(EditorError::new(
                            "UPLOAD_CONTENT_MISMATCH",
                            "Chunk exceeds declared length",
                        ));
                    }
                    bytes.extend_from_slice(&part);
                }
            }
        }
        if bytes.len() != expected {
            return Err(EditorError::new(
                "UPLOAD_CONTENT_MISMATCH",
                "Chunk length differs from declaration",
            ));
        }
        Ok(bytes)
    };
    let bytes = match tokio::time::timeout(
        Duration::from_secs(opencut_automation::capabilities::UPLOAD_CHUNK_TIMEOUT_SECONDS),
        receive,
    )
    .await
    {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(e)) => return failure(e),
        Err(_) => {
            return failure(EditorError::new(
                "UPLOAD_TIMEOUT",
                "Chunk deadline exceeded",
            ));
        }
    };
    let task_host = host.clone();
    let result = tokio::task::spawn_blocking(move || {
        // A disconnected HTTP caller cannot release capacity while its durable write still runs.
        let _permit = permit;
        let mut app = EditorApplication::open(task_host.database.as_ref())?;
        task_host.identity.validate(&mut app)?;
        Ok::<_, EditorError>(
            Automation::open(task_host.database.as_ref())?
                .with_source(CallSource::Http)
                .with_caller(task_host.identity.caller)
                .put_upload_chunk(&input, &bytes),
        )
    })
    .await;
    match result {
        Ok(Ok(response)) => into_http(response),
        Ok(Err(e)) => failure(e),
        Err(_) => failure(internal()),
    }
}
