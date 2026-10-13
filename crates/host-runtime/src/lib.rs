//! Shared typed automation entry points. Transports must not implement editing rules.
pub use opencut_api_contracts::audio_preview_v4 as audio_preview_contracts;
pub use opencut_api_contracts::preview;
pub use opencut_api_contracts::{context, event_stream};
pub use opencut_automation::capabilities;
pub use opencut_editor_application::access::Caller;
pub use opencut_editor_application::audit::{CallSource, ExecutionContext};
pub mod audio_preview_v4;
mod browser_runtime;
mod classic_audio_runtime;
mod classic_visual_runtime;
mod font_import_v1;
pub mod denials;
mod source_art_runtime;
mod source_art_v1;
pub use opencut_editor_application::uploads;
use std::path::Path;
use uploads::{ImportStore, UploadRetentionStore};

use opencut_contracts::EditorError;
use opencut_native_services::{media, plugins, render};
use opencut_sqlite_store::EditorApplication;
use opencut_sqlite_store::jobs;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

pub use opencut_api_contracts::domains;
pub use opencut_api_contracts::tools::{ToolSpec, tool_spec, tool_specs};
pub use opencut_api_contracts::{
    EditInput, EmptyInput, EventsInput, HistoryInput, JobEventsInput, ProjectCreateInput,
    ProjectInput, Response, RestoreInput, RevisionInput,
};

pub use opencut_automation::response;

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
        let context = ExecutionContext::for_caller(self.source, self.caller.clone());
        opencut_automation::dispatch(self, context, name, arguments)
    }

    /// Native data-plane entry; never registered as a JSON/base64 control tool.
    pub fn put_upload_chunk(&mut self, input: &uploads::ChunkDescriptor, bytes: &[u8]) -> Response {
        self.put_upload_bytes(input, bytes, false)
    }
    pub fn put_stream_upload_chunk(
        &mut self,
        input: &uploads::ChunkDescriptor,
        bytes: &[u8],
    ) -> Response {
        self.put_upload_bytes(input, bytes, true)
    }
    fn put_upload_bytes(
        &mut self,
        input: &uploads::ChunkDescriptor,
        bytes: &[u8],
        streaming: bool,
    ) -> Response {
        let context = ExecutionContext::for_caller(self.source, self.caller.clone());
        let request_id = context.request_id().to_owned();
        self.app.set_execution_context(context.clone());
        let root = self
            .app
            .asset_store_root()
            .parent()
            .expect("asset store parent")
            .join("uploads");
        let result = (|| {
            if !streaming {
                uploads::UploadStore::inspect_upload(&self.app, &input.upload_id)?;
            }
            let view = opencut_native_services::uploads::put(&root, &mut self.app, input, bytes)?;
            if streaming {
                to_value(view.stream())
            } else {
                to_value(view.legacy()?)
            }
        })()
        .map_err(uploads::public_error);
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
    fn analyze_classic(
        &self,
        input: &opencut_api_contracts::classic_import::ClassicAnalyze,
    ) -> Result<opencut_api_contracts::classic_import::ClassicAnalysis, EditorError> {
        self.analyze_classic_source(input, false)
    }
    fn analyze_classic_source(
        &self,
        input: &opencut_api_contracts::classic_import::ClassicAnalyze,
        recovery: bool,
    ) -> Result<opencut_api_contracts::classic_import::ClassicAnalysis, EditorError> {
        if input.media_mappings.len() > 256 {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Classic import supports at most 256 media mappings",
            ));
        }
        let mut assets = std::collections::BTreeMap::new();
        for mapping in &input.media_mappings {
            let source = self
                .app
                .snapshot_at(&mapping.source_project_id, &mapping.source_revision)?;
            let asset = source
                .project
                .assets
                .iter()
                .find(|a| a.id == mapping.asset_id)
                .ok_or_else(|| {
                    EditorError::new("NOT_FOUND", "Mapped Classic media asset was not found")
                })?;
            if asset.version.is_none() {
                return Err(EditorError::new(
                    "ASSET_LOCK_REQUIRED",
                    "Classic mappings require immutable imported media versions",
                ));
            }
            if assets
                .insert(mapping.classic_media_id.clone(), asset.clone())
                .is_some()
            {
                return Err(EditorError::new(
                    "INVALID_ARGUMENT",
                    "Duplicate Classic media mapping",
                ));
            }
        }
        opencut_native_services::media::verify_asset_versions(
            self.app.asset_store_root(),
            &assets.values().cloned().collect::<Vec<_>>(),
        )?;
        Ok(if recovery {
            opencut_editor_application::classic_import::analyze_recovery(input, &assets)
        } else {
            opencut_editor_application::classic_import::analyze(input, &assets)
        })
    }

    pub fn audio_preview_stream(
        &mut self,
        input: opencut_api_contracts::audio_preview_v4::AudioPreviewStreamV4,
    ) -> Result<audio_preview_v4::AudioPreviewStream, EditorError> {
        audio_preview_v4::open_stream(self, input)
    }
    pub fn authorize_audio_preview(&mut self, session_id: &str) -> Result<(), EditorError> {
        audio_preview_v4::authorize_stream(self, session_id)
    }

    pub fn authorize_preview(&mut self, session: &str, run: bool) -> Result<(), EditorError> {
        self.app.set_execution_context(ExecutionContext::for_caller(
            self.source,
            self.caller.clone(),
        ));
        self.app.access_view()?;
        let (project, revision) =
            opencut_native_services::preview::target(&self.database, session)?;
        if opencut_native_services::preview::is_v3(&self.database, session)? {
            if run {
                self.app.execution_snapshot_v3(&project, &revision)?;
            } else {
                self.app.inspect_v3_project(
                    &opencut_contracts::project_v3_ops::InspectV3Project {
                        project_id: project,
                        revision: Some(revision),
                    },
                )?;
            }
        } else if run {
            opencut_editor_application::access::ProjectExecutionRead::execution_snapshot(
                &self.app, &project, &revision,
            )?;
        } else {
            self.app.snapshot_at(&project, &revision)?;
        }
        Ok(())
    }
    pub fn preview_stream(
        &mut self,
        input: opencut_api_contracts::preview::StreamInput,
    ) -> Result<transport_io::PreviewStream, EditorError> {
        self.authorize_preview(&input.session_id, true)?;
        opencut_native_services::preview::stream(
            &self.database,
            self.caller.principal_id(),
            &input.session_id,
            &input.generation,
            &input.owner_id,
            &input.lease_id,
        )
        .map(transport_io::PreviewStream)
    }

    fn execute(&mut self, name: &str, arguments: Value) -> Result<Value, EditorError> {
        match name {
            "opencut_font_recovery_v1_stage" => to_value(self.app.stage_font_recovery_v1(&parse(arguments)?)?),
            "opencut_font_recovery_v1_inspect" => to_value(self.app.inspect_font_recovery_v1(&parse(arguments)?)?),
            "opencut_font_recovery_v1_ack" => to_value(self.app.ack_font_recovery_v1(&parse(arguments)?)?),
            "opencut_font_v1_catalog" => {
                let _: EmptyInput = parse(arguments)?;
                to_value(font_import_v1::catalog())
            }
            "opencut_font_v1_import" => {
                to_value(font_import_v1::import(self, &parse(arguments)?)?)
            }
            "opencut_text_v1_plan" => {
                to_value(opencut_editor_application::text_authoring_v1::plan(&self.app, &parse(arguments)?)?)
            }
            "opencut_media_import_plan_v1" => {
                let input: opencut_contracts::media_import::MediaImportRequestV1 =
                    parse(arguments)?;
                opencut_editor_application::media_import::validate(&input)?;
                let snapshot = self.app.inspect_v3_project(
                    &opencut_contracts::project_v3_ops::InspectV3Project {
                        project_id: input.project_id.clone(),
                        revision: Some(input.expected_revision.clone()),
                    },
                )?;
                let assets = input
                    .media
                    .iter()
                    .map(|binding| {
                        self.app
                            .project_media(
                                &opencut_editor_application::uploads::ProjectMediaInput {
                                    project_id: input.project_id.clone(),
                                    media_id: binding.media_id.clone(),
                                },
                            )
                            .map(|resource| resource.asset)
                    })
                    .collect::<Result<Vec<_>, EditorError>>()?;
                let result =
                    opencut_editor_application::media_import::plan(&snapshot, &input, &assets)?;
                self.app.plan_v3_project(&result.transaction)?;
                to_value(result)
            }
            "opencut_edit_classic_plan_v1" => {
                let input: opencut_contracts::timeline_edit::ClassicEditRequestV1 =
                    parse(arguments)?;
                let snapshot = self.app.inspect_v3_project(
                    &opencut_contracts::project_v3_ops::InspectV3Project {
                        project_id: input.project_id.clone(),
                        revision: Some(input.expected_revision.clone()),
                    },
                )?;
                let result =
                    opencut_editor_application::timeline_edit::classic::plan(&snapshot, &input)?;
                self.app.plan_v3_project(&result.plan.transaction)?;
                to_value(result)
            }
            "opencut_edit_timeline_plan" => {
                let input: opencut_contracts::timeline_edit::TimelineEditRequest =
                    parse(arguments)?;
                let snapshot = self.app.inspect_v3_project(
                    &opencut_contracts::project_v3_ops::InspectV3Project {
                        project_id: input.project_id.clone(),
                        revision: Some(input.expected_revision.clone()),
                    },
                )?;
                let plan = opencut_editor_application::timeline_edit::plan(&snapshot, &input)?;
                // Recheck edit authority and the current head through the same
                // application/storage boundary used by all visual transactions.
                self.app.plan_v3_project(&plan.transaction)?;
                to_value(plan)
            }
            "opencut_job_execution_receipt_v1" => {
                let input: opencut_api_contracts::execution_receipt::InspectExecutionV1 =
                    parse(arguments)?;
                let job = self.app.artifact_job(&input.job_id)?;
                to_value(worker::receipts::inspect(&self.database, &job)?)
            }
            "opencut_project_v4_audio_capability" => {
                let _: EmptyInput = parse(arguments)?;
                to_value(classic_audio_runtime::capability())
            }
            "opencut_project_v4_audio_range_capability" => {
                let _: EmptyInput = parse(arguments)?;
                to_value(classic_audio_runtime::range_capability())
            }
            "opencut_job_v4_audio_range_wav" => {
                let result =
                    opencut_sqlite_store::sqlite_project_v4_audio_jobs::start_audio_range_wav(
                        &mut self.app,
                        parse(arguments)?,
                    )
                    .map_err(opencut_editor_application::job_views::public_error)?;
                to_value(opencut_editor_application::job_views::project(result))
            }
            "opencut_job_v4_audio_wav" => {
                let result = opencut_sqlite_store::sqlite_project_v4_audio_jobs::start_audio_wav(
                    &mut self.app,
                    parse(arguments)?,
                )
                .map_err(opencut_editor_application::job_views::public_error)?;
                to_value(opencut_editor_application::job_views::project(result))
            }
            "opencut_project_v4_text_frame_capability" => {
                let _: EmptyInput = parse(arguments)?;
                to_value(classic_visual_runtime::text_capability())
            }
            "opencut_job_v4_text_frame_png" => {
                let result = opencut_sqlite_store::sqlite_project_v4_text_frame_jobs::start_text_frame_png(
                    &mut self.app, parse(arguments)?,
                ).map_err(opencut_editor_application::job_views::public_error)?;
                to_value(opencut_editor_application::job_views::project(result))
            }
            "opencut_project_v4_frame_capability" => {
                let _: EmptyInput = parse(arguments)?;
                to_value(classic_visual_runtime::capability())
            }
            "opencut_job_v4_frame_png" => {
                let result = opencut_sqlite_store::sqlite_project_v4_frame_jobs::start_frame_png(
                    &mut self.app,
                    parse(arguments)?,
                )
                .map_err(opencut_editor_application::job_views::public_error)?;
                to_value(opencut_editor_application::job_views::project(result))
            }
            "opencut_job_v3_render" | "opencut_job_v3_frame" | "opencut_job_v3_range" => {
                use opencut_sqlite_store::sqlite_project_v3_jobs as v3_jobs;
                let result = match name {
                    "opencut_job_v3_render" => {
                        v3_jobs::start_v3_render(&mut self.app, parse(arguments)?)
                    }
                    "opencut_job_v3_frame" => {
                        v3_jobs::start_v3_frame(&mut self.app, parse(arguments)?)
                    }
                    _ => v3_jobs::start_v3_range(&mut self.app, parse(arguments)?),
                }
                .map_err(opencut_editor_application::job_views::public_error)?;
                to_value(opencut_editor_application::job_views::project(result))
            }
            "opencut_project_edit_plan" => {
                to_value(self.app.plan_project_edit(&parse(arguments)?)?)
            }
            "opencut_project_edit_apply" => {
                to_value(self.app.apply_project_edit(&parse(arguments)?)?)
            }
            "opencut_project_v4_list" => to_value(self.app.list_v4_projects(&parse(arguments)?)?),
            "opencut_project_v4_create" => {
                let snapshot = self.app.create_v4_project(&parse(arguments)?)?;
                to_value(opencut_editor_application::project_v4::public::project(
                    &snapshot,
                ))
            }
            "opencut_project_v4_promote" => {
                to_value(self.app.promote_v4_project(&parse(arguments)?)?)
            }
            "opencut_project_v4_inspect" => {
                let snapshot = self.app.inspect_v4_project(&parse(arguments)?)?;
                to_value(opencut_editor_application::project_v4::public::project(
                    &snapshot,
                ))
            }
            "opencut_project_v4_media_import_plan" => {
                to_value(self.app.plan_v4_media_import(&parse(arguments)?)?)
            }
            "opencut_media_workspace_plan_v4" => {
                to_value(self.app.plan_v4_media_workspace(&parse(arguments)?)?)
            }
            "opencut_project_v4_audio_gain_plan" => {
                to_value(self.app.plan_v4_audio_gain(&parse(arguments)?)?)
            }
            "opencut_edit_timeline_plan_v4" => {
                to_value(self.app.plan_v4_timeline(&parse(arguments)?)?)
            }
            "opencut_timeline_v4_copy" => to_value(
                opencut_editor_application::timeline_clipboard_v4::copy_selection(
                    &self.app,
                    &parse(arguments)?,
                )?,
            ),
            "opencut_timeline_v4_paste_plan" => to_value(
                opencut_editor_application::timeline_clipboard_v4::plan_paste(
                    &self.app,
                    &parse(arguments)?,
                )?,
            ),
            "opencut_edit_recovery_v1_stage" => {
                to_value(self.app.stage_edit_recovery_v1(&parse(arguments)?)?)
            }
            "opencut_edit_recovery_v1_read" => {
                to_value(self.app.read_edit_recovery_v1(&parse(arguments)?)?)
            }
            "opencut_edit_recovery_v1_inspect" => to_value(
                opencut_contracts::edit_recovery_v1::EditRecoveryInspectionV1 {
                    schema_version: 1,
                    record: self.app.read_edit_recovery_v1(&parse(arguments)?)?,
                },
            ),
            "opencut_edit_recovery_v1_ack" => {
                to_value(self.app.ack_edit_recovery_v1(&parse(arguments)?)?)
            }
            "opencut_edit_recovery_v2_stage" => {
                to_value(self.app.stage_edit_recovery_v2(&parse(arguments)?)?)
            }
            "opencut_edit_recovery_v2_inspect" => {
                to_value(self.app.inspect_edit_recovery_v2(&parse(arguments)?)?)
            }
            "opencut_edit_recovery_v2_ack" => {
                to_value(self.app.ack_edit_recovery_v2(&parse(arguments)?)?)
            }
            "opencut_project_v4_source_audio_plan" => {
                to_value(self.app.plan_v4_source_audio(&parse(arguments)?)?)
            }
            "opencut_project_v4_resource_inspect" => {
                to_value(self.app.inspect_v4_resource(&parse(arguments)?)?)
            }
            "opencut_project_v4_copy" => {
                let snapshot = self.app.copy_v4_project(&parse(arguments)?)?;
                to_value(opencut_editor_application::project_v4::public::project(
                    &snapshot,
                ))
            }
            "opencut_project_v4_restore" => {
                to_value(self.app.restore_v4_revision(&parse(arguments)?)?)
            }
            "opencut_edit_v4_plan" => {
                to_value(self.app.plan_public_v4_project(&parse(arguments)?)?)
            }
            "opencut_edit_v4_apply" => {
                to_value(self.app.apply_public_v4_project(&parse(arguments)?)?)
            }
            "opencut_edit_v4_undo_plan" => to_value(self.app.plan_v4_undo(&parse(arguments)?)?),
            "opencut_edit_v4_undo_apply" => to_value(self.app.apply_v4_undo(&parse(arguments)?)?),
            "opencut_edit_v4_redo_plan" => to_value(self.app.plan_v4_redo(&parse(arguments)?)?),
            "opencut_edit_v4_redo_apply" => to_value(self.app.apply_v4_redo(&parse(arguments)?)?),
            "opencut_project_v4_history_navigation" => {
                to_value(self.app.v4_history_navigation(&parse(arguments)?)?)
            }
            "opencut_project_v3_list" => to_value(self.app.list_v3_projects(&parse(arguments)?)?),
            "opencut_project_v3_create" => {
                to_value(self.app.create_v3_project(&parse(arguments)?)?)
            }
            "opencut_project_v3_promote" => {
                to_value(self.app.promote_v3_project(&parse(arguments)?)?)
            }
            "opencut_project_v3_inspect" => {
                to_value(self.app.inspect_v3_project(&parse(arguments)?)?)
            }
            "opencut_edit_v3_plan" => to_value(self.app.plan_v3_project(&parse(arguments)?)?),
            "opencut_edit_v3_apply" => to_value(self.app.apply_v3_project(&parse(arguments)?)?),
            "opencut_classic_analyze" => to_value(self.analyze_classic(&parse(arguments)?)?),
            "opencut_classic_import" => {
                let input: opencut_api_contracts::classic_import::ClassicImport = parse(arguments)?;
                if let Some(receipt) = self.app.classic_import_replay(&input)? {
                    return to_value(receipt);
                }
                let analysis = self.analyze_classic(&input.source)?;
                to_value(self.app.classic_import_commit(&input, &analysis)?)
            }
            "opencut_classic_recovery_analyze_v1" => {
                to_value(self.analyze_classic_source(&parse(arguments)?, true)?)
            }
            "opencut_classic_recovery_import_v1" => {
                let input: opencut_api_contracts::classic_import::ClassicImport = parse(arguments)?;
                if let Some(receipt) = self.app.classic_recovery_replay(&input)? {
                    return to_value(receipt);
                }
                let analysis = self.analyze_classic_source(&input.source, true)?;
                to_value(self.app.classic_recovery_commit(&input, &analysis)?)
            }
            "opencut_subtitle_generate" => {
                to_value(self.app.generate_captions(&parse(arguments)?)?)
            }
            "opencut_subtitle_import" => {
                let input: opencut_api_contracts::subtitle_exchange::SubtitleImport =
                    parse(arguments)?;
                let (snapshot, visual) = match self
                    .app
                    .snapshot_at(&input.project_id, &input.expected_revision)
                {
                    Ok(snapshot) => (snapshot, false),
                    Err(error) if error.code == "UNSUPPORTED_PROJECT_VERSION" => {
                        let snapshot = self.app.inspect_v3_project(
                            &opencut_contracts::project_v3_ops::InspectV3Project {
                                project_id: input.project_id.clone(),
                                revision: Some(input.expected_revision.clone()),
                            },
                        )?;
                        (
                            opencut_contracts::ProjectSnapshot {
                                project: snapshot.project.base,
                                revision: snapshot.revision,
                            },
                            true,
                        )
                    }
                    Err(error) => return Err(error),
                };
                let request = opencut_editor_application::subtitle_exchange::import_transaction(
                    &snapshot, &input,
                )?;
                let cue_count = request.operations.len();
                let transaction = if visual {
                    self.app.apply_v3_project(
                        &opencut_contracts::project_v3_ops::ApplyV3Project {
                            project_id: input.project_id.clone(),
                            expected_revision: request.expected_revision,
                            idempotency_key: request.idempotency_key,
                            label: request.label,
                            operations: request
                                .operations
                                .into_iter()
                                .map(|operation| {
                                    opencut_contracts::project_v3_ops::VisualOperation::Base {
                                        operation,
                                    }
                                })
                                .collect(),
                        },
                    )?
                } else {
                    self.app.apply(&input.project_id, &request)?
                };
                to_value(
                    opencut_api_contracts::subtitle_exchange::SubtitleImportReceipt {
                        transaction,
                        cue_count,
                        warnings: vec![],
                    },
                )
            }
            "opencut_subtitle_export" => {
                let input: opencut_api_contracts::subtitle_exchange::SubtitleExport =
                    parse(arguments)?;
                let snapshot = match self.app.snapshot_at(&input.project_id, &input.revision) {
                    Ok(snapshot) => snapshot,
                    Err(error) if error.code == "UNSUPPORTED_PROJECT_VERSION" => {
                        let snapshot = self.app.inspect_v3_project(
                            &opencut_contracts::project_v3_ops::InspectV3Project {
                                project_id: input.project_id.clone(),
                                revision: Some(input.revision.clone()),
                            },
                        )?;
                        opencut_contracts::ProjectSnapshot {
                            project: snapshot.project.base,
                            revision: snapshot.revision,
                        }
                    }
                    Err(error) => return Err(error),
                };
                to_value(
                    opencut_editor_application::subtitle_exchange::export_subtitles(
                        &snapshot, &input,
                    )?,
                )
            }
            "opencut_audio_preview_v4_capability" => {
                let _: EmptyInput = parse(arguments)?;
                to_value(classic_audio_runtime::preview_capability())
            }
            "opencut_source_art_v1_get" => to_value(source_art_v1::get(self, parse(arguments)?)?),
            "opencut_source_art_v1_cancel" => {
                to_value(source_art_v1::cancel(self, parse(arguments)?)?)
            }
            "opencut_source_art_waveform_display_v1_get" => {
                to_value(source_art_v1::waveform_display(self, parse(arguments)?)?)
            }
            "opencut_audio_preview_v4_create" => {
                if matches!(self.source, CallSource::Cli | CallSource::McpStdio) {
                    return Err(EditorError::new(
                        "LONG_LIVED_HOST_REQUIRED",
                        "Audio preview requires a long-lived host",
                    ));
                }
                to_value(audio_preview_v4::create(self, parse(arguments)?)?)
            }
            "opencut_visual_preview_v4_frame" => {
                to_value(audio_preview_v4::picture(self, parse(arguments)?)?)
            }
            "opencut_audio_preview_v4_inspect" => {
                to_value(audio_preview_v4::inspect(self, parse(arguments)?)?)
            }
            "opencut_audio_preview_v4_events" => {
                to_value(audio_preview_v4::events(self, parse(arguments)?)?)
            }
            "opencut_audio_preview_v4_control" => {
                to_value(audio_preview_v4::control(self, parse(arguments)?)?)
            }
            "opencut_audio_preview_v4_acquire" => {
                to_value(audio_preview_v4::acquire(self, parse(arguments)?)?)
            }
            "opencut_preview_v3_create" => {
                if matches!(self.source, CallSource::Cli | CallSource::McpStdio) {
                    return Err(EditorError::new(
                        "CAPABILITY_UNAVAILABLE",
                        "Preview media requires an HTTP host; use CLI --endpoint or Streamable HTTP MCP for session control",
                    ));
                }
                let input: opencut_api_contracts::preview::Create = parse(arguments)?;
                let authorized = self
                    .app
                    .execution_snapshot_v3(&input.project_id, &input.revision)?;
                to_value(opencut_native_services::preview::create_v3(
                    &self.database,
                    self.app.asset_store_root(),
                    self.caller.principal_id(),
                    &input,
                    authorized.snapshot,
                    authorized.sources,
                    || {
                        self.app
                            .execution_snapshot_v3(&input.project_id, &input.revision)
                            .map(|_| ())
                    },
                )?)
            }
            "opencut_preview_create" => {
                if matches!(self.source, CallSource::Cli | CallSource::McpStdio) {
                    return Err(EditorError::new(
                        "CAPABILITY_UNAVAILABLE",
                        "Preview media requires an HTTP host; use CLI --endpoint or Streamable HTTP MCP for session control",
                    ));
                }
                let input: opencut_api_contracts::preview::Create = parse(arguments)?;
                let authorized =
                    opencut_editor_application::access::ProjectExecutionRead::execution_snapshot(
                        &self.app,
                        &input.project_id,
                        &input.revision,
                    )?;
                to_value(opencut_native_services::preview::create(
                    &self.database,
                    self.app.asset_store_root(),
                    self.caller.principal_id(),
                    &input,
                    authorized.snapshot,
                    authorized.sources,
                    || {
                        opencut_editor_application::access::ProjectExecutionRead::execution_snapshot(&self.app, &input.project_id, &input.revision).map(|_| ())
                    },
                )?)
            }
            "opencut_preview_acquire" => {
                let input: opencut_api_contracts::preview::Acquire = parse(arguments)?;
                self.authorize_preview(&input.session_id, true)?;
                to_value(opencut_native_services::preview::acquire(
                    &self.database,
                    self.caller.principal_id(),
                    &input,
                )?)
            }
            "opencut_preview_inspect" => {
                let input: opencut_api_contracts::preview::SessionInput = parse(arguments)?;
                self.authorize_preview(&input.session_id, false)?;
                to_value(opencut_native_services::preview::inspect(
                    &self.database,
                    &input.session_id,
                )?)
            }
            "opencut_preview_events" => {
                let input: opencut_api_contracts::preview::EventsInput = parse(arguments)?;
                self.authorize_preview(&input.session_id, false)?;
                to_value(opencut_native_services::preview::events(
                    &self.database,
                    &input,
                )?)
            }
            "opencut_preview_control" => {
                let input: opencut_api_contracts::preview::Control = parse(arguments)?;
                self.authorize_preview(&input.session_id, true)?;
                to_value(opencut_native_services::preview::control(
                    &self.database,
                    self.caller.principal_id(),
                    &input,
                )?)
            }
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
            "opencut_artifact_retention_inspect" => {
                to_value(opencut_editor_application::artifact_retention::inspect(
                    &self.app,
                    parse(arguments)?,
                )?)
            }
            "opencut_artifact_references_inspect" => {
                to_value(opencut_editor_application::artifact_retention::references(
                    &self.app,
                    parse(arguments)?,
                )?)
            }
            "opencut_artifact_gc_plan" => {
                let artifact_root = self
                    .app
                    .asset_store_root()
                    .parent()
                    .expect("workspace root")
                    .join("artifacts");
                to_value(opencut_editor_application::artifact_gc::plan(
                    &mut self.app,
                    &opencut_native_services::artifacts::ArtifactPlanner(&artifact_root),
                    parse(arguments)?,
                )?)
            }
            "opencut_artifact_gc_inspect" => to_value(
                opencut_editor_application::artifact_gc::inspect(&self.app, parse(arguments)?)?,
            ),
            "opencut_artifact_gc_query" => to_value(
                opencut_editor_application::artifact_gc::query(&self.app, parse(arguments)?)?,
            ),
            "opencut_artifact_gc_execute" => {
                let artifact_root = self
                    .app
                    .asset_store_root()
                    .parent()
                    .expect("workspace root")
                    .join("artifacts");
                to_value(opencut_editor_application::artifact_gc::execute(
                    &mut self.app,
                    &opencut_native_services::artifacts::ArtifactPlanner(&artifact_root),
                    parse(arguments)?,
                )?)
            }
            "opencut_artifact_gc_execution_inspect" => {
                to_value(opencut_editor_application::artifact_gc::execution_inspect(
                    &self.app,
                    parse(arguments)?,
                )?)
            }
            "opencut_artifact_retention_release" => {
                to_value(opencut_editor_application::artifact_retention::release(
                    &mut self.app,
                    parse(arguments)?,
                )?)
            }
            "opencut_storage_inspect" => to_value(opencut_editor_application::storage::inspect(
                &self.caller,
                &opencut_native_services::storage::NativeStorage(&self.database),
                parse(arguments)?,
            )?),
            "opencut_upload_stream_begin" => to_value(
                uploads::StreamUploadStore::begin_upload(&mut self.app, &parse(arguments)?)
                    .map_err(uploads::public_error)?
                    .stream(),
            ),
            "opencut_upload_stream_inspect" => {
                let input: uploads::UploadInput = parse(arguments)?;
                to_value(
                    uploads::StreamUploadStore::inspect_stream_upload(&self.app, &input.upload_id)
                        .map_err(uploads::public_error)?
                        .stream(),
                )
            }
            "opencut_upload_stream_complete" => to_value(
                uploads::StreamUploadStore::complete_upload(&mut self.app, &parse(arguments)?)
                    .map_err(uploads::public_error)?
                    .stream(),
            ),
            "opencut_upload_stream_query" => to_value(
                uploads::query_stream_uploads(&self.app, parse(arguments)?)
                    .map_err(uploads::public_error)?,
            ),
            "opencut_upload_create" => to_value(
                uploads::create(&mut self.app, parse(arguments)?).map_err(uploads::public_error)?,
            ),
            "opencut_upload_inspect" => to_value(
                uploads::inspect(&self.app, parse(arguments)?).map_err(uploads::public_error)?,
            ),
            "opencut_upload_abort" | "opencut_upload_stream_abort" => {
                let input: uploads::UploadInput = parse(arguments)?;
                if name == "opencut_upload_abort" {
                    uploads::UploadStore::inspect_upload(&self.app, &input.upload_id)
                        .map_err(uploads::public_error)?;
                }
                let root = self
                    .app
                    .asset_store_root()
                    .parent()
                    .expect("asset store parent")
                    .join("uploads");
                let session = opencut_native_services::uploads::abort(&root, &mut self.app, input)
                    .map_err(uploads::public_error)?;
                if name == "opencut_upload_abort" {
                    to_value(session.legacy().map_err(uploads::public_error)?)
                } else {
                    to_value(session.stream())
                }
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
            "opencut_analysis_speaker_turns" => {
                let root = self.app.asset_store_root().to_path_buf();
                let record = opencut_sqlite_store::speaker_turns::start(
                    &mut self.app,
                    &root,
                    parse(arguments)?,
                )
                .map_err(opencut_editor_application::job_views::public_error)?;
                to_value(opencut_editor_application::job_views::project(record))
            }
            "opencut_analysis_transcribe" => {
                let asset_store_root = self.app.asset_store_root().to_path_buf();
                let record = opencut_sqlite_store::jobs::start_transcribe(
                    &mut self.app,
                    &asset_store_root,
                    parse(arguments)?,
                )
                .map_err(opencut_editor_application::job_views::public_error)?;
                to_value(opencut_editor_application::job_views::project(record))
            }
            "opencut_analysis_sequence_transcribe" => {
                let record =
                    opencut_sqlite_store::sqlite_project_v3_jobs::start_sequence_transcribe(
                        &mut self.app,
                        parse(arguments)?,
                    )
                    .map_err(opencut_editor_application::job_views::public_error)?;
                to_value(opencut_editor_application::job_views::project(record))
            }
            "opencut_analysis_query" => to_value(self.app.analysis_query(parse(arguments)?)?),
            "opencut_analysis_evidence_query" => {
                to_value(self.app.analysis_evidence_query(parse(arguments)?)?)
            }
            "opencut_analysis_silence" | "opencut_analysis_shots" | "opencut_analysis_waveform" => {
                let root = self.app.asset_store_root().to_path_buf();
                let record = match name {
                    "opencut_analysis_silence" => opencut_sqlite_store::analysis::start_silence(
                        &mut self.app,
                        &root,
                        parse(arguments)?,
                    ),
                    "opencut_analysis_shots" => opencut_sqlite_store::analysis::start_shots(
                        &mut self.app,
                        &root,
                        parse(arguments)?,
                    ),
                    _ => opencut_sqlite_store::analysis::start_waveform(
                        &mut self.app,
                        &root,
                        parse(arguments)?,
                    ),
                }
                .map_err(opencut_editor_application::job_views::public_error)?;
                to_value(opencut_editor_application::job_views::project(record))
            }
            "opencut_edit_v3_undo_plan" => to_value(self.app.plan_v3_undo(&parse(arguments)?)?),
            "opencut_edit_v3_undo_apply" => to_value(self.app.apply_v3_undo(&parse(arguments)?)?),
            "opencut_edit_v3_redo_plan" => to_value(self.app.plan_v3_redo(&parse(arguments)?)?),
            "opencut_edit_v3_redo_apply" => to_value(self.app.apply_v3_redo(&parse(arguments)?)?),
            "opencut_edit_v3_branch_create" => {
                to_value(self.app.create_v3_branch(&parse(arguments)?)?)
            }
            "opencut_edit_v3_branch_list" => {
                to_value(self.app.list_v3_branches(&parse(arguments)?)?)
            }
            "opencut_edit_v3_branch_apply" => {
                to_value(self.app.apply_v3_branch_edit(&parse(arguments)?)?)
            }
            "opencut_edit_v3_branch_compare" => {
                to_value(self.app.compare_v3_branches(&parse(arguments)?)?)
            }
            "opencut_edit_v3_branch_merge" => {
                to_value(self.app.merge_v3_branches(&parse(arguments)?)?)
            }
            "opencut_edit_undo_plan" => to_value(self.app.plan_undo(&parse(arguments)?)?),
            "opencut_edit_undo_apply" => to_value(self.app.apply_undo(&parse(arguments)?)?),
            "opencut_edit_redo_plan" => to_value(self.app.plan_redo(&parse(arguments)?)?),
            "opencut_edit_redo_apply" => to_value(self.app.apply_redo(&parse(arguments)?)?),
            "opencut_edit_branch_create" => to_value(self.app.create_branch(&parse(arguments)?)?),
            "opencut_edit_branch_list" => to_value(self.app.list_branches(&parse(arguments)?)?),
            "opencut_edit_branch_apply" => {
                to_value(self.app.apply_branch_edit(&parse(arguments)?)?)
            }
            "opencut_edit_branch_compare" => {
                to_value(self.app.compare_branches(&parse(arguments)?)?)
            }
            "opencut_edit_branch_merge" => to_value(self.app.merge_branches(&parse(arguments)?)?),
            "opencut_edit_control_get" => to_value(self.app.get_edit_control(&parse(arguments)?)?),
            "opencut_edit_control_set" => to_value(self.app.set_edit_control(&parse(arguments)?)?),
            "opencut_project_package_reproducibility_v1" => {
                self.app.package_require_owner()?;
                to_value(opencut_editor_application::reproducibility::inspect(
                    &parse(arguments)?,
                    &self.caller,
                    &opencut_native_services::reproducibility::NativeReproducibility,
                )?)
            }
            "opencut_project_v3_package_export" => {
                self.app.package_require_owner()?;
                let input: opencut_api_contracts::project_package_v3::ExportPackageV3 =
                    parse(arguments)?;
                let source = self
                    .app
                    .package_v3_export_source(&input.project_id, &input.revision)?;
                to_value(
                    opencut_native_services::project_package_v3::export_package_v3(
                        &input,
                        &source,
                        self.app.asset_store_root(),
                        &self.plugin_root,
                    )?,
                )
            }
            "opencut_project_v3_package_import" => {
                self.app.package_require_owner()?;
                let input: opencut_api_contracts::project_package_v3::ImportPackageV3 =
                    parse(arguments)?;
                if let Some(receipt) = self.app.package_v3_import_replay(&input)? {
                    return to_value(receipt);
                }
                let prepared = opencut_native_services::project_package_v3::read_package_v3(
                    &input,
                    self.app.asset_store_root(),
                )?;
                to_value(self.app.package_v3_import_commit(&input, &prepared)?)
            }
            "opencut_project_package_export" => {
                self.app.package_require_owner()?;
                let input: opencut_api_contracts::project_package::ExportPackage =
                    parse(arguments)?;
                let source = self
                    .app
                    .package_export_source(&input.project_id, &input.revision)?;
                to_value(opencut_native_services::project_package::export_package(
                    &input,
                    &source,
                    self.app.asset_store_root(),
                )?)
            }
            "opencut_project_package_import" => {
                self.app.package_require_owner()?;
                let input: opencut_api_contracts::project_package::ImportPackage =
                    parse(arguments)?;
                if let Some(receipt) = self.app.package_import_replay(&input)? {
                    return to_value(receipt);
                }
                let prepared = opencut_native_services::project_package::read_package(
                    &input,
                    self.app.asset_store_root(),
                )?;
                to_value(self.app.package_import_commit(&input, &prepared)?)
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
            "opencut_project_library" => to_value(self.app.project_library(&parse(arguments)?)?),
            "opencut_project_media_remove" => {
                to_value(self.app.remove_project_media(&parse(arguments)?)?)
            }
            "opencut_project_duplicate" => {
                to_value(self.app.duplicate_projects(&parse(arguments)?)?)
            }
            "opencut_project_delete" => {
                let root = self
                    .app
                    .asset_store_root()
                    .parent()
                    .expect("asset store parent")
                    .join("uploads");
                to_value(opencut_native_services::uploads::delete_projects(
                    &root,
                    &mut self.app,
                    &parse(arguments)?,
                )?)
            }
            "opencut_project_history_navigation" => {
                to_value(self.app.history_navigation(&parse(arguments)?)?)
            }
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
            "opencut_plugin_v2_preset_v3_plan" => {
                let workspace = self
                    .app
                    .asset_store_root()
                    .parent()
                    .expect("managed workspace");
                let catalog = opencut_native_services::plugin_v2::PresetPackages::new(workspace);
                to_value(opencut_editor_application::plugin_v3::plan(
                    &self.app,
                    &catalog,
                    parse(arguments)?,
                )?)
            }
            "opencut_plugin_v2_install"
            | "opencut_plugin_v2_install_pinned"
            | "opencut_plugin_v2_list"
            | "opencut_plugin_v2_grant"
            | "opencut_plugin_v2_grants"
            | "opencut_plugin_v2_inspect"
            | "opencut_plugin_v2_preset_plan" => {
                use opencut_editor_application::plugin_v2::{self as presets, PresetGrantStore};
                let workspace = self
                    .app
                    .asset_store_root()
                    .parent()
                    .expect("managed workspace");
                let catalog = opencut_native_services::plugin_v2::PresetPackages::new(workspace);
                match name {
                    "opencut_plugin_v2_install" => {
                        to_value(presets::install(&catalog, &parse(arguments)?)?)
                    }
                    "opencut_plugin_v2_install_pinned" => {
                        to_value(presets::install_pinned(&catalog, &parse(arguments)?)?)
                    }
                    "opencut_plugin_v2_list" => {
                        let _: EmptyInput = parse(arguments)?;
                        to_value(presets::list(&catalog)?)
                    }
                    "opencut_plugin_v2_grant" => to_value(presets::set_grant(
                        &mut self.app,
                        &catalog,
                        &parse(arguments)?,
                    )?),
                    "opencut_plugin_v2_grants" => {
                        let input: ProjectInput = parse(arguments)?;
                        to_value(self.app.preset_grants(&input.project_id)?)
                    }
                    "opencut_plugin_v2_inspect" => {
                        to_value(presets::inspect(&self.app, &catalog, parse(arguments)?)?)
                    }
                    _ => to_value(presets::plan(&self.app, &catalog, parse(arguments)?)?),
                }
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
            "opencut_plugin_install_pinned" => to_value(plugins::install_pinned_observed(
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
            "opencut_render_v3_preflight" => {
                to_value(opencut_native_services::render_v3::preflight(
                    &self.app,
                    self.app.asset_store_root(),
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

impl opencut_editor_application::tool_execution::ToolExecution for Automation {
    fn begin(&mut self, context: ExecutionContext) -> Result<(), EditorError> {
        self.source = context.actor().source;
        self.caller = context.caller().clone();
        self.app.set_execution_context(context);
        self.app.access_view().map(|_| ())
    }
    fn execute_tool(
        &mut self,
        request: opencut_api_contracts::tools::ToolRequest,
    ) -> Result<Value, EditorError> {
        let (name, arguments) = request.into_parts()?;
        self.execute(name, arguments)
    }
    fn record_tool_denial(
        &self,
        denial: &opencut_editor_application::denials::Denial<'_>,
    ) -> Result<(), EditorError> {
        opencut_editor_application::denials::DenialStore::record_denial(&self.app, denial)
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

pub use opencut_automation::{ErrorPolicy, error_policy};

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

pub mod worker;

pub mod transport_io;
pub mod workspace;

#[cfg(test)]
mod project_v4_public_tests;
