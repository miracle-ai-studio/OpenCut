//! Public static application assets. Editor routes retain their existing authentication.
use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};
use tokio_util::io::ReaderStream;

pub fn validate_root(path: PathBuf) -> std::io::Result<PathBuf> {
    let root = path.canonicalize()?;
    if !root.join("index.html").is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Web root must contain a built index.html",
        ));
    }
    Ok(root)
}

mod metadata;
mod public_content;
mod session;
pub use session::BrowserSession;
pub(crate) use session::CookieGrant;

pub fn attach(
    router: Router,
    root: Option<PathBuf>,
    listen: SocketAddr,
    session: Option<BrowserSession>,
) -> Router {
    let content = public_content::PublicContent::new();
    let response_sessions = session.clone();
    let router = if let Some(root) = root {
        router
            .route(
                "/app/session",
                axum::routing::any(move |request: Request| {
                    let session = session.clone();
                    async move {
                        match session {
                            Some(session) => session.handle(request),
                            None => StatusCode::NOT_FOUND.into_response(),
                        }
                    }
                }),
            )
            .fallback(move |request: Request| static_file(root.clone(), request, content.clone()))
    } else {
        router.fallback(|| async { crate::http::error_response("NOT_FOUND", "Unknown API route") })
    };
    // Loopback binding alone does not prevent a public DNS name from resolving to this host.
    router.layer(middleware::from_fn(move |request: Request, next: Next| {
        let response_sessions = response_sessions.clone();
        async move {
            let host = request
                .headers()
                .get(header::HOST)
                .and_then(|value| value.to_str().ok());
            let allowed = [listen.to_string(), format!("localhost:{}", listen.port())];
            if !host.is_some_and(|host| allowed.iter().any(|allowed| allowed == host)) {
                return (StatusCode::FORBIDDEN, "Invalid local host").into_response();
            }
            let grant = response_sessions
                .as_ref()
                .and_then(|sessions| sessions.response_grant(&request));
            let response = next.run(request).await;
            match grant {
                Some(grant) => grant.wrap(response),
                None => response,
            }
        }
    }))
}

async fn static_file(
    root: PathBuf,
    request: Request,
    content: public_content::PublicContent,
) -> Response {
    // Static responses never read the request body; dropping it keeps the handler future Send.
    let request = request.map(|_| ());
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    if let Some(response) = metadata::serve(&root, &request, &content).await {
        return response;
    }
    let path = request.uri().path();
    let relative = match path {
        "/" | "/projects" | "/editor" | "/editor/" | "/brand" | "/roadmap" | "/sponsors"
        | "/privacy" | "/terms" | "/contributors" | "/blog" | "/changelog" => "index.html",
        path if editor_route(path) => "index.html",
        path if path.starts_with("/assets/") => &path[1..],
        "/favicon.ico"
        | "/manifest.json"
        | "/robots.txt"
        | "/open-graph/default.jpg"
        | "/open-graph/roadmap.jpg"
        | "/open-graph/changelog.jpg" => &path[1..],
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    if relative.contains('%')
        || relative.contains('\\')
        || Path::new(relative)
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(path) = tokio::fs::canonicalize(root.join(relative)).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !path.starts_with(&root) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(file) = tokio::fs::File::open(&path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(metadata) = file.metadata().await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !metadata.is_file() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let mime = match path.extension().and_then(|value| value.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    };
    let body = if request.method() == Method::HEAD {
        Body::empty()
    } else {
        Body::from_stream(ReaderStream::new(file))
    };
    let mut response = body.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(metadata.len()));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(if relative.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-store"
        }),
    );
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    response
}

// Project IDs are application identifiers, not file names or UUIDs. Every valid
// single encoded segment serves the same index; static assets retain their checks.
fn editor_route(path: &str) -> bool {
    let Some(segment) = path.strip_prefix("/editor/") else {
        return false;
    };
    if segment.is_empty() || segment.contains('/') || segment.contains('\\') {
        return false;
    }
    let mut decoded = Vec::with_capacity(segment.len());
    let mut bytes = segment.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let Some(high) = bytes.next().and_then(|b| (b as char).to_digit(16)) else {
                return false;
            };
            let Some(low) = bytes.next().and_then(|b| (b as char).to_digit(16)) else {
                return false;
            };
            decoded.push((high * 16 + low) as u8);
        } else {
            decoded.push(byte);
        }
    }
    std::str::from_utf8(&decoded).is_ok_and(|id| {
        !id.trim().is_empty() && !matches!(id, "." | "..") && !id.chars().any(char::is_control)
    })
}
