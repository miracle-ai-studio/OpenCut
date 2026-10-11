//! Transport refusals not routed through Automation share its bounded audit store.
use axum::{
    http::{HeaderValue, StatusCode},
    response::Response as HttpResponse,
};
use opencut_automation::{CallSource, Caller, Response, denials::*};
use opencut_host_runtime::denials::record_at;
use std::path::Path;
pub(super) const REQUEST_HEADER: &str = "x-opencut-request-id";
pub(super) const WARNING_HEADER: &str = "x-opencut-audit-warning";
pub(super) fn response(
    database: &Path,
    caller: Option<&Caller>,
    source: CallSource,
    operation: DeniedOperation,
    reason: DenialReason,
    mut response: Response,
) -> Response {
    if response.error.as_ref().is_some_and(is_denial)
        && record_at(
            database,
            &Denial {
                request_id: &response.request_id,
                caller,
                source,
                operation,
                reason,
            },
        )
        .is_err()
        && !response.warnings.iter().any(|w| w == FAILURE_WARNING)
    {
        response.warnings.push(FAILURE_WARNING.into());
    }
    response
}
pub(super) fn request_id(response: &mut HttpResponse) -> String {
    let id = response
        .headers()
        .get(REQUEST_HEADER)
        .and_then(|h| h.to_str().ok())
        .filter(|s| uuid::Uuid::parse_str(s).is_ok())
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    response.headers_mut().insert(
        REQUEST_HEADER,
        HeaderValue::from_str(&id).expect("UUID header"),
    );
    id
}
pub(super) async fn http(
    database: std::sync::Arc<std::path::PathBuf>,
    caller: Option<Caller>,
    source: CallSource,
    operation: DeniedOperation,
    reason: DenialReason,
    mut response: HttpResponse,
) -> HttpResponse {
    let id = request_id(&mut response);
    if response.status() == StatusCode::FORBIDDEN {
        let failed = tokio::task::spawn_blocking(move || {
            record_at(
                database.as_ref(),
                &Denial {
                    request_id: &id,
                    caller: caller.as_ref(),
                    source,
                    operation,
                    reason,
                },
            )
        })
        .await;
        if !matches!(failed, Ok(Ok(()))) {
            response
                .headers_mut()
                .insert(WARNING_HEADER, HeaderValue::from_static(FAILURE_WARNING));
        }
    }
    response
}
pub(super) async fn refuse(
    host: &crate::http::Host,
    source: CallSource,
    reason: DenialReason,
    message: &str,
) -> HttpResponse {
    http(
        host.database.clone(),
        None,
        source,
        DeniedOperation::Authentication,
        reason,
        crate::http::error_response("NOT_AUTHORIZED", message),
    )
    .await
}
