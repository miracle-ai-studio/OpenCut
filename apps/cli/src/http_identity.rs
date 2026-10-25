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
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    pub caller: Caller,
    pub policy_revision: Option<String>,
    pub browser_grant: Option<crate::web_host::CookieGrant>,
}
impl Identity {
    pub fn owner() -> Self {
        Self {
            caller: Caller::LocalOwner,
            policy_revision: None,
            browser_grant: None,
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
            browser_grant: None,
        })
    }
    pub fn validate(&self, app: &mut EditorApplication) -> Result<(), EditorError> {
        self.check_lifetime()?;
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
    /// The dispatch checkpoint. Once a native operation passes this check and
    /// starts, logout does not pretend to cancel an accepted atomic transaction
    /// or durable Job. Queued operations must check when their worker starts.
    pub fn check_lifetime(&self) -> Result<(), EditorError> {
        self.browser_grant
            .as_ref()
            .map_or(Ok(()), |grant| grant.check())
    }
}

/// A handler cannot accidentally inherit the global host's owner identity.
/// Only middleware can populate this request extension; missing identity fails closed.
pub(super) struct AuthorizedHost(pub super::http::Host);
#[derive(Clone)]
pub(super) struct RevalidateStream;
#[derive(Clone)]
pub(super) struct RevalidateProjectResource(
    pub opencut_contracts::project_resources::InspectResource,
);
#[derive(Clone)]
pub(super) struct RevalidateVisualPreviewFont(
    pub opencut_host_runtime::visual_preview_contracts::VisualPreviewFont,
);

/// A font response owns its connections, never an authorization result. Moving
/// this context through each blocking check keeps live reads in autocommit
/// mode while avoiding workspace initialization for every 64KiB body chunk.
struct StreamValidation {
    app: EditorApplication,
    font_host: Option<opencut_host_runtime::Automation>,
    database: PathBuf,
}
impl StreamValidation {
    fn open(database: &Path) -> Result<Self, EditorError> {
        Ok(Self {
            app: EditorApplication::open_managed(database)?,
            font_host: None,
            database: database.to_path_buf(),
        })
    }

    fn check(
        &mut self,
        identity: &Identity,
        call_source: opencut_automation::CallSource,
        request_id: &str,
        resource: Option<&opencut_contracts::project_resources::InspectResource>,
        font: Option<&opencut_host_runtime::visual_preview_contracts::VisualPreviewFont>,
    ) -> Result<(), EditorError> {
        let result = identity.validate(&mut self.app).and_then(|()| {
            self.app.set_caller(identity.caller.clone());
            if let Some(resource) = resource {
                self.app.revalidate_project_resource_read(resource)?;
            }
            if let Some(font) = font {
                if self.font_host.is_none() {
                    self.font_host = Some(
                        opencut_host_runtime::Automation::open(&self.database)?
                            .with_source(call_source)
                            .with_caller(identity.caller.clone()),
                    );
                }
                self.font_host
                    .as_mut()
                    .ok_or_else(|| EditorError::new("INTERNAL_ERROR", "Missing font stream host"))?
                    .authorize_visual_preview_font(font)?;
            }
            Ok(())
        });
        use opencut_automation::denials::*;
        if result.as_ref().err().is_some_and(is_denial)
            && self
                .app
                .record_denial(&Denial {
                    request_id,
                    source: call_source,
                    caller: Some(&identity.caller),
                    operation: DeniedOperation::Stream,
                    reason: DenialReason::PolicyChanged,
                })
                .is_err()
        {
            eprintln!("{FAILURE_WARNING}: {request_id}");
        }
        result
    }
}
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
        identity
            .check_lifetime()
            .map_err(|error| super::http::error_response(&error.code, &error.message))?;
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
        .get::<RevalidateProjectResource>()
        .map(|value| value.0.clone());
    let font_scope = response
        .extensions()
        .get::<RevalidateVisualPreviewFont>()
        .map(|value| value.0.clone());
    if (host.identity.caller.is_owner() && resource_scope.is_none() && font_scope.is_none())
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
        (
            source,
            database,
            identity,
            request_id,
            resource_scope,
            font_scope,
            None::<StreamValidation>,
        ),
        move |(
            mut source,
            database,
            identity,
            request_id,
            resource_scope,
            font_scope,
            mut validation,
        )| async move {
            loop {
                let chunk = tokio::select! {
                    value=source.next()=>Some(value),
                    _=tokio::time::sleep(Duration::from_millis(250))=>None,
                };
                let path = database.clone();
                let expected = identity.clone();
                let trace = request_id.clone();
                let scope = resource_scope.clone();
                let font = font_scope.clone();
                let checked = tokio::task::spawn_blocking(move || {
                    let mut context = match validation {
                        Some(context) => context,
                        None => StreamValidation::open(path.as_ref())?,
                    };
                    context.check(
                        &expected,
                        call_source,
                        &trace,
                        scope.as_ref(),
                        font.as_ref(),
                    )?;
                    // Only fixed preview-font responses retain connections.
                    // Other stream routes preserve their existing lifecycle.
                    Ok::<_, EditorError>(font.map(|_| context))
                })
                .await;
                validation = match checked {
                    Ok(Ok(context)) => context,
                    _ => return None,
                };
                if let Some(chunk) = chunk {
                    return chunk.map(|value| {
                        (
                            value,
                            (
                                source,
                                database,
                                identity,
                                request_id,
                                resource_scope,
                                font_scope,
                                validation,
                            ),
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
