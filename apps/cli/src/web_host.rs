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

#[derive(Clone)]
pub struct BrowserSession {
    secret: String,
}
impl BrowserSession {
    pub fn new() -> Self {
        Self {
            secret: format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ),
        }
    }
    fn same_origin(request: &Request) -> bool {
        let headers = request.headers();
        let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
        let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
        headers.get_all(header::ORIGIN).iter().count() <= 1
            && headers
                .get("sec-fetch-site")
                .is_none_or(|v| v == "same-origin")
            && match (origin, host) {
                (Some(origin), Some(host)) => origin == format!("http://{host}"),
                (None, _) => matches!(*request.method(), Method::GET | Method::HEAD),
                _ => false,
            }
    }
    pub fn accepts(&self, request: &Request) -> bool {
        Self::same_origin(request)
            && request
                .headers()
                .get_all(header::COOKIE)
                .iter()
                .filter_map(|v| v.to_str().ok())
                .flat_map(|v| v.split(';'))
                .any(|v| v.trim().strip_prefix("opencut_session=") == Some(self.secret.as_str()))
    }
    async fn open(self, request: Request) -> Response {
        if !Self::same_origin(&request)
            || request
                .headers()
                .get("x-opencut-app")
                .is_none_or(|v| v != "web")
        {
            return StatusCode::FORBIDDEN.into_response();
        }
        let mut response = StatusCode::NO_CONTENT.into_response();
        response.headers_mut().insert(
            header::SET_COOKIE,
            HeaderValue::from_str(&format!(
                "opencut_session={}; HttpOnly; SameSite=Strict; Path=/",
                self.secret
            ))
            .unwrap(),
        );
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response
    }
}

pub fn attach(
    router: Router,
    root: Option<PathBuf>,
    listen: SocketAddr,
    session: Option<BrowserSession>,
) -> Router {
    let router = if let Some(root) = root {
        router
            .route(
                "/app/session",
                axum::routing::post(move |request: Request| {
                    let session = session.clone();
                    async move {
                        match session {
                            Some(session) => session.open(request).await,
                            None => StatusCode::NOT_FOUND.into_response(),
                        }
                    }
                }),
            )
            .fallback(move |request: Request| static_file(root.clone(), request))
    } else {
        router.fallback(|| async { crate::http::error_response("NOT_FOUND", "Unknown API route") })
    };
    // Loopback binding alone does not prevent a public DNS name from resolving to this host.
    router.layer(middleware::from_fn(
        move |request: Request, next: Next| async move {
            let host = request
                .headers()
                .get(header::HOST)
                .and_then(|value| value.to_str().ok());
            let allowed = [listen.to_string(), format!("localhost:{}", listen.port())];
            if !host.is_some_and(|host| allowed.iter().any(|allowed| allowed == host)) {
                return (StatusCode::FORBIDDEN, "Invalid local host").into_response();
            }
            next.run(request).await
        },
    ))
}

async fn static_file(root: PathBuf, request: Request) -> Response {
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let path = request.uri().path();
    let relative = match path {
        "/" | "/projects" | "/editor" | "/editor/" | "/brand" | "/roadmap" | "/sponsors"
        | "/privacy" | "/terms" | "/contributors" | "/blog" | "/changelog" => "index.html",
        path if public_detail_route(path) => "index.html",
        path if editor_route(path) => "index.html",
        path if path.starts_with("/assets/") => &path[1..],
        "/favicon.ico" | "/manifest.json" | "/robots.txt" => &path[1..],
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

fn public_detail_route(path: &str) -> bool {
    if let Some(slug) = path.strip_prefix("/blog/") {
        return !slug.is_empty()
            && slug.len() <= 200
            && slug
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    }
    if let Some(version) = path.strip_prefix("/changelog/") {
        let parts: Vec<_> = version.split('.').collect();
        return version.len() <= 32
            && parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()));
    }
    false
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
