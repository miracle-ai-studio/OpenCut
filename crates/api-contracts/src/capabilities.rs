use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Capabilities {
    pub host: String,
    pub features: Features,
    pub limits: Limits,
    pub job_execution: JobExecution,
    pub rendering: Rendering,
    pub edit_operation_schema: Value,
    pub job_results: Vec<crate::job_results::JobResultDescriptor>,
    pub tools: Vec<String>,
}

/// Host discovery uses the same declarations as every generated client.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HostCapabilities {
    #[serde(default)]
    pub project: Option<ProjectCapabilities>,
    #[serde(default)]
    pub analysis_evidence_query_tool: Option<String>,
    #[serde(default)]
    pub preview: Option<crate::preview::Capabilities>,
    #[serde(flatten)]
    pub api: Capabilities,
    pub mcp: McpCapabilities,
    #[serde(default)]
    pub access: AccessCapabilities,
    #[serde(default)]
    pub uploads: Option<UploadCapabilities>,
    #[serde(default)]
    pub project_plugins: Option<crate::project_plugins::Capabilities>,
    #[serde(default)]
    pub denials: Option<crate::denials::DenialCapabilities>,
    #[serde(default)]
    pub storage: Option<crate::storage::Capabilities>,
    #[serde(default)]
    pub cas_gc: Option<crate::gc::Capabilities>,
    #[serde(default)]
    pub artifact_gc: Option<crate::artifact_gc::Capabilities>,
    #[serde(default)]
    pub storage_budget: Option<crate::budget::Capabilities>,
    #[serde(default)]
    pub database: Option<crate::database::Capabilities>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct AccessCapabilities {
    pub scoped_transports: Vec<String>,
    pub project_actions: Vec<crate::access::ProjectAction>,
    pub mcp_session_binding: Option<String>,
    pub public_event_stream_path: Option<String>,
    pub policy_management: Option<String>,
    /// Scoped preflight requires run_jobs and immutable project content.
    #[serde(default)]
    pub scoped_preflight: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Features {
    pub persistent_projects: bool,
    pub atomic_transactions: bool,
    pub optimistic_concurrency: bool,
    pub idempotent_writes: bool,
    pub edit_preview_plan: bool,
    pub durable_events: bool,
    pub multiple_branches: bool,
    pub persistent_jobs: bool,
    pub revision_history: bool,
    pub revision_restore: bool,
    pub plugin_execution: bool,
    pub timeline_rendering: bool,
    pub new_asset_bindings_versioned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Limits {
    pub event_page_size: usize,
    pub input_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobExecution {
    pub worker_required: bool,
    pub service_starts_worker: bool,
    pub managed_output: bool,
    pub durable_progress: bool,
    pub progress_fraction_scope: String,
    pub project_asset_locking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Rendering {
    pub backend: String,
    pub requires_executables: Vec<String>,
    pub fixed_revision: bool,
    pub sequential_video: bool,
    pub source_audio: bool,
    pub audio_mix: bool,
    pub static_captions: bool,
    pub constant_rate: bool,
    pub multiple_video_tracks: bool,
    pub images: bool,
    pub visual_transforms: bool,
    pub keyframes: bool,
    pub text_shaping: bool,
    pub project_frame_preview: bool,
    pub preview_frame_execution: String,
    pub composited_frame_preview: bool,
    pub composited_frame_execution: String,
    pub project_range_preview: bool,
    pub preview_range_execution: String,
    pub availability: String,
}

pub const MCP_SESSION_LIMIT: usize = 32;
pub const MCP_RESOURCE_SUBSCRIPTION_LIMIT: usize = 32;
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct McpCapabilities {
    pub protocol_versions: Vec<String>,
    pub transports: Vec<String>,
    pub http_path: String,
    pub network_scope: String,
    pub resources: bool,
    pub prompts: bool,
    pub resource_subscriptions: bool,
    pub max_sessions: usize,
    pub max_resource_subscriptions: usize,
}

pub const UPLOAD_INFLIGHT_LIMIT: usize = 16;
pub const UPLOAD_CHUNK_TIMEOUT_SECONDS: u64 = 60;
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UploadCapabilities {
    #[serde(default)]
    pub staging: Option<UploadStagingCapabilities>,
    pub chunk_method: String,
    pub chunk_path_template: String,
    pub chunk_query_fields: Vec<String>,
    pub requires_bearer: bool,
    /// Exact resource addressability;not a disk or media quota.
    pub max_source_bytes: String,
    pub max_chunk_bytes: String,
    pub max_receiving_per_project: usize,
    pub max_receiving_per_workspace: usize,
    pub max_inflight_chunks: usize,
    pub chunk_timeout_seconds: u64,
    pub sequential: bool,
    pub accept_tool: String,
    pub result_field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UploadStagingCapabilities {
    pub retention_tool: String,
    pub release_tool: String,
    pub quota_tool: String,
    pub query_tool: String,
    pub release_requires_terminal_job: bool,
    pub automatic_terminal_cleanup: bool,
    pub measures_filesystem_usage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProjectCapabilities {
    pub create_tool: String,
    pub promote_tool: String,
    pub list_tool: String,
    pub inspect_tool: String,
    pub plan_tool: String,
    pub apply_tool: String,
    pub preview_create_tool: String,
    pub preset_plan_tool: String,
    pub preflight_tool: String,
    pub undo_plan_tool: String,
    pub undo_apply_tool: String,
    pub redo_plan_tool: String,
    pub redo_apply_tool: String,
    pub branch_tools: Vec<String>,
    pub package_export_tool: String,
    pub package_import_tool: String,
    pub selective_undo_redo: bool,
    pub named_branches: bool,
    pub promotion_reversible_by_undo: bool,
}

/// Discovery and tool inputs derive from the same Rust operation declaration.
pub fn edit_operation_schema() -> Value {
    serde_json::to_value(schemars::schema_for!(
        opencut_contracts::project_public::ProjectEditOperation
    ))
    .expect("Edit schema is serializable")
}
