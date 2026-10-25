//! Private resumable upload chunks. Confirmed offsets live in the application store.
use fs2::FileExt;
use opencut_contracts::{EditorError, LockedAsset};
use opencut_editor_application::uploads::{self, ChunkDescriptor, UploadStore, UploadView};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
pub struct UploadChunks {
    root: PathBuf,
    budget: Option<opencut_storage_budget::Budget>,
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        if !self.0.as_os_str().is_empty() {
            let _ = fs::remove_file(&self.0);
        }
    }
}
fn io(_: std::io::Error) -> EditorError {
    storage_error()
}
fn storage_error() -> EditorError {
    EditorError::new(
        "UPLOAD_STORAGE_ERROR",
        "Upload bytes could not be stored or verified",
    )
}
fn directory(path: &Path) -> Result<(), EditorError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(io)
}
impl UploadChunks {
    /// Constructing a reader/service does not create workspace files.
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            budget: None,
        }
    }
    pub fn managed(root: impl AsRef<Path>) -> Self {
        let mut chunks = Self::new(root);
        #[cfg(unix)]
        {
            chunks.budget = chunks
                .root
                .parent()
                .map(opencut_storage_budget::Budget::new);
        }
        chunks
    }
    fn reserve(
        &self,
        category: &str,
        bytes: u64,
    ) -> Result<Option<opencut_storage_budget::Reservation>, EditorError> {
        self.budget
            .as_ref()
            .map(|b| b.reserve(category, bytes))
            .transpose()
    }
    fn lock(&self, id: &str, exclusive: bool) -> Result<File, EditorError> {
        let locks = self.root.join(".locks");
        directory(&locks)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(locks.join(id))
            .map_err(io)?;
        if exclusive {
            FileExt::lock_exclusive(&lock).map_err(io)?;
        } else {
            FileExt::lock_shared(&lock).map_err(io)?;
        }
        Ok(lock)
    }

    pub fn put(
        &self,
        store: &mut impl UploadStore,
        input: &ChunkDescriptor,
        bytes: &[u8],
    ) -> Result<UploadView, EditorError> {
        uploads::validate_chunk(input)?;
        if bytes.len() as u64 != uploads::decimal(&input.size_bytes)?
            || format!("{:x}", Sha256::digest(bytes)) != input.content_hash
        {
            return Err(EditorError::new(
                "UPLOAD_CONTENT_MISMATCH",
                "Chunk bytes do not match their declared length and SHA-256",
            ));
        }
        store.authorize_chunk(input)?;
        let _lock = self.lock(&input.upload_id, true)?;
        let view = store.authorize_chunk(input)?;
        let folder = self.root.join(&input.upload_id);
        let target = folder.join(format!("{}.chunk", input.offset));
        let expected = LockedAsset {
            asset_id: input.upload_id.clone(),
            content_hash: input.content_hash.clone(),
            size: uploads::decimal(&input.size_bytes)?,
        };
        if uploads::decimal(&input.offset)? < uploads::decimal(&view.next_offset)? {
            super::verify_file(&target, &expected).map_err(|_| {
                EditorError::new(
                    "UPLOAD_INTEGRITY_ERROR",
                    "Confirmed upload bytes are missing or corrupt; abort and upload again",
                )
            })?;
            return store.acknowledge_chunk(input);
        }
        directory(&folder)?;
        // The exclusive upload lock permits exactly one bounded staging slot.
        // Reclaim its crash residue directly;never rescan all confirmed chunks
        // for every append. Historical temporary files are removed by release.
        let staging = folder.join(".upload-chunk.tmp");
        match fs::remove_file(&staging) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io(error)),
        }
        if super::verify_file(&target, &expected).is_err() {
            let _reservation = self.reserve("upload_chunk", bytes.len() as u64)?;
            let temporary = Temporary(staging);
            let mut options = OpenOptions::new();
            options.create_new(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary.0).map_err(io)?;
            file.write_all(bytes).map_err(io)?;
            file.sync_all().map_err(io)?;
            drop(file);
            fs::rename(&temporary.0, &target).map_err(io)?;
        }
        // Reusing a private chunk left by a failed ACK still establishes durability.
        super::regular_file(&target)
            .map_err(|_| storage_error())?
            .sync_all()
            .map_err(io)?;
        super::sync_directory(&folder).map_err(|_| storage_error())?;
        super::sync_directory(&self.root).map_err(|_| storage_error())?;
        if let Some(parent) = self.root.parent().filter(|p| !p.as_os_str().is_empty()) {
            super::sync_directory(parent).map_err(|_| storage_error())?;
        } else {
            super::sync_directory(Path::new(".")).map_err(|_| storage_error())?;
        }
        store.acknowledge_chunk(input)
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl UploadChunks {
    /// Materialize a sealed manifest into a caller-owned attempt directory.
    /// Read only the declared bytes plus one sentinel byte per chunk.
    pub fn assemble(
        &self,
        input: &uploads::PreparedMediaImportInput,
        target: &Path,
        read_chunks: &mut dyn FnMut(u64) -> Result<Vec<uploads::PreparedUploadChunk>, EditorError>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), EditorError> {
        use std::io::Read;
        uploads::validate_manifest(input)?;
        let _lock = self.lock(&input.upload_id, false)?;
        let integrity = || {
            EditorError::new(
                "UPLOAD_INTEGRITY_ERROR",
                "Sealed upload bytes do not match their content identity",
            )
        };
        if cancelled() {
            return Err(EditorError::new("JOB_CANCELLED", "Import was cancelled"));
        }
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let _reservation = self.reserve("upload_assembly", input.size_bytes)?;
        let mut destination = options.open(target).map_err(io)?;
        let mut temporary = Temporary(target.to_path_buf());
        let mut whole = Sha256::new();
        let mut cursor = 0u64;
        loop {
            if cancelled() {
                return Err(EditorError::new("JOB_CANCELLED", "Import was cancelled"));
            }
            let chunks = if input.chunks.is_empty() {
                read_chunks(cursor)?
            } else {
                // Exact historical inline manifest only;current jobs page the store.
                input
                    .chunks
                    .iter()
                    .filter(|c| c.offset >= cursor)
                    .take(uploads::IMPORT_CHUNK_PAGE_SIZE)
                    .cloned()
                    .collect()
            };
            if chunks.len() > uploads::IMPORT_CHUNK_PAGE_SIZE {
                return Err(integrity());
            }
            if chunks.is_empty() {
                if cursor != input.size_bytes {
                    return Err(integrity());
                }
                break;
            }
            for chunk in chunks {
                uploads::validate_chunk(&uploads::ChunkDescriptor {
                    upload_id: input.upload_id.clone(),
                    offset: chunk.offset.to_string(),
                    size_bytes: chunk.size_bytes.to_string(),
                    content_hash: chunk.content_hash.clone(),
                })
                .map_err(|_| integrity())?;
                let end = chunk
                    .offset
                    .checked_add(chunk.size_bytes)
                    .ok_or_else(integrity)?;
                if chunk.offset != cursor || end > input.size_bytes {
                    return Err(integrity());
                }
                let source = self
                    .root
                    .join(&input.upload_id)
                    .join(format!("{}.chunk", chunk.offset));
                let source = super::regular_file(&source).map_err(|_| integrity())?;
                if source.metadata().map_err(io)?.len() != chunk.size_bytes {
                    return Err(integrity());
                }
                let mut source = source.take(chunk.size_bytes + 1);
                let mut hash = Sha256::new();
                let mut size = 0;
                let mut buffer = [0u8; 65_536];
                loop {
                    if cancelled() {
                        return Err(EditorError::new("JOB_CANCELLED", "Import was cancelled"));
                    }
                    let count = source.read(&mut buffer).map_err(io)?;
                    if count == 0 {
                        break;
                    }
                    size += count as u64;
                    if size > chunk.size_bytes {
                        return Err(integrity());
                    }
                    destination.write_all(&buffer[..count]).map_err(io)?;
                    hash.update(&buffer[..count]);
                    whole.update(&buffer[..count]);
                }
                if size != chunk.size_bytes
                    || format!("{:x}", hash.finalize()) != chunk.content_hash
                {
                    return Err(integrity());
                }
                cursor = end;
            }
        }
        if format!("{:x}", whole.finalize()) != input.content_hash {
            return Err(integrity());
        }
        destination.sync_all().map_err(io)?;
        // Ownership transfers to the worker's attempt-directory cleanup guard.
        temporary.0.clear();
        Ok(())
    }
}

impl uploads::UploadCleanup for UploadChunks {
    fn discard_chunks(
        &self,
        id: &str,
        authorize: &dyn Fn() -> Result<(), EditorError>,
    ) -> Result<(), EditorError> {
        uploads::validate_id(id)?;
        match fs::symlink_metadata(&self.root) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(io(e)),
            Ok(_) => {}
        }
        let _lock = self.lock(id, true)?;
        authorize()?;
        match fs::remove_dir_all(self.root.join(id)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io(e)),
        }
        super::sync_directory(&self.root).map_err(|_| storage_error())
    }
}
