use crate::*;
use schemars::schema_for;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub read_only: bool,
    pub input_schema: Value,
    pub output_schema: Value,
}

#[derive(Debug, Clone, Copy)]
pub struct ToolSummary {
    pub name: &'static str,
    pub description: &'static str,
    pub read_only: bool,
}

fn spec<T: JsonSchema, R: JsonSchema>(name: &str, description: &str, read_only: bool) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        description: description.into(),
        read_only,
        input_schema: serde_json::to_value(schema_for!(T)).expect("schema is JSON"),
        output_schema: serde_json::to_value(schema_for!(Response<R>)).expect("schema is JSON"),
    }
}

mod sealed {
    pub trait Sealed {}
}
/// A registered tool owns its name and associated request/response types.
pub trait Tool: sealed::Sealed {
    type Input: serde::Serialize + serde::de::DeserializeOwned + JsonSchema;
    type Output: serde::Serialize + serde::de::DeserializeOwned + JsonSchema;
    const NAME: &'static str;
    const READ_ONLY: bool;
}
macro_rules! tools {
    ($( $marker:ident: ($input:ty, $output:ty, $name:literal, $description:literal, $read_only:literal) ),* $(,)?) => {
        $(
            pub struct $marker;
            impl sealed::Sealed for $marker {}
            impl Tool for $marker {
                type Input = $input;
                type Output = $output;
                const NAME: &'static str = $name;
                const READ_ONLY: bool = $read_only;
            }
        )*
        pub fn is_tool(name: &str) -> bool { matches!(name, $($name)|*) }
        pub fn tool_summaries() -> &'static [ToolSummary] {
            const SUMMARIES: &[ToolSummary] = &[$(ToolSummary {
                name: $name,
                description: $description,
                read_only: $read_only,
            }),*];
            SUMMARIES
        }
        pub fn tool_spec(name: &str) -> Option<ToolSpec> {
            match name {
                $($name => Some(spec::<$input, $output>($name, $description, $read_only)),)*
                _ => None,
            }
        }
        pub fn tool_specs() -> Vec<ToolSpec> {
            vec![$(spec::<$input, $output>($name, $description, $read_only)),*]
        }
    }
}
tools! {
    DatabaseInspect: (EmptyInput, crate::database::Report, "opencut_database_inspect", "Owner-only live SQLite page/freelist and database/WAL/SHM file observations. Reports whether this managed connection uses the control-admitted VFS; whole-workspace physical enforcement and repair remain unavailable. File sizes and connection-local settings are non-atomic observations, not reserved space.", true),
    DatabaseCheckpoint: (crate::database::CheckpointInput, crate::database::CheckpointReport, "opencut_database_checkpoint", "Owner-only explicit passive or truncate WAL checkpoint. Reports complete, blocked or not_applicable; timeout bounds lock waiting, not filesystem IO. Does not delete history, vacuum, terminate readers or guarantee free space. Repeat only explicitly; results are live observations, not durable idempotency receipts.", false),
    StorageBudgetInspect: (EmptyInput, budget::Report, "opencut_storage_budget_inspect", "Owner-only read-only admission policy, payload observation and durable reservations. Optional control section reports a separate database limit and observed files, but write interception is not yet enabled. No files created or recovery performed; volume space is observational and all workspace writers are not covered.", true),
    StorageBudgetConfigure: (budget::Configure, budget::PolicyChange, "opencut_storage_budget_configure", "Owner-only persist payload and optional independent database-control limits with expected revision and stable idempotency key. Omitted control limit preserves it. Lower limits affect new reservations and growth; already admitted writes may finish. Does not delete data or change upload source quotas.", false),
    StorageBudgetRecover: (EmptyInput, budget::Report, "opencut_storage_budget_recover", "Owner-only reconcile crashed payload and control reservations using OS lifetime locks, then observe current managed bytes. Never expires a live writer by clock or deletes files. SQLite write interception and whole-workspace enforcement are not yet enabled.", false),
    CasGcQuery: (gc::Query, gc::Page, "opencut_cas_gc_query", "Owner-only bounded discovery of durable CAS collection plans. Defaults to unfinished plans. Limit bounds examined rows, so an empty filtered page may still have next_cursor. Forward cursor is bound to the filter and a stored plan in this workspace; plan states are observed live. Query never executes collection.", true),
    CasGcPlan: (gc::PlanInput, gc::Plan, "opencut_cas_gc_plan", "Owner-only: persist a reviewed collection plan for unreferenced canonical CAS objects and abandoned capture temporaries. Protects all retained revisions, accepted jobs and project media. No file deletion. Idempotent key; cursor enumerates live namespace. Does not reclaim artifacts or impose quotas.", false),
    CasGcInspect: (gc::PlanId, gc::Plan, "opencut_cas_gc_inspect", "Owner-only: inspect a durable CAS collection plan and per-object outcomes. Removing means unresolved; explicitly retry execute. Does not change the plan or files.", true),
    CasGcExecute: (gc::ExecuteInput, gc::Plan, "opencut_cas_gc_execute", "Owner-only: explicitly execute the exact reviewed plan hash. Rechecks all references under the CAS publication gate. Newly referenced or changed objects are retained. Durable receipts support crash recovery; retry the same plan, never infer reclaimed disk bytes from logical sizes.", false),
    StorageInspect: (storage::InspectInput, storage::UsageReport, "opencut_storage_inspect", "Owner-only bounded native storage observation. Reports managed categories, unique inode bytes, partial scans and volume availability without exposing paths. Live scan, not an atomic snapshot, quota reservation or garbage-collection plan. Never deletes or repairs files.", true),
    UploadCreate: (uploads::CreateUpload, uploads::UploadView, "opencut_upload_create", "Create an idempotent project upload session. Requires read+edit+run_jobs. Send raw chunks through the authenticated HTTP data endpoint advertised by capabilities; never include media bytes or host paths in tool JSON.", false),
    UploadInspect: (uploads::UploadInput, uploads::UploadView, "opencut_upload_inspect", "Read upload state and confirmed next_offset before resuming a transfer. Requires project read; does not create or repair files.", true),
    UploadAbort: (uploads::UploadInput, uploads::UploadView, "opencut_upload_abort", "Abort a receiving upload. Requires read+edit+run_jobs. Sealed uploads have accepted jobs; cancel those jobs explicitly instead.", false),
    UploadRetention: (uploads::UploadInput, uploads::UploadRetentionView, "opencut_upload_retention", "Read this upload's retained staging reservation and current release eligibility. Requires project read. Eligibility is observational; release rechecks terminal state and permissions.", true),
    UploadRelease: (uploads::UploadInput, uploads::UploadRetentionView, "opencut_upload_release", "Release private chunks only for an aborted upload or terminal import job. Requires read+edit+run_jobs; queued/running jobs reject. Does not delete the imported CAS resource or project history. Retry pending cleanup; quota is freed only after durable deletion.", false),
    UploadQuota: (ProjectInput, uploads::UploadQuotaView, "opencut_upload_quota", "Read this project's logical upload staging reservations and configured limits. Requires project read; never reveals another project's usage. Does not measure CAS, artifact or filesystem disk usage.", true),
    UploadQuery: (uploads::UploadQuery, uploads::UploadPage, "opencut_upload_query", "Discover project uploads and their retention/cleanup eligibility with bounded cursor pagination. Defaults to retained reservations; set retained_only=false for history. Cursor fixes creation upper bound and filter; status remains live. Requires project read; query does not clean files.", true),
    UploadSeal: (uploads::UploadInput, uploads::UploadView, "opencut_upload_seal", "Atomically seal a complete upload and accept one durable media.import job. Retrying returns the same job_id. Does not edit the project. Read the job extended_result, then bind its project media asset explicitly.", false),
    ProjectMediaInspect: (uploads::ProjectMediaInput, uploads::MediaImportJobResult, "opencut_project_media_inspect", "Read a verified imported resource belonging to this project. Opaque references do not grant access to another project's media or global CAS hashes.", true),
    ProjectMediaQuery: (uploads::ProjectMediaQuery, uploads::ProjectMediaPage, "opencut_project_media_query", "Read a bounded page of this project's imported media. Cursor fixes first-page membership, is bound to stored project anchors, and is not a capability.", true),
    AccessInspect: (EmptyInput, access::AccessView, "opencut_access_inspect", "Inspect the authenticated principal and its current project grants. Does not reveal credentials or other principals.", true),
    PluginPublicationQuery: (crate::plugin_publication::Query, crate::plugin_publication::Page, "opencut_plugin_publication_query", "Owner-only plugin package and permission publication provenance since schema 7. Independent from media publication history. Prepared is not success; observed phases record locked reconciliation without asserting historical causality.", true),
    PluginPublicationReconcile: (crate::plugin_publication::ReconcileInput, crate::plugin_publication::ReconcileResult, "opencut_plugin_publication_reconcile", "Owner-only: lock a plugin version, observe the package or grants digest, and resolve pending publication records. Does not rewrite files or grant permissions.", false),
    MaintenanceQuery: (maintenance::MaintenanceQuery, maintenance::MaintenancePage, "opencut_maintenance_query", "Owner-only append-only native publication provenance since schema 6. Prepared is not success. Completed confirms normal publication; observed_* records locked reconciliation without claiming historical causality. Separate decimal sequence space.", true),
    MaintenanceReconcile: (maintenance::ReconcileInput, maintenance::ReconcileResult, "opencut_maintenance_reconcile", "Owner-only: lock a media object, observe its actual digest and append resolutions for pending publication records. Does not rewrite media bytes or metadata. Waits for cooperating writers; never classifies their live publication while unlocked.", false),
    AccessAuditQuery: (access::AccessAuditQuery, access::AccessAuditPage, "opencut_access_audit_query", "Owner-only administration history since schema 5: committed policies and credential lifecycle, without secrets or digests. Paginate with decimal last_sequence; pages are also bounded by a 4 MB record budget. Separate sequence space from project/job audit.", true),
    ResourceRead: (context::ResourceReadInput, context::ResourceView, "opencut_resource_read", "Read a published OpenCut resource URI through the shared application authority. Includes immutable revision context, current summaries, safe task views, artifact descriptions and schemas. Does not execute edits or return arbitrary files.", true),
    EventChanges: (event_stream::EventChangesInput, event_stream::EventChangesPage, "opencut_event_changes", "Read committed project or public task events with per-event cursors bound to stream, filter and stored event identity. Observing never cancels a task. Resume with the last fully processed cursor.", true),
    JobView: (jobs::JobInput, job_views::JobView, "opencut_job_view", "Read the public task view with typed results and content-bound artifact references. Excludes execution input, native paths, leases and raw diagnostics. Artifact availability is checked at download, not inferred from historical task success.", true),
    JobQuery: (job_views::JobQueryInput, job_views::JobPage, "opencut_job_query", "Read a bounded page of public task views. Cursor fixes membership at the first page; states remain current observations. Do not reuse a cursor from another workspace.", true),
    JobChanges: (job_views::JobChangesInput, job_views::JobChangesPage, "opencut_job_changes", "Read durable public task changes using an opaque cursor bound to the event stream and job filter. No worker payload or lease details are exposed.", true),
    JobSubmit: (jobs::JobStartInput, job_views::JobView, "opencut_job_submit", "Accept a durable task and return its public view. Same idempotency scope as legacy job_start. Native source-only v1 input locations are still host-local; project inputs bind an immutable revision.", false),
    JobRequestCancel: (jobs::JobInput, job_views::JobView, "opencut_job_request_cancel", "Explicitly request durable task cancellation and return the public view. Stopping a client wait is a separate action. Terminal results remain terminal.", false),

    MediaInspect: (media::InspectMediaInput, media::CatalogEntry, "opencut_media_inspect", "Inspect uploaded media metadata and its compare-and-repair token. Does not probe or change project revisions.", true),
    MediaRepair: (media::RepairMediaInput, media::CatalogEntry, "opencut_media_repair", "Repair uploaded media metadata from verified content bytes. Requires the metadata_hash observed by media_inspect (null for missing metadata). Does not modify historical projects or media bytes.", false),
    ProjectHistory: (HistoryInput, crate::RevisionPage, "opencut_project_history", "Read immutable revision ancestry newest first. Pass next_cursor as from_revision to continue without page shifts after new edits.", true),
    RevisionInspect: (RevisionInput, opencut_contracts::ProjectSnapshot, "opencut_revision_inspect", "Read a specific immutable project revision.", true),
    RevisionRestore: (RestoreInput, opencut_contracts::TransactionResult, "opencut_revision_restore", "Create a new revision with the contents of a prior revision. Restores the entire project, preserves history and render snapshots, requires expected_revision and an idempotency key.", false),
    AuditQuery: (audit::AuditQuery, audit::AuditPage, "opencut_audit_query", "Read committed mutation provenance since database schema 4. Does not reconstruct unknown historical actors; sequence positions are decimal strings.", true),
    Capabilities: (EmptyInput, capabilities::Capabilities, "opencut_capabilities", "Read implemented capabilities and runtime limits.", true),
    ProjectList: (EmptyInput, Vec<opencut_contracts::ProjectSnapshot>, "opencut_project_list", "List persisted projects and their current revisions.", true),
    ProjectCreate: (ProjectCreateInput, opencut_contracts::ProjectSnapshot, "opencut_project_create", "Create a persistent project. Reuse the same idempotency key on retry.", false),
    ProjectInspect: (ProjectInput, opencut_contracts::ProjectSnapshot, "opencut_project_inspect", "Read the authoritative project snapshot and revision.", true),
    EditPlan: (EditInput, opencut_contracts::EditPlan, "opencut_edit_plan", "Validate an ordered edit transaction against its expected revision without changing the project.", true),
    EditApply: (EditInput, opencut_contracts::TransactionResult, "opencut_edit_apply", "Atomically apply editing operations. Requires the read revision and a stable idempotency key.", false),
    EventsRead: (EventsInput, Vec<crate::StoredEvent>, "opencut_events_read", "Read committed events after a sequence number (maximum 500).", true),
    JobEvents: (JobEventsInput, Vec<crate::JobEvent>, "opencut_job_events", "Read durable job lifecycle and progress events after a sequence number, optionally filtered by job ID (maximum 500).", true),
    JobStart: (jobs::JobStartInput, crate::JobRecord, "opencut_job_start", "Accept a durable media or fixed-revision render job. Outputs are stored in the workspace, not arbitrary requested paths. A worker must be running.", false),
    RenderPreflight: (render::RenderPreflightInput, render::ProjectPreflightReport, "opencut_render_preflight", "Check whether a fixed project revision and sequence can be rendered, returning structured issues without creating a job or output.", true),
    JobGet: (jobs::JobInput, crate::JobRecord, "opencut_job_get", "Read durable task state, result and failure details. A failed job is a successful query result.", true),
    JobList: (EmptyInput, Vec<crate::JobRecord>, "opencut_job_list", "List persisted jobs and their current state.", true),
    JobCancel: (jobs::JobInput, crate::JobRecord, "opencut_job_cancel", "Request task cancellation. Already completed results remain completed.", false),
    PluginList: (EmptyInput, Vec<plugins::InstalledPlugin>, "opencut_plugin_list", "List installed local plugins and their exact package digests and grants.", true),
    PluginInspect: (plugins::PluginInput, plugins::InstalledPlugin, "opencut_plugin_inspect", "Inspect a specific installed plugin version and manifest.", true),
    PluginInstall: (plugins::InstallInput, plugins::InstalledPlugin, "opencut_plugin_install", "Install a local WASM component package with explicitly supplied permission grants.", false),
    DenialQuery: (denials::DenialQuery, denials::DenialPage, "opencut_denial_query", "Owner-only bounded authorization denial audit, including anonymous authentication failures. Filter by server request, verified principal or credential. At most 10000 records retained, 200 per page; stale cursors expire. Never includes tokens, request bodies or original URLs.", true),
    ProjectPluginGrant: (project_plugins::SetGrant, project_plugins::Grant, "opencut_project_plugin_grant", "Owner-only compare-and-set project plugin policy, pinned to package SHA and named contributions. Revision 0 creates. Idempotent receipt; does not change project head or install packages.", false),
    ProjectPluginList: (ProjectInput, Vec<project_plugins::Grant>, "opencut_project_plugin_list", "Read this project's plugin policies (maximum 64, including disabled). Grants are not installation availability; inspect a named contribution for its current contract.", true),
    ProjectPluginInspect: (project_plugins::InspectInput, project_plugins::OperationView, "opencut_project_plugin_inspect", "Discover a granted project's contribution with inline JSON schemas and effective host limits, without native paths. Requires project read, enabled exact-package policy and global plugin permissions.", true),
    ProjectPluginPlan: (plugins::PlanInput, project_plugins::PlanView, "opencut_project_plugin_plan", "Propose validated operations against a fixed revision using the granted exact package. Requires read, edit and run_jobs. Rechecks policy before execution and return. Does not commit; explicitly apply with the same expected revision and a new idempotency key.", true),
    PluginPlan: (plugins::PlanInput, plugins::PlanInvocation, "opencut_plugin_plan", "Run a sandboxed plugin against an immutable project revision. Returns validated operations and a plan; does not modify the project. Apply the operations with that same expected revision.", true),
}
