//! Upload rules and transactional ports. Native byte storage is a separate adapter.
pub use opencut_api_contracts::uploads::*;
use opencut_contracts::EditorError;
pub const MAX_UPLOAD_BYTES: u64 = 1_073_741_824;
pub const MAX_CHUNK_BYTES: u64 = 4_194_304;
/// Bounds database/event growth independently from the byte quota.
pub const MAX_UPLOAD_CHUNKS: i64 = 4096;
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
pub fn decimal(value: &str) -> Result<u64, EditorError> {
    let number = value
        .parse::<u64>()
        .map_err(|_| invalid("Expected a canonical byte position"))?;
    if number.to_string() != value {
        return Err(invalid("Expected a canonical byte position"));
    }
    Ok(number)
}
pub fn validate_hash(hash: &str) -> Result<(), EditorError> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid("Expected a lowercase SHA-256 content hash"));
    }
    Ok(())
}
pub fn validate_id(id: &str) -> Result<(), EditorError> {
    let Some(value) = id.strip_prefix("upload_") else {
        return Err(invalid("Invalid upload identity"));
    };
    if value.len() != 32
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid("Invalid upload identity"));
    }
    Ok(())
}
pub fn validate_create(input: &CreateUpload) -> Result<(), EditorError> {
    if input.project_id.is_empty()
        || input.project_id.len() > 1024
        || input.project_id.chars().any(char::is_control)
        || input.name.trim().is_empty()
        || input.name.len() > 512
        || input.name.chars().any(char::is_control)
    {
        return Err(invalid("Invalid upload project or display name"));
    }
    crate::require_idempotency_key(&input.idempotency_key)?;
    if !(1..=MAX_UPLOAD_BYTES).contains(&decimal(&input.size_bytes)?) {
        return Err(invalid("Upload size must be between 1 byte and 1 GiB"));
    }
    validate_hash(&input.content_hash)
}
pub fn validate_chunk(input: &ChunkDescriptor) -> Result<(), EditorError> {
    validate_id(&input.upload_id)?;
    validate_hash(&input.content_hash)?;
    let offset = decimal(&input.offset)?;
    let size = decimal(&input.size_bytes)?;
    if !(1..=MAX_CHUNK_BYTES).contains(&size)
        || offset
            .checked_add(size)
            .is_none_or(|end| end > MAX_UPLOAD_BYTES)
    {
        return Err(invalid("Invalid chunk byte range"));
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkPosition {
    New,
    Replay,
}
/// A storage transaction still checks the exact replay's stored length and digest.
pub fn chunk_position(
    upload: &UploadView,
    chunk: &ChunkDescriptor,
) -> Result<ChunkPosition, EditorError> {
    validate_chunk(chunk)?;
    if upload.upload_id != chunk.upload_id {
        return Err(EditorError::new(
            "INTERNAL_ERROR",
            "Upload store returned a different object",
        ));
    }
    if upload.state != UploadState::Receiving {
        return Err(EditorError::new(
            "UPLOAD_STATE_CONFLICT",
            "Upload is no longer receiving chunks",
        ));
    }
    let offset = decimal(&chunk.offset)?;
    let end = offset + decimal(&chunk.size_bytes)?;
    let next = decimal(&upload.next_offset)?;
    if end > decimal(&upload.size_bytes)? || offset > next || (offset < next && end > next) {
        return Err(EditorError::new(
            "UPLOAD_OFFSET_CONFLICT",
            "Query the confirmed upload offset before retrying",
        ));
    }
    Ok(if offset == next {
        ChunkPosition::New
    } else {
        ChunkPosition::Replay
    })
}
/// Implementations enforce project authorization before replay and again inside
/// every state-changing transaction. Native files must be durable before ACK.
pub trait UploadStore {
    fn create_upload(&mut self, input: &CreateUpload) -> Result<UploadView, EditorError>;
    fn inspect_upload(&self, id: &str) -> Result<UploadView, EditorError>;
    fn authorize_chunk(&self, input: &ChunkDescriptor) -> Result<UploadView, EditorError>;
    fn acknowledge_chunk(&mut self, input: &ChunkDescriptor) -> Result<UploadView, EditorError>;
    fn abort_upload(&mut self, id: &str) -> Result<UploadView, EditorError>;
}
pub fn create(
    store: &mut impl UploadStore,
    input: CreateUpload,
) -> Result<UploadView, EditorError> {
    validate_create(&input)?;
    store.create_upload(&input)
}
pub fn inspect(store: &impl UploadStore, input: UploadInput) -> Result<UploadView, EditorError> {
    validate_id(&input.upload_id)?;
    store.inspect_upload(&input.upload_id)
}
pub fn abort(store: &mut impl UploadStore, input: UploadInput) -> Result<UploadView, EditorError> {
    validate_id(&input.upload_id)?;
    store.abort_upload(&input.upload_id)
}

pub use opencut_job_execution::{PreparedMediaImportInput, PreparedUploadChunk};
/// Import acceptance and project resource lookup are separate from byte ACK.
pub trait ImportStore {
    /// Atomically seal, accept one durable job and bind that job to this project.
    fn seal_upload(&mut self, id: &str) -> Result<UploadView, EditorError>;
    fn project_media(&self, input: &ProjectMediaInput)
    -> Result<MediaImportJobResult, EditorError>;
}
pub fn seal(store: &mut impl ImportStore, input: UploadInput) -> Result<UploadView, EditorError> {
    validate_id(&input.upload_id)?;
    store.seal_upload(&input.upload_id)
}
pub fn validate_media_id(id: &str) -> Result<(), EditorError> {
    let value = id
        .strip_prefix("media_")
        .ok_or_else(|| invalid("Invalid project media identity"))?;
    if value.len() != 32
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid("Invalid project media identity"));
    }
    Ok(())
}
pub fn validate_manifest(input: &PreparedMediaImportInput) -> Result<(), EditorError> {
    validate_id(&input.upload_id)?;
    validate_media_id(&input.media_id)?;
    validate_create(&CreateUpload {
        project_id: input.project_id.clone(),
        idempotency_key: "sealed".into(),
        name: input.name.clone(),
        size_bytes: input.size_bytes.to_string(),
        content_hash: input.content_hash.clone(),
    })?;
    if input.chunks.is_empty() || input.chunks.len() > MAX_UPLOAD_CHUNKS as usize {
        return Err(invalid("Invalid sealed chunk count"));
    }
    let mut next = 0;
    for chunk in &input.chunks {
        validate_chunk(&ChunkDescriptor {
            upload_id: input.upload_id.clone(),
            offset: chunk.offset.to_string(),
            size_bytes: chunk.size_bytes.to_string(),
            content_hash: chunk.content_hash.clone(),
        })?;
        if chunk.offset != next {
            return Err(invalid("Sealed chunks must form one contiguous prefix"));
        }
        next += chunk.size_bytes;
    }
    if next != input.size_bytes {
        return Err(invalid("Sealed chunks must cover the complete upload"));
    }
    Ok(())
}
/// Immutable identity check shared by worker publication and public projection.
pub fn validate_import_result(
    input: &PreparedMediaImportInput,
    job_id: &str,
    result: &MediaImportJobResult,
) -> Result<(), EditorError> {
    validate_manifest(input)?;
    let metadata = &result.asset.metadata;
    let valid_metadata = metadata.iter().all(|(key, value)| match key.as_str() {
        "content_hash" => value.as_str() == Some(input.content_hash.as_str()),
        "size" => value.as_u64() == Some(input.size_bytes),
        "has_audio" => value.is_boolean(),
        "width" | "height" => {
            value.is_null() || value.as_u64().is_some_and(|v| v <= u32::MAX as u64)
        }
        "mime_type" => value.as_str().is_some_and(|v| {
            matches!(
                v,
                "image/png"
                    | "image/jpeg"
                    | "image/webp"
                    | "image/bmp"
                    | "image/tiff"
                    | "image/gif"
                    | "video/mp4"
                    | "video/quicktime"
                    | "video/x-msvideo"
                    | "application/octet-stream"
                    | "audio/wav"
                    | "audio/mpeg"
                    | "audio/ogg"
                    | "audio/flac"
                    | "audio/mp4"
            )
        }),
        _ => false,
    }) && ["content_hash", "size", "mime_type", "has_audio"]
        .iter()
        .all(|key| metadata.contains_key(*key));
    let valid = valid_metadata
        && matches!(
            result.asset.kind,
            opencut_contracts::AssetKind::Video
                | opencut_contracts::AssetKind::Audio
                | opencut_contracts::AssetKind::Image
        )
        && result.media_id == input.media_id
        && result.project_id == input.project_id
        && result.upload_id == input.upload_id
        && result.job_id == job_id
        && result.asset.id == input.media_id
        && result.asset.name == input.name
        && result.asset.uri == format!("opencut-media://{}", input.media_id)
        && result.asset.version.as_ref().is_some_and(|v| {
            v.content_hash == input.content_hash
                && v.size == input.size_bytes
                && v.id == format!("av_{}", input.content_hash)
        });
    if !valid {
        return Err(EditorError::new(
            "INVALID_JOB_RESULT",
            "Imported resource does not match the sealed upload",
        ));
    }
    Ok(())
}

pub struct MediaPageSlice {
    pub records: Vec<MediaImportJobResult>,
    pub upper_id: Option<String>,
    pub has_more: bool,
}
pub trait ProjectMediaRead {
    /// Authorize the project, resolve cursor anchors and read rows in one snapshot.
    fn read_media_page(
        &self,
        project: &str,
        upper: Option<&str>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<MediaPageSlice, EditorError>;
}
pub fn query_media(
    store: &impl ProjectMediaRead,
    input: ProjectMediaQuery,
) -> Result<ProjectMediaPage, EditorError> {
    if !(1..=100).contains(&input.limit) {
        return Err(invalid("Page size must be 1..100"));
    }
    let anchors = match input.cursor.as_deref() {
        None => None,
        Some(cursor) => {
            let p: Vec<_> = cursor.split(':').collect();
            if p.len() != 3
                || p[0] != "project-media"
                || validate_media_id(p[1]).is_err()
                || validate_media_id(p[2]).is_err()
            {
                return Err(EditorError::new(
                    "INVALID_CURSOR",
                    "Invalid project media cursor",
                ));
            }
            Some((p[1], p[2]))
        }
    };
    let page = store.read_media_page(
        &input.project_id,
        anchors.map(|a| a.0),
        anchors.map(|a| a.1),
        input.limit,
    )?;
    let next_cursor = if page.has_more {
        page.upper_id
            .as_ref()
            .zip(page.records.last())
            .map(|(upper, last)| format!("project-media:{upper}:{}", last.media_id))
    } else {
        None
    };
    Ok(ProjectMediaPage {
        media: page.records,
        next_cursor,
    })
}
/// Public upload/resource errors never include database or native path diagnostics.
pub fn public_error(error: EditorError) -> EditorError {
    let message = match error.code.as_str() {
        "NOT_AUTHORIZED" => return crate::access::denied(),
        "INVALID_ARGUMENT" => "Invalid upload or project media arguments",
        "NOT_FOUND" => "Upload or project media was not found",
        "INVALID_CURSOR" => "Cursor does not match this query",
        "CURSOR_EXPIRED" => "A cursor anchor is unavailable; start a new query",
        "IDEMPOTENCY_KEY_REUSED" => "Upload key was used with different parameters",
        "RESOURCE_TOO_LARGE" => "Upload or page exceeds an advertised resource limit",
        "UPLOAD_INCOMPLETE" => "Upload all declared bytes before sealing",
        "UPLOAD_STATE_CONFLICT" => "Upload state does not allow this operation",
        "UPLOAD_OFFSET_CONFLICT" => "Query the confirmed upload offset before retrying",
        "UPLOAD_CHUNK_CONFLICT" => "Different bytes were already confirmed at this offset",
        "UPLOAD_CONTENT_MISMATCH" => "Chunk length or SHA-256 does not match the declaration",
        "UPLOAD_INTEGRITY_ERROR" => {
            "Confirmed upload content is missing or corrupt; abort and upload again"
        }
        "UPLOAD_CLEANUP_PENDING" => "Upload is aborted; retry abort to finish private byte cleanup",
        "UPLOAD_RELEASE_PENDING" => {
            "Staging release is incomplete; inspect retention and retry release"
        }
        "UPLOAD_QUOTA_EXCEEDED" => {
            "Upload staging reservation limit reached; release eligible staging first"
        }
        "STORAGE_BUDGET_EXCEEDED" => {
            "Workspace payload admission limit reached; inspect storage policy and release eligible data"
        }
        "STORAGE_CAPACITY_LOW" => "Volume headroom is insufficient for this write",
        "STORAGE_BUDGET_BUSY" => "Workspace admission is busy; retry after the current operation",
        "STORAGE_BUDGET_ERROR" => {
            "Workspace admission state could not be safely observed or persisted"
        }
        "UPLOAD_BUSY" => "Too many concurrent chunks; retry after a transfer finishes",
        "UPLOAD_TIMEOUT" => {
            "Chunk transfer exceeded its time budget; query the offset before retrying"
        }
        _ => {
            return EditorError::new(
                "UPLOAD_STORAGE_ERROR",
                "Upload state or content could not be read or committed",
            );
        }
    };
    EditorError::new(error.code, message)
}

pub trait UploadCleanup {
    /// Discard only eligible terminal staging, under the same lock used by writers and readers.
    fn discard_chunks(
        &self,
        id: &str,
        authorize: &dyn Fn() -> Result<(), EditorError>,
    ) -> Result<(), EditorError>;
}
pub fn abort_with_cleanup(
    store: &mut (impl UploadStore + UploadRetentionStore),
    cleanup: &impl UploadCleanup,
    input: UploadInput,
) -> Result<UploadView, EditorError> {
    let expected_id = input.upload_id.clone();
    let view = abort(store, input)?;
    if view.upload_id != expected_id || view.state != UploadState::Aborted {
        return Err(EditorError::new(
            "INTERNAL_ERROR",
            "Upload store returned an invalid aborted object",
        ));
    }
    release_upload(
        store,
        cleanup,
        UploadInput {
            upload_id: view.upload_id.clone(),
        },
    )
    .map_err(|error| {
        if error.code == "NOT_AUTHORIZED" {
            return error;
        }
        EditorError::new(
            "UPLOAD_CLEANUP_PENDING",
            "Upload is aborted; retry abort to finish private byte cleanup",
        )
    })?;
    Ok(view)
}

/// Logical source-byte reservations, not a quota on CAS, outputs or filesystem blocks.
pub const PROJECT_UPLOAD_RESERVATION_LIMIT: u64 = 8 * MAX_UPLOAD_BYTES;
pub const WORKSPACE_UPLOAD_RESERVATION_LIMIT: u64 = 32 * MAX_UPLOAD_BYTES;
pub trait UploadRetentionStore {
    fn read_upload_page(
        &self,
        project: &str,
        retained_only: bool,
        upper: Option<&str>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<UploadPageSlice, EditorError>;
    fn upload_retention(&self, id: &str) -> Result<UploadRetentionView, EditorError>;
    fn upload_quota(&self, project_id: &str) -> Result<UploadQuotaView, EditorError>;
    /// Current write authorization and immutable terminal-state eligibility.
    fn authorize_upload_release(&self, id: &str) -> Result<UploadRetentionView, EditorError>;
    /// Only invoked after durable native deletion. Failure must retain the reservation.
    fn finish_upload_release(&mut self, id: &str) -> Result<UploadRetentionView, EditorError>;
}
pub fn release_upload(
    store: &mut impl UploadRetentionStore,
    cleanup: &impl UploadCleanup,
    input: UploadInput,
) -> Result<UploadRetentionView, EditorError> {
    validate_id(&input.upload_id)?;
    let view = store.authorize_upload_release(&input.upload_id)?;
    if view.upload_id != input.upload_id || !view.release_eligible {
        return Err(EditorError::new(
            "UPLOAD_STATE_CONFLICT",
            "Only aborted or terminal imports can release staging",
        ));
    }
    if view.state == UploadRetentionState::Released {
        return Ok(view);
    }
    let pending = || {
        EditorError::new(
            "UPLOAD_RELEASE_PENDING",
            "Staging release is incomplete; inspect retention and retry release",
        )
    };
    cleanup
        .discard_chunks(&input.upload_id, &|| {
            let current = store.authorize_upload_release(&input.upload_id)?;
            if current.upload_id != input.upload_id || !current.release_eligible {
                return Err(EditorError::new(
                    "UPLOAD_STATE_CONFLICT",
                    "Upload staging is not eligible for release",
                ));
            }
            Ok(())
        })
        .map_err(|e| {
            if e.code == "NOT_AUTHORIZED" {
                e
            } else {
                pending()
            }
        })?;
    store.finish_upload_release(&input.upload_id).map_err(|e| {
        if e.code == "NOT_AUTHORIZED" {
            e
        } else {
            pending()
        }
    })
}

pub struct UploadPageSlice {
    pub records: Vec<UploadEntry>,
    pub upper_id: Option<String>,
    pub has_more: bool,
}
pub fn query_uploads(
    store: &impl UploadRetentionStore,
    input: UploadQuery,
) -> Result<UploadPage, EditorError> {
    if !(1..=100).contains(&input.limit) {
        return Err(invalid("Upload page size must be 1 to 100"));
    }
    let scope = if input.retained_only {
        "retained"
    } else {
        "all"
    };
    let prefix = format!("uploads:{scope}:");
    let (upper, after) = match input.cursor.as_deref() {
        None => (None, None),
        Some(cursor) => {
            let value = cursor.strip_prefix(&prefix).ok_or_else(|| {
                EditorError::new("INVALID_CURSOR", "Upload cursor filter differs")
            })?;
            let (upper, after) = value
                .split_once(':')
                .ok_or_else(|| EditorError::new("INVALID_CURSOR", "Invalid upload cursor"))?;
            for id in [upper, after] {
                validate_id(id).map_err(|_| {
                    EditorError::new("INVALID_CURSOR", "Invalid upload cursor anchor")
                })?;
            }
            (Some(upper), Some(after))
        }
    };
    let page = store.read_upload_page(
        &input.project_id,
        input.retained_only,
        upper,
        after,
        input.limit,
    )?;
    let next_cursor = if page.has_more {
        page.upper_id
            .as_ref()
            .zip(page.records.last())
            .map(|(upper, last)| format!("{prefix}{upper}:{}", last.upload.upload_id))
    } else {
        None
    };
    Ok(UploadPage {
        uploads: page.records,
        next_cursor,
    })
}
