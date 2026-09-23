//! Byte adapter; application metadata and acknowledgement remain behind ports.
use opencut_contracts::EditorError;
use opencut_editor_application::uploads::*;
use std::path::Path;
pub fn put(
    root: &Path,
    store: &mut impl UploadStore,
    input: &ChunkDescriptor,
    bytes: &[u8],
) -> Result<UploadView, EditorError> {
    opencut_asset_store::uploads::UploadChunks::managed(root).put(store, input, bytes)
}

pub fn abort(
    root: &Path,
    store: &mut (impl UploadStore + UploadRetentionStore),
    input: UploadInput,
) -> Result<UploadView, EditorError> {
    opencut_editor_application::uploads::abort_with_cleanup(
        store,
        &opencut_asset_store::uploads::UploadChunks::managed(root),
        input,
    )
}

pub fn release(
    root: &Path,
    store: &mut impl UploadRetentionStore,
    input: UploadInput,
) -> Result<UploadRetentionView, EditorError> {
    release_upload(
        store,
        &opencut_asset_store::uploads::UploadChunks::managed(root),
        input,
    )
}
