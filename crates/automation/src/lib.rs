//! Shared typed automation entry points. Transports must not implement editing rules.
pub use opencut_api_contracts::{context, event_stream};
pub use opencut_editor_application::access::Caller;
pub use opencut_editor_application::audit::{CallSource, ExecutionContext};
pub mod capabilities;
pub mod denials;
pub use capabilities::capabilities;
pub mod jobs;
pub mod media;
pub mod plugins;
pub mod render;
pub use opencut_editor_application::uploads;
use std::path::Path;
use uploads::{ImportStore, UploadRetentionStore};

use opencut_contracts::EditorError;
use opencut_sqlite_store::EditorApplication;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use uuid::Uuid;

pub use opencut_api_contracts::domains;
pub use opencut_api_contracts::tools::{ToolSpec, tool_spec, tool_specs};
pub use opencut_api_contracts::{
    EditInput, EmptyInput, EventsInput, HistoryInput, JobEventsInput, ProjectCreateInput,
    ProjectInput, Response, RestoreInput, RevisionInput,
};

pub fn response(result: Result<Value, EditorError>) -> Response {
    Response::from_result_with_id(Uuid::new_v4().to_string(), result)
}

pub struct Automation {
    app: EditorApplication,
    database: std::path::PathBuf,
    plugin_root: std::path::PathBuf,
    media_root: std::path::PathBuf,
    source: CallSource,
    caller: Caller,
}

impl Automation {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, EditorError> {
        let plugin_root = path
            .as_ref()
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("plugins");
        let app = EditorApplication::open_managed(path.as_ref())?;
        let media_root = app
            .asset_store_root()
            .parent()
            .expect("asset store has a parent")
            .join("media");
        Ok(Self {
            database: path.as_ref().to_path_buf(),
            app,
            plugin_root,
            media_root,
            source: CallSource::Embedded,
            caller: Caller::LocalOwner,
        })
    }

    pub fn call(&mut self, name: &str, arguments: Value) -> Response {
        let operation = domains::operation_name(name, &arguments)
            .map(|name| denials::operation(&name))
            .unwrap_or_else(|| denials::operation(name));
        let context = ExecutionContext::for_caller(self.source, self.caller.clone());
        let request_id = context.request_id().to_owned();
        self.app.set_execution_context(context.clone());
        let result = self.execute(name, arguments);
        denials::observe_response(
            &self.app,
            Some(context.caller()),
            self.source,
            operation,
            denials::DenialReason::AuthorizationDenied,
            Response::from_result_with_id(request_id, result),
        )
    }

    /// Native data-plane entry; never registered as a JSON/base64 control tool.
    pub fn put_upload_chunk(&mut self, input: &uploads::ChunkDescriptor, bytes: &[u8]) -> Response {
        let context = ExecutionContext::for_caller(self.source, self.caller.clone());
        let request_id = context.request_id().to_owned();
        self.app.set_execution_context(context.clone());
        let root = self
            .app
            .asset_store_root()
            .parent()
            .expect("asset store parent")
            .join("uploads");
        let result = opencut_native_services::uploads::put(&root, &mut self.app, input, bytes)
            .map_err(uploads::public_error)
            .and_then(to_value);
        denials::observe_response(
            &self.app,
            Some(context.caller()),
            self.source,
            denials::DeniedOperation::UploadChunk,
            denials::DenialReason::AuthorizationDenied,
            Response::from_result_with_id(request_id, result),
        )
    }

    /// Trusted native host metadata. Never populate from client identity claims.
    pub fn with_source(mut self, source: CallSource) -> Self {
        self.source = source;
        self
    }

    pub fn with_caller(mut self, caller: Caller) -> Self {
        self.caller = caller;
        self
    }

    pub fn authenticate(&self, secret: &str) -> Result<Caller, EditorError> {
        self.app.authenticate(secret)
    }

    fn execute(&mut self, name: &str, arguments: Value) -> Result<Value, EditorError> {
        // Re-check revocation even for discovery and malformed domain calls.
        self.app.access_view()?;
        if serde_json::to_vec(&arguments).map_err(invalid)?.len() > 2_097_152 {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Control request exceeds 2 MiB",
            ));
        }
        let (name, arguments) = domains::resolve(name, arguments)?;
        let name = name.as_str();
        // Resolve a domain action before checking permission. A broad domain
        // name never grants access to its owner-only leaf operations.
        opencut_editor_application::access::authorize_tool(&self.caller, name)?;
        match name {
            "opencut_database_inspect" => {
                let _: EmptyInput = parse(arguments)?;
                to_value(opencut_editor_application::database::inspect(
                    &self.caller,
                    &self.app,
                )?)
            }
            "opencut_database_checkpoint" => {
                to_value(opencut_editor_application::database::checkpoint(
                    &self.caller,
                    &self.app,
                    parse(arguments)?,
                )?)
            }
            "opencut_storage_budget_inspect" | "opencut_storage_budget_recover" => {
                let _: opencut_api_contracts::EmptyInput = parse(arguments)?;
                to_value(opencut_editor_application::budget::inspect(
                    &self.caller,
                    &opencut_native_services::budget::NativeBudget(&self.database),
                    name == "opencut_storage_budget_recover",
                )?)
            }
            "opencut_storage_budget_configure" => {
                to_value(opencut_editor_application::budget::configure(
                    self.app.execution_context(),
                    &opencut_native_services::budget::NativeBudget(&self.database),
                    parse(arguments)?,
                )?)
            }
            "opencut_cas_gc_query" => to_value(opencut_editor_application::gc::query(
                &self.app,
                parse(arguments)?,
            )?),
            "opencut_cas_gc_plan" => {
                let root = self.app.asset_store_root().to_path_buf();
                to_value(opencut_editor_application::gc::plan(
                    &mut self.app,
                    &opencut_native_services::gc::NativeGc(&root, &self.database),
                    parse(arguments)?,
                )?)
            }
            "opencut_cas_gc_execute" => {
                let root = self.app.asset_store_root().to_path_buf();
                to_value(opencut_editor_application::gc::execute(
                    &mut self.app,
                    &opencut_native_services::gc::NativeGc(&root, &self.database),
                    parse(arguments)?,
                )?)
            }
            "opencut_cas_gc_inspect" => to_value(opencut_editor_application::gc::inspect(
                &self.app,
                parse(arguments)?,
            )?),
            "opencut_storage_inspect" => to_value(opencut_editor_application::storage::inspect(
                &self.caller,
                &opencut_native_services::storage::NativeStorage(&self.database),
                parse(arguments)?,
            )?),
            "opencut_upload_create" => to_value(
                uploads::create(&mut self.app, parse(arguments)?).map_err(uploads::public_error)?,
            ),
            "opencut_upload_inspect" => to_value(
                uploads::inspect(&self.app, parse(arguments)?).map_err(uploads::public_error)?,
            ),
            "opencut_upload_abort" => {
                let root = self
                    .app
                    .asset_store_root()
                    .parent()
                    .expect("asset store parent")
                    .join("uploads");
                to_value(
                    opencut_native_services::uploads::abort(
                        &root,
                        &mut self.app,
                        parse(arguments)?,
                    )
                    .map_err(uploads::public_error)?,
                )
            }
            "opencut_upload_seal" => to_value(
                uploads::seal(&mut self.app, parse(arguments)?).map_err(uploads::public_error)?,
            ),
            "opencut_upload_retention" => {
                let input: uploads::UploadInput = parse(arguments)?;
                to_value(
                    self.app
                        .upload_retention(&input.upload_id)
                        .map_err(uploads::public_error)?,
                )
            }
            "opencut_upload_query" => to_value(
                uploads::query_uploads(&self.app, parse(arguments)?)
                    .map_err(uploads::public_error)?,
            ),
            "opencut_upload_quota" => {
                let input: ProjectInput = parse(arguments)?;
                to_value(
                    self.app
                        .upload_quota(&input.project_id)
                        .map_err(uploads::public_error)?,
                )
            }
            "opencut_upload_release" => {
                let root = self
                    .app
                    .asset_store_root()
                    .parent()
                    .expect("asset store parent")
                    .join("uploads");
                to_value(
                    opencut_native_services::uploads::release(
                        &root,
                        &mut self.app,
                        parse(arguments)?,
                    )
                    .map_err(uploads::public_error)?,
                )
            }
            "opencut_project_media_inspect" => to_value(
                self.app
                    .project_media(&parse(arguments)?)
                    .map_err(uploads::public_error)?,
            ),
            "opencut_project_media_query" => to_value(
                uploads::query_media(&self.app, parse(arguments)?)
                    .map_err(uploads::public_error)?,
            ),
            "opencut_access_inspect" => {
                let _: EmptyInput = parse(arguments)?;
                to_value(self.app.access_view()?)
            }
            "opencut_denial_query" => {
                use opencut_editor_application::denials::DenialStore;
                to_value(self.app.read_denials(&parse(arguments)?)?)
            }
            "opencut_audit_query" => to_value(self.app.audit_query(parse(arguments)?)?),
            "opencut_access_audit_query" => {
                to_value(self.app.access_audit_query(parse(arguments)?)?)
            }
            "opencut_plugin_publication_query" => {
                to_value(self.app.plugin_publication_query(parse(arguments)?)?)
            }
            "opencut_plugin_publication_reconcile" => to_value(plugins::reconcile(
                &self.plugin_root,
                parse(arguments)?,
                &self.app,
            )?),
            "opencut_maintenance_query" => to_value(self.app.maintenance_query(parse(arguments)?)?),
            "opencut_maintenance_reconcile" => to_value(media::reconcile(
                &self.media_root,
                parse(arguments)?,
                &self.app,
            )?),
            "opencut_job_view" => {
                let input: jobs::JobInput = parse(arguments)?;
                to_value(self.app.job_view(&input.job_id)?)
            }
            "opencut_resource_read" => to_value(
                self.app
                    .read_resource(parse(arguments)?, capabilities::document)?,
            ),
            "opencut_event_changes" => to_value(self.app.event_changes(parse(arguments)?)?),
            "opencut_job_query" => to_value(
                self.app
                    .job_query(parse(arguments)?)
                    .map_err(opencut_editor_application::job_views::public_error)?,
            ),
            "opencut_job_changes" => to_value(
                self.app
                    .job_changes(parse(arguments)?)
                    .map_err(opencut_editor_application::job_views::public_error)?,
            ),
            "opencut_job_submit" => {
                let asset_store_root = self.app.asset_store_root().to_path_buf();
                let record = jobs::start(&mut self.app, &asset_store_root, parse(arguments)?)
                    .map_err(opencut_editor_application::job_views::public_error)?;
                to_value(opencut_editor_application::job_views::project(record))
            }
            "opencut_job_request_cancel" => {
                let input: jobs::JobInput = parse(arguments)?;
                let record = self
                    .app
                    .cancel_job(&input.job_id)
                    .map_err(opencut_editor_application::job_views::public_error)?;
                to_value(opencut_editor_application::job_views::project(record))
            }

            "opencut_media_inspect" => {
                let input: media::InspectMediaInput = parse(arguments)?;
                to_value(media::inspect(&self.media_root, input)?)
            }
            "opencut_media_repair" => to_value(media::repair_observed(
                &self.media_root,
                parse(arguments)?,
                &self.app,
            )?),
            "opencut_project_history" => {
                let input: HistoryInput = parse(arguments)?;
                to_value(self.app.history(
                    &input.project_id,
                    input.from_revision.as_deref(),
                    input.limit,
                )?)
            }
            "opencut_revision_inspect" => {
                let input: RevisionInput = parse(arguments)?;
                to_value(self.app.snapshot_at(&input.project_id, &input.revision)?)
            }
            "opencut_revision_restore" => {
                let input: RestoreInput = parse(arguments)?;
                to_value(
                    self.app
                        .restore_revision(&input.project_id, &input.request)?,
                )
            }
            "opencut_project_plugin_grant" => {
                use opencut_editor_application::project_plugins::{
                    self as scoped, ProjectPluginStore,
                };
                to_value(
                    self.app
                        .set_plugin_grant(&parse(arguments)?)
                        .map_err(scoped::public_error)?,
                )
            }
            "opencut_project_plugin_list" => {
                use opencut_editor_application::project_plugins::{
                    self as scoped, ProjectPluginStore,
                };
                let input: ProjectInput = parse(arguments)?;
                to_value(
                    self.app
                        .plugin_grants(&input.project_id)
                        .map_err(scoped::public_error)?,
                )
            }
            "opencut_project_plugin_inspect" => {
                use opencut_editor_application::project_plugins as scoped;
                to_value(
                    scoped::inspect(
                        &self.app,
                        &plugins::NativePlugins(&self.plugin_root, None),
                        parse(arguments)?,
                    )
                    .map_err(scoped::public_error)?,
                )
            }
            "opencut_project_plugin_plan" => {
                use opencut_editor_application::project_plugins as scoped;
                to_value(
                    scoped::plan(
                        &self.app,
                        &plugins::NativePlugins(&self.plugin_root, None),
                        parse(arguments)?,
                    )
                    .map_err(scoped::public_error)?,
                )
            }
            "opencut_plugin_list" => {
                let _: EmptyInput = parse(arguments)?;
                to_value(plugins::list(&self.plugin_root)?)
            }
            "opencut_plugin_inspect" => {
                to_value(plugins::inspect(&self.plugin_root, parse(arguments)?)?)
            }
            "opencut_plugin_install" => to_value(plugins::install_observed(
                &self.plugin_root,
                parse(arguments)?,
                &self.app,
            )?),
            "opencut_plugin_plan" => to_value(plugins::plan(
                &self.app,
                &self.plugin_root,
                parse(arguments)?,
            )?),
            "opencut_job_start" => {
                let asset_store_root = self.app.asset_store_root().to_path_buf();
                to_value(jobs::start(
                    &mut self.app,
                    &asset_store_root,
                    parse(arguments)?,
                )?)
            }
            "opencut_render_preflight" => {
                let asset_store_root = self.app.asset_store_root().to_path_buf();
                to_value(render::preflight(
                    &self.app,
                    &asset_store_root,
                    parse(arguments)?,
                )?)
            }
            "opencut_job_get" => {
                let input: jobs::JobInput = parse(arguments)?;
                to_value(self.app.get_job(&input.job_id)?)
            }
            "opencut_job_cancel" => {
                let input: jobs::JobInput = parse(arguments)?;
                to_value(self.app.cancel_job(&input.job_id)?)
            }
            "opencut_job_list" => {
                let _: EmptyInput = parse(arguments)?;
                to_value(self.app.list_jobs()?)
            }
            "opencut_capabilities" => {
                let _: EmptyInput = parse(arguments)?;
                Ok(capabilities())
            }
            "opencut_project_list" => {
                let _: EmptyInput = parse(arguments)?;
                to_value(self.app.list_projects()?)
            }
            "opencut_project_create" => {
                let input: ProjectCreateInput = parse(arguments)?;
                to_value(
                    self.app
                        .create_project(input.project, &input.idempotency_key)?,
                )
            }
            "opencut_project_inspect" => {
                let input: ProjectInput = parse(arguments)?;
                to_value(self.app.snapshot(&input.project_id)?)
            }
            "opencut_edit_plan" => {
                let input: EditInput = parse(arguments)?;
                to_value(self.app.plan(&input.project_id, &input.request)?)
            }
            "opencut_edit_apply" => {
                let input: EditInput = parse(arguments)?;
                to_value(self.app.apply(&input.project_id, &input.request)?)
            }
            "opencut_events_read" => {
                let input: EventsInput = parse(arguments)?;
                if input.after_sequence < 0 || input.limit == 0 || input.limit > 500 {
                    return Err(EditorError::new(
                        "INVALID_ARGUMENT",
                        "after_sequence must be nonnegative; limit must be 1..500",
                    ));
                }
                to_value(self.app.events_after(input.after_sequence, input.limit)?)
            }
            "opencut_job_events" => {
                let input: JobEventsInput = parse(arguments)?;
                if input.after_sequence < 0 || input.limit == 0 || input.limit > 500 {
                    return Err(EditorError::new(
                        "INVALID_ARGUMENT",
                        "after_sequence must be nonnegative; limit must be 1..500",
                    ));
                }
                to_value(self.app.job_events_after(
                    input.after_sequence,
                    input.limit,
                    input.job_id.as_deref(),
                )?)
            }
            _ => Err(EditorError::new(
                "NOT_FOUND",
                format!("Unknown operation: {name}"),
            )),
        }
    }
}

fn parse<T: DeserializeOwned>(value: Value) -> Result<T, EditorError> {
    serde_json::from_value(value).map_err(invalid)
}
fn invalid(error: impl std::fmt::Display) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", error.to_string())
}
fn to_value(value: impl Serialize) -> Result<Value, EditorError> {
    serde_json::to_value(value).map_err(|e| EditorError::new("INTERNAL_ERROR", e.to_string()))
}

pub fn exit_code(error: &EditorError) -> i32 {
    error_policy(&error.code).exit_code
}

pub use error_policy::{ErrorPolicy, error_policy};
mod error_policy;

#[cfg(test)]
mod integration_tests {
    use super::*;
    use serde_json::json;

    fn data(response: Response) -> Value {
        assert!(response.error.is_none(), "{:?}", response.error);
        response.data.unwrap()
    }

    #[test]
    fn in_memory_bind_preflight_and_job_share_private_asset_store() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("media.bin");
        std::fs::write(&source, b"captured bytes").unwrap();
        let mut automation = Automation::open(":memory:").unwrap();
        let asset_store_root = automation.app.asset_store_root().to_path_buf();
        assert!(asset_store_root.starts_with(std::env::temp_dir()));
        assert_ne!(
            asset_store_root,
            std::env::current_dir().unwrap().join("asset-store")
        );

        let created = data(automation.call(
            "opencut_project_create",
            json!({"project":{"id":"memory","name":"Memory","sequence_id":"main","width":64,"height":48,"fps":{"numerator":10,"denominator":1}},"idempotency_key":"create"}),
        ));
        let span =
            json!({"start":{"ticks":"0","timebase":10},"duration":{"ticks":"10","timebase":10}});
        let applied = data(automation.call(
            "opencut_edit_apply",
            json!({"project_id":"memory","request":{"expected_revision":created["revision"],"idempotency_key":"bind","label":"bind","operations":[
                {"type":"asset.bind","asset":{"id":"asset","name":"Asset","kind":"video","uri":source,"duration":{"ticks":"10","timebase":10},"metadata":{}}},
                {"type":"track.create","sequence_id":"main","track":{"id":"video","name":"Video","kind":"video","enabled":true,"muted":false,"locked":false,"clips":[],"params":{}}},
                {"type":"clip.insert","sequence_id":"main","track_id":"video","clip":{"id":"clip","asset_id":"asset","timeline_range":span,"source_range":span,"params":{},"channels":[]}}
            ]}}),
        ));
        std::fs::remove_file(&source).unwrap();

        let preflight = data(automation.call(
            "opencut_render_preflight",
            json!({"project_id":"memory","revision":applied["revision"],"sequence_id":"main","container":"mp4","video_codec":"h264"}),
        ));
        assert_eq!(preflight["revision"], applied["revision"]);
        assert!(
            preflight["issues"]
                .as_array()
                .unwrap()
                .iter()
                .all(|issue| { !issue["code"].as_str().unwrap().starts_with("ASSET_") })
        );

        let job = data(automation.call(
            "opencut_job_start",
            json!({"kind":"render.project","idempotency_key":"render","input":{"project_id":"memory","revision":applied["revision"],"sequence_id":"main","container":"mp4","video_codec":"h264"}}),
        ));
        assert_eq!(job["input"]["asset_lock"]["assets"][0]["asset_id"], "asset");
        assert!(asset_store_root.exists());
        drop(automation);
        assert!(!asset_store_root.exists());
    }
}
