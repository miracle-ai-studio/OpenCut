use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use opencut_automation::{domains, tool_spec, tool_specs};
use opencut_host_runtime::Automation;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
    },
    service::RequestContext,
    transport::stdio,
};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub(super) enum ToolSurface {
    #[default]
    Domains,
    Legacy,
}

#[derive(Clone)]
pub(super) struct EditorMcp {
    database: Arc<PathBuf>,
    subscriptions: crate::mcp_context::Subscriptions,
    source: opencut_automation::CallSource,
    caller: opencut_automation::Caller,
    tool_surface: ToolSurface,
}

impl EditorMcp {
    fn request_caller(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<opencut_automation::Caller, McpError> {
        if self.source == opencut_automation::CallSource::McpHttp {
            return context
                .extensions
                .get::<axum::http::request::Parts>()
                .and_then(|parts| parts.extensions.get::<crate::http_identity::Identity>())
                .map(|identity| identity.caller.clone())
                .ok_or_else(|| {
                    McpError::invalid_params(
                        "Authenticated request identity is missing",
                        Some(json!({"code":"NOT_AUTHORIZED"})),
                    )
                });
        }
        Ok(self.caller.clone())
    }

    async fn authorized_caller(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<opencut_automation::Caller, McpError> {
        let caller = self.request_caller(context)?;
        if caller.is_owner() {
            return Ok(caller);
        }
        let database = self.database.clone();
        let source = self.source;
        let checked = caller.clone();
        tokio::task::spawn_blocking(move|| {
            let mut app=opencut_host_runtime::workspace::Workspace::open_managed(database.as_ref()).map_err(|_|McpError::internal_error("Identity check unavailable",None))?;
            app.set_caller(checked.clone());
            if let Err(error)=app.access_view() {
                let response=opencut_automation::denials::observe_response(&app,Some(&checked),source,opencut_automation::denials::DeniedOperation::McpRequest,opencut_automation::denials::DenialReason::AuthorizationDenied,opencut_automation::response(Err(error)));
                return Err(McpError::invalid_params("Current identity is not authorized",Some(json!({"code":response.error.as_ref().map(|e|&e.code),"request_id":response.request_id,"warnings":response.warnings}))));
            }
            Ok(())
        }).await.map_err(|_|McpError::internal_error("Identity check failed",None))??;
        Ok(caller)
    }

    fn with_caller(mut self, caller: opencut_automation::Caller) -> Self {
        self.caller = caller;
        self
    }
    pub(super) fn new(
        database: Arc<PathBuf>,
        source: opencut_automation::CallSource,
        tool_surface: ToolSurface,
    ) -> Self {
        Self {
            database,
            subscriptions: Default::default(),
            source,
            caller: opencut_automation::Caller::LocalOwner,
            tool_surface,
        }
    }
}

fn tool(spec: opencut_automation::ToolSpec) -> Tool {
    Tool::new(spec.name, spec.description, mcp_schema(spec.input_schema))
        .with_raw_output_schema(Arc::new(mcp_schema(spec.output_schema)))
        .with_annotations(
            ToolAnnotations::new()
                .read_only(spec.read_only)
                .idempotent(true)
                .open_world(false),
        )
}

fn tools(surface: ToolSurface) -> Vec<Tool> {
    let specs = match surface {
        ToolSurface::Domains => domains::tool_specs_compact(),
        ToolSurface::Legacy => tool_specs(),
    };
    specs.into_iter().map(tool).collect()
}

/// MCP requires a root object type and object-valued top-level property schemas.
/// Keep the canonical v1 schema unchanged; these are equivalent wire forms for
/// our object DTOs (including unions whose alternatives are all objects).
fn mcp_schema(schema: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut schema = schema.as_object().expect("object schema").clone();
    schema.insert("type".into(), serde_json::json!("object"));
    if let Some(properties) = schema.get_mut("properties").and_then(|v| v.as_object_mut()) {
        for property in properties.values_mut() {
            match property {
                serde_json::Value::Bool(true) => *property = serde_json::json!({}),
                serde_json::Value::Bool(false) => *property = serde_json::json!({"not": {}}),
                _ => {}
            }
        }
    }
    schema
}

impl ServerHandler for EditorMcp {
    fn supported_protocol_versions(
        &self,
    ) -> std::borrow::Cow<'static, [rmcp::model::ProtocolVersion]> {
        std::borrow::Cow::Owned(vec![rmcp::model::ProtocolVersion::V_2025_11_25])
    }
    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListResourcesResult, McpError> {
        self.authorized_caller(&context).await?;
        crate::mcp_context::first_page(request)?;
        Ok(crate::mcp_context::resources_list())
    }
    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListResourceTemplatesResult, McpError> {
        self.authorized_caller(&context).await?;
        crate::mcp_context::first_page(request)?;
        Ok(crate::mcp_context::templates_list())
    }
    async fn read_resource(
        &self,
        request: rmcp::model::ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, McpError> {
        let data = crate::mcp_context::read(
            self.database.clone(),
            self.authorized_caller(&context).await?,
            self.source,
            request.uri.clone(),
        )
        .await?;
        Ok(
            rmcp::model::ReadResourceResult::new(vec![crate::mcp_context::content(
                request.uri,
                &data,
            )])
            .into(),
        )
    }
    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListPromptsResult, McpError> {
        self.authorized_caller(&context).await?;
        crate::mcp_context::first_page(request)?;
        Ok(crate::mcp_context::prompts_list())
    }
    async fn get_prompt(
        &self,
        request: rmcp::model::GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::GetPromptResponse, McpError> {
        crate::mcp_context::prompt(
            self.database.clone(),
            self.authorized_caller(&context).await?,
            self.source,
            request,
        )
        .await
    }
    async fn subscribe(
        &self,
        request: rmcp::model::SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        self.subscriptions
            .subscribe(
                self.database.clone(),
                self.authorized_caller(&context).await?,
                self.source,
                request.uri,
                context.peer,
            )
            .await
    }
    async fn unsubscribe(
        &self,
        request: rmcp::model::UnsubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        self.authorized_caller(&context).await?;
        self.subscriptions.unsubscribe(&request.uri).await
    }
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().enable_resources().enable_resources_subscribe().enable_prompts().build())
            .with_server_info(Implementation::new("opencut", env!("CARGO_PKG_VERSION")))
            .with_instructions("Use the nine domain tools. Read opencut://schemas/domains for the action catalog and opencut://schemas/domains/{domain} for exact action schemas. Inspect capabilities and the project revision before editing. Plan edits before applying them. Reuse the same idempotency key for a retried write. Existing leaf tool names remain callable for compatibility.")
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        self.authorized_caller(&context).await?;
        if request.and_then(|r| r.cursor).is_some() {
            return Err(McpError::invalid_params("Unknown tools cursor", None));
        }
        Ok(ListToolsResult::with_all_items(tools(self.tool_surface)))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        domains::domain_tool_spec(name)
            .or_else(|| tool_spec(name))
            .map(tool)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        if self.get_tool(&request.name).is_none() {
            return Err(McpError::invalid_params(
                "Unknown OpenCut tool",
                Some(json!({"name":request.name})),
            ));
        }
        let database = self.database.clone();
        let source = self.source;
        let caller = self.request_caller(&context)?;
        let response =
            tokio::task::spawn_blocking(move || match Automation::open(database.as_ref()) {
                Ok(automation) => automation.with_source(source).with_caller(caller).call(
                    &request.name,
                    Value::Object(request.arguments.unwrap_or_default()),
                ),
                Err(error) => opencut_automation::response(Err(error)),
            })
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let is_error = response.error.is_some();
        let value = serde_json::to_value(response)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(if is_error {
            CallToolResult::structured_error(value)
        } else {
            CallToolResult::structured(value)
        }
        .into())
    }
}

pub async fn serve(
    database: &Path,
    caller: opencut_automation::Caller,
    tool_surface: ToolSurface,
    with_worker: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if with_worker && !caller.is_owner() {
        return Err(opencut_editor_application::access::denied().into());
    }
    drop(Automation::open(database)?);
    let handler = EditorMcp::new(
        Arc::new(database.to_path_buf()),
        opencut_automation::CallSource::McpStdio,
        tool_surface,
    )
    .with_caller(caller);
    let subscriptions = handler.subscriptions.clone();
    let disconnected = tokio_util::sync::CancellationToken::new();
    let (input, output) = stdio();
    let input = DisconnectReader {
        input,
        disconnected: disconnected.clone(),
    };
    let server = handler.serve((input, output)).await?;
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut worker = with_worker.then(|| {
        let worker_database = database.to_path_buf();
        let worker_stop = stop.clone();
        tokio::task::spawn_blocking(move || {
            opencut_host_runtime::worker::run(&worker_database, false, worker_stop)
        })
    });
    let mut worker_finished = false;
    let result: Result<(), Box<dyn std::error::Error>> = tokio::select! {
        result=server.waiting()=>result.map(|_| ()).map_err(Into::into),
        _=disconnected.cancelled()=>Ok(()),
        _=crate::signals::termination()=>Ok(()),
        result=async {
            match worker.as_mut() {
                Some(task) => task.await.map_err(|error| -> Box<dyn std::error::Error> {error.into()})?
                    .map_err(|error| -> Box<dyn std::error::Error> {error.into()}),
                None => std::future::pending::<Result<(), Box<dyn std::error::Error>>>().await,
            }
        } => {
            worker_finished = true;
            result.and_then(|_| Err("Managed Worker stopped unexpectedly".into()))
        },
    };
    // Disconnect is a host stop, never a durable job cancellation. The scheduler
    // fences unfinished attempts and a later Worker recovers their leases.
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    subscriptions.shutdown().await;
    if !worker_finished && let Some(worker) = worker {
        worker.await??;
    }
    result
}

// EOF must be observable independently of the SDK flushing its writer during
// transport.close(). A peer that stopped reading may otherwise block teardown.
struct DisconnectReader<R> {
    input: R,
    disconnected: tokio_util::sync::CancellationToken,
}
impl<R: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for DisconnectReader<R> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let before = buffer.filled().len();
        let result = std::pin::Pin::new(&mut self.input).poll_read(cx, buffer);
        if matches!(&result, std::task::Poll::Ready(Err(_)))
            || (matches!(&result, std::task::Poll::Ready(Ok(())))
                && buffer.filled().len() == before
                && buffer.remaining() > 0)
        {
            self.disconnected.cancel();
        }
        result
    }
}
