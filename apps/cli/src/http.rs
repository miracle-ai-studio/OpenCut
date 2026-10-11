use std::{
    fs,
    net::SocketAddr,
    path::Path as FsPath,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::http_identity::{AuthorizedHost, Identity};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{
        IntoResponse, Response as HttpResponse, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post, put},
};
use opencut_automation::{Response, capabilities, domains, tool_specs};
use opencut_contracts::EditorError;
use opencut_host_runtime::Automation;
use serde_json::{Value, json};

#[derive(Clone)]
pub(super) struct Host {
    pub database: Arc<std::path::PathBuf>,
    token: Arc<String>,
    browser_session: Option<crate::web_host::BrowserSession>,
    origins: Arc<Vec<String>>,
    pub media_root: Arc<std::path::PathBuf>,
    pub artifact_root: Arc<std::path::PathBuf>,
    shutdown: Arc<AtomicBool>,
    pub identity: Identity,
    pub upload_slots: Arc<tokio::sync::Semaphore>,
}

pub struct Options {
    pub origins: Vec<String>,
    pub mcp_session_timeout_seconds: u64,
    pub tool_surface: crate::mcp::ToolSurface,
    pub web_root: Option<std::path::PathBuf>,
}

pub async fn serve(
    database: &FsPath,
    listen: SocketAddr,
    token_path: &FsPath,
    options: Options,
    service: Option<&crate::server::Runtime>,
) -> Result<(), Box<dyn std::error::Error>> {
    let Options {
        mut origins,
        mcp_session_timeout_seconds,
        tool_surface,
        web_root,
    } = options;
    let web_root = web_root.map(crate::web_host::validate_root).transpose()?;
    struct PreviewShutdown(std::path::PathBuf);
    impl Drop for PreviewShutdown {
        fn drop(&mut self) {
            let _ = opencut_host_runtime::transport_io::shutdown_previews(&self.0);
        }
    }
    if !listen.ip().is_loopback() {
        return Err("This local host only accepts loopback addresses; remote authorization is not implemented".into());
    }
    // Bind before workspace initialization so a port collision has no workspace side effects.
    let listener = tokio::net::TcpListener::bind(listen).await?;
    if web_root.is_some() {
        origins.push(format!("http://{}", listener.local_addr()?));
        origins.push(format!(
            "http://localhost:{}",
            listener.local_addr()?.port()
        ));
    }
    let token = read_or_create_token(token_path)?;
    let media_root = database
        .parent()
        .unwrap_or_else(|| FsPath::new("."))
        .join("media");
    fs::create_dir_all(&media_root)?;
    let artifact_root = database
        .parent()
        .unwrap_or_else(|| FsPath::new("."))
        .join("artifacts");
    fs::create_dir_all(&artifact_root)?;
    drop(Automation::open(database)?);
    opencut_host_runtime::transport_io::recover_previews(database)?;
    let _preview_shutdown_guard = PreviewShutdown(database.to_path_buf());
    let stop = Arc::new(AtomicBool::new(false));
    let mcp = crate::mcp_http::HttpMcp::new(mcp_session_timeout_seconds, tool_surface);
    let mcp_router = mcp.router(Arc::new(database.to_path_buf()), origins.clone());
    let _mcp_shutdown_guard = mcp.cancellation.clone().drop_guard();
    let browser_session = web_root
        .as_ref()
        .map(|_| crate::web_host::BrowserSession::new());
    let host = Host {
        database: Arc::new(database.to_path_buf()),
        token: Arc::new(token),
        browser_session: browser_session.clone(),
        origins: Arc::new(origins),
        media_root: Arc::new(media_root),
        artifact_root: Arc::new(artifact_root),
        shutdown: stop.clone(),
        identity: Identity::owner(),
        upload_slots: Arc::new(tokio::sync::Semaphore::new(
            opencut_automation::capabilities::UPLOAD_INFLIGHT_LIMIT,
        )),
    };
    let router = Router::new()
        .route(
            "/v1/capabilities",
            get(|| async { Json(opencut_automation::response(Ok(capabilities()))) }),
        )
        .route(
            "/v1/operations",
            get(|| async { Json(opencut_automation::response(Ok(json!(tool_specs())))) }),
        )
        .route("/v1/domains", get(domain_catalog))
        .route("/v1/domains/{domain}", get(domain_contract))
        .route("/v1/projects", get(projects).post(create_project))
        .route("/v1/projects/{project}/snapshot", get(snapshot))
        .route("/v1/projects/{project}/revisions", get(history))
        .route("/v1/projects/{project}/revisions/{revision}", get(revision))
        .route(
            "/v1/projects/{project}/branches/main/restore",
            post(restore),
        )
        .route(
            "/v1/projects/{project}/branches/main/snapshot",
            get(snapshot),
        )
        .route(
            "/v1/projects/{project}/branches/main/edit-plans",
            post(plan),
        )
        .route(
            "/v1/projects/{project}/branches/main/transactions",
            post(apply),
        )
        .route("/v1/tools/{name}", post(tool))
        .route("/v1/preview/stream", post(crate::preview_http::stream))
        .route("/v1/media/upload", post(crate::media_http::upload))
        .route(
            "/v1/stream-uploads/{upload_id}/chunks/{offset}",
            put(crate::upload_http::put_stream_chunk),
        )
        .route(
            "/v1/uploads/{upload_id}/chunks/{offset}",
            put(crate::upload_http::put_chunk),
        )
        .route("/v1/media/{hash}", get(crate::media_http::read))
        .route(
            "/v1/projects/{project}/media/{media_id}/content",
            get(crate::media_http::read_project),
        )
        .route("/v1/jobs", get(jobs).post(start_job))
        .route("/v1/jobs/{job}", get(job))
        .route("/v1/jobs/{job}/cancel", post(cancel_job))
        .route("/v1/jobs/{job}/artifact", get(crate::media_http::artifact))
        .route(
            "/v1/artifacts/{artifact}",
            get(crate::media_http::artifact_reference),
        )
        .route("/v1/render/preflight", post(render_preflight))
        .route("/v1/events", get(events))
        .route("/v1/job-events", get(job_events))
        .route("/v1/job-events/stream", get(job_event_stream))
        .route("/v1/changes/stream", post(changes_stream))
        .merge(mcp_router.with_state(()))
        .method_not_allowed_fallback(|| async {
            (
                StatusCode::METHOD_NOT_ALLOWED,
                Json(opencut_automation::response(Err(EditorError::new(
                    "INVALID_ARGUMENT",
                    "HTTP method is not allowed",
                )))),
            )
        })
        .layer(DefaultBodyLimit::max(2_097_152))
        .layer(middleware::from_fn_with_state(host.clone(), authorize))
        .with_state(host);
    let router = crate::web_host::attach(router, web_root, listener.local_addr()?, browser_session);
    if let Some(service) = service {
        service.publish(listener.local_addr()?)?;
    }
    eprintln!(
        "OpenCut API listening on http://{}; bearer token file: {}",
        listener.local_addr()?,
        token_path.display()
    );
    let worker_stop = stop.clone();
    let worker_database = database.to_path_buf();
    let mut worker = tokio::task::spawn_blocking(move || {
        opencut_host_runtime::worker::run(&worker_database, false, worker_stop)
    });
    let signal_stop = stop.clone();
    let preview_database = database.to_path_buf();
    let service_shutdown = crate::server::shutdown_requested(service);
    let server = axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = crate::signals::termination() => {},
                _ = service_shutdown => {},
            }
            signal_stop.store(true, Ordering::SeqCst);
            let _ = tokio::task::spawn_blocking(move || {
                opencut_host_runtime::transport_io::shutdown_previews(&preview_database)
            })
            .await;
            mcp.shutdown().await;
        })
        .into_future();
    tokio::pin!(server);
    tokio::select! {
        result=&mut server=>{
            stop.store(true,Ordering::SeqCst);
            worker.await??;
            result?;
        },
        result=&mut worker=>{
            if !stop.load(Ordering::SeqCst) {
                result??;
                return Err("Job worker stopped unexpectedly".into());
            }
            result??;
            server.await?;
        }
    }
    Ok(())
}

fn read_or_create_token(path: &FsPath) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            use std::io::Write;
            let token = format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            );
            file.write_all(token.as_bytes())?;
            file.sync_all()?;
            Ok(token)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let token = fs::read_to_string(path)?.trim().to_string();
            if token.len() < 32 || token.chars().any(char::is_whitespace) {
                return Err("API token must contain at least 32 non-whitespace characters".into());
            }
            Ok(token)
        }
        Err(e) => Err(e.into()),
    }
}

async fn authorize(
    State(host): State<Host>,
    mut request: axum::extract::Request,
    next: Next,
) -> HttpResponse {
    use opencut_automation::{
        CallSource,
        denials::{DenialReason, DeniedOperation},
    };
    let source = if request.uri().path() == "/mcp" {
        CallSource::McpHttp
    } else {
        CallSource::Http
    };
    let operation = if source == CallSource::McpHttp {
        DeniedOperation::McpRequest
    } else {
        DeniedOperation::HttpRequest
    };
    if request.headers().get_all("origin").iter().count() > 1
        || request.headers().get_all("authorization").iter().count() > 1
    {
        return crate::denials::refuse(
            &host,
            source,
            DenialReason::AuthenticationRejected,
            "Duplicate authorization or Origin header",
        )
        .await;
    }
    let origin = request.headers().get("origin").cloned();
    if let Some(origin) = &origin {
        let allowed = origin
            .to_str()
            .ok()
            .is_some_and(|o| host.origins.iter().any(|a| a == o));
        if !allowed {
            return crate::denials::refuse(
                &host,
                source,
                DenialReason::OriginRejected,
                "Origin is not permitted",
            )
            .await;
        }
    }
    if request.method() == Method::OPTIONS && origin.is_some() {
        let mut response = StatusCode::NO_CONTENT.into_response();
        add_cors(response.headers_mut(), origin.as_ref());
        return response;
    }
    let browser_authorized = host
        .browser_session
        .as_ref()
        .is_some_and(|session| session.accepts(&request));
    let Some(secret) = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .or_else(|| browser_authorized.then_some(host.token.as_str()))
    else {
        return crate::denials::refuse(
            &host,
            source,
            DenialReason::AuthenticationRejected,
            "A valid bearer token is required",
        )
        .await;
    };
    let identity = if secret == host.token.as_str() {
        Identity::owner()
    } else {
        let database = host.database.clone();
        let secret = secret.to_owned();
        match tokio::task::spawn_blocking(move || {
            Identity::authenticate(database.as_ref(), &secret)
        })
        .await
        {
            Ok(Ok(identity)) => identity,
            _ => {
                return crate::denials::refuse(
                    &host,
                    source,
                    DenialReason::AuthenticationRejected,
                    "A valid bearer token is required",
                )
                .await;
            }
        }
    };
    request.extensions_mut().insert(identity.clone());
    let mut request_host = host;
    request_host.identity = identity;
    let response = crate::denials::http(
        request_host.database.clone(),
        Some(request_host.identity.caller.clone()),
        source,
        operation,
        DenialReason::AuthorizationDenied,
        next.run(request).await,
    )
    .await;
    let mut response = crate::http_identity::guard_stream(&request_host, response, source);
    add_cors(response.headers_mut(), origin.as_ref());
    response
}

fn add_cors(headers: &mut HeaderMap, origin: Option<&HeaderValue>) {
    if let Some(origin) = origin {
        headers.insert("access-control-allow-origin", origin.clone());
        headers.insert("vary", HeaderValue::from_static("Origin"));
        headers.insert(
            "access-control-expose-headers",
            HeaderValue::from_static("MCP-Session-Id, MCP-Protocol-Version, X-OpenCut-Request-Id, X-OpenCut-Audit-Warning"),
        );
        headers.insert(
            "access-control-allow-methods",
            HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS"),
        );
        headers.insert(
            "access-control-allow-headers",
            HeaderValue::from_static(
                "Authorization, Content-Type, Last-Event-ID, MCP-Session-Id, MCP-Protocol-Version",
            ),
        );
    }
}

async fn invoke(host: Host, name: String, args: Value) -> HttpResponse {
    match tokio::task::spawn_blocking(move || {
        Automation::open(host.database.as_ref()).map(|a| {
            a.with_source(opencut_automation::CallSource::Http)
                .with_caller(host.identity.caller)
                .call(&name, args)
        })
    })
    .await
    {
        Ok(Ok(response)) => into_http(response),
        Ok(Err(error)) => into_http(opencut_automation::response(Err(error))),
        Err(error) => error_response("INTERNAL_ERROR", &error.to_string()),
    }
}
async fn domain_catalog() -> HttpResponse {
    into_http(opencut_automation::response(
        domains::catalog(None, false).map(|v| json!(v)),
    ))
}
async fn domain_contract(Path(domain): Path<String>) -> HttpResponse {
    into_http(opencut_automation::response(
        domains::catalog(Some(&domain), true).map(|v| json!(v)),
    ))
}
fn body(body: Result<Json<Value>, JsonRejection>) -> Result<Value, Box<HttpResponse>> {
    body.map(|Json(value)| value)
        .map_err(|error| Box::new(error_response("INVALID_ARGUMENT", &error.body_text())))
}
async fn projects(AuthorizedHost(host): AuthorizedHost) -> HttpResponse {
    invoke(host, "opencut_project_list".into(), json!({})).await
}
async fn jobs(AuthorizedHost(host): AuthorizedHost) -> HttpResponse {
    invoke(host, "opencut_job_list".into(), json!({})).await
}
async fn job(AuthorizedHost(host): AuthorizedHost, Path(job): Path<String>) -> HttpResponse {
    invoke(host, "opencut_job_get".into(), json!({"job_id":job})).await
}
async fn cancel_job(AuthorizedHost(host): AuthorizedHost, Path(job): Path<String>) -> HttpResponse {
    invoke(host, "opencut_job_cancel".into(), json!({"job_id":job})).await
}
async fn start_job(
    AuthorizedHost(host): AuthorizedHost,
    input: Result<Json<Value>, JsonRejection>,
) -> HttpResponse {
    match body(input) {
        Ok(args) => invoke(host, "opencut_job_start".into(), args).await,
        Err(e) => *e,
    }
}
async fn render_preflight(
    AuthorizedHost(host): AuthorizedHost,
    input: Result<Json<Value>, JsonRejection>,
) -> HttpResponse {
    match body(input) {
        Ok(args) => invoke(host, "opencut_render_preflight".into(), args).await,
        Err(e) => *e,
    }
}
async fn snapshot(
    AuthorizedHost(host): AuthorizedHost,
    Path(project): Path<String>,
) -> HttpResponse {
    invoke(
        host,
        "opencut_project_inspect".into(),
        json!({"project_id":project}),
    )
    .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryQuery {
    from_revision: Option<String>,
    limit: Option<usize>,
}

async fn history(
    AuthorizedHost(host): AuthorizedHost,
    Path(project): Path<String>,
    query: Result<Query<HistoryQuery>, axum::extract::rejection::QueryRejection>,
) -> HttpResponse {
    let query = match query {
        Ok(Query(query)) => query,
        Err(e) => return error_response("INVALID_ARGUMENT", &e.to_string()),
    };
    invoke(host,"opencut_project_history".into(),json!({"project_id":project,"from_revision":query.from_revision,"limit":query.limit.unwrap_or(50)})).await
}
async fn revision(
    AuthorizedHost(host): AuthorizedHost,
    Path((project, revision)): Path<(String, String)>,
) -> HttpResponse {
    invoke(
        host,
        "opencut_revision_inspect".into(),
        json!({"project_id":project,"revision":revision}),
    )
    .await
}
async fn restore(
    AuthorizedHost(host): AuthorizedHost,
    Path(project): Path<String>,
    input: Result<Json<Value>, JsonRejection>,
) -> HttpResponse {
    match body(input) {
        Ok(request) => {
            invoke(
                host,
                "opencut_revision_restore".into(),
                json!({"project_id":project,"request":request}),
            )
            .await
        }
        Err(e) => *e,
    }
}
async fn create_project(
    AuthorizedHost(host): AuthorizedHost,
    input: Result<Json<Value>, JsonRejection>,
) -> HttpResponse {
    match body(input) {
        Ok(args) => invoke(host, "opencut_project_create".into(), args).await,
        Err(e) => *e,
    }
}
async fn plan(
    AuthorizedHost(host): AuthorizedHost,
    Path(project): Path<String>,
    input: Result<Json<Value>, JsonRejection>,
) -> HttpResponse {
    match body(input) {
        Ok(args) => {
            invoke(
                host,
                "opencut_edit_plan".into(),
                json!({"project_id":project,"request":args}),
            )
            .await
        }
        Err(e) => *e,
    }
}
async fn apply(
    AuthorizedHost(host): AuthorizedHost,
    Path(project): Path<String>,
    input: Result<Json<Value>, JsonRejection>,
) -> HttpResponse {
    match body(input) {
        Ok(args) => {
            invoke(
                host,
                "opencut_edit_apply".into(),
                json!({"project_id":project,"request":args}),
            )
            .await
        }
        Err(e) => *e,
    }
}
async fn tool(
    AuthorizedHost(host): AuthorizedHost,
    Path(name): Path<String>,
    input: Result<Json<Value>, JsonRejection>,
) -> HttpResponse {
    match body(input) {
        Ok(args) => invoke(host, name, args).await,
        Err(e) => *e,
    }
}
pub(super) fn into_http(response: Response) -> HttpResponse {
    let status = response.error.as_ref().map_or(StatusCode::OK, |error| {
        StatusCode::from_u16(opencut_automation::error_policy(&error.code).http_status)
            .expect("shared error policy contains valid HTTP statuses")
    });
    let id = HeaderValue::from_str(&response.request_id).ok();
    let warning = response
        .warnings
        .iter()
        .any(|s| s == opencut_automation::denials::FAILURE_WARNING);
    let mut response = (status, Json(response)).into_response();
    if let Some(id) = id {
        response
            .headers_mut()
            .insert(crate::denials::REQUEST_HEADER, id);
    }
    if warning {
        response.headers_mut().insert(
            crate::denials::WARNING_HEADER,
            HeaderValue::from_static(opencut_automation::denials::FAILURE_WARNING),
        );
    }
    response
}
pub(super) fn error_response(code: &str, message: &str) -> HttpResponse {
    into_http(opencut_automation::response(Err(EditorError::new(
        code, message,
    ))))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EventQuery {
    after_sequence: Option<i64>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct JobEventQuery {
    after_sequence: Option<i64>,
    limit: Option<usize>,
    job_id: Option<String>,
}

async fn job_events(
    AuthorizedHost(host): AuthorizedHost,
    query: Result<Query<JobEventQuery>, axum::extract::rejection::QueryRejection>,
) -> HttpResponse {
    let query = match query {
        Ok(Query(query)) => query,
        Err(error) => return error_response("INVALID_ARGUMENT", &error.to_string()),
    };
    invoke(
        host,
        "opencut_job_events".into(),
        json!({
            "after_sequence": query.after_sequence.unwrap_or(0),
            "limit": query.limit.unwrap_or(100),
            "job_id": query.job_id,
        }),
    )
    .await
}

async fn job_event_stream(
    AuthorizedHost(host): AuthorizedHost,
    query: Result<Query<JobEventQuery>, axum::extract::rejection::QueryRejection>,
    headers: HeaderMap,
) -> HttpResponse {
    if !host.identity.caller.is_owner() {
        return error_response(
            "NOT_AUTHORIZED",
            "Use the public changes stream for restricted task observation",
        );
    }
    let query = match query {
        Ok(Query(query)) => query,
        Err(error) => return error_response("INVALID_ARGUMENT", &error.to_string()),
    };
    let header_cursor = match headers.get("last-event-id") {
        Some(value) => match value
            .to_str()
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|cursor| *cursor >= 0)
        {
            Some(cursor) => Some(cursor),
            None => {
                return error_response(
                    "INVALID_ARGUMENT",
                    "Last-Event-ID must be a nonnegative sequence number",
                );
            }
        },
        None => None,
    };
    let cursor = header_cursor.or(query.after_sequence).unwrap_or(0);
    let limit = query.limit.unwrap_or(100);
    if cursor < 0 || limit == 0 || limit > 500 {
        return error_response(
            "INVALID_ARGUMENT",
            "after_sequence must be nonnegative; limit must be 1..500",
        );
    }
    let state = (
        host.database,
        cursor,
        limit,
        query.job_id,
        std::collections::VecDeque::<opencut_editor_application::JobEvent>::new(),
        host.shutdown,
    );
    let stream = futures_util::stream::unfold(
        state,
        |(database, mut cursor, limit, job_id, mut pending, shutdown)| async move {
            loop {
                if shutdown.load(Ordering::SeqCst) {
                    return None;
                }
                if let Some(event) = pending.pop_front() {
                    cursor = event.sequence;
                    let wire = Event::default()
                        .id(cursor.to_string())
                        .event(&event.event_type)
                        .json_data(&event)
                        .expect("serializable job event");
                    return Some((
                        Ok::<_, std::convert::Infallible>(wire),
                        (database, cursor, limit, job_id, pending, shutdown),
                    ));
                }
                let path = database.clone();
                let filter = job_id.clone();
                let page = tokio::task::spawn_blocking(move || {
                    opencut_host_runtime::workspace::Workspace::open_managed(path.as_ref())?
                        .job_events_after(cursor, limit, filter.as_deref())
                })
                .await;
                match page {
                    Ok(Ok(events)) => pending.extend(events),
                    _ => {
                        let wire = Event::default()
                            .event("error")
                            .data("{\"code\":\"JOB_EVENT_READ_FAILED\",\"retryable\":true}");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        return Some((
                            Ok(wire),
                            (database, cursor, limit, job_id, pending, shutdown),
                        ));
                    }
                }
                if pending.is_empty() {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
            }
        },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

async fn events(
    AuthorizedHost(host): AuthorizedHost,
    query: Result<Query<EventQuery>, axum::extract::rejection::QueryRejection>,
    headers: HeaderMap,
) -> HttpResponse {
    let query = match query {
        Ok(Query(query)) => query,
        Err(error) => return error_response("INVALID_ARGUMENT", &error.to_string()),
    };
    let header_cursor = match headers.get("last-event-id") {
        Some(value) => match value.to_str().ok().and_then(|v| v.parse::<i64>().ok()) {
            Some(cursor) => Some(cursor),
            None => {
                return error_response(
                    "INVALID_ARGUMENT",
                    "Last-Event-ID must be a nonnegative sequence number",
                );
            }
        },
        None => None,
    };
    // Browser reconnection sends Last-Event-ID alongside the original URL.
    let cursor = header_cursor.or(query.after_sequence).unwrap_or(0);
    if cursor < 0 {
        return error_response("INVALID_ARGUMENT", "Event cursor must be nonnegative");
    }
    let state = (
        host.database,
        host.identity.caller,
        cursor,
        std::collections::VecDeque::<opencut_editor_application::StoredEvent>::new(),
        host.shutdown,
    );
    let stream = futures_util::stream::unfold(
        state,
        |(database, caller, mut cursor, mut pending, shutdown)| async move {
            loop {
                if shutdown.load(Ordering::SeqCst) {
                    return None;
                }
                if let Some(event) = pending.pop_front() {
                    cursor = event.sequence;
                    let wire = Event::default()
                        .id(cursor.to_string())
                        .event("committed")
                        .json_data(&event)
                        .expect("serializable event");
                    return Some((
                        Ok::<_, std::convert::Infallible>(wire),
                        (database, caller, cursor, pending, shutdown),
                    ));
                }
                let path = database.clone();
                let identity = caller.clone();
                let page = tokio::task::spawn_blocking(move || {
                    let mut app =
                        opencut_host_runtime::workspace::Workspace::open_managed(path.as_ref())?;
                    app.set_caller(identity);
                    app.events_after(cursor, 100)
                })
                .await;
                match page {
                    Ok(Ok(events)) => pending.extend(events),
                    _ => {
                        let wire = Event::default()
                            .event("error")
                            .data("{\"code\":\"EVENT_READ_FAILED\",\"retryable\":true}");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        return Some((Ok(wire), (database, caller, cursor, pending, shutdown)));
                    }
                }
                if pending.is_empty() {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
            }
        },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Shared public project/job stream with the same opaque cursors and records as
/// CLI NDJSON. POST avoids encoding a tagged, potentially long scope/cursor in a URL.
async fn changes_stream(
    AuthorizedHost(host): AuthorizedHost,
    headers: HeaderMap,
    input: Result<Json<opencut_automation::event_stream::EventChangesInput>, JsonRejection>,
) -> HttpResponse {
    use opencut_automation::event_stream::{StreamRecord, StreamRecordData};
    let mut input = match input {
        Ok(Json(input)) => input,
        Err(error) => return error_response("INVALID_ARGUMENT", &error.body_text()),
    };
    if headers.get_all("last-event-id").iter().count() > 1 {
        return error_response("INVALID_CURSOR", "Duplicate Last-Event-ID header");
    }
    if let Some(cursor) = headers.get("last-event-id") {
        let Ok(cursor) = cursor.to_str() else {
            return error_response("INVALID_CURSOR", "Invalid Last-Event-ID header");
        };
        input.cursor = Some(cursor.into());
    }
    let page = match read_changes(&host, input.clone()).await {
        Ok(page) => page,
        Err(error) => return into_http(opencut_automation::response(Err(error))),
    };
    let state = (
        host,
        input,
        std::collections::VecDeque::from(page.events),
        false,
        false,
    );
    let stream = futures_util::stream::unfold(
        state,
        |(host, mut input, mut pending, mut ready, mut finished)| async move {
            loop {
                if finished || host.shutdown.load(Ordering::SeqCst) {
                    return None;
                }
                let (name, id, record) = if !ready {
                    ready = true;
                    (
                        "ready",
                        None,
                        StreamRecordData::Ready {
                            scope: input.scope.clone(),
                            cursor: input.cursor.clone(),
                        },
                    )
                } else if let Some(change) = pending.pop_front() {
                    input.cursor = Some(change.cursor.clone());
                    (
                        "change",
                        Some(change.cursor.clone()),
                        StreamRecordData::Event {
                            cursor: change.cursor,
                            event: change.event,
                        },
                    )
                } else {
                    match read_changes(&host, input.clone()).await {
                        Ok(page) => {
                            pending.extend(page.events);
                            if pending.is_empty() {
                                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                            }
                            continue;
                        }
                        Err(error) => {
                            finished = true;
                            (
                                "error",
                                None,
                                StreamRecordData::Error {
                                    cursor: input.cursor.clone(),
                                    error,
                                },
                            )
                        }
                    }
                };
                let mut wire = Event::default()
                    .event(name)
                    .json_data(StreamRecord::new(record))
                    .expect("serializable public event");
                if let Some(id) = id {
                    wire = wire.id(id);
                }
                return Some((
                    Ok::<_, std::convert::Infallible>(wire),
                    (host, input, pending, ready, finished),
                ));
            }
        },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}
async fn read_changes(
    host: &Host,
    input: opencut_automation::event_stream::EventChangesInput,
) -> Result<opencut_automation::event_stream::EventChangesPage, EditorError> {
    let path = host.database.clone();
    let caller = host.identity.caller.clone();
    tokio::task::spawn_blocking(move || {
        let mut app = opencut_host_runtime::workspace::Workspace::open_managed(path.as_ref())?;
        app.set_caller(caller);
        app.event_changes(input)
    })
    .await
    .map_err(|_| EditorError::new("INTERNAL_ERROR", "Event query failed"))?
}
