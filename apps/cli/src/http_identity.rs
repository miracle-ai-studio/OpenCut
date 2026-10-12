//! Trusted request identity and bounded revalidation for long-lived responses.
use axum::{
    extract::{FromRequestParts, Request},
    http::request::Parts,
    response::Response,
};
use futures_util::StreamExt;
use opencut_automation::Caller;
use opencut_contracts::EditorError;
use opencut_host_runtime::workspace::Workspace as EditorApplication;
use std::{path::Path, time::Duration};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    pub caller: Caller,
    pub policy_revision: Option<String>,
}
impl Identity {
    pub fn owner() -> Self {
        Self {
            caller: Caller::LocalOwner,
            policy_revision: None,
        }
    }
    pub fn authenticate(database: &Path, secret: &str) -> Result<Self, EditorError> {
        let mut app = EditorApplication::open_managed(database)?;
        let caller = app.authenticate(secret)?;
        app.set_caller(caller.clone());
        let policy_revision = app.access_view()?.policy_revision;
        Ok(Self {
            caller,
            policy_revision,
        })
    }
    pub fn validate(&self, app: &mut EditorApplication) -> Result<(), EditorError> {
        if self.caller.is_owner() {
            return Ok(());
        }
        app.set_caller(self.caller.clone());
        if app.access_view()?.policy_revision != self.policy_revision {
            return Err(EditorError::new(
                "NOT_AUTHORIZED",
                "Authorization changed; reconnect using current permissions",
            ));
        }
        Ok(())
    }
}

/// A handler cannot accidentally inherit the global host's owner identity.
/// Only middleware can populate this request extension; missing identity fails closed.
pub(super) struct AuthorizedHost(pub super::http::Host);
#[derive(Clone)]
pub(super) struct RevalidateStream;
#[derive(Clone)]
pub(super) struct RevalidateV4Resource(
    pub opencut_contracts::project_v4_resources::InspectResourceV4,
);
impl FromRequestParts<super::http::Host> for AuthorizedHost {
    type Rejection = Response;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &super::http::Host,
    ) -> Result<Self, Self::Rejection> {
        let identity = parts.extensions.get::<Identity>().cloned().ok_or_else(|| {
            super::http::error_response(
                "NOT_AUTHORIZED",
                "Authenticated request identity is missing",
            )
        })?;
        let mut host = state.clone();
        host.identity = identity;
        Ok(Self(host))
    }
}

/// Revalidate before yielding each body chunk and while an SSE connection is
/// idle. Bytes already in a client/kernel buffer cannot be recalled.
pub(super) fn guard_stream(
    host: &super::http::Host,
    response: Response,
    call_source: opencut_automation::CallSource,
) -> Response {
    let resource_scope = response
        .extensions()
        .get::<RevalidateV4Resource>()
        .map(|value| value.0.clone());
    if (host.identity.caller.is_owner() && resource_scope.is_none())
        || !response.status().is_success()
    {
        return response;
    }
    let is_stream = response.extensions().get::<RevalidateStream>().is_some()
        || response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                v.starts_with("text/event-stream") || v.starts_with("application/x-ndjson")
            });
    if !is_stream {
        return response;
    }
    let request_id = response
        .headers()
        .get(crate::denials::REQUEST_HEADER)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let (parts, body) = response.into_parts();
    let database = host.database.clone();
    let identity = host.identity.clone();
    let source = body.into_data_stream();
    let stream = futures_util::stream::unfold(
        (source, database, identity, request_id, resource_scope),
        move |(mut source, database, identity, request_id, resource_scope)| async move {
            loop {
                let chunk = tokio::select! {
                    value=source.next()=>Some(value),
                    _=tokio::time::sleep(Duration::from_millis(250))=>None,
                };
                let path = database.clone();
                let expected = identity.clone();
                let trace = request_id.clone();
                let scope = resource_scope.clone();
                let valid = tokio::task::spawn_blocking(move || {
                    let mut app = EditorApplication::open_managed(path.as_ref())?;
                    let result = expected.validate(&mut app).and_then(|()| {
                        app.set_caller(expected.caller.clone());
                        if let Some(scope) = scope {
                            app.revalidate_v4_resource_read(&scope)?;
                        }
                        Ok(())
                    });
                    use opencut_automation::denials::*;
                    if result.as_ref().err().is_some_and(is_denial)
                        && app
                            .record_denial(&Denial {
                                request_id: &trace,
                                source: call_source,
                                caller: Some(&expected.caller),
                                operation: DeniedOperation::Stream,
                                reason: DenialReason::PolicyChanged,
                            })
                            .is_err()
                    {
                        eprintln!("{FAILURE_WARNING}: {trace}");
                    }
                    result
                })
                .await;
                if !matches!(valid, Ok(Ok(()))) {
                    return None;
                }
                if let Some(chunk) = chunk {
                    return chunk.map(|value| {
                        (
                            value,
                            (source, database, identity, request_id, resource_scope),
                        )
                    });
                }
            }
        },
    );
    Response::from_parts(parts, axum::body::Body::from_stream(stream))
}

pub(super) fn request_identity(request: &Request) -> Option<Identity> {
    request.extensions().get::<Identity>().cloned()
}
