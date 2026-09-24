// Generated from the Rust automation registry. Do not edit.

export namespace OpencutDatabaseInspectInput {
  export interface EmptyInput {}

}

export namespace OpencutDatabaseInspectOutput {
  export interface Response {
  api_version: string;
  data?: Report | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface Report {
  connection_busy_timeout_ms: number;
  connection_wal_autocheckpoint_pages: number;
  consistency: string;
  /**
   * None means unavailable; for a persistent database an absent sidecar is "0".
   */
  database_file_bytes?: string | null;
  enforces_total_storage_limit: boolean;
  freelist_pages: string;
  journal_mode: string;
  logical_database_bytes: string;
  /**
   * This particular managed connection uses OpenCut's control-admitted VFS.
   */
  managed_vfs_enabled: boolean;
  max_page_count: string;
  page_count: string;
  page_size_bytes: number;
  persistent: boolean;
  shm_file_bytes?: string | null;
  sqlite_version: string;
  wal_file_bytes?: string | null;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutDatabaseCheckpointInput {
  export type CheckpointMode = 'passive' | 'truncate';

  export interface CheckpointInput {
  busy_timeout_ms?: number;
  mode: CheckpointMode;
}

}

export namespace OpencutDatabaseCheckpointOutput {
  export type CheckpointMode = 'passive' | 'truncate';
  export type CheckpointOutcome = 'complete' | 'blocked' | 'not_applicable';

  export interface Response {
  api_version: string;
  data?: CheckpointReport | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface CheckpointReport {
  checkpointed_frames?: number | null;
  log_frames?: number | null;
  mode: CheckpointMode;
  /**
   * File size is observed after the operation; concurrent writers may change it.
   */
  observed_wal_file_bytes?: string | null;
  outcome: CheckpointOutcome;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutStorageBudgetInspectInput {
  export interface EmptyInput {}

}

export namespace OpencutStorageBudgetInspectOutput {
  export type RepairAction = 'enter' | 'exit';
  export type CallSource = 'cli' | 'http' | 'mcp_stdio' | 'mcp_http' | 'embedded' | 'worker';
  export type RepairState = 'ready' | 'active' | 'degraded';

  export interface Response {
  api_version: string;
  data?: Report | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface Report {
  admission_headroom_bytes: string;
  control?: ControlReport | null;
  coverage: string[];
  enforces_all_workspace_writes: boolean;
  last_policy_change?: PolicyChange | null;
  media_output_limit_bytes?: string | null;
  observed_payload_bytes: string;
  policy: Policy;
  recovered_reservations: number;
  reservations: Reservation[];
  reserved_bytes: string;
  volume_available_bytes: string;
  [k: string]: any;
}
  export interface ControlReport {
  /**
   * Absent until the workspace database file name has been bound.
   */
  admission_headroom_bytes?: string | null;
  control_limit_bytes: string;
  database_name?: string | null;
  observed_control_bytes: string;
  repair?: RepairReport | null;
  reservations: Reservation[];
  reserved_control_bytes: string;
  /**
   * True only once all managed SQLite writes use the guarded VFS.
   */
  write_interception_enabled: boolean;
  [k: string]: any;
}
  export interface RepairReport {
  allocated_blocks_bytes: string;
  allocated_file_bytes: string;
  last_change?: RepairChange | null;
  revision: number;
  state: RepairState;
  target_bytes: string;
  [k: string]: any;
}
  export interface RepairChange {
  action: RepairAction;
  actor: Actor;
  input: RepairRequest;
  request_id: string;
  revision: number;
  state: RepairState;
}
  export interface Actor {
  principal_id: string;
  source: CallSource;
  worker_id?: string | null;
  [k: string]: any;
}
  export interface RepairRequest {
  expected_revision: number;
  idempotency_key: string;
}
  export interface Reservation {
  category: string;
  reservation_id: string;
  reserved_bytes: string;
  writer?: Writer | null;
  [k: string]: any;
}
  export interface Writer {
  attempt: number;
  job_id: string;
}
  export interface PolicyChange {
  actor: Actor;
  control_limit_bytes?: string | null;
  media_output_limit_bytes?: string | null;
  policy: Policy;
  request_id: string;
  [k: string]: any;
}
  export interface Policy {
  minimum_available_bytes: string;
  payload_limit_bytes: string;
  revision: number;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutStorageBudgetConfigureInput {
  export interface Configure {
  /**
   * Omission preserves the current database-control ceiling.
   */
  control_limit_bytes?: string | null;
  expected_revision: number;
  idempotency_key: string;
  /**
   * Omission preserves the current ceiling, including for older consumers.
   */
  media_output_limit_bytes?: string | null;
  minimum_available_bytes: string;
  payload_limit_bytes: string;
}

}

export namespace OpencutStorageBudgetConfigureOutput {
  export type CallSource = 'cli' | 'http' | 'mcp_stdio' | 'mcp_http' | 'embedded' | 'worker';

  export interface Response {
  api_version: string;
  data?: PolicyChange | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface PolicyChange {
  actor: Actor;
  control_limit_bytes?: string | null;
  media_output_limit_bytes?: string | null;
  policy: Policy;
  request_id: string;
  [k: string]: any;
}
  export interface Actor {
  principal_id: string;
  source: CallSource;
  worker_id?: string | null;
  [k: string]: any;
}
  export interface Policy {
  minimum_available_bytes: string;
  payload_limit_bytes: string;
  revision: number;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutStorageBudgetRecoverInput {
  export interface EmptyInput {}

}

export namespace OpencutStorageBudgetRecoverOutput {
  export type RepairAction = 'enter' | 'exit';
  export type CallSource = 'cli' | 'http' | 'mcp_stdio' | 'mcp_http' | 'embedded' | 'worker';
  export type RepairState = 'ready' | 'active' | 'degraded';

  export interface Response {
  api_version: string;
  data?: Report | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface Report {
  admission_headroom_bytes: string;
  control?: ControlReport | null;
  coverage: string[];
  enforces_all_workspace_writes: boolean;
  last_policy_change?: PolicyChange | null;
  media_output_limit_bytes?: string | null;
  observed_payload_bytes: string;
  policy: Policy;
  recovered_reservations: number;
  reservations: Reservation[];
  reserved_bytes: string;
  volume_available_bytes: string;
  [k: string]: any;
}
  export interface ControlReport {
  /**
   * Absent until the workspace database file name has been bound.
   */
  admission_headroom_bytes?: string | null;
  control_limit_bytes: string;
  database_name?: string | null;
  observed_control_bytes: string;
  repair?: RepairReport | null;
  reservations: Reservation[];
  reserved_control_bytes: string;
  /**
   * True only once all managed SQLite writes use the guarded VFS.
   */
  write_interception_enabled: boolean;
  [k: string]: any;
}
  export interface RepairReport {
  allocated_blocks_bytes: string;
  allocated_file_bytes: string;
  last_change?: RepairChange | null;
  revision: number;
  state: RepairState;
  target_bytes: string;
  [k: string]: any;
}
  export interface RepairChange {
  action: RepairAction;
  actor: Actor;
  input: RepairRequest;
  request_id: string;
  revision: number;
  state: RepairState;
}
  export interface Actor {
  principal_id: string;
  source: CallSource;
  worker_id?: string | null;
  [k: string]: any;
}
  export interface RepairRequest {
  expected_revision: number;
  idempotency_key: string;
}
  export interface Reservation {
  category: string;
  reservation_id: string;
  reserved_bytes: string;
  writer?: Writer | null;
  [k: string]: any;
}
  export interface Writer {
  attempt: number;
  job_id: string;
}
  export interface PolicyChange {
  actor: Actor;
  control_limit_bytes?: string | null;
  media_output_limit_bytes?: string | null;
  policy: Policy;
  request_id: string;
  [k: string]: any;
}
  export interface Policy {
  minimum_available_bytes: string;
  payload_limit_bytes: string;
  revision: number;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutCasGcQueryInput {
  export interface Query {
  cursor?: string | null;
  include_complete?: boolean;
  limit?: number;
}

}

export namespace OpencutCasGcQueryOutput {
  export type CallSource = 'cli' | 'http' | 'mcp_stdio' | 'mcp_http' | 'embedded' | 'worker';
  export type PlanState = 'planned' | 'executing' | 'complete';

  export interface Response {
  api_version: string;
  data?: Page | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface Page {
  examined: number;
  next_cursor?: string | null;
  plans: Summary[];
  [k: string]: any;
}
  export interface Summary {
  actor: Actor;
  object_count: number;
  origin_request_id: string;
  plan_hash: string;
  plan_id: string;
  remaining: number;
  state: PlanState;
  [k: string]: any;
}
  export interface Actor {
  principal_id: string;
  source: CallSource;
  worker_id?: string | null;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutCasGcPlanInput {
  export interface PlanInput {
  after?: string | null;
  idempotency_key: string;
  limit?: number;
}

}

export namespace OpencutCasGcPlanOutput {
  export type CallSource = 'cli' | 'http' | 'mcp_stdio' | 'mcp_http' | 'embedded' | 'worker';
  export type Object =
  | {
      content_hash: string;
      kind: 'content';
    }
  | {
      capture_id: string;
      kind: 'capture';
    };
  export type ItemState = 'planned' | 'removing' | 'removed' | 'already_absent' | 'retained' | 'changed';
  export type PlanState = 'planned' | 'executing' | 'complete';

  export interface Response {
  api_version: string;
  data?: Plan | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface Plan {
  actor: Actor;
  examined: number;
  items: Item[];
  next_cursor?: string | null;
  origin_request_id: string;
  plan_hash: string;
  plan_id: string;
  protected: number;
  state: PlanState;
  [k: string]: any;
}
  export interface Actor {
  principal_id: string;
  source: CallSource;
  worker_id?: string | null;
  [k: string]: any;
}
  export interface Item {
  actor?: Actor | null;
  object: Object;
  request_id?: string | null;
  size_bytes: string;
  state: ItemState;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutCasGcInspectInput {
  export interface PlanId {
  plan_id: string;
}

}

export namespace OpencutCasGcInspectOutput {
  export type CallSource = 'cli' | 'http' | 'mcp_stdio' | 'mcp_http' | 'embedded' | 'worker';
  export type Object =
  | {
      content_hash: string;
      kind: 'content';
    }
  | {
      capture_id: string;
      kind: 'capture';
    };
  export type ItemState = 'planned' | 'removing' | 'removed' | 'already_absent' | 'retained' | 'changed';
  export type PlanState = 'planned' | 'executing' | 'complete';

  export interface Response {
  api_version: string;
  data?: Plan | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface Plan {
  actor: Actor;
  examined: number;
  items: Item[];
  next_cursor?: string | null;
  origin_request_id: string;
  plan_hash: string;
  plan_id: string;
  protected: number;
  state: PlanState;
  [k: string]: any;
}
  export interface Actor {
  principal_id: string;
  source: CallSource;
  worker_id?: string | null;
  [k: string]: any;
}
  export interface Item {
  actor?: Actor | null;
  object: Object;
  request_id?: string | null;
  size_bytes: string;
  state: ItemState;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutCasGcExecuteInput {
  export interface ExecuteInput {
  expected_plan_hash: string;
  plan_id: string;
}

}

export namespace OpencutCasGcExecuteOutput {
  export type CallSource = 'cli' | 'http' | 'mcp_stdio' | 'mcp_http' | 'embedded' | 'worker';
  export type Object =
  | {
      content_hash: string;
      kind: 'content';
    }
  | {
      capture_id: string;
      kind: 'capture';
    };
  export type ItemState = 'planned' | 'removing' | 'removed' | 'already_absent' | 'retained' | 'changed';
  export type PlanState = 'planned' | 'executing' | 'complete';

  export interface Response {
  api_version: string;
  data?: Plan | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface Plan {
  actor: Actor;
  examined: number;
  items: Item[];
  next_cursor?: string | null;
  origin_request_id: string;
  plan_hash: string;
  plan_id: string;
  protected: number;
  state: PlanState;
  [k: string]: any;
}
  export interface Actor {
  principal_id: string;
  source: CallSource;
  worker_id?: string | null;
  [k: string]: any;
}
  export interface Item {
  actor?: Actor | null;
  object: Object;
  request_id?: string | null;
  size_bytes: string;
  state: ItemState;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutArtifactRetentionInspectInput {
  export interface InspectInput {
  artifact_id: string;
}

}

export namespace OpencutArtifactRetentionInspectOutput {
  export type RetentionState = 'retained' | 'released';

  export interface Response {
  api_version: string;
  data?: View | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface View {
  artifact_id: string;
  content_hash: string;
  job_id: string;
  release_request_id?: string | null;
  released_at?: string | null;
  size_bytes: string;
  state: RetentionState;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutArtifactReferencesInspectInput {
  export interface ReferencesInput {
  artifact_id: string;
}

}

export namespace OpencutArtifactReferencesInspectOutput {
  export type ReferenceKind =
  'revision' | 'job_input' | 'job_result' | 'project_media' | 'plugin_publication' | 'project_plugin_grant';

  export interface Response {
  api_version: string;
  data?: ReferencesView | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface ReferencesView {
  artifact_id: string;
  default_retained: boolean;
  examined_rows: string;
  native_source_candidates: string;
  root_count: string;
  roots: ReferenceRoot[];
  roots_truncated: boolean;
  [k: string]: any;
}
  export interface ReferenceRoot {
  kind: ReferenceKind;
  resource_id: string;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutArtifactGcPlanInput {
  export interface PlanInput {
  artifact_id: string;
  idempotency_key: string;
}

}

export namespace OpencutArtifactGcPlanOutput {
  export type CallSource = 'cli' | 'http' | 'mcp_stdio' | 'mcp_http' | 'embedded' | 'worker';
  export type ReferenceKind =
  'revision' | 'job_input' | 'job_result' | 'project_media' | 'plugin_publication' | 'project_plugin_grant';

  export interface Response {
  api_version: string;
  data?: Plan | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface Plan {
  actor: Actor;
  artifact_id: string;
  default_retained: boolean;
  examined_rows: string;
  execution_available: boolean;
  native_alias_root_count: string;
  native_paths_examined: string;
  observed_at: string;
  origin_request_id: string;
  plan_hash: string;
  plan_id: string;
  root_count: string;
  roots: ReferenceRoot[];
  roots_truncated: boolean;
  size_bytes: string;
  [k: string]: any;
}
  export interface Actor {
  principal_id: string;
  source: CallSource;
  worker_id?: string | null;
  [k: string]: any;
}
  export interface ReferenceRoot {
  kind: ReferenceKind;
  resource_id: string;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutArtifactGcInspectInput {
  export interface PlanId {
  plan_id: string;
}

}

export namespace OpencutArtifactGcInspectOutput {
  export type CallSource = 'cli' | 'http' | 'mcp_stdio' | 'mcp_http' | 'embedded' | 'worker';
  export type ReferenceKind =
  'revision' | 'job_input' | 'job_result' | 'project_media' | 'plugin_publication' | 'project_plugin_grant';

  export interface Response {
  api_version: string;
  data?: Plan | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface Plan {
  actor: Actor;
  artifact_id: string;
  default_retained: boolean;
  examined_rows: string;
  execution_available: boolean;
  native_alias_root_count: string;
  native_paths_examined: string;
  observed_at: string;
  origin_request_id: string;
  plan_hash: string;
  plan_id: string;
  root_count: string;
  roots: ReferenceRoot[];
  roots_truncated: boolean;
  size_bytes: string;
  [k: string]: any;
}
  export interface Actor {
  principal_id: string;
  source: CallSource;
  worker_id?: string | null;
  [k: string]: any;
}
  export interface ReferenceRoot {
  kind: ReferenceKind;
  resource_id: string;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutArtifactGcQueryInput {
  export interface Query {
  cursor?: string | null;
  limit?: number;
}

}

export namespace OpencutArtifactGcQueryOutput {
  export type CallSource = 'cli' | 'http' | 'mcp_stdio' | 'mcp_http' | 'embedded' | 'worker';

  export interface Response {
  api_version: string;
  data?: Page | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface Page {
  examined: number;
  next_cursor?: string | null;
  plans: Summary[];
  [k: string]: any;
}
  export interface Summary {
  actor: Actor;
  artifact_id: string;
  default_retained: boolean;
  observed_at: string;
  origin_request_id: string;
  plan_hash: string;
  plan_id: string;
  root_count: string;
  [k: string]: any;
}
  export interface Actor {
  principal_id: string;
  source: CallSource;
  worker_id?: string | null;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutArtifactGcExecuteInput {
  export interface ExecuteInput {
  expected_plan_hash: string;
  plan_id: string;
}

}

export namespace OpencutArtifactGcExecuteOutput {
  export type ExecutionState = 'planned' | 'busy' | 'retained' | 'removing' | 'removed' | 'absent' | 'changed';

  export interface Response {
  api_version: string;
  data?: ExecutionView | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface ExecutionView {
  artifact_id: string;
  durable: boolean;
  plan_hash: string;
  plan_id: string;
  state: ExecutionState;
  updated_at?: string | null;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutArtifactGcExecutionInspectInput {
  export interface PlanId {
  plan_id: string;
}

}

export namespace OpencutArtifactGcExecutionInspectOutput {
  export type ExecutionState = 'planned' | 'busy' | 'retained' | 'removing' | 'removed' | 'absent' | 'changed';

  export interface Response {
  api_version: string;
  data?: ExecutionView | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface ExecutionView {
  artifact_id: string;
  durable: boolean;
  plan_hash: string;
  plan_id: string;
  state: ExecutionState;
  updated_at?: string | null;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutArtifactRetentionReleaseInput {
  export interface ReleaseInput {
  artifact_id: string;
  idempotency_key: string;
}

}

export namespace OpencutArtifactRetentionReleaseOutput {
  export type RetentionState = 'retained' | 'released';

  export interface Response {
  api_version: string;
  data?: View | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface View {
  artifact_id: string;
  content_hash: string;
  job_id: string;
  release_request_id?: string | null;
  released_at?: string | null;
  size_bytes: string;
  state: RetentionState;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutStorageInspectInput {
  export interface InspectInput {
  max_entries?: number;
}

}

export namespace OpencutStorageInspectOutput {
  export type Category = 'database' | 'asset_store' | 'media' | 'uploads' | 'artifacts' | 'plugins';
  export type StopReason = 'entry_limit' | 'time_limit';

  export interface Response {
  api_version: string;
  data?: UsageReport | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface UsageReport {
  allocated_bytes: string;
  categories: Usage[];
  complete: boolean;
  consistency: string;
  logical_bytes: string;
  schema_version: number;
  stop_reason?: StopReason | null;
  visited_entries: number;
  volume_available_bytes?: string | null;
  volume_capacity_bytes?: string | null;
  volume_observed: boolean;
  [k: string]: any;
}
  export interface Usage {
  /**
   * st_blocks * 512 for unique regular files and directories, not exclusive space.
   */
  allocated_bytes: string;
  category: Category;
  complete: boolean;
  directories: number;
  hardlink_aliases: number;
  /**
   * Unique regular-file lengths. Decimal strings avoid client integer truncation.
   */
  logical_bytes: string;
  omissions: Omissions;
  /**
   * False also when the budget was exhausted before inspecting this category.
   */
  present: boolean;
  regular_files: number;
  [k: string]: any;
}
  export interface Omissions {
  depth_limited: number;
  io_errors: number;
  other_devices: number;
  other_file_types: number;
  symlinks: number;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutUploadCreateInput {
  export interface CreateUpload {
  content_hash: string;
  idempotency_key: string;
  name: string;
  project_id: string;
  size_bytes: string;
}

}

export namespace OpencutUploadCreateOutput {
  export type UploadState = 'receiving' | 'sealed' | 'aborted';

  export interface Response {
  api_version: string;
  data?: UploadView | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface UploadView {
  content_hash: string;
  job_id?: string | null;
  name: string;
  next_offset: string;
  project_id: string;
  size_bytes: string;
  state: UploadState;
  upload_id: string;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutUploadInspectInput {
  export interface UploadInput {
  upload_id: string;
}

}

export namespace OpencutUploadInspectOutput {
  export type UploadState = 'receiving' | 'sealed' | 'aborted';

  export interface Response {
  api_version: string;
  data?: UploadView | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface UploadView {
  content_hash: string;
  job_id?: string | null;
  name: string;
  next_offset: string;
  project_id: string;
  size_bytes: string;
  state: UploadState;
  upload_id: string;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutUploadAbortInput {
  export interface UploadInput {
  upload_id: string;
}

}

export namespace OpencutUploadAbortOutput {
  export type UploadState = 'receiving' | 'sealed' | 'aborted';

  export interface Response {
  api_version: string;
  data?: UploadView | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface UploadView {
  content_hash: string;
  job_id?: string | null;
  name: string;
  next_offset: string;
  project_id: string;
  size_bytes: string;
  state: UploadState;
  upload_id: string;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutUploadRetentionInput {
  export interface UploadInput {
  upload_id: string;
}

}

export namespace OpencutUploadRetentionOutput {
  export type UploadRetentionState = 'retained' | 'released';

  export interface Response {
  api_version: string;
  data?: UploadRetentionView | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface UploadRetentionView {
  project_id: string;
  /**
   * Observation only. Release rechecks authorization and terminal state.
   */
  release_eligible: boolean;
  reserved_bytes: string;
  state: UploadRetentionState;
  upload_id: string;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutUploadReleaseInput {
  export interface UploadInput {
  upload_id: string;
}

}

export namespace OpencutUploadReleaseOutput {
  export type UploadRetentionState = 'retained' | 'released';

  export interface Response {
  api_version: string;
  data?: UploadRetentionView | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface UploadRetentionView {
  project_id: string;
  /**
   * Observation only. Release rechecks authorization and terminal state.
   */
  release_eligible: boolean;
  reserved_bytes: string;
  state: UploadRetentionState;
  upload_id: string;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutUploadQuotaInput {
  export interface ProjectInput {
  project_id: string;
}

}

export namespace OpencutUploadQuotaOutput {
  export interface Response {
  api_version: string;
  data?: UploadQuotaView | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface UploadQuotaView {
  project_id: string;
  project_limit_bytes: string;
  project_reserved_bytes: string;
  /**
   * Configured limit, not another project's usage.
   */
  workspace_limit_bytes: string;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutUploadQueryInput {
  export interface UploadQuery {
  cursor?: string | null;
  limit?: number;
  project_id: string;
  retained_only?: boolean;
}

}

export namespace OpencutUploadQueryOutput {
  export type UploadRetentionState = 'retained' | 'released';
  export type UploadState = 'receiving' | 'sealed' | 'aborted';

  export interface Response {
  api_version: string;
  data?: UploadPage | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface UploadPage {
  next_cursor?: string | null;
  uploads: UploadEntry[];
  [k: string]: any;
}
  export interface UploadEntry {
  retention: UploadRetentionView;
  upload: UploadView;
  [k: string]: any;
}
  export interface UploadRetentionView {
  project_id: string;
  /**
   * Observation only. Release rechecks authorization and terminal state.
   */
  release_eligible: boolean;
  reserved_bytes: string;
  state: UploadRetentionState;
  upload_id: string;
  [k: string]: any;
}
  export interface UploadView {
  content_hash: string;
  job_id?: string | null;
  name: string;
  next_offset: string;
  project_id: string;
  size_bytes: string;
  state: UploadState;
  upload_id: string;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutUploadSealInput {
  export interface UploadInput {
  upload_id: string;
}

}

export namespace OpencutUploadSealOutput {
  export type UploadState = 'receiving' | 'sealed' | 'aborted';

  export interface Response {
  api_version: string;
  data?: UploadView | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface UploadView {
  content_hash: string;
  job_id?: string | null;
  name: string;
  next_offset: string;
  project_id: string;
  size_bytes: string;
  state: UploadState;
  upload_id: string;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutProjectMediaInspectInput {
  export interface ProjectMediaInput {
  media_id: string;
  project_id: string;
}

}

export namespace OpencutProjectMediaInspectOutput {
  export type AssetKind = 'video' | 'audio' | 'image' | 'text' | 'subtitle' | 'graphic';

  export interface Response {
  api_version: string;
  data?: MediaImportJobResult | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
/**
 * A project resource, not a global CAS capability or a host filesystem location.
 */
  export interface MediaImportJobResult {
  asset: Asset;
  job_id: string;
  media_id: string;
  project_id: string;
  upload_id: string;
  [k: string]: any;
}
  export interface Asset {
  duration?: Time | null;
  id: string;
  kind: AssetKind;
  metadata?: {
    [k: string]: any;
  };
  name: string;
  uri: string;
  version?: AssetVersion | null;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface AssetVersion {
  content_hash: string;
  id: string;
  size: number;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutProjectMediaQueryInput {
  export interface ProjectMediaQuery {
  cursor?: string | null;
  limit?: number;
  project_id: string;
}

}

export namespace OpencutProjectMediaQueryOutput {
  export type AssetKind = 'video' | 'audio' | 'image' | 'text' | 'subtitle' | 'graphic';

  export interface Response {
  api_version: string;
  data?: ProjectMediaPage | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface ProjectMediaPage {
  media: MediaImportJobResult[];
  next_cursor?: string | null;
  [k: string]: any;
}
/**
 * A project resource, not a global CAS capability or a host filesystem location.
 */
  export interface MediaImportJobResult {
  asset: Asset;
  job_id: string;
  media_id: string;
  project_id: string;
  upload_id: string;
  [k: string]: any;
}
  export interface Asset {
  duration?: Time | null;
  id: string;
  kind: AssetKind;
  metadata?: {
    [k: string]: any;
  };
  name: string;
  uri: string;
  version?: AssetVersion | null;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface AssetVersion {
  content_hash: string;
  id: string;
  size: number;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutAccessInspectInput {
  export interface EmptyInput {}

}

export namespace OpencutAccessInspectOutput {
  export type ProjectAction = 'read' | 'edit' | 'restore' | 'run_jobs' | 'cancel_jobs' | 'read_artifacts' | 'read_audit';

  export interface Response {
  api_version: string;
  data?: AccessView | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface AccessView {
  credential_id?: string | null;
  grants: ProjectGrant[];
  owner: boolean;
  policy_revision?: string | null;
  principal_id: string;
  [k: string]: any;
}
  export interface ProjectGrant {
  actions: ProjectAction[];
  project_id: string;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutPluginPublicationQueryInput {
  export type MaintenanceTarget =
  | {
      kind: 'workspace';
    }
  | {
      kind: 'object';
      object: Object;
    }
  | {
      id: string;
      kind: 'publication';
    }
  | {
      id: string;
      kind: 'request';
    };
  export type Object =
  | {
      kind: 'plugin_package';
      package_sha256: string;
      plugin_id: string;
      version: string;
    }
  | {
      kind: 'plugin_grants';
      package_sha256: string;
      plugin_id: string;
      version: string;
    };

  export interface MaintenanceQuery {
  after_sequence?: string;
  limit?: number;
  target: MaintenanceTarget;
}

}

export namespace OpencutPluginPublicationQueryOutput {
  export type Coverage = 'recorded_plugin_publications_since_schema7';
  export type CallSource = 'cli' | 'http' | 'mcp_stdio' | 'mcp_http' | 'embedded' | 'worker';
  export type Action = 'plugin_package_publish' | 'plugin_grants_update';
  export type Object =
  | {
      kind: 'plugin_package';
      package_sha256: string;
      plugin_id: string;
      version: string;
    }
  | {
      kind: 'plugin_grants';
      package_sha256: string;
      plugin_id: string;
      version: string;
    };
  export type PublicationPhase =
  'prepared' | 'completed' | 'observed_applied' | 'observed_unchanged' | 'observed_diverged';

  export interface Response {
  api_version: string;
  data?: MaintenancePage | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface MaintenancePage {
  coverage: Coverage;
  events: MaintenanceEvent[];
  has_more: boolean;
  last_sequence: string;
  [k: string]: any;
}
  export interface MaintenanceEvent {
  actor: Actor;
  created_at: string;
  intent: PublicationIntent;
  observed_hash?: string | null;
  origin_request_id: string;
  phase: PublicationPhase;
  publication_id: string;
  request_id: string;
  sequence: string;
  [k: string]: any;
}
  export interface Actor {
  principal_id: string;
  source: CallSource;
  worker_id?: string | null;
  [k: string]: any;
}
  export interface PublicationIntent {
  action: Action;
  after_hash: string;
  before_hash?: string | null;
  object: Object;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutPluginPublicationReconcileInput {
  export type Object =
  | {
      kind: 'plugin_package';
      package_sha256: string;
      plugin_id: string;
      version: string;
    }
  | {
      kind: 'plugin_grants';
      package_sha256: string;
      plugin_id: string;
      version: string;
    };

  export interface ReconcileInput {
  object: Object;
}

}

export namespace OpencutPluginPublicationReconcileOutput {
  export type Object =
  | {
      kind: 'plugin_package';
      package_sha256: string;
      plugin_id: string;
      version: string;
    }
  | {
      kind: 'plugin_grants';
      package_sha256: string;
      plugin_id: string;
      version: string;
    };

  export interface Response {
  api_version: string;
  data?: ReconcileResult | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface ReconcileResult {
  object: Object;
  observed_hash?: string | null;
  resolved_publication_ids: string[];
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutMaintenanceQueryInput {
  export type MaintenanceTarget =
  | {
      kind: 'workspace';
    }
  | {
      kind: 'object';
      object: MaintenanceObject;
    }
  | {
      id: string;
      kind: 'publication';
    }
  | {
      id: string;
      kind: 'request';
    };
  export type MaintenanceObject =
  | {
      content_hash: string;
      kind: 'media_content';
    }
  | {
      content_hash: string;
      kind: 'media_metadata';
    };

  export interface MaintenanceQuery {
  after_sequence?: string;
  limit?: number;
  target: MaintenanceTarget;
}

}

export namespace OpencutMaintenanceQueryOutput {
  export type MaintenanceCoverage = 'recorded_publications_since_schema6';
  export type CallSource = 'cli' | 'http' | 'mcp_stdio' | 'mcp_http' | 'embedded' | 'worker';
  export type MaintenanceAction = 'media_content_publish' | 'media_metadata_publish' | 'media_metadata_repair';
  export type MaintenanceObject =
  | {
      content_hash: string;
      kind: 'media_content';
    }
  | {
      content_hash: string;
      kind: 'media_metadata';
    };
  export type PublicationPhase =
  'prepared' | 'completed' | 'observed_applied' | 'observed_unchanged' | 'observed_diverged';

  export interface Response {
  api_version: string;
  data?: MaintenancePage | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface MaintenancePage {
  coverage: MaintenanceCoverage;
  events: MaintenanceEvent[];
  has_more: boolean;
  last_sequence: string;
  [k: string]: any;
}
  export interface MaintenanceEvent {
  actor: Actor;
  created_at: string;
  intent: PublicationIntent;
  observed_hash?: string | null;
  origin_request_id: string;
  phase: PublicationPhase;
  publication_id: string;
  request_id: string;
  sequence: string;
  [k: string]: any;
}
  export interface Actor {
  principal_id: string;
  source: CallSource;
  worker_id?: string | null;
  [k: string]: any;
}
  export interface PublicationIntent {
  action: MaintenanceAction;
  after_hash: string;
  before_hash?: string | null;
  object: MaintenanceObject;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutMaintenanceReconcileInput {
  export type MaintenanceObject =
  | {
      content_hash: string;
      kind: 'media_content';
    }
  | {
      content_hash: string;
      kind: 'media_metadata';
    };

  export interface ReconcileInput {
  object: MaintenanceObject;
}

}

export namespace OpencutMaintenanceReconcileOutput {
  export type MaintenanceObject =
  | {
      content_hash: string;
      kind: 'media_content';
    }
  | {
      content_hash: string;
      kind: 'media_metadata';
    };

  export interface Response {
  api_version: string;
  data?: ReconcileResult | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface ReconcileResult {
  object: MaintenanceObject;
  observed_hash?: string | null;
  resolved_publication_ids: string[];
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutAccessAuditQueryInput {
  export type AccessAuditTarget =
  | {
      kind: 'workspace';
    }
  | {
      id: string;
      kind: 'principal';
    }
  | {
      id: string;
      kind: 'credential';
    }
  | {
      id: string;
      kind: 'request';
    };

  export interface AccessAuditQuery {
  after_sequence?: string;
  limit?: number;
  target: AccessAuditTarget;
}

}

export namespace OpencutAccessAuditQueryOutput {
  export type AccessAuditCoverage = 'recorded_administration_since_schema5';
  export type AccessAuditAction = 'policy.updated' | 'credential.issued' | 'credential.revoked';
  export type CallSource = 'cli' | 'http' | 'mcp_stdio' | 'mcp_http' | 'embedded' | 'worker';
  export type ProjectAction = 'read' | 'edit' | 'restore' | 'run_jobs' | 'cancel_jobs' | 'read_artifacts' | 'read_audit';

  export interface Response {
  api_version: string;
  data?: AccessAuditPage | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface AccessAuditPage {
  coverage: AccessAuditCoverage;
  has_more: boolean;
  last_sequence: string;
  records: AccessAuditRecord[];
  [k: string]: any;
}
/**
 * Administration history, never a credential or an authorization decision.
 */
  export interface AccessAuditRecord {
  action: AccessAuditAction;
  actor: Actor;
  created_at: string;
  credential_id?: string | null;
  /**
   * Policy as committed at this event; later changes do not rewrite it.
   */
  policy?: PrincipalView | null;
  principal_id: string;
  request_id: string;
  sequence: string;
  [k: string]: any;
}
  export interface Actor {
  principal_id: string;
  source: CallSource;
  worker_id?: string | null;
  [k: string]: any;
}
  export interface PrincipalView {
  enabled: boolean;
  grants: ProjectGrant[];
  principal_id: string;
  revision: string;
  [k: string]: any;
}
  export interface ProjectGrant {
  actions: ProjectAction[];
  project_id: string;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutResourceReadInput {
  export interface ResourceReadInput {
  uri: string;
}

}

export namespace OpencutResourceReadOutput {
  export type ResourcePayload =
  | {
      data: HostCapabilities;
      kind: 'capabilities';
      [k: string]: any;
    }
  | {
      data: ToolSpec[];
      kind: 'tools';
      [k: string]: any;
    }
  | {
      data: DomainDescriptor[];
      kind: 'domains';
      [k: string]: any;
    }
  | {
      data: any;
      kind: 'event_schema';
      [k: string]: any;
    }
  | {
      data: OperationSchema;
      kind: 'operation_schema';
      [k: string]: any;
    }
  | {
      data: ProjectSummary;
      kind: 'project_summary';
      [k: string]: any;
    }
  | {
      data: RevisionTimeline;
      kind: 'timeline';
      [k: string]: any;
    }
  | {
      data: JobView;
      kind: 'job';
      [k: string]: any;
    }
  | {
      data: ArtifactReference;
      kind: 'artifact';
      [k: string]: any;
    };
  export type ProjectAction = 'read' | 'edit' | 'restore' | 'run_jobs' | 'cancel_jobs' | 'read_artifacts' | 'read_audit';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };
  export type TextAlign = 'left' | 'center' | 'right';
  export type KeyframeInterpolation = 'hold' | 'linear';
  export type TrackKind = 'video' | 'audio' | 'caption' | 'graphic';
  export type ArtifactAvailability = 'unchecked';
  export type ArtifactRole = 'primary';
  export type PublicJobStage =
  'resolving_assets' | 'probing' | 'selecting_frame' | 'decoding' | 'encoding' | 'verifying' | 'unknown';
  export type JobResultView =
  | {
      data: MediaInfo;
      kind: 'media.probe';
      [k: string]: any;
    }
  | {
      data: SourceFrameView;
      kind: 'preview.frame';
      [k: string]: any;
    }
  | {
      data: ProjectFrameView;
      kind: 'preview.project_frame';
      [k: string]: any;
    }
  | {
      data: CompositedFrameView;
      kind: 'preview.composited_frame';
      [k: string]: any;
    }
  | {
      data: ProjectRangeView;
      kind: 'preview.project_range';
      [k: string]: any;
    }
  | {
      data: ExportView;
      kind: 'render.export';
      [k: string]: any;
    }
  | {
      data: ProjectRenderView;
      kind: 'render.project';
      [k: string]: any;
    }
  | {
      data: VerificationView;
      kind: 'artifact.verify';
      [k: string]: any;
    };
  export type StreamKind = 'video' | 'audio' | 'subtitle' | 'data' | 'attachment' | 'unknown';
  export type FrameAlignment = 'exact' | 'nearest' | 'floor' | 'ceil';
  export type FrameQuality = 'accurate_source_frame';
  export type AssetLockingMode = 'media_content_locked' | 'legacy_unlocked';
  export type ProjectFrameQuality = 'encoded_project_frame';
  export type FontResolutionPolicy2 =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };
  export type CompositedProjectFrameQuality = 'composited_project_frame';
  export type ProjectRangeQuality = 'encoded_project_range';
  export type JobState = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

  export interface Response {
  api_version: string;
  data?: ResourceView | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface ResourceView {
  mime_type: string;
  payload: ResourcePayload;
  uri: string;
  [k: string]: any;
}
/**
 * Extended host discovery. The v1 tool retains its frozen, extension-tolerant DTO.
 */
  export interface HostCapabilities {
  access?: AccessCapabilities;
  api_version: string;
  artifact_gc?: Capabilities4 | null;
  cas_gc?: Capabilities3 | null;
  database?: Capabilities6 | null;
  denials?: DenialCapabilities | null;
  features: Features;
  host: string;
  job_execution: JobExecution;
  job_kinds: JobKindDescriptor[];
  limits: Limits;
  mcp: McpCapabilities;
  operations: OperationDescriptor[];
  parameters: ParameterDescriptor[];
  project_plugins?: Capabilities | null;
  project_schema_version: number;
  rendering: Rendering;
  storage?: Capabilities2 | null;
  storage_budget?: Capabilities5 | null;
  tools: string[];
  uploads?: UploadCapabilities | null;
  [k: string]: any;
}
  export interface AccessCapabilities {
  mcp_session_binding?: string | null;
  policy_management?: string | null;
  project_actions: ProjectAction[];
  public_event_stream_path?: string | null;
  /**
   * Scoped preflight requires run_jobs and immutable project content.
   */
  scoped_preflight?: boolean;
  scoped_transports: string[];
  [k: string]: any;
}
  export interface Capabilities4 {
  execute_available: boolean;
  execute_tool: string;
  execution_inspect_tool: string;
  inspect_tool: string;
  max_reference_bytes: number;
  max_reference_rows: number;
  owner_only: boolean;
  plan_available: boolean;
  plan_tool: string;
  query_tool: string;
  references_tool: string;
  [k: string]: any;
}
  export interface Capabilities3 {
  available: boolean;
  deletes_published_artifacts: boolean;
  execute_tool: string;
  inspect_tool: string;
  max_namespace_entries: number;
  max_objects: number;
  max_reference_bytes: number;
  max_reference_document_bytes: number;
  max_reference_rows: number;
  owner_only: boolean;
  plan_tool: string;
  protects_all_jobs: boolean;
  protects_all_revisions: boolean;
  query_tool: string;
  reference_cooperative_time_limit_ms: number;
  [k: string]: any;
}
  export interface Capabilities6 {
  checkpoint_tool: string;
  enforces_total_storage_limit: boolean;
  inspect_tool: string;
  max_checkpoint_busy_timeout_ms: number;
  owner_only: boolean;
  [k: string]: any;
}
  export interface DenialCapabilities {
  captures_request_bodies: boolean;
  failure_warning: string;
  max_retained_records: number;
  query_tool: string;
  request_id_header: string;
  [k: string]: any;
}
  export interface Features {
  atomic_transactions: boolean;
  durable_events: boolean;
  edit_preview_plan: boolean;
  idempotent_writes: boolean;
  multiple_branches: boolean;
  new_asset_bindings_versioned: boolean;
  optimistic_concurrency: boolean;
  persistent_jobs: boolean;
  persistent_projects: boolean;
  plugin_execution: boolean;
  revision_history: boolean;
  revision_restore: boolean;
  timeline_rendering: boolean;
  [k: string]: any;
}
  export interface JobExecution {
  durable_progress: boolean;
  managed_output: boolean;
  progress_fraction_scope: string;
  project_asset_locking: boolean;
  service_starts_worker: boolean;
  worker_required: boolean;
  [k: string]: any;
}
  export interface JobKindDescriptor {
  input_schema: any;
  name: string;
  result_schema: any;
  [k: string]: any;
}
  export interface Limits {
  event_page_size: number;
  input_bytes: number;
  [k: string]: any;
}
  export interface McpCapabilities {
  http_path: string;
  max_resource_subscriptions: number;
  max_sessions: number;
  network_scope: string;
  prompts: boolean;
  protocol_versions: string[];
  resource_subscriptions: boolean;
  resources: boolean;
  transports: string[];
  [k: string]: any;
}
  export interface OperationDescriptor {
  conflict_policy: string;
  declared_effects: string[];
  deterministic: boolean;
  idempotency_policy: string;
  kind: string;
  name: string;
  required_capabilities: string[];
  version: number;
}
  export interface ParameterDescriptor {
  animatable: boolean;
  default_value: any;
  interpolation: string[];
  maximum?: number | null;
  minimum?: number | null;
  name: string;
  target: string;
  unit: string;
  value_type: string;
  version: number;
}
  export interface Capabilities {
  ceilings: PluginLimits;
  grant_tool: string;
  inspect_tool: string;
  list_tool: string;
  max_component_bytes: number;
  max_concurrent_per_process: number;
  max_grants_per_project: number;
  max_operations_per_grant: number;
  max_parameter_bytes: number;
  max_schema_bytes: number;
  max_snapshot_bytes: number;
  plan_tool: string;
  [k: string]: any;
}
  export interface PluginLimits {
  fuel?: number;
  memory_mib: number;
  output_bytes: number;
  wall_time_ms: number;
}
  export interface Rendering {
  audio_mix: boolean;
  availability: string;
  backend: string;
  composited_frame_execution: string;
  composited_frame_preview: boolean;
  constant_rate: boolean;
  fixed_revision: boolean;
  images: boolean;
  keyframes: boolean;
  multiple_video_tracks: boolean;
  preview_frame_execution: string;
  preview_range_execution: string;
  project_frame_preview: boolean;
  project_range_preview: boolean;
  requires_executables: string[];
  sequential_video: boolean;
  source_audio: boolean;
  static_captions: boolean;
  text_shaping: boolean;
  visual_transforms: boolean;
  [k: string]: any;
}
  export interface Capabilities2 {
  available: boolean;
  cooperative_time_limit_ms: number;
  enforces_quota: boolean;
  garbage_collection: boolean;
  inspect_tool: string;
  max_concurrent_per_process: number;
  max_depth: number;
  max_entries: number;
  owner_only: boolean;
  [k: string]: any;
}
  export interface Capabilities5 {
  available: boolean;
  configure_tool: string;
  enforces_all_workspace_writes: boolean;
  inspect_tool: string;
  max_active_reservations: number;
  max_policy_changes: number;
  owner_only: boolean;
  recover_tool: string;
  [k: string]: any;
}
  export interface UploadCapabilities {
  accept_tool: string;
  chunk_method: string;
  chunk_path_template: string;
  chunk_query_fields: string[];
  chunk_timeout_seconds: number;
  max_chunk_bytes: string;
  max_chunks: number;
  max_inflight_chunks: number;
  max_receiving_per_project: number;
  max_receiving_per_workspace: number;
  max_upload_bytes: string;
  requires_bearer: boolean;
  result_field: string;
  result_schema_version: number;
  sequential: boolean;
  staging?: UploadStagingCapabilities | null;
  [k: string]: any;
}
  export interface UploadStagingCapabilities {
  automatic_terminal_cleanup: boolean;
  measures_filesystem_usage: boolean;
  project_reservation_limit_bytes: string;
  query_tool: string;
  quota_tool: string;
  release_requires_terminal_job: boolean;
  release_tool: string;
  retention_tool: string;
  workspace_reservation_limit_bytes: string;
  [k: string]: any;
}
  export interface ToolSpec {
  description: string;
  input_schema: any;
  name: string;
  output_schema: any;
  read_only: boolean;
  [k: string]: any;
}
  export interface DomainDescriptor {
  actions: ActionDescriptor[];
  description: string;
  domain: string;
  [k: string]: any;
}
  export interface ActionDescriptor {
  action: string;
  description: string;
  input_schema?: any;
  operation: string;
  output_schema?: any;
  read_only: boolean;
  [k: string]: any;
}
  export interface OperationSchema {
  descriptor: OperationDescriptor;
  schema: any;
  [k: string]: any;
}
  export interface ProjectSummary {
  asset_count: number;
  branch: string;
  default_sequence_id: string;
  name: string;
  project_id: string;
  revision: string;
  schema_version: number;
  sequences: SequenceSummary[];
  [k: string]: any;
}
  export interface SequenceSummary {
  caption_count: number;
  clip_count: number;
  fps: FrameRate;
  height: number;
  name: string;
  sequence_id: string;
  track_count: number;
  width: number;
  [k: string]: any;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface RevisionTimeline {
  project_id: string;
  revision: string;
  sequence: Sequence;
  [k: string]: any;
}
  export interface Sequence {
  caption_cues?: CaptionCue[];
  fps: FrameRate;
  height: number;
  id: string;
  name: string;
  tracks: Track[];
  width: number;
}
  export interface CaptionCue {
  id: string;
  source_anchor?: CaptionSourceAnchor | null;
  style: CaptionStyle;
  text: string;
  timeline_range: TimeRange;
}
  export interface CaptionSourceAnchor {
  asset_id: string;
  source_range: TimeRange;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface CaptionStyle {
  background_color?: string | null;
  color: string;
  font_family: string;
  font_policy?: FontResolutionPolicy;
  font_size: number;
  /**
   * Line height in canvas pixels. None derives it from font_size.
   */
  line_height?: number | null;
  /**
   * Maximum layout width in canvas pixels. None uses the backend's canvas margin.
   */
  max_width?: number | null;
  text_align?: TextAlign;
}
  export interface Track {
  clips: Clip[];
  enabled: boolean;
  id: string;
  kind: TrackKind;
  locked: boolean;
  muted: boolean;
  name: string;
  params?: {
    [k: string]: any;
  };
}
  export interface Clip {
  asset_id?: string | null;
  channels?: KeyframeChannel[];
  id: string;
  params?: {
    [k: string]: any;
  };
  source_range?: TimeRange | null;
  timeline_range: TimeRange;
}
  export interface KeyframeChannel {
  keyframes: Keyframe[];
  parameter: string;
}
  export interface Keyframe {
  id: string;
  interpolation: KeyframeInterpolation;
  time: Time;
  value: any;
}
  export interface JobView {
  artifacts: ArtifactReference[];
  cancel_requested: boolean;
  /**
   * New task kinds use the open registry envelope; legacy result variants stay frozen.
   */
  extended_result?: ExtendedJobResult | null;
  failure?: PublicJobFailure | null;
  job_id: string;
  kind: string;
  progress?: ProgressView | null;
  resource_target?: JobResourceTarget | null;
  result?: JobResultView | null;
  result_unavailable?: PublicJobFailure | null;
  state: JobState;
  target?: ProjectTaskTarget | null;
  [k: string]: any;
}
  export interface ArtifactReference {
  artifact_id: string;
  availability: ArtifactAvailability;
  content_hash: string;
  job_id: string;
  mime_type: string;
  role: ArtifactRole;
  size_bytes: string;
  [k: string]: any;
}
  export interface ExtendedJobResult {
  data: any;
  kind: string;
  schema_version: number;
  [k: string]: any;
}
  export interface PublicJobFailure {
  code: string;
  diagnostic_job_id: string;
  message: string;
  [k: string]: any;
}
  export interface ProgressView {
  fraction?: number | null;
  media_time?: Time | null;
  stage: PublicJobStage;
  [k: string]: any;
}
  export interface JobResourceTarget {
  project_id: string;
  resource_id: string;
  resource_kind: string;
  [k: string]: any;
}
  export interface MediaInfo {
  bit_rate?: number | null;
  duration?: MediaTime | null;
  format_name?: string | null;
  streams: MediaStream[];
  [k: string]: any;
}
  export interface MediaTime {
  ticks: string;
  timebase: number;
}
  export interface MediaStream {
  average_frame_rate?: FrameRate2 | null;
  channel_layout?: string | null;
  channels?: number | null;
  codec_name?: string | null;
  codec_type: StreamKind;
  color_primaries?: string | null;
  color_space?: string | null;
  color_transfer?: string | null;
  duration?: MediaTime | null;
  height?: number | null;
  index: number;
  pixel_format?: string | null;
  sample_rate?: number | null;
  time_base?: FrameRate2 | null;
  width?: number | null;
  [k: string]: any;
}
  export interface FrameRate2 {
  denominator: number;
  numerator: number;
}
  export interface SourceFrameView {
  actual_time: MediaTime;
  alignment: FrameAlignment;
  exact: boolean;
  quality: FrameQuality;
  requested_time: MediaTime;
  source_pts: string;
  source_time_base: FrameRate2;
  [k: string]: any;
}
  export interface ProjectFrameView {
  actual_sequence_time: Time;
  alignment: FrameAlignment;
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  height: number;
  quality: ProjectFrameQuality;
  requested_time: Time;
  revision: string;
  sequence_id: string;
  text_layouts: TextLayoutView[];
  width: number;
  [k: string]: any;
}
  export interface AssetLockManifest {
  assets: LockedAsset[];
  revision: string;
  sequence_id: string;
  version: number;
}
  export interface LockedAsset {
  asset_id: string;
  content_hash: string;
  size: number;
}
  export interface TextLayoutView {
  cluster_ranges: TextClusterRange[];
  glyph_count: number;
  line_count: number;
  pixel_bounds: PixelBounds;
  policy: FontResolutionPolicy2;
  requested_family: string;
  rtl_runs: number;
  used_faces: FontFaceView[];
  [k: string]: any;
}
  export interface TextClusterRange {
  end: number;
  rtl: boolean;
  start: number;
  [k: string]: any;
}
  export interface PixelBounds {
  height: number;
  width: number;
  x: number;
  y: number;
  [k: string]: any;
}
  export interface FontFaceView {
  face_index: number;
  family: string;
  postscript_name: string;
  [k: string]: any;
}
  export interface CompositedFrameView {
  actual_sequence_time: Time;
  alignment: FrameAlignment;
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  height: number;
  quality: CompositedProjectFrameQuality;
  requested_time: Time;
  revision: string;
  sequence_id: string;
  text_layouts: TextLayoutView[];
  width: number;
  [k: string]: any;
}
  export interface ProjectRangeView {
  actual_sequence_range: TimeRange;
  alignment: FrameAlignment;
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  media: ExportView;
  quality: ProjectRangeQuality;
  requested_range: TimeRange;
  revision: string;
  sequence_id: string;
  [k: string]: any;
}
  export interface ExportView {
  metadata: MediaInfo;
  text_layouts: TextLayoutView[];
  verification: VerificationView;
  [k: string]: any;
}
  export interface VerificationView {
  checks: VerificationCheckView[];
  decoded_samples: DecodedSample[];
  metadata: MediaInfo;
  passed: boolean;
  [k: string]: any;
}
  export interface VerificationCheckView {
  name: string;
  passed: boolean;
  [k: string]: any;
}
  export interface DecodedSample {
  decoded: boolean;
  requested_time: MediaTime;
  [k: string]: any;
}
  export interface ProjectRenderView {
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  media: ExportView;
  revision: string;
  [k: string]: any;
}
  export interface ProjectTaskTarget {
  project_id: string;
  revision: string;
  sequence_id: string;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutEventChangesInput {
  export type EventScope =
  | {
      kind: 'projects';
      project_id?: string | null;
    }
  | {
      job_id?: string | null;
      kind: 'jobs';
    };

  export interface EventChangesInput {
  cursor?: string | null;
  limit?: number;
  scope: EventScope;
}

}

export namespace OpencutEventChangesOutput {
  export type EventData =
  | {
      data: ProjectEvent;
      type: 'project';
      [k: string]: any;
    }
  | {
      data: JobChange;
      type: 'job';
      [k: string]: any;
    };
  export type JobChangeKind =
  | 'queued'
  | 'started'
  | 'progress'
  | 'succeeded'
  | 'failed'
  | 'cancel_requested'
  | 'cancelled'
  | 'recovered'
  | 'unknown';
  export type PublicJobStage =
  'resolving_assets' | 'probing' | 'selecting_frame' | 'decoding' | 'encoding' | 'verifying' | 'unknown';
  export type JobState = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

  export interface Response {
  api_version: string;
  data?: EventChangesPage | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface EventChangesPage {
  events: EventChange[];
  next_cursor?: string | null;
  [k: string]: any;
}
  export interface EventChange {
  cursor: string;
  event: EventData;
  [k: string]: any;
}
  export interface ProjectEvent {
  event_id: string;
  event_type: string;
  payload: any;
  project_id: string;
  revision: string;
  [k: string]: any;
}
  export interface JobChange {
  created_at: string;
  event_id: string;
  event_type: JobChangeKind;
  job_id: string;
  progress?: ProgressView | null;
  state: JobState;
  [k: string]: any;
}
  export interface ProgressView {
  fraction?: number | null;
  media_time?: Time | null;
  stage: PublicJobStage;
  [k: string]: any;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutJobViewInput {
  export interface JobInput {
  job_id: string;
}

}

export namespace OpencutJobViewOutput {
  export type ArtifactAvailability = 'unchecked';
  export type ArtifactRole = 'primary';
  export type PublicJobStage =
  'resolving_assets' | 'probing' | 'selecting_frame' | 'decoding' | 'encoding' | 'verifying' | 'unknown';
  export type JobResultView =
  | {
      data: MediaInfo;
      kind: 'media.probe';
      [k: string]: any;
    }
  | {
      data: SourceFrameView;
      kind: 'preview.frame';
      [k: string]: any;
    }
  | {
      data: ProjectFrameView;
      kind: 'preview.project_frame';
      [k: string]: any;
    }
  | {
      data: CompositedFrameView;
      kind: 'preview.composited_frame';
      [k: string]: any;
    }
  | {
      data: ProjectRangeView;
      kind: 'preview.project_range';
      [k: string]: any;
    }
  | {
      data: ExportView;
      kind: 'render.export';
      [k: string]: any;
    }
  | {
      data: ProjectRenderView;
      kind: 'render.project';
      [k: string]: any;
    }
  | {
      data: VerificationView;
      kind: 'artifact.verify';
      [k: string]: any;
    };
  export type StreamKind = 'video' | 'audio' | 'subtitle' | 'data' | 'attachment' | 'unknown';
  export type FrameAlignment = 'exact' | 'nearest' | 'floor' | 'ceil';
  export type FrameQuality = 'accurate_source_frame';
  export type AssetLockingMode = 'media_content_locked' | 'legacy_unlocked';
  export type ProjectFrameQuality = 'encoded_project_frame';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };
  export type CompositedProjectFrameQuality = 'composited_project_frame';
  export type ProjectRangeQuality = 'encoded_project_range';
  export type JobState = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

  export interface Response {
  api_version: string;
  data?: JobView | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface JobView {
  artifacts: ArtifactReference[];
  cancel_requested: boolean;
  /**
   * New task kinds use the open registry envelope; legacy result variants stay frozen.
   */
  extended_result?: ExtendedJobResult | null;
  failure?: PublicJobFailure | null;
  job_id: string;
  kind: string;
  progress?: ProgressView | null;
  resource_target?: JobResourceTarget | null;
  result?: JobResultView | null;
  result_unavailable?: PublicJobFailure | null;
  state: JobState;
  target?: ProjectTaskTarget | null;
  [k: string]: any;
}
  export interface ArtifactReference {
  artifact_id: string;
  availability: ArtifactAvailability;
  content_hash: string;
  job_id: string;
  mime_type: string;
  role: ArtifactRole;
  size_bytes: string;
  [k: string]: any;
}
  export interface ExtendedJobResult {
  data: any;
  kind: string;
  schema_version: number;
  [k: string]: any;
}
  export interface PublicJobFailure {
  code: string;
  diagnostic_job_id: string;
  message: string;
  [k: string]: any;
}
  export interface ProgressView {
  fraction?: number | null;
  media_time?: Time | null;
  stage: PublicJobStage;
  [k: string]: any;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface JobResourceTarget {
  project_id: string;
  resource_id: string;
  resource_kind: string;
  [k: string]: any;
}
  export interface MediaInfo {
  bit_rate?: number | null;
  duration?: MediaTime | null;
  format_name?: string | null;
  streams: MediaStream[];
  [k: string]: any;
}
  export interface MediaTime {
  ticks: string;
  timebase: number;
}
  export interface MediaStream {
  average_frame_rate?: FrameRate | null;
  channel_layout?: string | null;
  channels?: number | null;
  codec_name?: string | null;
  codec_type: StreamKind;
  color_primaries?: string | null;
  color_space?: string | null;
  color_transfer?: string | null;
  duration?: MediaTime | null;
  height?: number | null;
  index: number;
  pixel_format?: string | null;
  sample_rate?: number | null;
  time_base?: FrameRate | null;
  width?: number | null;
  [k: string]: any;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface SourceFrameView {
  actual_time: MediaTime;
  alignment: FrameAlignment;
  exact: boolean;
  quality: FrameQuality;
  requested_time: MediaTime;
  source_pts: string;
  source_time_base: FrameRate;
  [k: string]: any;
}
  export interface ProjectFrameView {
  actual_sequence_time: Time;
  alignment: FrameAlignment;
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  height: number;
  quality: ProjectFrameQuality;
  requested_time: Time;
  revision: string;
  sequence_id: string;
  text_layouts: TextLayoutView[];
  width: number;
  [k: string]: any;
}
  export interface AssetLockManifest {
  assets: LockedAsset[];
  revision: string;
  sequence_id: string;
  version: number;
}
  export interface LockedAsset {
  asset_id: string;
  content_hash: string;
  size: number;
}
  export interface TextLayoutView {
  cluster_ranges: TextClusterRange[];
  glyph_count: number;
  line_count: number;
  pixel_bounds: PixelBounds;
  policy: FontResolutionPolicy;
  requested_family: string;
  rtl_runs: number;
  used_faces: FontFaceView[];
  [k: string]: any;
}
  export interface TextClusterRange {
  end: number;
  rtl: boolean;
  start: number;
  [k: string]: any;
}
  export interface PixelBounds {
  height: number;
  width: number;
  x: number;
  y: number;
  [k: string]: any;
}
  export interface FontFaceView {
  face_index: number;
  family: string;
  postscript_name: string;
  [k: string]: any;
}
  export interface CompositedFrameView {
  actual_sequence_time: Time;
  alignment: FrameAlignment;
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  height: number;
  quality: CompositedProjectFrameQuality;
  requested_time: Time;
  revision: string;
  sequence_id: string;
  text_layouts: TextLayoutView[];
  width: number;
  [k: string]: any;
}
  export interface ProjectRangeView {
  actual_sequence_range: TimeRange;
  alignment: FrameAlignment;
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  media: ExportView;
  quality: ProjectRangeQuality;
  requested_range: TimeRange;
  revision: string;
  sequence_id: string;
  [k: string]: any;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface ExportView {
  metadata: MediaInfo;
  text_layouts: TextLayoutView[];
  verification: VerificationView;
  [k: string]: any;
}
  export interface VerificationView {
  checks: VerificationCheckView[];
  decoded_samples: DecodedSample[];
  metadata: MediaInfo;
  passed: boolean;
  [k: string]: any;
}
  export interface VerificationCheckView {
  name: string;
  passed: boolean;
  [k: string]: any;
}
  export interface DecodedSample {
  decoded: boolean;
  requested_time: MediaTime;
  [k: string]: any;
}
  export interface ProjectRenderView {
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  media: ExportView;
  revision: string;
  [k: string]: any;
}
  export interface ProjectTaskTarget {
  project_id: string;
  revision: string;
  sequence_id: string;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutJobQueryInput {
  export interface JobQueryInput {
  cursor?: string | null;
  limit?: number;
}

}

export namespace OpencutJobQueryOutput {
  export type ArtifactAvailability = 'unchecked';
  export type ArtifactRole = 'primary';
  export type PublicJobStage =
  'resolving_assets' | 'probing' | 'selecting_frame' | 'decoding' | 'encoding' | 'verifying' | 'unknown';
  export type JobResultView =
  | {
      data: MediaInfo;
      kind: 'media.probe';
      [k: string]: any;
    }
  | {
      data: SourceFrameView;
      kind: 'preview.frame';
      [k: string]: any;
    }
  | {
      data: ProjectFrameView;
      kind: 'preview.project_frame';
      [k: string]: any;
    }
  | {
      data: CompositedFrameView;
      kind: 'preview.composited_frame';
      [k: string]: any;
    }
  | {
      data: ProjectRangeView;
      kind: 'preview.project_range';
      [k: string]: any;
    }
  | {
      data: ExportView;
      kind: 'render.export';
      [k: string]: any;
    }
  | {
      data: ProjectRenderView;
      kind: 'render.project';
      [k: string]: any;
    }
  | {
      data: VerificationView;
      kind: 'artifact.verify';
      [k: string]: any;
    };
  export type StreamKind = 'video' | 'audio' | 'subtitle' | 'data' | 'attachment' | 'unknown';
  export type FrameAlignment = 'exact' | 'nearest' | 'floor' | 'ceil';
  export type FrameQuality = 'accurate_source_frame';
  export type AssetLockingMode = 'media_content_locked' | 'legacy_unlocked';
  export type ProjectFrameQuality = 'encoded_project_frame';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };
  export type CompositedProjectFrameQuality = 'composited_project_frame';
  export type ProjectRangeQuality = 'encoded_project_range';
  export type JobState = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

  export interface Response {
  api_version: string;
  data?: JobPage | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface JobPage {
  jobs: JobView[];
  next_cursor?: string | null;
  [k: string]: any;
}
  export interface JobView {
  artifacts: ArtifactReference[];
  cancel_requested: boolean;
  /**
   * New task kinds use the open registry envelope; legacy result variants stay frozen.
   */
  extended_result?: ExtendedJobResult | null;
  failure?: PublicJobFailure | null;
  job_id: string;
  kind: string;
  progress?: ProgressView | null;
  resource_target?: JobResourceTarget | null;
  result?: JobResultView | null;
  result_unavailable?: PublicJobFailure | null;
  state: JobState;
  target?: ProjectTaskTarget | null;
  [k: string]: any;
}
  export interface ArtifactReference {
  artifact_id: string;
  availability: ArtifactAvailability;
  content_hash: string;
  job_id: string;
  mime_type: string;
  role: ArtifactRole;
  size_bytes: string;
  [k: string]: any;
}
  export interface ExtendedJobResult {
  data: any;
  kind: string;
  schema_version: number;
  [k: string]: any;
}
  export interface PublicJobFailure {
  code: string;
  diagnostic_job_id: string;
  message: string;
  [k: string]: any;
}
  export interface ProgressView {
  fraction?: number | null;
  media_time?: Time | null;
  stage: PublicJobStage;
  [k: string]: any;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface JobResourceTarget {
  project_id: string;
  resource_id: string;
  resource_kind: string;
  [k: string]: any;
}
  export interface MediaInfo {
  bit_rate?: number | null;
  duration?: MediaTime | null;
  format_name?: string | null;
  streams: MediaStream[];
  [k: string]: any;
}
  export interface MediaTime {
  ticks: string;
  timebase: number;
}
  export interface MediaStream {
  average_frame_rate?: FrameRate | null;
  channel_layout?: string | null;
  channels?: number | null;
  codec_name?: string | null;
  codec_type: StreamKind;
  color_primaries?: string | null;
  color_space?: string | null;
  color_transfer?: string | null;
  duration?: MediaTime | null;
  height?: number | null;
  index: number;
  pixel_format?: string | null;
  sample_rate?: number | null;
  time_base?: FrameRate | null;
  width?: number | null;
  [k: string]: any;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface SourceFrameView {
  actual_time: MediaTime;
  alignment: FrameAlignment;
  exact: boolean;
  quality: FrameQuality;
  requested_time: MediaTime;
  source_pts: string;
  source_time_base: FrameRate;
  [k: string]: any;
}
  export interface ProjectFrameView {
  actual_sequence_time: Time;
  alignment: FrameAlignment;
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  height: number;
  quality: ProjectFrameQuality;
  requested_time: Time;
  revision: string;
  sequence_id: string;
  text_layouts: TextLayoutView[];
  width: number;
  [k: string]: any;
}
  export interface AssetLockManifest {
  assets: LockedAsset[];
  revision: string;
  sequence_id: string;
  version: number;
}
  export interface LockedAsset {
  asset_id: string;
  content_hash: string;
  size: number;
}
  export interface TextLayoutView {
  cluster_ranges: TextClusterRange[];
  glyph_count: number;
  line_count: number;
  pixel_bounds: PixelBounds;
  policy: FontResolutionPolicy;
  requested_family: string;
  rtl_runs: number;
  used_faces: FontFaceView[];
  [k: string]: any;
}
  export interface TextClusterRange {
  end: number;
  rtl: boolean;
  start: number;
  [k: string]: any;
}
  export interface PixelBounds {
  height: number;
  width: number;
  x: number;
  y: number;
  [k: string]: any;
}
  export interface FontFaceView {
  face_index: number;
  family: string;
  postscript_name: string;
  [k: string]: any;
}
  export interface CompositedFrameView {
  actual_sequence_time: Time;
  alignment: FrameAlignment;
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  height: number;
  quality: CompositedProjectFrameQuality;
  requested_time: Time;
  revision: string;
  sequence_id: string;
  text_layouts: TextLayoutView[];
  width: number;
  [k: string]: any;
}
  export interface ProjectRangeView {
  actual_sequence_range: TimeRange;
  alignment: FrameAlignment;
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  media: ExportView;
  quality: ProjectRangeQuality;
  requested_range: TimeRange;
  revision: string;
  sequence_id: string;
  [k: string]: any;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface ExportView {
  metadata: MediaInfo;
  text_layouts: TextLayoutView[];
  verification: VerificationView;
  [k: string]: any;
}
  export interface VerificationView {
  checks: VerificationCheckView[];
  decoded_samples: DecodedSample[];
  metadata: MediaInfo;
  passed: boolean;
  [k: string]: any;
}
  export interface VerificationCheckView {
  name: string;
  passed: boolean;
  [k: string]: any;
}
  export interface DecodedSample {
  decoded: boolean;
  requested_time: MediaTime;
  [k: string]: any;
}
  export interface ProjectRenderView {
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  media: ExportView;
  revision: string;
  [k: string]: any;
}
  export interface ProjectTaskTarget {
  project_id: string;
  revision: string;
  sequence_id: string;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutJobChangesInput {
  export interface JobChangesInput {
  cursor?: string | null;
  job_id?: string | null;
  limit?: number;
}

}

export namespace OpencutJobChangesOutput {
  export type JobChangeKind =
  | 'queued'
  | 'started'
  | 'progress'
  | 'succeeded'
  | 'failed'
  | 'cancel_requested'
  | 'cancelled'
  | 'recovered'
  | 'unknown';
  export type PublicJobStage =
  'resolving_assets' | 'probing' | 'selecting_frame' | 'decoding' | 'encoding' | 'verifying' | 'unknown';
  export type JobState = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

  export interface Response {
  api_version: string;
  data?: JobChangesPage | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface JobChangesPage {
  events: JobChange[];
  next_cursor?: string | null;
  [k: string]: any;
}
  export interface JobChange {
  created_at: string;
  event_id: string;
  event_type: JobChangeKind;
  job_id: string;
  progress?: ProgressView | null;
  state: JobState;
  [k: string]: any;
}
  export interface ProgressView {
  fraction?: number | null;
  media_time?: Time | null;
  stage: PublicJobStage;
  [k: string]: any;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutJobSubmitInput {
  export type JobStartInput =
  | {
      idempotency_key: string;
      input: ProbeRequest;
      kind: 'media.probe';
    }
  | {
      idempotency_key: string;
      input: FrameJobInput;
      kind: 'preview.frame';
    }
  | {
      idempotency_key: string;
      input: ProjectFrameInput;
      kind: 'preview.project_frame';
    }
  | {
      idempotency_key: string;
      input: ProjectFrameInput;
      kind: 'preview.composited_frame';
    }
  | {
      idempotency_key: string;
      input: ProjectRangeInput;
      kind: 'preview.project_range';
    }
  | {
      idempotency_key: string;
      input: ExportJobInput;
      kind: 'render.export';
    }
  | {
      idempotency_key: string;
      input: ProjectExportInput;
      kind: 'render.project';
    }
  | {
      idempotency_key: string;
      input: VerifyRequest;
      kind: 'artifact.verify';
    };
  export type FrameAlignment = 'exact' | 'nearest' | 'floor' | 'ceil';
  export type AudioOutput =
  | {
      gain_db?: number;
      mode: 'preserve';
    }
  | {
      mode: 'drop';
    };
  export type Container = 'mp4' | 'matroska';
  export type AspectMode = 'fit' | 'fill' | 'stretch';
  export type VideoCodec = 'h264' | 'hevc' | 'vp9';

  export interface ProbeRequest {
  input: string;
}
  export interface FrameJobInput {
  alignment: FrameAlignment;
  input: string;
  requested_time: MediaTime;
  video_stream?: number | null;
}
  export interface MediaTime {
  ticks: string;
  timebase: number;
}
  export interface ProjectFrameInput {
  alignment: FrameAlignment;
  project_id: string;
  requested_time: Time;
  revision: string;
  sequence_id: string;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface ProjectRangeInput {
  alignment: FrameAlignment;
  project_id: string;
  requested_range: TimeRange;
  revision: string;
  sequence_id: string;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface ExportJobInput {
  audio: AudioOutput;
  container: Container;
  segments: ExportSegment[];
  video: VideoOutput;
}
  export interface ExportSegment {
  audio_stream?: number | null;
  input: string;
  source_duration: MediaTime;
  source_start: MediaTime;
  video_stream?: number | null;
}
  export interface VideoOutput {
  aspect_mode: AspectMode;
  codec: VideoCodec;
  crf?: number | null;
  frame_rate: FrameRate;
  height: number;
  width: number;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface ProjectExportInput {
  container: Container;
  project_id: string;
  revision: string;
  sequence_id: string;
  video_codec: VideoCodec;
}
  export interface VerifyRequest {
  decode_samples?: number;
  duration_tolerance?: MediaTime | null;
  expected_audio_streams?: number | null;
  expected_duration?: MediaTime | null;
  expected_height?: number | null;
  expected_video_streams?: number | null;
  expected_width?: number | null;
  input: string;
}

}

export namespace OpencutJobSubmitOutput {
  export type ArtifactAvailability = 'unchecked';
  export type ArtifactRole = 'primary';
  export type PublicJobStage =
  'resolving_assets' | 'probing' | 'selecting_frame' | 'decoding' | 'encoding' | 'verifying' | 'unknown';
  export type JobResultView =
  | {
      data: MediaInfo;
      kind: 'media.probe';
      [k: string]: any;
    }
  | {
      data: SourceFrameView;
      kind: 'preview.frame';
      [k: string]: any;
    }
  | {
      data: ProjectFrameView;
      kind: 'preview.project_frame';
      [k: string]: any;
    }
  | {
      data: CompositedFrameView;
      kind: 'preview.composited_frame';
      [k: string]: any;
    }
  | {
      data: ProjectRangeView;
      kind: 'preview.project_range';
      [k: string]: any;
    }
  | {
      data: ExportView;
      kind: 'render.export';
      [k: string]: any;
    }
  | {
      data: ProjectRenderView;
      kind: 'render.project';
      [k: string]: any;
    }
  | {
      data: VerificationView;
      kind: 'artifact.verify';
      [k: string]: any;
    };
  export type StreamKind = 'video' | 'audio' | 'subtitle' | 'data' | 'attachment' | 'unknown';
  export type FrameAlignment = 'exact' | 'nearest' | 'floor' | 'ceil';
  export type FrameQuality = 'accurate_source_frame';
  export type AssetLockingMode = 'media_content_locked' | 'legacy_unlocked';
  export type ProjectFrameQuality = 'encoded_project_frame';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };
  export type CompositedProjectFrameQuality = 'composited_project_frame';
  export type ProjectRangeQuality = 'encoded_project_range';
  export type JobState = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

  export interface Response {
  api_version: string;
  data?: JobView | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface JobView {
  artifacts: ArtifactReference[];
  cancel_requested: boolean;
  /**
   * New task kinds use the open registry envelope; legacy result variants stay frozen.
   */
  extended_result?: ExtendedJobResult | null;
  failure?: PublicJobFailure | null;
  job_id: string;
  kind: string;
  progress?: ProgressView | null;
  resource_target?: JobResourceTarget | null;
  result?: JobResultView | null;
  result_unavailable?: PublicJobFailure | null;
  state: JobState;
  target?: ProjectTaskTarget | null;
  [k: string]: any;
}
  export interface ArtifactReference {
  artifact_id: string;
  availability: ArtifactAvailability;
  content_hash: string;
  job_id: string;
  mime_type: string;
  role: ArtifactRole;
  size_bytes: string;
  [k: string]: any;
}
  export interface ExtendedJobResult {
  data: any;
  kind: string;
  schema_version: number;
  [k: string]: any;
}
  export interface PublicJobFailure {
  code: string;
  diagnostic_job_id: string;
  message: string;
  [k: string]: any;
}
  export interface ProgressView {
  fraction?: number | null;
  media_time?: Time | null;
  stage: PublicJobStage;
  [k: string]: any;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface JobResourceTarget {
  project_id: string;
  resource_id: string;
  resource_kind: string;
  [k: string]: any;
}
  export interface MediaInfo {
  bit_rate?: number | null;
  duration?: MediaTime | null;
  format_name?: string | null;
  streams: MediaStream[];
  [k: string]: any;
}
  export interface MediaTime {
  ticks: string;
  timebase: number;
}
  export interface MediaStream {
  average_frame_rate?: FrameRate | null;
  channel_layout?: string | null;
  channels?: number | null;
  codec_name?: string | null;
  codec_type: StreamKind;
  color_primaries?: string | null;
  color_space?: string | null;
  color_transfer?: string | null;
  duration?: MediaTime | null;
  height?: number | null;
  index: number;
  pixel_format?: string | null;
  sample_rate?: number | null;
  time_base?: FrameRate | null;
  width?: number | null;
  [k: string]: any;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface SourceFrameView {
  actual_time: MediaTime;
  alignment: FrameAlignment;
  exact: boolean;
  quality: FrameQuality;
  requested_time: MediaTime;
  source_pts: string;
  source_time_base: FrameRate;
  [k: string]: any;
}
  export interface ProjectFrameView {
  actual_sequence_time: Time;
  alignment: FrameAlignment;
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  height: number;
  quality: ProjectFrameQuality;
  requested_time: Time;
  revision: string;
  sequence_id: string;
  text_layouts: TextLayoutView[];
  width: number;
  [k: string]: any;
}
  export interface AssetLockManifest {
  assets: LockedAsset[];
  revision: string;
  sequence_id: string;
  version: number;
}
  export interface LockedAsset {
  asset_id: string;
  content_hash: string;
  size: number;
}
  export interface TextLayoutView {
  cluster_ranges: TextClusterRange[];
  glyph_count: number;
  line_count: number;
  pixel_bounds: PixelBounds;
  policy: FontResolutionPolicy;
  requested_family: string;
  rtl_runs: number;
  used_faces: FontFaceView[];
  [k: string]: any;
}
  export interface TextClusterRange {
  end: number;
  rtl: boolean;
  start: number;
  [k: string]: any;
}
  export interface PixelBounds {
  height: number;
  width: number;
  x: number;
  y: number;
  [k: string]: any;
}
  export interface FontFaceView {
  face_index: number;
  family: string;
  postscript_name: string;
  [k: string]: any;
}
  export interface CompositedFrameView {
  actual_sequence_time: Time;
  alignment: FrameAlignment;
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  height: number;
  quality: CompositedProjectFrameQuality;
  requested_time: Time;
  revision: string;
  sequence_id: string;
  text_layouts: TextLayoutView[];
  width: number;
  [k: string]: any;
}
  export interface ProjectRangeView {
  actual_sequence_range: TimeRange;
  alignment: FrameAlignment;
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  media: ExportView;
  quality: ProjectRangeQuality;
  requested_range: TimeRange;
  revision: string;
  sequence_id: string;
  [k: string]: any;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface ExportView {
  metadata: MediaInfo;
  text_layouts: TextLayoutView[];
  verification: VerificationView;
  [k: string]: any;
}
  export interface VerificationView {
  checks: VerificationCheckView[];
  decoded_samples: DecodedSample[];
  metadata: MediaInfo;
  passed: boolean;
  [k: string]: any;
}
  export interface VerificationCheckView {
  name: string;
  passed: boolean;
  [k: string]: any;
}
  export interface DecodedSample {
  decoded: boolean;
  requested_time: MediaTime;
  [k: string]: any;
}
  export interface ProjectRenderView {
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  media: ExportView;
  revision: string;
  [k: string]: any;
}
  export interface ProjectTaskTarget {
  project_id: string;
  revision: string;
  sequence_id: string;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutJobRequestCancelInput {
  export interface JobInput {
  job_id: string;
}

}

export namespace OpencutJobRequestCancelOutput {
  export type ArtifactAvailability = 'unchecked';
  export type ArtifactRole = 'primary';
  export type PublicJobStage =
  'resolving_assets' | 'probing' | 'selecting_frame' | 'decoding' | 'encoding' | 'verifying' | 'unknown';
  export type JobResultView =
  | {
      data: MediaInfo;
      kind: 'media.probe';
      [k: string]: any;
    }
  | {
      data: SourceFrameView;
      kind: 'preview.frame';
      [k: string]: any;
    }
  | {
      data: ProjectFrameView;
      kind: 'preview.project_frame';
      [k: string]: any;
    }
  | {
      data: CompositedFrameView;
      kind: 'preview.composited_frame';
      [k: string]: any;
    }
  | {
      data: ProjectRangeView;
      kind: 'preview.project_range';
      [k: string]: any;
    }
  | {
      data: ExportView;
      kind: 'render.export';
      [k: string]: any;
    }
  | {
      data: ProjectRenderView;
      kind: 'render.project';
      [k: string]: any;
    }
  | {
      data: VerificationView;
      kind: 'artifact.verify';
      [k: string]: any;
    };
  export type StreamKind = 'video' | 'audio' | 'subtitle' | 'data' | 'attachment' | 'unknown';
  export type FrameAlignment = 'exact' | 'nearest' | 'floor' | 'ceil';
  export type FrameQuality = 'accurate_source_frame';
  export type AssetLockingMode = 'media_content_locked' | 'legacy_unlocked';
  export type ProjectFrameQuality = 'encoded_project_frame';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };
  export type CompositedProjectFrameQuality = 'composited_project_frame';
  export type ProjectRangeQuality = 'encoded_project_range';
  export type JobState = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

  export interface Response {
  api_version: string;
  data?: JobView | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface JobView {
  artifacts: ArtifactReference[];
  cancel_requested: boolean;
  /**
   * New task kinds use the open registry envelope; legacy result variants stay frozen.
   */
  extended_result?: ExtendedJobResult | null;
  failure?: PublicJobFailure | null;
  job_id: string;
  kind: string;
  progress?: ProgressView | null;
  resource_target?: JobResourceTarget | null;
  result?: JobResultView | null;
  result_unavailable?: PublicJobFailure | null;
  state: JobState;
  target?: ProjectTaskTarget | null;
  [k: string]: any;
}
  export interface ArtifactReference {
  artifact_id: string;
  availability: ArtifactAvailability;
  content_hash: string;
  job_id: string;
  mime_type: string;
  role: ArtifactRole;
  size_bytes: string;
  [k: string]: any;
}
  export interface ExtendedJobResult {
  data: any;
  kind: string;
  schema_version: number;
  [k: string]: any;
}
  export interface PublicJobFailure {
  code: string;
  diagnostic_job_id: string;
  message: string;
  [k: string]: any;
}
  export interface ProgressView {
  fraction?: number | null;
  media_time?: Time | null;
  stage: PublicJobStage;
  [k: string]: any;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface JobResourceTarget {
  project_id: string;
  resource_id: string;
  resource_kind: string;
  [k: string]: any;
}
  export interface MediaInfo {
  bit_rate?: number | null;
  duration?: MediaTime | null;
  format_name?: string | null;
  streams: MediaStream[];
  [k: string]: any;
}
  export interface MediaTime {
  ticks: string;
  timebase: number;
}
  export interface MediaStream {
  average_frame_rate?: FrameRate | null;
  channel_layout?: string | null;
  channels?: number | null;
  codec_name?: string | null;
  codec_type: StreamKind;
  color_primaries?: string | null;
  color_space?: string | null;
  color_transfer?: string | null;
  duration?: MediaTime | null;
  height?: number | null;
  index: number;
  pixel_format?: string | null;
  sample_rate?: number | null;
  time_base?: FrameRate | null;
  width?: number | null;
  [k: string]: any;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface SourceFrameView {
  actual_time: MediaTime;
  alignment: FrameAlignment;
  exact: boolean;
  quality: FrameQuality;
  requested_time: MediaTime;
  source_pts: string;
  source_time_base: FrameRate;
  [k: string]: any;
}
  export interface ProjectFrameView {
  actual_sequence_time: Time;
  alignment: FrameAlignment;
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  height: number;
  quality: ProjectFrameQuality;
  requested_time: Time;
  revision: string;
  sequence_id: string;
  text_layouts: TextLayoutView[];
  width: number;
  [k: string]: any;
}
  export interface AssetLockManifest {
  assets: LockedAsset[];
  revision: string;
  sequence_id: string;
  version: number;
}
  export interface LockedAsset {
  asset_id: string;
  content_hash: string;
  size: number;
}
  export interface TextLayoutView {
  cluster_ranges: TextClusterRange[];
  glyph_count: number;
  line_count: number;
  pixel_bounds: PixelBounds;
  policy: FontResolutionPolicy;
  requested_family: string;
  rtl_runs: number;
  used_faces: FontFaceView[];
  [k: string]: any;
}
  export interface TextClusterRange {
  end: number;
  rtl: boolean;
  start: number;
  [k: string]: any;
}
  export interface PixelBounds {
  height: number;
  width: number;
  x: number;
  y: number;
  [k: string]: any;
}
  export interface FontFaceView {
  face_index: number;
  family: string;
  postscript_name: string;
  [k: string]: any;
}
  export interface CompositedFrameView {
  actual_sequence_time: Time;
  alignment: FrameAlignment;
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  height: number;
  quality: CompositedProjectFrameQuality;
  requested_time: Time;
  revision: string;
  sequence_id: string;
  text_layouts: TextLayoutView[];
  width: number;
  [k: string]: any;
}
  export interface ProjectRangeView {
  actual_sequence_range: TimeRange;
  alignment: FrameAlignment;
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  media: ExportView;
  quality: ProjectRangeQuality;
  requested_range: TimeRange;
  revision: string;
  sequence_id: string;
  [k: string]: any;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface ExportView {
  metadata: MediaInfo;
  text_layouts: TextLayoutView[];
  verification: VerificationView;
  [k: string]: any;
}
  export interface VerificationView {
  checks: VerificationCheckView[];
  decoded_samples: DecodedSample[];
  metadata: MediaInfo;
  passed: boolean;
  [k: string]: any;
}
  export interface VerificationCheckView {
  name: string;
  passed: boolean;
  [k: string]: any;
}
  export interface DecodedSample {
  decoded: boolean;
  requested_time: MediaTime;
  [k: string]: any;
}
  export interface ProjectRenderView {
  asset_lock?: AssetLockManifest | null;
  asset_locking: AssetLockingMode;
  media: ExportView;
  revision: string;
  [k: string]: any;
}
  export interface ProjectTaskTarget {
  project_id: string;
  revision: string;
  sequence_id: string;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutMediaInspectInput {
  export interface InspectMediaInput {
  content_hash: string;
}

}

export namespace OpencutMediaInspectOutput {
  export type AssetKind = 'video' | 'audio' | 'image' | 'text' | 'subtitle' | 'graphic';

  export interface Response {
  api_version: string;
  data?: CatalogEntry | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface CatalogEntry {
  asset?: Asset | null;
  content_hash: string;
  /**
   * Compare-and-repair token over exact stored JSON, including malformed JSON.
   */
  metadata_hash?: string | null;
}
  export interface Asset {
  duration?: Time | null;
  id: string;
  kind: AssetKind;
  metadata?: {
    [k: string]: any;
  };
  name: string;
  uri: string;
  version?: AssetVersion | null;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface AssetVersion {
  content_hash: string;
  id: string;
  size: number;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutMediaRepairInput {
  export interface RepairMediaInput {
  content_hash: string;
  expected_metadata_hash?: string | null;
}

}

export namespace OpencutMediaRepairOutput {
  export type AssetKind = 'video' | 'audio' | 'image' | 'text' | 'subtitle' | 'graphic';

  export interface Response {
  api_version: string;
  data?: CatalogEntry | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface CatalogEntry {
  asset?: Asset | null;
  content_hash: string;
  /**
   * Compare-and-repair token over exact stored JSON, including malformed JSON.
   */
  metadata_hash?: string | null;
}
  export interface Asset {
  duration?: Time | null;
  id: string;
  kind: AssetKind;
  metadata?: {
    [k: string]: any;
  };
  name: string;
  uri: string;
  version?: AssetVersion | null;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface AssetVersion {
  content_hash: string;
  id: string;
  size: number;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutProjectHistoryInput {
  export interface HistoryInput {
  from_revision?: string | null;
  limit?: number;
  project_id: string;
}

}

export namespace OpencutProjectHistoryOutput {
  export interface Response {
  api_version: string;
  data?: RevisionPage | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface RevisionPage {
  head_revision: string;
  next_cursor?: string | null;
  project_id: string;
  revisions: RevisionSummary[];
  [k: string]: any;
}
  export interface RevisionSummary {
  created_at: string;
  label?: string | null;
  parent_revision?: string | null;
  revision: string;
  transaction_id?: string | null;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutRevisionInspectInput {
  export interface RevisionInput {
  project_id: string;
  revision: string;
}

}

export namespace OpencutRevisionInspectOutput {
  export type AssetKind = 'video' | 'audio' | 'image' | 'text' | 'subtitle' | 'graphic';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };
  export type TextAlign = 'left' | 'center' | 'right';
  export type KeyframeInterpolation = 'hold' | 'linear';
  export type TrackKind = 'video' | 'audio' | 'caption' | 'graphic';

  export interface Response {
  api_version: string;
  data?: ProjectSnapshot | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface ProjectSnapshot {
  project: ProjectDocument;
  revision: string;
}
  export interface ProjectDocument {
  assets: Asset[];
  default_sequence_id: string;
  id: string;
  name: string;
  schema_version: number;
  sequences: Sequence[];
}
  export interface Asset {
  duration?: Time | null;
  id: string;
  kind: AssetKind;
  metadata?: {
    [k: string]: any;
  };
  name: string;
  uri: string;
  version?: AssetVersion | null;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface AssetVersion {
  content_hash: string;
  id: string;
  size: number;
}
  export interface Sequence {
  caption_cues?: CaptionCue[];
  fps: FrameRate;
  height: number;
  id: string;
  name: string;
  tracks: Track[];
  width: number;
}
  export interface CaptionCue {
  id: string;
  source_anchor?: CaptionSourceAnchor | null;
  style: CaptionStyle;
  text: string;
  timeline_range: TimeRange;
}
  export interface CaptionSourceAnchor {
  asset_id: string;
  source_range: TimeRange;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface CaptionStyle {
  background_color?: string | null;
  color: string;
  font_family: string;
  font_policy?: FontResolutionPolicy;
  font_size: number;
  /**
   * Line height in canvas pixels. None derives it from font_size.
   */
  line_height?: number | null;
  /**
   * Maximum layout width in canvas pixels. None uses the backend's canvas margin.
   */
  max_width?: number | null;
  text_align?: TextAlign;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface Track {
  clips: Clip[];
  enabled: boolean;
  id: string;
  kind: TrackKind;
  locked: boolean;
  muted: boolean;
  name: string;
  params?: {
    [k: string]: any;
  };
}
  export interface Clip {
  asset_id?: string | null;
  channels?: KeyframeChannel[];
  id: string;
  params?: {
    [k: string]: any;
  };
  source_range?: TimeRange | null;
  timeline_range: TimeRange;
}
  export interface KeyframeChannel {
  keyframes: Keyframe[];
  parameter: string;
}
  export interface Keyframe {
  id: string;
  interpolation: KeyframeInterpolation;
  time: Time;
  value: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutRevisionRestoreInput {
  export interface RestoreInput {
  project_id: string;
  request: RestoreRevisionRequest;
}
  export interface RestoreRevisionRequest {
  expected_revision: string;
  idempotency_key: string;
  label: string;
  target_revision: string;
}

}

export namespace OpencutRevisionRestoreOutput {
  export interface Response {
  api_version: string;
  data?: TransactionResult | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface TransactionResult {
  base_revision: string;
  changed_ids: string[];
  created_ids: string[];
  deleted_ids: string[];
  revision: string;
  transaction_id: string;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutAuditQueryInput {
  export type AuditTarget =
  | {
      kind: 'workspace';
    }
  | {
      id: string;
      kind: 'project';
    }
  | {
      id: string;
      kind: 'transaction';
    }
  | {
      id: string;
      kind: 'job';
    }
  | {
      id: string;
      kind: 'request';
    };

  export interface AuditQuery {
  after_sequence?: string;
  limit?: number;
  target: AuditTarget;
}

}

export namespace OpencutAuditQueryOutput {
  export type AuditCoverage = 'recorded_mutations_since_schema4';
  export type CallSource = 'cli' | 'http' | 'mcp_stdio' | 'mcp_http' | 'embedded' | 'worker';

  export interface Response {
  api_version: string;
  data?: AuditPage | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface AuditPage {
  coverage: AuditCoverage;
  has_more: boolean;
  last_sequence: string;
  records: AuditRecord[];
  [k: string]: any;
}
  export interface AuditRecord {
  action: string;
  actor: Actor;
  artifact_ids: string[];
  attempt?: number | null;
  audit_id: string;
  created_at: string;
  job_id?: string | null;
  origin_request_id?: string | null;
  project_id?: string | null;
  request_id: string;
  revision?: string | null;
  sequence: string;
  transaction_id?: string | null;
  [k: string]: any;
}
  export interface Actor {
  principal_id: string;
  source: CallSource;
  worker_id?: string | null;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutCapabilitiesInput {
  export interface EmptyInput {}

}

export namespace OpencutCapabilitiesOutput {
  export interface Response {
  api_version: string;
  data?: Capabilities | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface Capabilities {
  api_version: string;
  features: Features;
  host: string;
  job_execution: JobExecution;
  job_kinds: JobKindDescriptor[];
  limits: Limits;
  operations: OperationDescriptor[];
  parameters: ParameterDescriptor[];
  project_schema_version: number;
  rendering: Rendering;
  tools: string[];
  [k: string]: any;
}
  export interface Features {
  atomic_transactions: boolean;
  durable_events: boolean;
  edit_preview_plan: boolean;
  idempotent_writes: boolean;
  multiple_branches: boolean;
  new_asset_bindings_versioned: boolean;
  optimistic_concurrency: boolean;
  persistent_jobs: boolean;
  persistent_projects: boolean;
  plugin_execution: boolean;
  revision_history: boolean;
  revision_restore: boolean;
  timeline_rendering: boolean;
  [k: string]: any;
}
  export interface JobExecution {
  durable_progress: boolean;
  managed_output: boolean;
  progress_fraction_scope: string;
  project_asset_locking: boolean;
  service_starts_worker: boolean;
  worker_required: boolean;
  [k: string]: any;
}
  export interface JobKindDescriptor {
  input_schema: any;
  name: string;
  result_schema: any;
  [k: string]: any;
}
  export interface Limits {
  event_page_size: number;
  input_bytes: number;
  [k: string]: any;
}
  export interface OperationDescriptor {
  conflict_policy: string;
  declared_effects: string[];
  deterministic: boolean;
  idempotency_policy: string;
  kind: string;
  name: string;
  required_capabilities: string[];
  version: number;
}
  export interface ParameterDescriptor {
  animatable: boolean;
  default_value: any;
  interpolation: string[];
  maximum?: number | null;
  minimum?: number | null;
  name: string;
  target: string;
  unit: string;
  value_type: string;
  version: number;
}
  export interface Rendering {
  audio_mix: boolean;
  availability: string;
  backend: string;
  composited_frame_execution: string;
  composited_frame_preview: boolean;
  constant_rate: boolean;
  fixed_revision: boolean;
  images: boolean;
  keyframes: boolean;
  multiple_video_tracks: boolean;
  preview_frame_execution: string;
  preview_range_execution: string;
  project_frame_preview: boolean;
  project_range_preview: boolean;
  requires_executables: string[];
  sequential_video: boolean;
  source_audio: boolean;
  static_captions: boolean;
  text_shaping: boolean;
  visual_transforms: boolean;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutProjectListInput {
  export interface EmptyInput {}

}

export namespace OpencutProjectListOutput {
  export type AssetKind = 'video' | 'audio' | 'image' | 'text' | 'subtitle' | 'graphic';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };
  export type TextAlign = 'left' | 'center' | 'right';
  export type KeyframeInterpolation = 'hold' | 'linear';
  export type TrackKind = 'video' | 'audio' | 'caption' | 'graphic';

  export interface Response {
  api_version: string;
  data?: ProjectSnapshot[] | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface ProjectSnapshot {
  project: ProjectDocument;
  revision: string;
}
  export interface ProjectDocument {
  assets: Asset[];
  default_sequence_id: string;
  id: string;
  name: string;
  schema_version: number;
  sequences: Sequence[];
}
  export interface Asset {
  duration?: Time | null;
  id: string;
  kind: AssetKind;
  metadata?: {
    [k: string]: any;
  };
  name: string;
  uri: string;
  version?: AssetVersion | null;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface AssetVersion {
  content_hash: string;
  id: string;
  size: number;
}
  export interface Sequence {
  caption_cues?: CaptionCue[];
  fps: FrameRate;
  height: number;
  id: string;
  name: string;
  tracks: Track[];
  width: number;
}
  export interface CaptionCue {
  id: string;
  source_anchor?: CaptionSourceAnchor | null;
  style: CaptionStyle;
  text: string;
  timeline_range: TimeRange;
}
  export interface CaptionSourceAnchor {
  asset_id: string;
  source_range: TimeRange;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface CaptionStyle {
  background_color?: string | null;
  color: string;
  font_family: string;
  font_policy?: FontResolutionPolicy;
  font_size: number;
  /**
   * Line height in canvas pixels. None derives it from font_size.
   */
  line_height?: number | null;
  /**
   * Maximum layout width in canvas pixels. None uses the backend's canvas margin.
   */
  max_width?: number | null;
  text_align?: TextAlign;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface Track {
  clips: Clip[];
  enabled: boolean;
  id: string;
  kind: TrackKind;
  locked: boolean;
  muted: boolean;
  name: string;
  params?: {
    [k: string]: any;
  };
}
  export interface Clip {
  asset_id?: string | null;
  channels?: KeyframeChannel[];
  id: string;
  params?: {
    [k: string]: any;
  };
  source_range?: TimeRange | null;
  timeline_range: TimeRange;
}
  export interface KeyframeChannel {
  keyframes: Keyframe[];
  parameter: string;
}
  export interface Keyframe {
  id: string;
  interpolation: KeyframeInterpolation;
  time: Time;
  value: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutProjectCreateInput {
  export interface ProjectCreateInput {
  idempotency_key: string;
  project: CreateProject;
}
  export interface CreateProject {
  fps: FrameRate;
  height: number;
  id: string;
  name: string;
  sequence_id: string;
  width: number;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}

}

export namespace OpencutProjectCreateOutput {
  export type AssetKind = 'video' | 'audio' | 'image' | 'text' | 'subtitle' | 'graphic';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };
  export type TextAlign = 'left' | 'center' | 'right';
  export type KeyframeInterpolation = 'hold' | 'linear';
  export type TrackKind = 'video' | 'audio' | 'caption' | 'graphic';

  export interface Response {
  api_version: string;
  data?: ProjectSnapshot | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface ProjectSnapshot {
  project: ProjectDocument;
  revision: string;
}
  export interface ProjectDocument {
  assets: Asset[];
  default_sequence_id: string;
  id: string;
  name: string;
  schema_version: number;
  sequences: Sequence[];
}
  export interface Asset {
  duration?: Time | null;
  id: string;
  kind: AssetKind;
  metadata?: {
    [k: string]: any;
  };
  name: string;
  uri: string;
  version?: AssetVersion | null;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface AssetVersion {
  content_hash: string;
  id: string;
  size: number;
}
  export interface Sequence {
  caption_cues?: CaptionCue[];
  fps: FrameRate;
  height: number;
  id: string;
  name: string;
  tracks: Track[];
  width: number;
}
  export interface CaptionCue {
  id: string;
  source_anchor?: CaptionSourceAnchor | null;
  style: CaptionStyle;
  text: string;
  timeline_range: TimeRange;
}
  export interface CaptionSourceAnchor {
  asset_id: string;
  source_range: TimeRange;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface CaptionStyle {
  background_color?: string | null;
  color: string;
  font_family: string;
  font_policy?: FontResolutionPolicy;
  font_size: number;
  /**
   * Line height in canvas pixels. None derives it from font_size.
   */
  line_height?: number | null;
  /**
   * Maximum layout width in canvas pixels. None uses the backend's canvas margin.
   */
  max_width?: number | null;
  text_align?: TextAlign;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface Track {
  clips: Clip[];
  enabled: boolean;
  id: string;
  kind: TrackKind;
  locked: boolean;
  muted: boolean;
  name: string;
  params?: {
    [k: string]: any;
  };
}
  export interface Clip {
  asset_id?: string | null;
  channels?: KeyframeChannel[];
  id: string;
  params?: {
    [k: string]: any;
  };
  source_range?: TimeRange | null;
  timeline_range: TimeRange;
}
  export interface KeyframeChannel {
  keyframes: Keyframe[];
  parameter: string;
}
  export interface Keyframe {
  id: string;
  interpolation: KeyframeInterpolation;
  time: Time;
  value: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutProjectInspectInput {
  export interface ProjectInput {
  project_id: string;
}

}

export namespace OpencutProjectInspectOutput {
  export type AssetKind = 'video' | 'audio' | 'image' | 'text' | 'subtitle' | 'graphic';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };
  export type TextAlign = 'left' | 'center' | 'right';
  export type KeyframeInterpolation = 'hold' | 'linear';
  export type TrackKind = 'video' | 'audio' | 'caption' | 'graphic';

  export interface Response {
  api_version: string;
  data?: ProjectSnapshot | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface ProjectSnapshot {
  project: ProjectDocument;
  revision: string;
}
  export interface ProjectDocument {
  assets: Asset[];
  default_sequence_id: string;
  id: string;
  name: string;
  schema_version: number;
  sequences: Sequence[];
}
  export interface Asset {
  duration?: Time | null;
  id: string;
  kind: AssetKind;
  metadata?: {
    [k: string]: any;
  };
  name: string;
  uri: string;
  version?: AssetVersion | null;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface AssetVersion {
  content_hash: string;
  id: string;
  size: number;
}
  export interface Sequence {
  caption_cues?: CaptionCue[];
  fps: FrameRate;
  height: number;
  id: string;
  name: string;
  tracks: Track[];
  width: number;
}
  export interface CaptionCue {
  id: string;
  source_anchor?: CaptionSourceAnchor | null;
  style: CaptionStyle;
  text: string;
  timeline_range: TimeRange;
}
  export interface CaptionSourceAnchor {
  asset_id: string;
  source_range: TimeRange;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface CaptionStyle {
  background_color?: string | null;
  color: string;
  font_family: string;
  font_policy?: FontResolutionPolicy;
  font_size: number;
  /**
   * Line height in canvas pixels. None derives it from font_size.
   */
  line_height?: number | null;
  /**
   * Maximum layout width in canvas pixels. None uses the backend's canvas margin.
   */
  max_width?: number | null;
  text_align?: TextAlign;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface Track {
  clips: Clip[];
  enabled: boolean;
  id: string;
  kind: TrackKind;
  locked: boolean;
  muted: boolean;
  name: string;
  params?: {
    [k: string]: any;
  };
}
  export interface Clip {
  asset_id?: string | null;
  channels?: KeyframeChannel[];
  id: string;
  params?: {
    [k: string]: any;
  };
  source_range?: TimeRange | null;
  timeline_range: TimeRange;
}
  export interface KeyframeChannel {
  keyframes: Keyframe[];
  parameter: string;
}
  export interface Keyframe {
  id: string;
  interpolation: KeyframeInterpolation;
  time: Time;
  value: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutEditPlanInput {
  export type Operation =
  | {
      fps?: FrameRate | null;
      height?: number | null;
      name?: string | null;
      sequence_id: string;
      type: 'sequence.update_settings';
      width?: number | null;
    }
  | {
      sequence_id: string;
      track: Track;
      type: 'track.create';
    }
  | {
      sequence_id: string;
      track_id: string;
      type: 'track.delete';
    }
  | {
      asset: Asset;
      type: 'asset.bind';
    }
  | {
      asset: Asset;
      type: 'asset.replace';
    }
  | {
      asset_id: string;
      type: 'asset.unbind';
    }
  | {
      clip: Clip;
      sequence_id: string;
      track_id: string;
      type: 'clip.insert';
    }
  | {
      at: Time;
      clip_id: string;
      right_clip_id: string;
      sequence_id: string;
      track_id: string;
      type: 'clip.split';
    }
  | {
      clip_id: string;
      destination_track_id: string;
      sequence_id: string;
      start: Time;
      track_id: string;
      type: 'clip.move';
    }
  | {
      clip_id: string;
      sequence_id: string;
      source_range?: TimeRange | null;
      timeline_range: TimeRange;
      track_id: string;
      type: 'clip.trim';
    }
  | {
      clip_id: string;
      sequence_id: string;
      track_id: string;
      type: 'clip.delete';
    }
  | {
      clip_id: string;
      params: {
        [k: string]: any;
      };
      sequence_id: string;
      track_id: string;
      type: 'parameter.set_clip';
    }
  | {
      params: {
        [k: string]: any;
      };
      sequence_id: string;
      track_id: string;
      type: 'parameter.set_track';
    }
  | {
      enabled?: boolean | null;
      locked?: boolean | null;
      muted?: boolean | null;
      name?: string | null;
      sequence_id: string;
      track_id: string;
      type: 'track.update';
    }
  | {
      clip_id: string;
      sequence_id: string;
      timeline_duration: Time;
      track_id: string;
      type: 'clip.retime';
    }
  | {
      channel: KeyframeChannel;
      clip_id: string;
      sequence_id: string;
      track_id: string;
      type: 'keyframe.channel_set';
    }
  | {
      clip_id: string;
      parameter: string;
      sequence_id: string;
      track_id: string;
      type: 'keyframe.channel_delete';
    }
  | {
      cue: CaptionCue;
      sequence_id: string;
      type: 'caption.insert';
    }
  | {
      cue: CaptionCue;
      sequence_id: string;
      type: 'caption.update';
    }
  | {
      cue_id: string;
      sequence_id: string;
      type: 'caption.delete';
    };
  export type KeyframeInterpolation = 'hold' | 'linear';
  export type TrackKind = 'video' | 'audio' | 'caption' | 'graphic';
  export type AssetKind = 'video' | 'audio' | 'image' | 'text' | 'subtitle' | 'graphic';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };
  export type TextAlign = 'left' | 'center' | 'right';

  export interface EditInput {
  project_id: string;
  request: TransactionRequest;
}
  export interface TransactionRequest {
  expected_revision: string;
  idempotency_key: string;
  label: string;
  operations: Operation[];
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface Track {
  clips: Clip[];
  enabled: boolean;
  id: string;
  kind: TrackKind;
  locked: boolean;
  muted: boolean;
  name: string;
  params?: {
    [k: string]: any;
  };
}
  export interface Clip {
  asset_id?: string | null;
  channels?: KeyframeChannel[];
  id: string;
  params?: {
    [k: string]: any;
  };
  source_range?: TimeRange | null;
  timeline_range: TimeRange;
}
  export interface KeyframeChannel {
  keyframes: Keyframe[];
  parameter: string;
}
  export interface Keyframe {
  id: string;
  interpolation: KeyframeInterpolation;
  time: Time;
  value: any;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface Asset {
  duration?: Time | null;
  id: string;
  kind: AssetKind;
  metadata?: {
    [k: string]: any;
  };
  name: string;
  uri: string;
  version?: AssetVersion | null;
}
  export interface AssetVersion {
  content_hash: string;
  id: string;
  size: number;
}
  export interface CaptionCue {
  id: string;
  source_anchor?: CaptionSourceAnchor | null;
  style: CaptionStyle;
  text: string;
  timeline_range: TimeRange;
}
  export interface CaptionSourceAnchor {
  asset_id: string;
  source_range: TimeRange;
}
  export interface CaptionStyle {
  background_color?: string | null;
  color: string;
  font_family: string;
  font_policy?: FontResolutionPolicy;
  font_size: number;
  /**
   * Line height in canvas pixels. None derives it from font_size.
   */
  line_height?: number | null;
  /**
   * Maximum layout width in canvas pixels. None uses the backend's canvas margin.
   */
  max_width?: number | null;
  text_align?: TextAlign;
}

}

export namespace OpencutEditPlanOutput {
  export type AssetKind = 'video' | 'audio' | 'image' | 'text' | 'subtitle' | 'graphic';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };
  export type TextAlign = 'left' | 'center' | 'right';
  export type KeyframeInterpolation = 'hold' | 'linear';
  export type TrackKind = 'video' | 'audio' | 'caption' | 'graphic';

  export interface Response {
  api_version: string;
  data?: EditPlan | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface EditPlan {
  base_revision: string;
  changed_ids: string[];
  created_ids: string[];
  deleted_ids: string[];
  proposed_project: ProjectDocument;
  unresolved_asset_ids?: string[];
}
  export interface ProjectDocument {
  assets: Asset[];
  default_sequence_id: string;
  id: string;
  name: string;
  schema_version: number;
  sequences: Sequence[];
}
  export interface Asset {
  duration?: Time | null;
  id: string;
  kind: AssetKind;
  metadata?: {
    [k: string]: any;
  };
  name: string;
  uri: string;
  version?: AssetVersion | null;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface AssetVersion {
  content_hash: string;
  id: string;
  size: number;
}
  export interface Sequence {
  caption_cues?: CaptionCue[];
  fps: FrameRate;
  height: number;
  id: string;
  name: string;
  tracks: Track[];
  width: number;
}
  export interface CaptionCue {
  id: string;
  source_anchor?: CaptionSourceAnchor | null;
  style: CaptionStyle;
  text: string;
  timeline_range: TimeRange;
}
  export interface CaptionSourceAnchor {
  asset_id: string;
  source_range: TimeRange;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface CaptionStyle {
  background_color?: string | null;
  color: string;
  font_family: string;
  font_policy?: FontResolutionPolicy;
  font_size: number;
  /**
   * Line height in canvas pixels. None derives it from font_size.
   */
  line_height?: number | null;
  /**
   * Maximum layout width in canvas pixels. None uses the backend's canvas margin.
   */
  max_width?: number | null;
  text_align?: TextAlign;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface Track {
  clips: Clip[];
  enabled: boolean;
  id: string;
  kind: TrackKind;
  locked: boolean;
  muted: boolean;
  name: string;
  params?: {
    [k: string]: any;
  };
}
  export interface Clip {
  asset_id?: string | null;
  channels?: KeyframeChannel[];
  id: string;
  params?: {
    [k: string]: any;
  };
  source_range?: TimeRange | null;
  timeline_range: TimeRange;
}
  export interface KeyframeChannel {
  keyframes: Keyframe[];
  parameter: string;
}
  export interface Keyframe {
  id: string;
  interpolation: KeyframeInterpolation;
  time: Time;
  value: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutEditApplyInput {
  export type Operation =
  | {
      fps?: FrameRate | null;
      height?: number | null;
      name?: string | null;
      sequence_id: string;
      type: 'sequence.update_settings';
      width?: number | null;
    }
  | {
      sequence_id: string;
      track: Track;
      type: 'track.create';
    }
  | {
      sequence_id: string;
      track_id: string;
      type: 'track.delete';
    }
  | {
      asset: Asset;
      type: 'asset.bind';
    }
  | {
      asset: Asset;
      type: 'asset.replace';
    }
  | {
      asset_id: string;
      type: 'asset.unbind';
    }
  | {
      clip: Clip;
      sequence_id: string;
      track_id: string;
      type: 'clip.insert';
    }
  | {
      at: Time;
      clip_id: string;
      right_clip_id: string;
      sequence_id: string;
      track_id: string;
      type: 'clip.split';
    }
  | {
      clip_id: string;
      destination_track_id: string;
      sequence_id: string;
      start: Time;
      track_id: string;
      type: 'clip.move';
    }
  | {
      clip_id: string;
      sequence_id: string;
      source_range?: TimeRange | null;
      timeline_range: TimeRange;
      track_id: string;
      type: 'clip.trim';
    }
  | {
      clip_id: string;
      sequence_id: string;
      track_id: string;
      type: 'clip.delete';
    }
  | {
      clip_id: string;
      params: {
        [k: string]: any;
      };
      sequence_id: string;
      track_id: string;
      type: 'parameter.set_clip';
    }
  | {
      params: {
        [k: string]: any;
      };
      sequence_id: string;
      track_id: string;
      type: 'parameter.set_track';
    }
  | {
      enabled?: boolean | null;
      locked?: boolean | null;
      muted?: boolean | null;
      name?: string | null;
      sequence_id: string;
      track_id: string;
      type: 'track.update';
    }
  | {
      clip_id: string;
      sequence_id: string;
      timeline_duration: Time;
      track_id: string;
      type: 'clip.retime';
    }
  | {
      channel: KeyframeChannel;
      clip_id: string;
      sequence_id: string;
      track_id: string;
      type: 'keyframe.channel_set';
    }
  | {
      clip_id: string;
      parameter: string;
      sequence_id: string;
      track_id: string;
      type: 'keyframe.channel_delete';
    }
  | {
      cue: CaptionCue;
      sequence_id: string;
      type: 'caption.insert';
    }
  | {
      cue: CaptionCue;
      sequence_id: string;
      type: 'caption.update';
    }
  | {
      cue_id: string;
      sequence_id: string;
      type: 'caption.delete';
    };
  export type KeyframeInterpolation = 'hold' | 'linear';
  export type TrackKind = 'video' | 'audio' | 'caption' | 'graphic';
  export type AssetKind = 'video' | 'audio' | 'image' | 'text' | 'subtitle' | 'graphic';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };
  export type TextAlign = 'left' | 'center' | 'right';

  export interface EditInput {
  project_id: string;
  request: TransactionRequest;
}
  export interface TransactionRequest {
  expected_revision: string;
  idempotency_key: string;
  label: string;
  operations: Operation[];
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface Track {
  clips: Clip[];
  enabled: boolean;
  id: string;
  kind: TrackKind;
  locked: boolean;
  muted: boolean;
  name: string;
  params?: {
    [k: string]: any;
  };
}
  export interface Clip {
  asset_id?: string | null;
  channels?: KeyframeChannel[];
  id: string;
  params?: {
    [k: string]: any;
  };
  source_range?: TimeRange | null;
  timeline_range: TimeRange;
}
  export interface KeyframeChannel {
  keyframes: Keyframe[];
  parameter: string;
}
  export interface Keyframe {
  id: string;
  interpolation: KeyframeInterpolation;
  time: Time;
  value: any;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface Asset {
  duration?: Time | null;
  id: string;
  kind: AssetKind;
  metadata?: {
    [k: string]: any;
  };
  name: string;
  uri: string;
  version?: AssetVersion | null;
}
  export interface AssetVersion {
  content_hash: string;
  id: string;
  size: number;
}
  export interface CaptionCue {
  id: string;
  source_anchor?: CaptionSourceAnchor | null;
  style: CaptionStyle;
  text: string;
  timeline_range: TimeRange;
}
  export interface CaptionSourceAnchor {
  asset_id: string;
  source_range: TimeRange;
}
  export interface CaptionStyle {
  background_color?: string | null;
  color: string;
  font_family: string;
  font_policy?: FontResolutionPolicy;
  font_size: number;
  /**
   * Line height in canvas pixels. None derives it from font_size.
   */
  line_height?: number | null;
  /**
   * Maximum layout width in canvas pixels. None uses the backend's canvas margin.
   */
  max_width?: number | null;
  text_align?: TextAlign;
}

}

export namespace OpencutEditApplyOutput {
  export interface Response {
  api_version: string;
  data?: TransactionResult | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface TransactionResult {
  base_revision: string;
  changed_ids: string[];
  created_ids: string[];
  deleted_ids: string[];
  revision: string;
  transaction_id: string;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutEventsReadInput {
  export interface EventsInput {
  after_sequence?: number;
  limit?: number;
}

}

export namespace OpencutEventsReadOutput {
  export interface Response {
  api_version: string;
  data?: StoredEvent[] | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface StoredEvent {
  event_id: string;
  event_type: string;
  payload: any;
  project_id: string;
  revision: string;
  sequence: number;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutJobEventsInput {
  export interface JobEventsInput {
  after_sequence?: number;
  job_id?: string | null;
  limit?: number;
}

}

export namespace OpencutJobEventsOutput {
  export type JobState = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

  export interface Response {
  api_version: string;
  data?: JobEvent[] | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface JobEvent {
  attempt: number;
  created_at: string;
  event_id: string;
  event_type: string;
  job_id: string;
  payload: any;
  sequence: number;
  state: JobState;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutJobStartInput {
  export type JobStartInput =
  | {
      idempotency_key: string;
      input: ProbeRequest;
      kind: 'media.probe';
    }
  | {
      idempotency_key: string;
      input: FrameJobInput;
      kind: 'preview.frame';
    }
  | {
      idempotency_key: string;
      input: ProjectFrameInput;
      kind: 'preview.project_frame';
    }
  | {
      idempotency_key: string;
      input: ProjectFrameInput;
      kind: 'preview.composited_frame';
    }
  | {
      idempotency_key: string;
      input: ProjectRangeInput;
      kind: 'preview.project_range';
    }
  | {
      idempotency_key: string;
      input: ExportJobInput;
      kind: 'render.export';
    }
  | {
      idempotency_key: string;
      input: ProjectExportInput;
      kind: 'render.project';
    }
  | {
      idempotency_key: string;
      input: VerifyRequest;
      kind: 'artifact.verify';
    };
  export type FrameAlignment = 'exact' | 'nearest' | 'floor' | 'ceil';
  export type AudioOutput =
  | {
      gain_db?: number;
      mode: 'preserve';
    }
  | {
      mode: 'drop';
    };
  export type Container = 'mp4' | 'matroska';
  export type AspectMode = 'fit' | 'fill' | 'stretch';
  export type VideoCodec = 'h264' | 'hevc' | 'vp9';

  export interface ProbeRequest {
  input: string;
}
  export interface FrameJobInput {
  alignment: FrameAlignment;
  input: string;
  requested_time: MediaTime;
  video_stream?: number | null;
}
  export interface MediaTime {
  ticks: string;
  timebase: number;
}
  export interface ProjectFrameInput {
  alignment: FrameAlignment;
  project_id: string;
  requested_time: Time;
  revision: string;
  sequence_id: string;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface ProjectRangeInput {
  alignment: FrameAlignment;
  project_id: string;
  requested_range: TimeRange;
  revision: string;
  sequence_id: string;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface ExportJobInput {
  audio: AudioOutput;
  container: Container;
  segments: ExportSegment[];
  video: VideoOutput;
}
  export interface ExportSegment {
  audio_stream?: number | null;
  input: string;
  source_duration: MediaTime;
  source_start: MediaTime;
  video_stream?: number | null;
}
  export interface VideoOutput {
  aspect_mode: AspectMode;
  codec: VideoCodec;
  crf?: number | null;
  frame_rate: FrameRate;
  height: number;
  width: number;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface ProjectExportInput {
  container: Container;
  project_id: string;
  revision: string;
  sequence_id: string;
  video_codec: VideoCodec;
}
  export interface VerifyRequest {
  decode_samples?: number;
  duration_tolerance?: MediaTime | null;
  expected_audio_streams?: number | null;
  expected_duration?: MediaTime | null;
  expected_height?: number | null;
  expected_video_streams?: number | null;
  expected_width?: number | null;
  input: string;
}

}

export namespace OpencutJobStartOutput {
  export type JobState = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

  export interface Response {
  api_version: string;
  data?: JobRecord | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface JobRecord {
  attempt: number;
  cancel_requested: boolean;
  error?: any;
  input: any;
  job_id: string;
  kind: string;
  lease_owner?: string | null;
  lease_until?: number | null;
  progress?: JobProgress | null;
  result?: any;
  state: JobState;
  [k: string]: any;
}
  export interface JobProgress {
  fraction?: number | null;
  media_time?: Time | null;
  stage: string;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutRenderPreflightInput {
  export type Container = 'mp4' | 'matroska';
  export type VideoCodec = 'h264' | 'hevc' | 'vp9';

  export interface RenderPreflightInput {
  container: Container;
  project_id: string;
  revision: string;
  sequence_id: string;
  video_codec: VideoCodec;
}

}

export namespace OpencutRenderPreflightOutput {
  export interface Response {
  api_version: string;
  data?: ProjectPreflightReport | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface ProjectPreflightReport {
  issues: PreflightIssue[];
  ready: boolean;
  revision: string;
  sequence_id: string;
  [k: string]: any;
}
  export interface PreflightIssue {
  asset_id?: string | null;
  clip_id?: string | null;
  code: string;
  cue_id?: string | null;
  message: string;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutJobGetInput {
  export interface JobInput {
  job_id: string;
}

}

export namespace OpencutJobGetOutput {
  export type JobState = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

  export interface Response {
  api_version: string;
  data?: JobRecord | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface JobRecord {
  attempt: number;
  cancel_requested: boolean;
  error?: any;
  input: any;
  job_id: string;
  kind: string;
  lease_owner?: string | null;
  lease_until?: number | null;
  progress?: JobProgress | null;
  result?: any;
  state: JobState;
  [k: string]: any;
}
  export interface JobProgress {
  fraction?: number | null;
  media_time?: Time | null;
  stage: string;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutJobListInput {
  export interface EmptyInput {}

}

export namespace OpencutJobListOutput {
  export type JobState = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

  export interface Response {
  api_version: string;
  data?: JobRecord[] | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface JobRecord {
  attempt: number;
  cancel_requested: boolean;
  error?: any;
  input: any;
  job_id: string;
  kind: string;
  lease_owner?: string | null;
  lease_until?: number | null;
  progress?: JobProgress | null;
  result?: any;
  state: JobState;
  [k: string]: any;
}
  export interface JobProgress {
  fraction?: number | null;
  media_time?: Time | null;
  stage: string;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutJobCancelInput {
  export interface JobInput {
  job_id: string;
}

}

export namespace OpencutJobCancelOutput {
  export type JobState = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

  export interface Response {
  api_version: string;
  data?: JobRecord | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface JobRecord {
  attempt: number;
  cancel_requested: boolean;
  error?: any;
  input: any;
  job_id: string;
  kind: string;
  lease_owner?: string | null;
  lease_until?: number | null;
  progress?: JobProgress | null;
  result?: any;
  state: JobState;
  [k: string]: any;
}
  export interface JobProgress {
  fraction?: number | null;
  media_time?: Time | null;
  stage: string;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutPluginListInput {
  export interface EmptyInput {}

}

export namespace OpencutPluginListOutput {
  export interface Response {
  api_version: string;
  data?: InstalledPlugin[] | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface InstalledPlugin {
  granted_permissions: string[];
  install_path: string;
  manifest: PluginManifest;
  package_sha256: string;
  [k: string]: any;
}
  export interface PluginManifest {
  contributes: Contributions;
  display_name: string;
  id: string;
  limits: PluginLimits;
  manifest_version: number;
  permissions: string[];
  plugin_api: string;
  runtime: RuntimeManifest;
  targets: string[];
  version: string;
}
  export interface Contributions {
  operations: PluginOperation[];
}
  export interface PluginOperation {
  input_schema: string;
  kind: string;
  name: string;
  output_schema: string;
  version: number;
}
  export interface PluginLimits {
  fuel?: number;
  memory_mib: number;
  output_bytes: number;
  wall_time_ms: number;
}
  export interface RuntimeManifest {
  entry: string;
  kind: string;
  world: string;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutPluginInspectInput {
  export interface PluginInput {
  id: string;
  version: string;
}

}

export namespace OpencutPluginInspectOutput {
  export interface Response {
  api_version: string;
  data?: InstalledPlugin | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface InstalledPlugin {
  granted_permissions: string[];
  install_path: string;
  manifest: PluginManifest;
  package_sha256: string;
  [k: string]: any;
}
  export interface PluginManifest {
  contributes: Contributions;
  display_name: string;
  id: string;
  limits: PluginLimits;
  manifest_version: number;
  permissions: string[];
  plugin_api: string;
  runtime: RuntimeManifest;
  targets: string[];
  version: string;
}
  export interface Contributions {
  operations: PluginOperation[];
}
  export interface PluginOperation {
  input_schema: string;
  kind: string;
  name: string;
  output_schema: string;
  version: number;
}
  export interface PluginLimits {
  fuel?: number;
  memory_mib: number;
  output_bytes: number;
  wall_time_ms: number;
}
  export interface RuntimeManifest {
  entry: string;
  kind: string;
  world: string;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutPluginInstallInput {
  export interface InstallInput {
  granted_permissions: string[];
  package_root: string;
}

}

export namespace OpencutPluginInstallOutput {
  export interface Response {
  api_version: string;
  data?: InstalledPlugin | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface InstalledPlugin {
  granted_permissions: string[];
  install_path: string;
  manifest: PluginManifest;
  package_sha256: string;
  [k: string]: any;
}
  export interface PluginManifest {
  contributes: Contributions;
  display_name: string;
  id: string;
  limits: PluginLimits;
  manifest_version: number;
  permissions: string[];
  plugin_api: string;
  runtime: RuntimeManifest;
  targets: string[];
  version: string;
}
  export interface Contributions {
  operations: PluginOperation[];
}
  export interface PluginOperation {
  input_schema: string;
  kind: string;
  name: string;
  output_schema: string;
  version: number;
}
  export interface PluginLimits {
  fuel?: number;
  memory_mib: number;
  output_bytes: number;
  wall_time_ms: number;
}
  export interface RuntimeManifest {
  entry: string;
  kind: string;
  world: string;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutDenialQueryInput {
  export type DenialTarget =
  | {
      kind: 'workspace';
    }
  | {
      id: string;
      kind: 'request';
    }
  | {
      id: string;
      kind: 'principal';
    }
  | {
      id: string;
      kind: 'credential';
    };

  export interface DenialQuery {
  after_sequence?: string;
  limit?: number;
  target: DenialTarget;
}

}

export namespace OpencutDenialQueryOutput {
  export type DeniedOperation =
  | {
      kind: 'tool';
      name: string;
    }
  | {
      kind: 'unknown_tool';
    }
  | {
      kind: 'authentication';
    }
  | {
      kind: 'http_request';
    }
  | {
      kind: 'cli_request';
    }
  | {
      kind: 'mcp_request';
    }
  | {
      kind: 'upload_chunk';
    }
  | {
      kind: 'stream';
    }
  | {
      kind: 'administration';
    }
  | {
      kind: 'host_start';
    };
  export type DenialReason = 'authentication_rejected' | 'origin_rejected' | 'authorization_denied' | 'policy_changed';
  export type CallSource = 'cli' | 'http' | 'mcp_stdio' | 'mcp_http' | 'embedded' | 'worker';

  export interface Response {
  api_version: string;
  data?: DenialPage | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface DenialPage {
  coverage: string;
  has_more: boolean;
  last_sequence: string;
  max_retained_records: number;
  oldest_available_sequence?: string | null;
  records: DenialRecord[];
  [k: string]: any;
}
  export interface DenialRecord {
  created_at: string;
  credential_id?: string | null;
  operation: DeniedOperation;
  principal_id?: string | null;
  reason: DenialReason;
  request_id: string;
  sequence: string;
  source: CallSource;
  [k: string]: any;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutProjectPluginGrantInput {
  export interface SetGrant {
  enabled: boolean;
  expected_revision: string;
  idempotency_key: string;
  operations: string[];
  package_sha256: string;
  target: PluginTarget;
}
  export interface PluginTarget {
  id: string;
  project_id: string;
  version: string;
}

}

export namespace OpencutProjectPluginGrantOutput {
  export interface Response {
  api_version: string;
  data?: Grant | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface Grant {
  enabled: boolean;
  operations: string[];
  package_sha256: string;
  revision: string;
  target: PluginTarget;
  [k: string]: any;
}
  export interface PluginTarget {
  id: string;
  project_id: string;
  version: string;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutProjectPluginListInput {
  export interface ProjectInput {
  project_id: string;
}

}

export namespace OpencutProjectPluginListOutput {
  export interface Response {
  api_version: string;
  data?: Grant[] | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface Grant {
  enabled: boolean;
  operations: string[];
  package_sha256: string;
  revision: string;
  target: PluginTarget;
  [k: string]: any;
}
  export interface PluginTarget {
  id: string;
  project_id: string;
  version: string;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutProjectPluginInspectInput {
  export interface InspectInput {
  operation_name: string;
  target: PluginTarget;
}
  export interface PluginTarget {
  id: string;
  project_id: string;
  version: string;
}

}

export namespace OpencutProjectPluginInspectOutput {
  export interface Response {
  api_version: string;
  data?: OperationView | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface OperationView {
  grant: Grant;
  input_schema: any;
  limits: PluginLimits;
  operation_name: string;
  operation_version: number;
  output_schema: any;
  [k: string]: any;
}
  export interface Grant {
  enabled: boolean;
  operations: string[];
  package_sha256: string;
  revision: string;
  target: PluginTarget;
  [k: string]: any;
}
  export interface PluginTarget {
  id: string;
  project_id: string;
  version: string;
}
  export interface PluginLimits {
  fuel?: number;
  memory_mib: number;
  output_bytes: number;
  wall_time_ms: number;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutProjectPluginPlanInput {
  export interface PlanInput {
  id: string;
  operation_name: string;
  parameters: any;
  project_id: string;
  revision: string;
  version: string;
}

}

export namespace OpencutProjectPluginPlanOutput {
  export type Operation =
  | {
      fps?: FrameRate | null;
      height?: number | null;
      name?: string | null;
      sequence_id: string;
      type: 'sequence.update_settings';
      width?: number | null;
    }
  | {
      sequence_id: string;
      track: Track;
      type: 'track.create';
    }
  | {
      sequence_id: string;
      track_id: string;
      type: 'track.delete';
    }
  | {
      asset: Asset;
      type: 'asset.bind';
    }
  | {
      asset: Asset;
      type: 'asset.replace';
    }
  | {
      asset_id: string;
      type: 'asset.unbind';
    }
  | {
      clip: Clip;
      sequence_id: string;
      track_id: string;
      type: 'clip.insert';
    }
  | {
      at: Time;
      clip_id: string;
      right_clip_id: string;
      sequence_id: string;
      track_id: string;
      type: 'clip.split';
    }
  | {
      clip_id: string;
      destination_track_id: string;
      sequence_id: string;
      start: Time;
      track_id: string;
      type: 'clip.move';
    }
  | {
      clip_id: string;
      sequence_id: string;
      source_range?: TimeRange | null;
      timeline_range: TimeRange;
      track_id: string;
      type: 'clip.trim';
    }
  | {
      clip_id: string;
      sequence_id: string;
      track_id: string;
      type: 'clip.delete';
    }
  | {
      clip_id: string;
      params: {
        [k: string]: any;
      };
      sequence_id: string;
      track_id: string;
      type: 'parameter.set_clip';
    }
  | {
      params: {
        [k: string]: any;
      };
      sequence_id: string;
      track_id: string;
      type: 'parameter.set_track';
    }
  | {
      enabled?: boolean | null;
      locked?: boolean | null;
      muted?: boolean | null;
      name?: string | null;
      sequence_id: string;
      track_id: string;
      type: 'track.update';
    }
  | {
      clip_id: string;
      sequence_id: string;
      timeline_duration: Time;
      track_id: string;
      type: 'clip.retime';
    }
  | {
      channel: KeyframeChannel;
      clip_id: string;
      sequence_id: string;
      track_id: string;
      type: 'keyframe.channel_set';
    }
  | {
      clip_id: string;
      parameter: string;
      sequence_id: string;
      track_id: string;
      type: 'keyframe.channel_delete';
    }
  | {
      cue: CaptionCue;
      sequence_id: string;
      type: 'caption.insert';
    }
  | {
      cue: CaptionCue;
      sequence_id: string;
      type: 'caption.update';
    }
  | {
      cue_id: string;
      sequence_id: string;
      type: 'caption.delete';
    };
  export type KeyframeInterpolation = 'hold' | 'linear';
  export type TrackKind = 'video' | 'audio' | 'caption' | 'graphic';
  export type AssetKind = 'video' | 'audio' | 'image' | 'text' | 'subtitle' | 'graphic';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };
  export type TextAlign = 'left' | 'center' | 'right';

  export interface Response {
  api_version: string;
  data?: PlanView | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface PlanView {
  grant: Grant;
  result: PlanInvocation;
  [k: string]: any;
}
  export interface Grant {
  enabled: boolean;
  operations: string[];
  package_sha256: string;
  revision: string;
  target: PluginTarget;
  [k: string]: any;
}
  export interface PluginTarget {
  id: string;
  project_id: string;
  version: string;
}
  export interface PlanInvocation {
  operations: Operation[];
  plan: EditPlan;
  [k: string]: any;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface Track {
  clips: Clip[];
  enabled: boolean;
  id: string;
  kind: TrackKind;
  locked: boolean;
  muted: boolean;
  name: string;
  params?: {
    [k: string]: any;
  };
}
  export interface Clip {
  asset_id?: string | null;
  channels?: KeyframeChannel[];
  id: string;
  params?: {
    [k: string]: any;
  };
  source_range?: TimeRange | null;
  timeline_range: TimeRange;
}
  export interface KeyframeChannel {
  keyframes: Keyframe[];
  parameter: string;
}
  export interface Keyframe {
  id: string;
  interpolation: KeyframeInterpolation;
  time: Time;
  value: any;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface Asset {
  duration?: Time | null;
  id: string;
  kind: AssetKind;
  metadata?: {
    [k: string]: any;
  };
  name: string;
  uri: string;
  version?: AssetVersion | null;
}
  export interface AssetVersion {
  content_hash: string;
  id: string;
  size: number;
}
  export interface CaptionCue {
  id: string;
  source_anchor?: CaptionSourceAnchor | null;
  style: CaptionStyle;
  text: string;
  timeline_range: TimeRange;
}
  export interface CaptionSourceAnchor {
  asset_id: string;
  source_range: TimeRange;
}
  export interface CaptionStyle {
  background_color?: string | null;
  color: string;
  font_family: string;
  font_policy?: FontResolutionPolicy;
  font_size: number;
  /**
   * Line height in canvas pixels. None derives it from font_size.
   */
  line_height?: number | null;
  /**
   * Maximum layout width in canvas pixels. None uses the backend's canvas margin.
   */
  max_width?: number | null;
  text_align?: TextAlign;
}
  export interface EditPlan {
  base_revision: string;
  changed_ids: string[];
  created_ids: string[];
  deleted_ids: string[];
  proposed_project: ProjectDocument;
  unresolved_asset_ids?: string[];
}
  export interface ProjectDocument {
  assets: Asset[];
  default_sequence_id: string;
  id: string;
  name: string;
  schema_version: number;
  sequences: Sequence[];
}
  export interface Sequence {
  caption_cues?: CaptionCue[];
  fps: FrameRate;
  height: number;
  id: string;
  name: string;
  tracks: Track[];
  width: number;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace OpencutPluginPlanInput {
  export interface PlanInput {
  id: string;
  operation_name: string;
  parameters: any;
  project_id: string;
  revision: string;
  version: string;
}

}

export namespace OpencutPluginPlanOutput {
  export type Operation =
  | {
      fps?: FrameRate | null;
      height?: number | null;
      name?: string | null;
      sequence_id: string;
      type: 'sequence.update_settings';
      width?: number | null;
    }
  | {
      sequence_id: string;
      track: Track;
      type: 'track.create';
    }
  | {
      sequence_id: string;
      track_id: string;
      type: 'track.delete';
    }
  | {
      asset: Asset;
      type: 'asset.bind';
    }
  | {
      asset: Asset;
      type: 'asset.replace';
    }
  | {
      asset_id: string;
      type: 'asset.unbind';
    }
  | {
      clip: Clip;
      sequence_id: string;
      track_id: string;
      type: 'clip.insert';
    }
  | {
      at: Time;
      clip_id: string;
      right_clip_id: string;
      sequence_id: string;
      track_id: string;
      type: 'clip.split';
    }
  | {
      clip_id: string;
      destination_track_id: string;
      sequence_id: string;
      start: Time;
      track_id: string;
      type: 'clip.move';
    }
  | {
      clip_id: string;
      sequence_id: string;
      source_range?: TimeRange | null;
      timeline_range: TimeRange;
      track_id: string;
      type: 'clip.trim';
    }
  | {
      clip_id: string;
      sequence_id: string;
      track_id: string;
      type: 'clip.delete';
    }
  | {
      clip_id: string;
      params: {
        [k: string]: any;
      };
      sequence_id: string;
      track_id: string;
      type: 'parameter.set_clip';
    }
  | {
      params: {
        [k: string]: any;
      };
      sequence_id: string;
      track_id: string;
      type: 'parameter.set_track';
    }
  | {
      enabled?: boolean | null;
      locked?: boolean | null;
      muted?: boolean | null;
      name?: string | null;
      sequence_id: string;
      track_id: string;
      type: 'track.update';
    }
  | {
      clip_id: string;
      sequence_id: string;
      timeline_duration: Time;
      track_id: string;
      type: 'clip.retime';
    }
  | {
      channel: KeyframeChannel;
      clip_id: string;
      sequence_id: string;
      track_id: string;
      type: 'keyframe.channel_set';
    }
  | {
      clip_id: string;
      parameter: string;
      sequence_id: string;
      track_id: string;
      type: 'keyframe.channel_delete';
    }
  | {
      cue: CaptionCue;
      sequence_id: string;
      type: 'caption.insert';
    }
  | {
      cue: CaptionCue;
      sequence_id: string;
      type: 'caption.update';
    }
  | {
      cue_id: string;
      sequence_id: string;
      type: 'caption.delete';
    };
  export type KeyframeInterpolation = 'hold' | 'linear';
  export type TrackKind = 'video' | 'audio' | 'caption' | 'graphic';
  export type AssetKind = 'video' | 'audio' | 'image' | 'text' | 'subtitle' | 'graphic';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };
  export type TextAlign = 'left' | 'center' | 'right';

  export interface Response {
  api_version: string;
  data?: PlanInvocation | null;
  error?: EditorError | null;
  request_id: string;
  warnings: string[];
  [k: string]: any;
}
  export interface PlanInvocation {
  operations: Operation[];
  plan: EditPlan;
  [k: string]: any;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface Track {
  clips: Clip[];
  enabled: boolean;
  id: string;
  kind: TrackKind;
  locked: boolean;
  muted: boolean;
  name: string;
  params?: {
    [k: string]: any;
  };
}
  export interface Clip {
  asset_id?: string | null;
  channels?: KeyframeChannel[];
  id: string;
  params?: {
    [k: string]: any;
  };
  source_range?: TimeRange | null;
  timeline_range: TimeRange;
}
  export interface KeyframeChannel {
  keyframes: Keyframe[];
  parameter: string;
}
  export interface Keyframe {
  id: string;
  interpolation: KeyframeInterpolation;
  time: Time;
  value: any;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface Asset {
  duration?: Time | null;
  id: string;
  kind: AssetKind;
  metadata?: {
    [k: string]: any;
  };
  name: string;
  uri: string;
  version?: AssetVersion | null;
}
  export interface AssetVersion {
  content_hash: string;
  id: string;
  size: number;
}
  export interface CaptionCue {
  id: string;
  source_anchor?: CaptionSourceAnchor | null;
  style: CaptionStyle;
  text: string;
  timeline_range: TimeRange;
}
  export interface CaptionSourceAnchor {
  asset_id: string;
  source_range: TimeRange;
}
  export interface CaptionStyle {
  background_color?: string | null;
  color: string;
  font_family: string;
  font_policy?: FontResolutionPolicy;
  font_size: number;
  /**
   * Line height in canvas pixels. None derives it from font_size.
   */
  line_height?: number | null;
  /**
   * Maximum layout width in canvas pixels. None uses the backend's canvas margin.
   */
  max_width?: number | null;
  text_align?: TextAlign;
}
  export interface EditPlan {
  base_revision: string;
  changed_ids: string[];
  created_ids: string[];
  deleted_ids: string[];
  proposed_project: ProjectDocument;
  unresolved_asset_ids?: string[];
}
  export interface ProjectDocument {
  assets: Asset[];
  default_sequence_id: string;
  id: string;
  name: string;
  schema_version: number;
  sequences: Sequence[];
}
  export interface Sequence {
  caption_cues?: CaptionCue[];
  fps: FrameRate;
  height: number;
  id: string;
  name: string;
  tracks: Track[];
  width: number;
}
  export interface EditorError {
  code: string;
  details: any;
  message: string;
}

}

export namespace JobMediaProbeInput {
  export interface ProbeRequest {
  input: string;
}

}

export namespace JobMediaProbeResult {
  export type StreamKind = 'video' | 'audio' | 'subtitle' | 'data' | 'attachment' | 'unknown';

  export interface MediaMetadata {
  bit_rate?: number | null;
  duration?: MediaTime | null;
  format_name?: string | null;
  input: string;
  streams: MediaStream[];
  [k: string]: any;
}
  export interface MediaTime {
  ticks: string;
  timebase: number;
}
  export interface MediaStream {
  average_frame_rate?: FrameRate | null;
  channel_layout?: string | null;
  channels?: number | null;
  codec_name?: string | null;
  codec_type: StreamKind;
  color_primaries?: string | null;
  color_space?: string | null;
  color_transfer?: string | null;
  duration?: MediaTime | null;
  height?: number | null;
  index: number;
  pixel_format?: string | null;
  sample_rate?: number | null;
  time_base?: FrameRate | null;
  width?: number | null;
  [k: string]: any;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}

}

export namespace JobPreviewFrameInput {
  export type FrameAlignment = 'exact' | 'nearest' | 'floor' | 'ceil';

  export interface FrameJobInput {
  alignment: FrameAlignment;
  input: string;
  requested_time: MediaTime;
  video_stream?: number | null;
}
  export interface MediaTime {
  ticks: string;
  timebase: number;
}

}

export namespace JobPreviewFrameResult {
  export type FrameAlignment = 'exact' | 'nearest' | 'floor' | 'ceil';
  export type FrameQuality = 'accurate_source_frame';

  export interface MediaFrameJobResult {
  artifact: ArtifactDescriptor;
  media: ExtractedFrame;
  [k: string]: any;
}
  export interface ArtifactDescriptor {
  content_hash: string;
  mime_type: string;
  path: string;
  size: number;
  [k: string]: any;
}
  export interface ExtractedFrame {
  actual_time: MediaTime;
  alignment: FrameAlignment;
  exact: boolean;
  output: string;
  quality: FrameQuality;
  requested_time: MediaTime;
  source_pts: number;
  source_time_base: FrameRate;
  [k: string]: any;
}
  export interface MediaTime {
  ticks: string;
  timebase: number;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}

}

export namespace JobPreviewProjectFrameInput {
  export type FrameAlignment = 'exact' | 'nearest' | 'floor' | 'ceil';

  export interface ProjectFrameInput {
  alignment: FrameAlignment;
  project_id: string;
  requested_time: Time;
  revision: string;
  sequence_id: string;
}
  export interface Time {
  ticks: string;
  timebase: number;
}

}

export namespace JobPreviewProjectFrameResult {
  export type FrameAlignment = 'exact' | 'nearest' | 'floor' | 'ceil';
  export type ProjectFrameQuality = 'encoded_project_frame';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };

  export interface ProjectFrameJobResult {
  actual_sequence_time: Time;
  alignment: FrameAlignment;
  artifact: ArtifactDescriptor;
  asset_lock?: AssetLockManifest | null;
  asset_locking?: 'media_content_locked' | 'legacy_unlocked';
  height: number;
  quality: ProjectFrameQuality;
  requested_time: Time;
  revision: string;
  sequence_id: string;
  text_layouts: TextLayoutReport[];
  width: number;
  [k: string]: any;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface ArtifactDescriptor {
  content_hash: string;
  mime_type: string;
  path: string;
  size: number;
  [k: string]: any;
}
  export interface AssetLockManifest {
  assets: LockedAsset[];
  revision: string;
  sequence_id: string;
  version: number;
}
  export interface LockedAsset {
  asset_id: string;
  content_hash: string;
  size: number;
}
  export interface TextLayoutReport {
  cluster_ranges: TextClusterRange[];
  glyph_count: number;
  line_count: number;
  pixel_bounds: PixelBounds;
  policy: FontResolutionPolicy;
  requested_family: string;
  rtl_runs: number;
  used_faces: UsedFontFace[];
  [k: string]: any;
}
  export interface TextClusterRange {
  end: number;
  rtl: boolean;
  start: number;
  [k: string]: any;
}
  export interface PixelBounds {
  height: number;
  width: number;
  x: number;
  y: number;
  [k: string]: any;
}
  export interface UsedFontFace {
  face_index: number;
  family: string;
  postscript_name: string;
  /**
   * Compact origin identifier: `file:<path>` or `binary:opaque:<length>bytes`.
   * Embedded font bytes are never included.
   */
  source: string;
  [k: string]: any;
}

}

export namespace JobPreviewCompositedFrameInput {
  export type FrameAlignment = 'exact' | 'nearest' | 'floor' | 'ceil';

  export interface ProjectFrameInput {
  alignment: FrameAlignment;
  project_id: string;
  requested_time: Time;
  revision: string;
  sequence_id: string;
}
  export interface Time {
  ticks: string;
  timebase: number;
}

}

export namespace JobPreviewCompositedFrameResult {
  export type FrameAlignment = 'exact' | 'nearest' | 'floor' | 'ceil';
  export type CompositedProjectFrameQuality = 'composited_project_frame';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };

  export interface CompositedProjectFrameJobResult {
  actual_sequence_time: Time;
  alignment: FrameAlignment;
  artifact: ArtifactDescriptor;
  asset_lock?: AssetLockManifest | null;
  asset_locking?: 'media_content_locked' | 'legacy_unlocked';
  height: number;
  quality: CompositedProjectFrameQuality;
  requested_time: Time;
  revision: string;
  sequence_id: string;
  text_layouts: TextLayoutReport[];
  width: number;
  [k: string]: any;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface ArtifactDescriptor {
  content_hash: string;
  mime_type: string;
  path: string;
  size: number;
  [k: string]: any;
}
  export interface AssetLockManifest {
  assets: LockedAsset[];
  revision: string;
  sequence_id: string;
  version: number;
}
  export interface LockedAsset {
  asset_id: string;
  content_hash: string;
  size: number;
}
  export interface TextLayoutReport {
  cluster_ranges: TextClusterRange[];
  glyph_count: number;
  line_count: number;
  pixel_bounds: PixelBounds;
  policy: FontResolutionPolicy;
  requested_family: string;
  rtl_runs: number;
  used_faces: UsedFontFace[];
  [k: string]: any;
}
  export interface TextClusterRange {
  end: number;
  rtl: boolean;
  start: number;
  [k: string]: any;
}
  export interface PixelBounds {
  height: number;
  width: number;
  x: number;
  y: number;
  [k: string]: any;
}
  export interface UsedFontFace {
  face_index: number;
  family: string;
  postscript_name: string;
  /**
   * Compact origin identifier: `file:<path>` or `binary:opaque:<length>bytes`.
   * Embedded font bytes are never included.
   */
  source: string;
  [k: string]: any;
}

}

export namespace JobPreviewProjectRangeInput {
  export type FrameAlignment = 'exact' | 'nearest' | 'floor' | 'ceil';

  export interface ProjectRangeInput {
  alignment: FrameAlignment;
  project_id: string;
  requested_range: TimeRange;
  revision: string;
  sequence_id: string;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface Time {
  ticks: string;
  timebase: number;
}

}

export namespace JobPreviewProjectRangeResult {
  export type FrameAlignment = 'exact' | 'nearest' | 'floor' | 'ceil';
  export type StreamKind = 'video' | 'audio' | 'subtitle' | 'data' | 'attachment' | 'unknown';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };
  export type ProjectRangeQuality = 'encoded_project_range';

  export interface ProjectRangeJobResult {
  actual_sequence_range: TimeRange;
  alignment: FrameAlignment;
  artifact: ArtifactDescriptor;
  asset_lock?: AssetLockManifest | null;
  asset_locking?: 'media_content_locked' | 'legacy_unlocked';
  media: ExportResult;
  quality: ProjectRangeQuality;
  requested_range: TimeRange;
  revision: string;
  sequence_id: string;
  [k: string]: any;
}
  export interface TimeRange {
  duration: Time;
  start: Time;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface ArtifactDescriptor {
  content_hash: string;
  mime_type: string;
  path: string;
  size: number;
  [k: string]: any;
}
  export interface AssetLockManifest {
  assets: LockedAsset[];
  revision: string;
  sequence_id: string;
  version: number;
}
  export interface LockedAsset {
  asset_id: string;
  content_hash: string;
  size: number;
}
  export interface ExportResult {
  metadata: MediaMetadata;
  output: string;
  text_layouts?: TextLayoutReport[];
  verification: VerificationReport;
  [k: string]: any;
}
  export interface MediaMetadata {
  bit_rate?: number | null;
  duration?: MediaTime | null;
  format_name?: string | null;
  input: string;
  streams: MediaStream[];
  [k: string]: any;
}
  export interface MediaTime {
  ticks: string;
  timebase: number;
}
  export interface MediaStream {
  average_frame_rate?: FrameRate | null;
  channel_layout?: string | null;
  channels?: number | null;
  codec_name?: string | null;
  codec_type: StreamKind;
  color_primaries?: string | null;
  color_space?: string | null;
  color_transfer?: string | null;
  duration?: MediaTime | null;
  height?: number | null;
  index: number;
  pixel_format?: string | null;
  sample_rate?: number | null;
  time_base?: FrameRate | null;
  width?: number | null;
  [k: string]: any;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface TextLayoutReport {
  cluster_ranges: TextClusterRange[];
  glyph_count: number;
  line_count: number;
  pixel_bounds: PixelBounds;
  policy: FontResolutionPolicy;
  requested_family: string;
  rtl_runs: number;
  used_faces: UsedFontFace[];
  [k: string]: any;
}
  export interface TextClusterRange {
  end: number;
  rtl: boolean;
  start: number;
  [k: string]: any;
}
  export interface PixelBounds {
  height: number;
  width: number;
  x: number;
  y: number;
  [k: string]: any;
}
  export interface UsedFontFace {
  face_index: number;
  family: string;
  postscript_name: string;
  /**
   * Compact origin identifier: `file:<path>` or `binary:opaque:<length>bytes`.
   * Embedded font bytes are never included.
   */
  source: string;
  [k: string]: any;
}
  export interface VerificationReport {
  checks: VerificationCheck[];
  decoded_samples: DecodedSample[];
  metadata: MediaMetadata;
  passed: boolean;
  [k: string]: any;
}
  export interface VerificationCheck {
  detail: string;
  name: string;
  passed: boolean;
  [k: string]: any;
}
  export interface DecodedSample {
  decoded: boolean;
  requested_time: MediaTime;
  [k: string]: any;
}

}

export namespace JobRenderExportInput {
  export type AudioOutput =
  | {
      gain_db?: number;
      mode: 'preserve';
    }
  | {
      mode: 'drop';
    };
  export type Container = 'mp4' | 'matroska';
  export type AspectMode = 'fit' | 'fill' | 'stretch';
  export type VideoCodec = 'h264' | 'hevc' | 'vp9';

  export interface ExportJobInput {
  audio: AudioOutput;
  container: Container;
  segments: ExportSegment[];
  video: VideoOutput;
}
  export interface ExportSegment {
  audio_stream?: number | null;
  input: string;
  source_duration: MediaTime;
  source_start: MediaTime;
  video_stream?: number | null;
}
  export interface MediaTime {
  ticks: string;
  timebase: number;
}
  export interface VideoOutput {
  aspect_mode: AspectMode;
  codec: VideoCodec;
  crf?: number | null;
  frame_rate: FrameRate;
  height: number;
  width: number;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}

}

export namespace JobRenderExportResult {
  export type StreamKind = 'video' | 'audio' | 'subtitle' | 'data' | 'attachment' | 'unknown';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };

  export interface MediaExportJobResult {
  artifact: ArtifactDescriptor;
  media: ExportResult;
  [k: string]: any;
}
  export interface ArtifactDescriptor {
  content_hash: string;
  mime_type: string;
  path: string;
  size: number;
  [k: string]: any;
}
  export interface ExportResult {
  metadata: MediaMetadata;
  output: string;
  text_layouts?: TextLayoutReport[];
  verification: VerificationReport;
  [k: string]: any;
}
  export interface MediaMetadata {
  bit_rate?: number | null;
  duration?: MediaTime | null;
  format_name?: string | null;
  input: string;
  streams: MediaStream[];
  [k: string]: any;
}
  export interface MediaTime {
  ticks: string;
  timebase: number;
}
  export interface MediaStream {
  average_frame_rate?: FrameRate | null;
  channel_layout?: string | null;
  channels?: number | null;
  codec_name?: string | null;
  codec_type: StreamKind;
  color_primaries?: string | null;
  color_space?: string | null;
  color_transfer?: string | null;
  duration?: MediaTime | null;
  height?: number | null;
  index: number;
  pixel_format?: string | null;
  sample_rate?: number | null;
  time_base?: FrameRate | null;
  width?: number | null;
  [k: string]: any;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface TextLayoutReport {
  cluster_ranges: TextClusterRange[];
  glyph_count: number;
  line_count: number;
  pixel_bounds: PixelBounds;
  policy: FontResolutionPolicy;
  requested_family: string;
  rtl_runs: number;
  used_faces: UsedFontFace[];
  [k: string]: any;
}
  export interface TextClusterRange {
  end: number;
  rtl: boolean;
  start: number;
  [k: string]: any;
}
  export interface PixelBounds {
  height: number;
  width: number;
  x: number;
  y: number;
  [k: string]: any;
}
  export interface UsedFontFace {
  face_index: number;
  family: string;
  postscript_name: string;
  /**
   * Compact origin identifier: `file:<path>` or `binary:opaque:<length>bytes`.
   * Embedded font bytes are never included.
   */
  source: string;
  [k: string]: any;
}
  export interface VerificationReport {
  checks: VerificationCheck[];
  decoded_samples: DecodedSample[];
  metadata: MediaMetadata;
  passed: boolean;
  [k: string]: any;
}
  export interface VerificationCheck {
  detail: string;
  name: string;
  passed: boolean;
  [k: string]: any;
}
  export interface DecodedSample {
  decoded: boolean;
  requested_time: MediaTime;
  [k: string]: any;
}

}

export namespace JobRenderProjectInput {
  export type Container = 'mp4' | 'matroska';
  export type VideoCodec = 'h264' | 'hevc' | 'vp9';

  export interface ProjectExportInput {
  container: Container;
  project_id: string;
  revision: string;
  sequence_id: string;
  video_codec: VideoCodec;
}

}

export namespace JobRenderProjectResult {
  export type StreamKind = 'video' | 'audio' | 'subtitle' | 'data' | 'attachment' | 'unknown';
  export type FontResolutionPolicy =
  | {
      mode: 'strict';
    }
  | {
      families: string[];
      mode: 'explicit_fallback';
    };

  export interface ProjectRenderJobResult {
  artifact: ArtifactDescriptor;
  asset_lock?: AssetLockManifest | null;
  asset_locking?: 'media_content_locked' | 'legacy_unlocked';
  media: ExportResult;
  revision: string;
  [k: string]: any;
}
  export interface ArtifactDescriptor {
  content_hash: string;
  mime_type: string;
  path: string;
  size: number;
  [k: string]: any;
}
  export interface AssetLockManifest {
  assets: LockedAsset[];
  revision: string;
  sequence_id: string;
  version: number;
}
  export interface LockedAsset {
  asset_id: string;
  content_hash: string;
  size: number;
}
  export interface ExportResult {
  metadata: MediaMetadata;
  output: string;
  text_layouts?: TextLayoutReport[];
  verification: VerificationReport;
  [k: string]: any;
}
  export interface MediaMetadata {
  bit_rate?: number | null;
  duration?: MediaTime | null;
  format_name?: string | null;
  input: string;
  streams: MediaStream[];
  [k: string]: any;
}
  export interface MediaTime {
  ticks: string;
  timebase: number;
}
  export interface MediaStream {
  average_frame_rate?: FrameRate | null;
  channel_layout?: string | null;
  channels?: number | null;
  codec_name?: string | null;
  codec_type: StreamKind;
  color_primaries?: string | null;
  color_space?: string | null;
  color_transfer?: string | null;
  duration?: MediaTime | null;
  height?: number | null;
  index: number;
  pixel_format?: string | null;
  sample_rate?: number | null;
  time_base?: FrameRate | null;
  width?: number | null;
  [k: string]: any;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}
  export interface TextLayoutReport {
  cluster_ranges: TextClusterRange[];
  glyph_count: number;
  line_count: number;
  pixel_bounds: PixelBounds;
  policy: FontResolutionPolicy;
  requested_family: string;
  rtl_runs: number;
  used_faces: UsedFontFace[];
  [k: string]: any;
}
  export interface TextClusterRange {
  end: number;
  rtl: boolean;
  start: number;
  [k: string]: any;
}
  export interface PixelBounds {
  height: number;
  width: number;
  x: number;
  y: number;
  [k: string]: any;
}
  export interface UsedFontFace {
  face_index: number;
  family: string;
  postscript_name: string;
  /**
   * Compact origin identifier: `file:<path>` or `binary:opaque:<length>bytes`.
   * Embedded font bytes are never included.
   */
  source: string;
  [k: string]: any;
}
  export interface VerificationReport {
  checks: VerificationCheck[];
  decoded_samples: DecodedSample[];
  metadata: MediaMetadata;
  passed: boolean;
  [k: string]: any;
}
  export interface VerificationCheck {
  detail: string;
  name: string;
  passed: boolean;
  [k: string]: any;
}
  export interface DecodedSample {
  decoded: boolean;
  requested_time: MediaTime;
  [k: string]: any;
}

}

export namespace JobArtifactVerifyInput {
  export interface VerifyRequest {
  decode_samples?: number;
  duration_tolerance?: MediaTime | null;
  expected_audio_streams?: number | null;
  expected_duration?: MediaTime | null;
  expected_height?: number | null;
  expected_video_streams?: number | null;
  expected_width?: number | null;
  input: string;
}
  export interface MediaTime {
  ticks: string;
  timebase: number;
}

}

export namespace JobArtifactVerifyResult {
  export type StreamKind = 'video' | 'audio' | 'subtitle' | 'data' | 'attachment' | 'unknown';

  export interface VerificationReport {
  checks: VerificationCheck[];
  decoded_samples: DecodedSample[];
  metadata: MediaMetadata;
  passed: boolean;
  [k: string]: any;
}
  export interface VerificationCheck {
  detail: string;
  name: string;
  passed: boolean;
  [k: string]: any;
}
  export interface DecodedSample {
  decoded: boolean;
  requested_time: MediaTime;
  [k: string]: any;
}
  export interface MediaTime {
  ticks: string;
  timebase: number;
}
  export interface MediaMetadata {
  bit_rate?: number | null;
  duration?: MediaTime | null;
  format_name?: string | null;
  input: string;
  streams: MediaStream[];
  [k: string]: any;
}
  export interface MediaStream {
  average_frame_rate?: FrameRate | null;
  channel_layout?: string | null;
  channels?: number | null;
  codec_name?: string | null;
  codec_type: StreamKind;
  color_primaries?: string | null;
  color_space?: string | null;
  color_transfer?: string | null;
  duration?: MediaTime | null;
  height?: number | null;
  index: number;
  pixel_format?: string | null;
  sample_rate?: number | null;
  time_base?: FrameRate | null;
  width?: number | null;
  [k: string]: any;
}
  export interface FrameRate {
  denominator: number;
  numerator: number;
}

}

export namespace JobMediaImportInput {
/**
 * Public import intent. Only the upload authority can prepare worker input.
 */
  export interface MediaImportInput {
  upload_id: string;
}

}

export namespace JobMediaImportResult {
  export type AssetKind = 'video' | 'audio' | 'image' | 'text' | 'subtitle' | 'graphic';

/**
 * A project resource, not a global CAS capability or a host filesystem location.
 */
  export interface MediaImportJobResult {
  asset: Asset;
  job_id: string;
  media_id: string;
  project_id: string;
  upload_id: string;
  [k: string]: any;
}
  export interface Asset {
  duration?: Time | null;
  id: string;
  kind: AssetKind;
  metadata?: {
    [k: string]: any;
  };
  name: string;
  uri: string;
  version?: AssetVersion | null;
}
  export interface Time {
  ticks: string;
  timebase: number;
}
  export interface AssetVersion {
  content_hash: string;
  id: string;
  size: number;
}

}

export interface ToolMap {
  opencut_database_inspect: { input: OpencutDatabaseInspectInput.EmptyInput; output: NonNullable<OpencutDatabaseInspectOutput.Response["data"]> };
  opencut_database_checkpoint: { input: OpencutDatabaseCheckpointInput.CheckpointInput; output: NonNullable<OpencutDatabaseCheckpointOutput.Response["data"]> };
  opencut_storage_budget_inspect: { input: OpencutStorageBudgetInspectInput.EmptyInput; output: NonNullable<OpencutStorageBudgetInspectOutput.Response["data"]> };
  opencut_storage_budget_configure: { input: OpencutStorageBudgetConfigureInput.Configure; output: NonNullable<OpencutStorageBudgetConfigureOutput.Response["data"]> };
  opencut_storage_budget_recover: { input: OpencutStorageBudgetRecoverInput.EmptyInput; output: NonNullable<OpencutStorageBudgetRecoverOutput.Response["data"]> };
  opencut_cas_gc_query: { input: OpencutCasGcQueryInput.Query; output: NonNullable<OpencutCasGcQueryOutput.Response["data"]> };
  opencut_cas_gc_plan: { input: OpencutCasGcPlanInput.PlanInput; output: NonNullable<OpencutCasGcPlanOutput.Response["data"]> };
  opencut_cas_gc_inspect: { input: OpencutCasGcInspectInput.PlanId; output: NonNullable<OpencutCasGcInspectOutput.Response["data"]> };
  opencut_cas_gc_execute: { input: OpencutCasGcExecuteInput.ExecuteInput; output: NonNullable<OpencutCasGcExecuteOutput.Response["data"]> };
  opencut_artifact_retention_inspect: { input: OpencutArtifactRetentionInspectInput.InspectInput; output: NonNullable<OpencutArtifactRetentionInspectOutput.Response["data"]> };
  opencut_artifact_references_inspect: { input: OpencutArtifactReferencesInspectInput.ReferencesInput; output: NonNullable<OpencutArtifactReferencesInspectOutput.Response["data"]> };
  opencut_artifact_gc_plan: { input: OpencutArtifactGcPlanInput.PlanInput; output: NonNullable<OpencutArtifactGcPlanOutput.Response["data"]> };
  opencut_artifact_gc_inspect: { input: OpencutArtifactGcInspectInput.PlanId; output: NonNullable<OpencutArtifactGcInspectOutput.Response["data"]> };
  opencut_artifact_gc_query: { input: OpencutArtifactGcQueryInput.Query; output: NonNullable<OpencutArtifactGcQueryOutput.Response["data"]> };
  opencut_artifact_gc_execute: { input: OpencutArtifactGcExecuteInput.ExecuteInput; output: NonNullable<OpencutArtifactGcExecuteOutput.Response["data"]> };
  opencut_artifact_gc_execution_inspect: { input: OpencutArtifactGcExecutionInspectInput.PlanId; output: NonNullable<OpencutArtifactGcExecutionInspectOutput.Response["data"]> };
  opencut_artifact_retention_release: { input: OpencutArtifactRetentionReleaseInput.ReleaseInput; output: NonNullable<OpencutArtifactRetentionReleaseOutput.Response["data"]> };
  opencut_storage_inspect: { input: OpencutStorageInspectInput.InspectInput; output: NonNullable<OpencutStorageInspectOutput.Response["data"]> };
  opencut_upload_create: { input: OpencutUploadCreateInput.CreateUpload; output: NonNullable<OpencutUploadCreateOutput.Response["data"]> };
  opencut_upload_inspect: { input: OpencutUploadInspectInput.UploadInput; output: NonNullable<OpencutUploadInspectOutput.Response["data"]> };
  opencut_upload_abort: { input: OpencutUploadAbortInput.UploadInput; output: NonNullable<OpencutUploadAbortOutput.Response["data"]> };
  opencut_upload_retention: { input: OpencutUploadRetentionInput.UploadInput; output: NonNullable<OpencutUploadRetentionOutput.Response["data"]> };
  opencut_upload_release: { input: OpencutUploadReleaseInput.UploadInput; output: NonNullable<OpencutUploadReleaseOutput.Response["data"]> };
  opencut_upload_quota: { input: OpencutUploadQuotaInput.ProjectInput; output: NonNullable<OpencutUploadQuotaOutput.Response["data"]> };
  opencut_upload_query: { input: OpencutUploadQueryInput.UploadQuery; output: NonNullable<OpencutUploadQueryOutput.Response["data"]> };
  opencut_upload_seal: { input: OpencutUploadSealInput.UploadInput; output: NonNullable<OpencutUploadSealOutput.Response["data"]> };
  opencut_project_media_inspect: { input: OpencutProjectMediaInspectInput.ProjectMediaInput; output: NonNullable<OpencutProjectMediaInspectOutput.Response["data"]> };
  opencut_project_media_query: { input: OpencutProjectMediaQueryInput.ProjectMediaQuery; output: NonNullable<OpencutProjectMediaQueryOutput.Response["data"]> };
  opencut_access_inspect: { input: OpencutAccessInspectInput.EmptyInput; output: NonNullable<OpencutAccessInspectOutput.Response["data"]> };
  opencut_plugin_publication_query: { input: OpencutPluginPublicationQueryInput.MaintenanceQuery; output: NonNullable<OpencutPluginPublicationQueryOutput.Response["data"]> };
  opencut_plugin_publication_reconcile: { input: OpencutPluginPublicationReconcileInput.ReconcileInput; output: NonNullable<OpencutPluginPublicationReconcileOutput.Response["data"]> };
  opencut_maintenance_query: { input: OpencutMaintenanceQueryInput.MaintenanceQuery; output: NonNullable<OpencutMaintenanceQueryOutput.Response["data"]> };
  opencut_maintenance_reconcile: { input: OpencutMaintenanceReconcileInput.ReconcileInput; output: NonNullable<OpencutMaintenanceReconcileOutput.Response["data"]> };
  opencut_access_audit_query: { input: OpencutAccessAuditQueryInput.AccessAuditQuery; output: NonNullable<OpencutAccessAuditQueryOutput.Response["data"]> };
  opencut_resource_read: { input: OpencutResourceReadInput.ResourceReadInput; output: NonNullable<OpencutResourceReadOutput.Response["data"]> };
  opencut_event_changes: { input: OpencutEventChangesInput.EventChangesInput; output: NonNullable<OpencutEventChangesOutput.Response["data"]> };
  opencut_job_view: { input: OpencutJobViewInput.JobInput; output: NonNullable<OpencutJobViewOutput.Response["data"]> };
  opencut_job_query: { input: OpencutJobQueryInput.JobQueryInput; output: NonNullable<OpencutJobQueryOutput.Response["data"]> };
  opencut_job_changes: { input: OpencutJobChangesInput.JobChangesInput; output: NonNullable<OpencutJobChangesOutput.Response["data"]> };
  opencut_job_submit: { input: OpencutJobSubmitInput.JobStartInput; output: NonNullable<OpencutJobSubmitOutput.Response["data"]> };
  opencut_job_request_cancel: { input: OpencutJobRequestCancelInput.JobInput; output: NonNullable<OpencutJobRequestCancelOutput.Response["data"]> };
  opencut_media_inspect: { input: OpencutMediaInspectInput.InspectMediaInput; output: NonNullable<OpencutMediaInspectOutput.Response["data"]> };
  opencut_media_repair: { input: OpencutMediaRepairInput.RepairMediaInput; output: NonNullable<OpencutMediaRepairOutput.Response["data"]> };
  opencut_project_history: { input: OpencutProjectHistoryInput.HistoryInput; output: NonNullable<OpencutProjectHistoryOutput.Response["data"]> };
  opencut_revision_inspect: { input: OpencutRevisionInspectInput.RevisionInput; output: NonNullable<OpencutRevisionInspectOutput.Response["data"]> };
  opencut_revision_restore: { input: OpencutRevisionRestoreInput.RestoreInput; output: NonNullable<OpencutRevisionRestoreOutput.Response["data"]> };
  opencut_audit_query: { input: OpencutAuditQueryInput.AuditQuery; output: NonNullable<OpencutAuditQueryOutput.Response["data"]> };
  opencut_capabilities: { input: OpencutCapabilitiesInput.EmptyInput; output: NonNullable<OpencutCapabilitiesOutput.Response["data"]> };
  opencut_project_list: { input: OpencutProjectListInput.EmptyInput; output: NonNullable<OpencutProjectListOutput.Response["data"]> };
  opencut_project_create: { input: OpencutProjectCreateInput.ProjectCreateInput; output: NonNullable<OpencutProjectCreateOutput.Response["data"]> };
  opencut_project_inspect: { input: OpencutProjectInspectInput.ProjectInput; output: NonNullable<OpencutProjectInspectOutput.Response["data"]> };
  opencut_edit_plan: { input: OpencutEditPlanInput.EditInput; output: NonNullable<OpencutEditPlanOutput.Response["data"]> };
  opencut_edit_apply: { input: OpencutEditApplyInput.EditInput; output: NonNullable<OpencutEditApplyOutput.Response["data"]> };
  opencut_events_read: { input: OpencutEventsReadInput.EventsInput; output: NonNullable<OpencutEventsReadOutput.Response["data"]> };
  opencut_job_events: { input: OpencutJobEventsInput.JobEventsInput; output: NonNullable<OpencutJobEventsOutput.Response["data"]> };
  opencut_job_start: { input: OpencutJobStartInput.JobStartInput; output: NonNullable<OpencutJobStartOutput.Response["data"]> };
  opencut_render_preflight: { input: OpencutRenderPreflightInput.RenderPreflightInput; output: NonNullable<OpencutRenderPreflightOutput.Response["data"]> };
  opencut_job_get: { input: OpencutJobGetInput.JobInput; output: NonNullable<OpencutJobGetOutput.Response["data"]> };
  opencut_job_list: { input: OpencutJobListInput.EmptyInput; output: NonNullable<OpencutJobListOutput.Response["data"]> };
  opencut_job_cancel: { input: OpencutJobCancelInput.JobInput; output: NonNullable<OpencutJobCancelOutput.Response["data"]> };
  opencut_plugin_list: { input: OpencutPluginListInput.EmptyInput; output: NonNullable<OpencutPluginListOutput.Response["data"]> };
  opencut_plugin_inspect: { input: OpencutPluginInspectInput.PluginInput; output: NonNullable<OpencutPluginInspectOutput.Response["data"]> };
  opencut_plugin_install: { input: OpencutPluginInstallInput.InstallInput; output: NonNullable<OpencutPluginInstallOutput.Response["data"]> };
  opencut_denial_query: { input: OpencutDenialQueryInput.DenialQuery; output: NonNullable<OpencutDenialQueryOutput.Response["data"]> };
  opencut_project_plugin_grant: { input: OpencutProjectPluginGrantInput.SetGrant; output: NonNullable<OpencutProjectPluginGrantOutput.Response["data"]> };
  opencut_project_plugin_list: { input: OpencutProjectPluginListInput.ProjectInput; output: NonNullable<OpencutProjectPluginListOutput.Response["data"]> };
  opencut_project_plugin_inspect: { input: OpencutProjectPluginInspectInput.InspectInput; output: NonNullable<OpencutProjectPluginInspectOutput.Response["data"]> };
  opencut_project_plugin_plan: { input: OpencutProjectPluginPlanInput.PlanInput; output: NonNullable<OpencutProjectPluginPlanOutput.Response["data"]> };
  opencut_plugin_plan: { input: OpencutPluginPlanInput.PlanInput; output: NonNullable<OpencutPluginPlanOutput.Response["data"]> };
}
export interface DomainMap {
  "system": {
    "capabilities": ToolMap["opencut_capabilities"];
  };
  "project": {
    "history": ToolMap["opencut_project_history"];
    "revision.inspect": ToolMap["opencut_revision_inspect"];
    "revision.restore": ToolMap["opencut_revision_restore"];
    "list": ToolMap["opencut_project_list"];
    "create": ToolMap["opencut_project_create"];
    "inspect": ToolMap["opencut_project_inspect"];
  };
  "edit": {
    "plan": ToolMap["opencut_edit_plan"];
    "apply": ToolMap["opencut_edit_apply"];
  };
  "media": {
    "upload.create": ToolMap["opencut_upload_create"];
    "upload.inspect": ToolMap["opencut_upload_inspect"];
    "upload.abort": ToolMap["opencut_upload_abort"];
    "upload.retention": ToolMap["opencut_upload_retention"];
    "upload.release": ToolMap["opencut_upload_release"];
    "upload.quota": ToolMap["opencut_upload_quota"];
    "upload.query": ToolMap["opencut_upload_query"];
    "upload.seal": ToolMap["opencut_upload_seal"];
    "project.media.inspect": ToolMap["opencut_project_media_inspect"];
    "project.media.query": ToolMap["opencut_project_media_query"];
    "media.inspect": ToolMap["opencut_media_inspect"];
    "media.repair": ToolMap["opencut_media_repair"];
  };
  "task": {
    "view": ToolMap["opencut_job_view"];
    "query": ToolMap["opencut_job_query"];
    "changes": ToolMap["opencut_job_changes"];
    "submit": ToolMap["opencut_job_submit"];
    "request.cancel": ToolMap["opencut_job_request_cancel"];
    "events": ToolMap["opencut_job_events"];
    "start": ToolMap["opencut_job_start"];
    "render.preflight": ToolMap["opencut_render_preflight"];
    "get": ToolMap["opencut_job_get"];
    "list": ToolMap["opencut_job_list"];
    "cancel": ToolMap["opencut_job_cancel"];
  };
  "observe": {
    "resource.read": ToolMap["opencut_resource_read"];
    "event.changes": ToolMap["opencut_event_changes"];
    "audit.query": ToolMap["opencut_audit_query"];
    "events.read": ToolMap["opencut_events_read"];
  };
  "plugin": {
    "list": ToolMap["opencut_plugin_list"];
    "inspect": ToolMap["opencut_plugin_inspect"];
    "install": ToolMap["opencut_plugin_install"];
    "project.plugin.grant": ToolMap["opencut_project_plugin_grant"];
    "project.plugin.list": ToolMap["opencut_project_plugin_list"];
    "project.plugin.inspect": ToolMap["opencut_project_plugin_inspect"];
    "project.plugin.plan": ToolMap["opencut_project_plugin_plan"];
    "plan": ToolMap["opencut_plugin_plan"];
  };
  "access": {
    "inspect": ToolMap["opencut_access_inspect"];
    "audit.query": ToolMap["opencut_access_audit_query"];
    "denial.query": ToolMap["opencut_denial_query"];
  };
  "workspace": {
    "database.inspect": ToolMap["opencut_database_inspect"];
    "database.checkpoint": ToolMap["opencut_database_checkpoint"];
    "storage.budget.inspect": ToolMap["opencut_storage_budget_inspect"];
    "storage.budget.configure": ToolMap["opencut_storage_budget_configure"];
    "storage.budget.recover": ToolMap["opencut_storage_budget_recover"];
    "cas.gc.query": ToolMap["opencut_cas_gc_query"];
    "cas.gc.plan": ToolMap["opencut_cas_gc_plan"];
    "cas.gc.inspect": ToolMap["opencut_cas_gc_inspect"];
    "cas.gc.execute": ToolMap["opencut_cas_gc_execute"];
    "artifact.retention.inspect": ToolMap["opencut_artifact_retention_inspect"];
    "artifact.references.inspect": ToolMap["opencut_artifact_references_inspect"];
    "artifact.gc.plan": ToolMap["opencut_artifact_gc_plan"];
    "artifact.gc.inspect": ToolMap["opencut_artifact_gc_inspect"];
    "artifact.gc.query": ToolMap["opencut_artifact_gc_query"];
    "artifact.gc.execute": ToolMap["opencut_artifact_gc_execute"];
    "artifact.gc.execution.inspect": ToolMap["opencut_artifact_gc_execution_inspect"];
    "artifact.retention.release": ToolMap["opencut_artifact_retention_release"];
    "storage.inspect": ToolMap["opencut_storage_inspect"];
    "plugin.publication.query": ToolMap["opencut_plugin_publication_query"];
    "plugin.publication.reconcile": ToolMap["opencut_plugin_publication_reconcile"];
    "maintenance.query": ToolMap["opencut_maintenance_query"];
    "maintenance.reconcile": ToolMap["opencut_maintenance_reconcile"];
  };
}
export const domainNames = [
  "system",
  "project",
  "edit",
  "media",
  "task",
  "observe",
  "plugin",
  "access",
  "workspace"
] as const;
export const domainActions = {
  "system": [
    "capabilities"
  ],
  "project": [
    "history",
    "revision.inspect",
    "revision.restore",
    "list",
    "create",
    "inspect"
  ],
  "edit": [
    "plan",
    "apply"
  ],
  "media": [
    "upload.create",
    "upload.inspect",
    "upload.abort",
    "upload.retention",
    "upload.release",
    "upload.quota",
    "upload.query",
    "upload.seal",
    "project.media.inspect",
    "project.media.query",
    "media.inspect",
    "media.repair"
  ],
  "task": [
    "view",
    "query",
    "changes",
    "submit",
    "request.cancel",
    "events",
    "start",
    "render.preflight",
    "get",
    "list",
    "cancel"
  ],
  "observe": [
    "resource.read",
    "event.changes",
    "audit.query",
    "events.read"
  ],
  "plugin": [
    "list",
    "inspect",
    "install",
    "project.plugin.grant",
    "project.plugin.list",
    "project.plugin.inspect",
    "project.plugin.plan",
    "plan"
  ],
  "access": [
    "inspect",
    "audit.query",
    "denial.query"
  ],
  "workspace": [
    "database.inspect",
    "database.checkpoint",
    "storage.budget.inspect",
    "storage.budget.configure",
    "storage.budget.recover",
    "cas.gc.query",
    "cas.gc.plan",
    "cas.gc.inspect",
    "cas.gc.execute",
    "artifact.retention.inspect",
    "artifact.references.inspect",
    "artifact.gc.plan",
    "artifact.gc.inspect",
    "artifact.gc.query",
    "artifact.gc.execute",
    "artifact.gc.execution.inspect",
    "artifact.retention.release",
    "storage.inspect",
    "plugin.publication.query",
    "plugin.publication.reconcile",
    "maintenance.query",
    "maintenance.reconcile"
  ]
} as const;
export interface JobResultMap {
  "media.probe": JobMediaProbeResult.MediaMetadata;
  "preview.frame": JobPreviewFrameResult.MediaFrameJobResult;
  "preview.project_frame": JobPreviewProjectFrameResult.ProjectFrameJobResult;
  "preview.composited_frame": JobPreviewCompositedFrameResult.CompositedProjectFrameJobResult;
  "preview.project_range": JobPreviewProjectRangeResult.ProjectRangeJobResult;
  "render.export": JobRenderExportResult.MediaExportJobResult;
  "render.project": JobRenderProjectResult.ProjectRenderJobResult;
  "artifact.verify": JobArtifactVerifyResult.VerificationReport;
  "media.import": JobMediaImportResult.MediaImportJobResult;
}
export const toolNames = [
  "opencut_database_inspect",
  "opencut_database_checkpoint",
  "opencut_storage_budget_inspect",
  "opencut_storage_budget_configure",
  "opencut_storage_budget_recover",
  "opencut_cas_gc_query",
  "opencut_cas_gc_plan",
  "opencut_cas_gc_inspect",
  "opencut_cas_gc_execute",
  "opencut_artifact_retention_inspect",
  "opencut_artifact_references_inspect",
  "opencut_artifact_gc_plan",
  "opencut_artifact_gc_inspect",
  "opencut_artifact_gc_query",
  "opencut_artifact_gc_execute",
  "opencut_artifact_gc_execution_inspect",
  "opencut_artifact_retention_release",
  "opencut_storage_inspect",
  "opencut_upload_create",
  "opencut_upload_inspect",
  "opencut_upload_abort",
  "opencut_upload_retention",
  "opencut_upload_release",
  "opencut_upload_quota",
  "opencut_upload_query",
  "opencut_upload_seal",
  "opencut_project_media_inspect",
  "opencut_project_media_query",
  "opencut_access_inspect",
  "opencut_plugin_publication_query",
  "opencut_plugin_publication_reconcile",
  "opencut_maintenance_query",
  "opencut_maintenance_reconcile",
  "opencut_access_audit_query",
  "opencut_resource_read",
  "opencut_event_changes",
  "opencut_job_view",
  "opencut_job_query",
  "opencut_job_changes",
  "opencut_job_submit",
  "opencut_job_request_cancel",
  "opencut_media_inspect",
  "opencut_media_repair",
  "opencut_project_history",
  "opencut_revision_inspect",
  "opencut_revision_restore",
  "opencut_audit_query",
  "opencut_capabilities",
  "opencut_project_list",
  "opencut_project_create",
  "opencut_project_inspect",
  "opencut_edit_plan",
  "opencut_edit_apply",
  "opencut_events_read",
  "opencut_job_events",
  "opencut_job_start",
  "opencut_render_preflight",
  "opencut_job_get",
  "opencut_job_list",
  "opencut_job_cancel",
  "opencut_plugin_list",
  "opencut_plugin_inspect",
  "opencut_plugin_install",
  "opencut_denial_query",
  "opencut_project_plugin_grant",
  "opencut_project_plugin_list",
  "opencut_project_plugin_inspect",
  "opencut_project_plugin_plan",
  "opencut_plugin_plan"
] as const;
