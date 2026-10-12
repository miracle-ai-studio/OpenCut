/// Adapter policy shared by CLI and HTTP. Unknown codes remain server errors;
/// callers must not infer that a failed response makes a write safe to repeat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ErrorPolicy {
    pub http_status: u16,
    pub exit_code: i32,
}

pub fn error_policy(code: &str) -> ErrorPolicy {
    let (http_status, exit_code) = match code {
        "DATABASE_CAPACITY_EXHAUSTED" => (507, 8),
        "INVALID_ARGUMENT" | "TIME_OVERFLOW" | "INVALID_CURSOR" => (400, 2),
        "UPLOAD_CONTENT_MISMATCH" => (400, 2),
        "UPLOAD_BUSY"
        | "STORAGE_SCAN_BUSY"
        | "STORAGE_BUSY"
        | "STORAGE_BUDGET_BUSY"
        | "DATABASE_BUSY" => (429, 7),
        "UPLOAD_TIMEOUT" | "SOURCE_ART_DEADLINE_EXCEEDED" | "SOURCE_ART_TIME_LIMIT" => (408, 124),
        "INVALID_PLUGIN" | "INVALID_PRESET" | "INVALID_PROJECT" => (422, 2),
        "NOT_AUTHORIZED" | "PERMISSION_DENIED" => (403, 3),
        "NOT_FOUND" => (404, 4),
        "CURSOR_EXPIRED" | "PREVIEW_EXPIRED" => (410, 4),
        "STORAGE_POLICY_CONFLICT"
        | "GC_PLAN_CONFLICT"
        | "ARTIFACT_ALREADY_RELEASED"
        | "REVISION_CONFLICT"
        | "PREVIEW_GENERATION_CONFLICT"
        | "PREVIEW_TELEMETRY_ORDER"
        | "PREVIEW_STREAM_CLAIMED"
        | "PREVIEW_NOT_PLAYING"
        | "PREVIEW_STOPPED"
        | "PREVIEW_OWNER_ACTIVE"
        | "EDIT_HELD_BY_OWNER"
        | "CAUSAL_CONFLICT"
        | "BRANCH_MERGE_CONFLICT"
        | "SELECTIVE_CONFLICT"
        | "UNDO_UNAVAILABLE"
        | "REDO_UNAVAILABLE"
        | "NO_EFFECT"
        | "COLLABORATION_CONFLICT"
        | "ANALYSIS_NOT_READY"
        | "IDEMPOTENCY_KEY_REUSED"
        | "ALREADY_EXISTS"
        | "PLUGIN_VERSION_CONFLICT"
        | "MEDIA_METADATA_CONFLICT"
        | "PUBLICATION_STATE_CONFLICT"
        | "PLUGIN_INSTALL_INCOMPLETE"
        | "INVALID_JOB_STATE"
        | "UPLOAD_STATE_CONFLICT"
        | "UPLOAD_OFFSET_CONFLICT"
        | "UPLOAD_CHUNK_CONFLICT"
        | "UPLOAD_HASH_CONFLICT"
        | "UPLOAD_HASH_REQUIRED"
        | "UPLOAD_INCOMPLETE"
        | "JOB_LEASE_LOST" => (409, 5),
        "SOURCE_ART_CANCELLED" | "SOURCE_ART_STOPPED" => (409, 7),
        "SOURCE_ART_REQUEST_CONFLICT" => (409, 5),
        "SOURCE_ART_BUDGET_EXCEEDED" => (422, 6),
        "MEDIA_METADATA_REQUIRED"
        | "NO_AUDIO_TRACK"
        | "MEDIA_OUTPUT_LIMIT_EXCEEDED"
        | "STORAGE_BUDGET_EXCEEDED"
        | "STORAGE_CAPACITY_LOW"
        | "RESOURCE_TOO_LARGE"
        | "UPLOAD_QUOTA_EXCEEDED"
        | "CONSTRAINT_VIOLATION"
        | "CAPABILITY_UNAVAILABLE"
        | "UNSUPPORTED_OPERATION"
        | "UNSUPPORTED_WAVEFORM_DISPLAY_PROFILE"
        | "UNSUPPORTED_PROJECT_VERSION"
        | "PREVIEW_LIMIT"
        | "PREVIEW_VIEWPORT_LIMIT"
        | "UNSUPPORTED_ANALYSIS_QUERY"
        | "SUBTITLE_INVALID"
        | "PACKAGE_INVALID"
        | "PACKAGE_DEPENDENCY_UNSUPPORTED"
        | "UNSUPPORTED_JOB_KIND"
        | "MISSING_ASSET"
        | "ASSET_LOCK_REQUIRED"
        | "UNSUPPORTED_DATABASE_VERSION" => (422, 6),
        "JOB_WAIT_TIMEOUT" => (408, 124),
        "JOB_CANCELLED" => (409, 7),
        "JOB_FAILED" | "PLUGIN_RUNTIME" | "PLUGIN_OUTPUT_LIMIT" | "SOURCE_ART_DECODE_FAILED" => {
            (422, 7)
        }
        // Integrity/storage/internal errors require investigation or repair.
        // A successful historical Job is not rewritten when an artifact decays.
        _ => (500, 8),
    };
    ErrorPolicy {
        http_status,
        exit_code,
    }
}
