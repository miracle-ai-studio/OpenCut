//! Authenticated Streamable HTTP framing; editing remains in EditorMcp/Application.
use crate::mcp_replay::{ReplayError, ReplayStore};
use axum::{
    Json, Router,
    body::Body,
    extract::{Request, State},
    http::{Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::SessionManager,
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct HttpMcp {
    pub cancellation: CancellationToken,
    manager: Arc<crate::mcp_sessions::Sessions>,
    replay: Arc<ReplayStore>,
    initialization: Arc<Semaphore>,
    tool_surface: crate::mcp::ToolSurface,
}
fn error(status: StatusCode, code: &str, message: &str) -> Response {
    (status,Json(json!({"jsonrpc":"2.0","id":null,"error":{"code":-32000,"message":message,"data":{"code":code}}}))).into_response()
}
fn accepts(request: &Request, mime: &str) -> bool {
    request
        .headers()
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(',').any(|part| {
                let mut fields = part.split(';');
                fields
                    .next()
                    .is_some_and(|v| v.trim().eq_ignore_ascii_case(mime))
                    && fields.all(|p| {
                        !p.trim().starts_with("q=")
                            || p.trim()[2..]
                                .parse::<f32>()
                                .is_ok_and(|q| q > 0.0 && q <= 1.0)
                    })
            })
        })
}
impl HttpMcp {
    pub fn new(timeout: u64, tool_surface: crate::mcp::ToolSurface) -> Self {
        let replay = Arc::new(ReplayStore::default());
        let manager = crate::mcp_sessions::Sessions::new(replay.clone(), timeout);
        Self {
            cancellation: CancellationToken::new(),
            manager: Arc::new(manager),
            replay,
            initialization: Arc::new(Semaphore::new(1)),
            tool_surface,
        }
    }
    pub fn router(&self, database: Arc<PathBuf>, origins: Vec<String>) -> Router {
        let watcher = self.clone();
        let path = database.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! { _=watcher.cancellation.cancelled()=>break, _=tokio::time::sleep(Duration::from_millis(250))=>{} }
                let bindings = watcher.manager.scoped_bindings().await;
                if bindings.is_empty() {
                    continue;
                }
                let path = path.clone();
                let invalid = tokio::task::spawn_blocking(move || {
                    let mut app = opencut_sqlite_store::EditorApplication::open(path.as_ref());
                    bindings
                        .into_iter()
                        .filter_map(|(id, identity)| {
                            let valid = app.as_mut().is_ok_and(|app| {
                                let checked = identity.validate(app);
                                use opencut_automation::denials::*;
                                if checked.as_ref().err().is_some_and(is_denial) {
                                    let request_id = uuid::Uuid::new_v4().to_string();
                                    if app
                                        .record_denial(&Denial {
                                            request_id: &request_id,
                                            source: opencut_automation::CallSource::McpHttp,
                                            caller: Some(&identity.caller),
                                            operation: DeniedOperation::McpRequest,
                                            reason: DenialReason::PolicyChanged,
                                        })
                                        .is_err()
                                    {
                                        eprintln!("{FAILURE_WARNING}: {request_id}");
                                    }
                                }
                                checked.is_ok()
                            });
                            if valid { None } else { Some(id) }
                        })
                        .collect::<Vec<_>>()
                })
                .await;
                if let Ok(ids) = invalid {
                    for id in ids {
                        let _ = watcher.manager.close_session(&id).await;
                    }
                }
            }
        });

        let mut config = StreamableHttpServerConfig::default()
            .with_allowed_origins(origins)
            .enforce_origin_validation()
            .with_max_request_body_bytes(2_097_152)
            .with_cancellation_token(self.cancellation.clone());
        // The outer gate validates normalized loopback IPs/localhost on every
        // path, including replay. Avoid the SDK's narrower string allowlist.
        config = config.disable_allowed_hosts();
        config.sse_keep_alive = Some(Duration::from_secs(5));
        // Request-wise priming is stored by SessionConfig. Avoid the SDK's
        // synthetic initialize/common ID "0", which is not a stored anchor.
        config.sse_retry = None;
        let tool_surface = self.tool_surface;
        let service = StreamableHttpService::new(
            move || {
                Ok(crate::mcp::EditorMcp::new(
                    database.clone(),
                    opencut_automation::CallSource::McpHttp,
                    tool_surface,
                ))
            },
            self.manager.clone(),
            config,
        );
        Router::new()
            .route_service("/mcp", service)
            .layer(middleware::from_fn_with_state(self.clone(), gate))
    }
    pub async fn shutdown(&self) {
        self.cancellation.cancel();
        let ids = self
            .manager
            .sessions
            .read()
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let closing =
            futures_util::future::join_all(ids.iter().map(|id| self.manager.close_session(id)));
        let _ = tokio::time::timeout(Duration::from_secs(3), closing).await;
        self.replay.clear();
    }
}
async fn gate(State(host): State<HttpMcp>, request: Request, next: Next) -> Response {
    let mut response = dispatch(host, request, next).await;
    response.headers_mut().insert(
        "cache-control",
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}
async fn dispatch(host: HttpMcp, mut request: Request, next: Next) -> Response {
    let Some(identity) = crate::http_identity::request_identity(&request) else {
        return error(
            StatusCode::FORBIDDEN,
            "NOT_AUTHORIZED",
            "Authenticated request identity is missing",
        );
    };

    if host.cancellation.is_cancelled() {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "HOST_STOPPING",
            "Host is stopping",
        );
    }
    for name in [
        "host",
        "mcp-session-id",
        "mcp-protocol-version",
        "last-event-id",
        "accept",
        "content-type",
    ] {
        if request.headers().get_all(name).iter().count() > 1 {
            return error(
                StatusCode::BAD_REQUEST,
                "INVALID_ARGUMENT",
                "Duplicate MCP transport header",
            );
        }
    }
    let valid_host = request
        .headers()
        .get("host")
        .and_then(|v| v.to_str().ok())
        .filter(|value| !value.contains('@'))
        .and_then(|value| value.parse::<axum::http::uri::Authority>().ok())
        .is_some_and(|authority| {
            let host = authority.host().trim_matches(['[', ']']);
            (authority.port().is_none() || authority.port_u16().is_some())
                && (host.eq_ignore_ascii_case("localhost")
                    || host
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback()))
        });
    if !valid_host {
        return error(
            StatusCode::FORBIDDEN,
            "NOT_AUTHORIZED",
            "Host is not permitted",
        );
    }
    let version = request
        .headers()
        .get("mcp-protocol-version")
        .map(|v| v.to_str().unwrap_or("").to_owned());
    if version.as_deref().is_some_and(|v| v != "2025-11-25") {
        return error(
            StatusCode::BAD_REQUEST,
            "UNSUPPORTED_PROTOCOL_VERSION",
            "Only MCP 2025-11-25 is supported",
        );
    }
    let session = match request.headers().get("mcp-session-id") {
        Some(value) => match value.to_str().ok().filter(|v| {
            !v.is_empty() && v.len() <= 128 && v.bytes().all(|b| (0x21..=0x7e).contains(&b))
        }) {
            Some(value) => Some(value.to_owned()),
            None => {
                return error(
                    StatusCode::BAD_REQUEST,
                    "INVALID_ARGUMENT",
                    "Invalid MCP-Session-Id header",
                );
            }
        },
        None => None,
    };
    if session.is_some() && version.is_none() {
        return error(
            StatusCode::BAD_REQUEST,
            "PROTOCOL_VERSION_REQUIRED",
            "Send the negotiated MCP-Protocol-Version header",
        );
    }
    if let Some(session) = &session
        && !host
            .manager
            .has_session(&session.clone().into())
            .await
            .unwrap_or(false)
    {
        return error(
            StatusCode::NOT_FOUND,
            "MCP_SESSION_NOT_FOUND",
            "Initialize a new MCP session",
        );
    }
    if let Some(id) = &session {
        let bound = host.manager.identity(&id.clone().into()).await;
        if bound.as_ref() != Some(&identity) {
            // A different credential cannot close or hijack the original session.
            if bound
                .as_ref()
                .is_some_and(|bound| bound.caller == identity.caller)
            {
                let _ = host.manager.close_session(&id.clone().into()).await;
            }
            return error(
                StatusCode::FORBIDDEN,
                "NOT_AUTHORIZED",
                "MCP session credential or authorization version does not match; initialize a new session",
            );
        }
    }
    if request.method() == Method::GET {
        if !accepts(&request, "text/event-stream") {
            return error(
                StatusCode::NOT_ACCEPTABLE,
                "INVALID_ARGUMENT",
                "Accept text/event-stream",
            );
        }
        if let Some(last) = request.headers().get("last-event-id") {
            let Some(session) = &session else {
                return error(
                    StatusCode::BAD_REQUEST,
                    "MCP_SESSION_REQUIRED",
                    "An MCP session is required",
                );
            };
            let Ok(id) = last.to_str() else {
                return error(
                    StatusCode::BAD_REQUEST,
                    "INVALID_CURSOR",
                    "Invalid MCP event cursor",
                );
            };
            if id.len() > 128 {
                return error(
                    StatusCode::BAD_REQUEST,
                    "INVALID_CURSOR",
                    "MCP event cursor is too long",
                );
            }
            let events = match host.replay.replay(session, id) {
                Ok(events) => events,
                Err(ReplayError::WrongSession) => {
                    return error(
                        StatusCode::BAD_REQUEST,
                        "INVALID_CURSOR",
                        "Cursor does not belong to this MCP session",
                    );
                }
                Err(ReplayError::Expired) => {
                    return error(
                        StatusCode::GONE,
                        "CURSOR_EXPIRED",
                        "Transport cursor expired; read authoritative state or resume durable application events",
                    );
                }
            };
            let checkpoint = events
                .last()
                .and_then(|event| event.event_id.as_deref())
                .unwrap_or(id)
                .to_owned();
            let mut records = events
                .into_iter()
                .map(|event| {
                    let mut record = String::new();
                    if let Some(id) = event.event_id {
                        record.push_str(&format!("id: {id}\n"));
                    }
                    if let Some(retry) = event.retry {
                        record.push_str(&format!("retry: {}\n", retry.as_millis()));
                    }
                    record.push_str("data: ");
                    if let Some(message) = event.message {
                        // JSON serialization escapes CR/LF in all user strings.
                        record.push_str(
                            &serde_json::to_string(message.as_ref()).expect("serializable RPC"),
                        );
                    }
                    record.push_str("\n\n");
                    Ok::<_, std::convert::Infallible>(record)
                })
                .collect::<Vec<_>>();
            // Repeat the last cursor in an empty checkpoint. Some clients start
            // each SSE parser without its preceding Last-Event-ID; without this,
            // an empty poll loses the request stream before a slow reply arrives.
            // Encode the explicit empty data field: Axum Event::data("") omits
            // it and data("\n") emits two data lines, giving clients invalid JSON.
            // All IDs here are server-generated, retained cache anchors.
            records.push(Ok(format!("id: {checkpoint}\nretry: 1000\ndata:\n\n")));
            return (
                [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                Body::from_stream(futures_util::stream::iter(records)),
            )
                .into_response();
        }
    }
    let _admission = if request.method() == Method::POST && session.is_none() {
        if !accepts(&request, "application/json") || !accepts(&request, "text/event-stream") {
            return error(
                StatusCode::NOT_ACCEPTABLE,
                "INVALID_ARGUMENT",
                "Accept application/json and text/event-stream",
            );
        }
        if request
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            != Some("application/json")
        {
            return error(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "INVALID_ARGUMENT",
                "Content-Type must be application/json",
            );
        }
        let (parts, body) = request.into_parts();
        let bytes = match axum::body::to_bytes(body, 2_097_152).await {
            Ok(bytes) => bytes,
            Err(_) => {
                return error(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "INPUT_TOO_LARGE",
                    "MCP request exceeds 2 MiB",
                );
            }
        };
        let mut message: Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => {
                return error(
                    StatusCode::BAD_REQUEST,
                    "INVALID_ARGUMENT",
                    "Invalid JSON-RPC input",
                );
            }
        };
        if message.get("method").and_then(Value::as_str) != Some("initialize") {
            return error(
                StatusCode::BAD_REQUEST,
                "MCP_SESSION_REQUIRED",
                "Initialize an MCP session first",
            );
        }
        // rmcp 3.4 chooses its stateless transport from the client's requested
        // version before negotiation. Bind our selected legacy transport first,
        // while retaining the original version in HTTP request extensions.
        let requested = message
            .pointer("/params/protocolVersion")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Some(requested) = requested {
            if version.as_deref().is_some_and(|v| v != requested) {
                return error(
                    StatusCode::BAD_REQUEST,
                    "PROTOCOL_VERSION_MISMATCH",
                    "Initialization header and body versions differ",
                );
            }
            message["params"]["protocolVersion"] = json!("2025-11-25");
            request = Request::from_parts(parts, Body::from(message.to_string()));
            request
                .extensions_mut()
                .insert(RequestedProtocol(requested));
        } else {
            request = Request::from_parts(parts, Body::from(bytes));
        }
        // Content-Length may have changed during protocol negotiation.
        request.headers_mut().remove("content-length");
        let permit = host
            .initialization
            .acquire()
            .await
            .expect("admission semaphore stays open");
        if host.manager.sessions.read().await.len()
            >= opencut_automation::capabilities::MCP_SESSION_LIMIT
        {
            return error(
                StatusCode::TOO_MANY_REQUESTS,
                "MCP_SESSION_LIMIT",
                "At most 32 active MCP sessions",
            );
        }
        Some(permit)
    } else {
        None
    };
    next.run(request).await
}
#[derive(Clone, Debug)]
struct RequestedProtocol(#[allow(dead_code)] String);
