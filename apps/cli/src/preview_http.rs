//! Bounded binary preview transport. Control remains in the shared automation service.
use crate::http_identity::AuthorizedHost;
use axum::{
    Json,
    body::{Body, Bytes},
    http::header,
    response::{IntoResponse, Response},
};
use opencut_automation::CallSource;
use opencut_automation::preview::StreamInput;
use opencut_host_runtime::Automation;
use opencut_host_runtime::transport_io::StreamRead;
use std::time::{Duration, Instant};

pub(super) async fn stream(
    AuthorizedHost(host): AuthorizedHost,
    input: Result<Json<StreamInput>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let input = match input {
        Ok(Json(input)) => input,
        Err(_) => {
            return crate::http::error_response(
                "INVALID_ARGUMENT",
                "Invalid preview stream request",
            );
        }
    };
    let session = input.session_id.clone();
    let database = host.database.clone();
    let caller = host.identity.caller.clone();
    let opened = tokio::task::spawn_blocking(move || {
        let mut app = Automation::open(database.as_ref())?
            .with_source(CallSource::Http)
            .with_caller(caller);
        app.preview_stream(input)
    })
    .await;
    let stream = match opened {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => return crate::http::error_response(&error.code, &error.message),
        Err(_) => {
            return crate::http::error_response("INTERNAL_ERROR", "Preview stream could not start");
        }
    };
    let (sender, receiver) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(2);
    tokio::task::spawn_blocking(move || {
        let mut checked = Instant::now();
        loop {
            if sender.is_closed() {
                break;
            }
            if checked.elapsed() >= Duration::from_secs(1) {
                let allowed = Automation::open(host.database.as_ref()).and_then(|a| {
                    a.with_source(CallSource::Http)
                        .with_caller(host.identity.caller.clone())
                        .authorize_preview(&session, true)
                });
                if allowed.is_err() {
                    break;
                }
                checked = Instant::now();
            }
            let bytes = match stream.read(Duration::from_millis(200)) {
                Ok(StreamRead::Chunk(bytes)) => bytes,
                Ok(StreamRead::Timeout) => continue,
                Ok(StreamRead::End) => break,
                Err(_) => {
                    let _ = sender.try_send(Err(std::io::Error::other(
                        "Preview generation ended before transport completion",
                    )));
                    break;
                }
            };
            let mut pending = Ok(Bytes::from(bytes));
            let wait = Instant::now();
            loop {
                match sender.try_send(pending) {
                    Ok(()) => break,
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => return,
                    Err(tokio::sync::mpsc::error::TrySendError::Full(value)) => {
                        if wait.elapsed() > Duration::from_secs(10) {
                            return;
                        }
                        pending = value;
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            }
        }
    });
    let body = Body::from_stream(futures_util::stream::unfold(
        receiver,
        |mut receiver| async move { receiver.recv().await.map(|chunk| (chunk, receiver)) },
    ));
    (
        [
            (header::CONTENT_TYPE, "video/mp4"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response()
}
