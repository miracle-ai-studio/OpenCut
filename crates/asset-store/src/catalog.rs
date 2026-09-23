//! Storage for uploaded media metadata. Detection is supplied by an adapter;
//! this module has no dependency on codecs, transports, or the project database.
use fs2::FileExt;
use opencut_contracts::{Asset, EditorError};
use opencut_editor_application::maintenance::{
    self, MaintenanceAction, MaintenanceObject, PublicationIntent, PublicationJournal,
    ReconcileResult, UntrackedPublication,
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub use opencut_contracts::CatalogEntry;

pub struct MediaCatalog {
    root: PathBuf,
    budget: Option<opencut_storage_budget::Budget>,
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn io(e: impl std::fmt::Display) -> EditorError {
    EditorError::new("STORAGE_ERROR", e.to_string())
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid_hash(hash: &str) -> Result<(), EditorError> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Expected a lowercase SHA-256 media identity",
        ));
    }
    Ok(())
}
impl MediaCatalog {
    pub fn read_only(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            budget: None,
        }
    }
    pub fn open(root: impl AsRef<Path>) -> Result<Self, EditorError> {
        fs::create_dir_all(root.as_ref()).map_err(io)?;
        Ok(Self {
            root: fs::canonicalize(root).map_err(io)?,
            budget: None,
        })
    }
    pub fn open_managed(root: impl AsRef<Path>) -> Result<Self, EditorError> {
        let mut catalog = Self::open(root)?;
        #[cfg(unix)]
        {
            catalog.budget = catalog
                .root
                .parent()
                .map(opencut_storage_budget::Budget::new);
        }
        Ok(catalog)
    }
    pub fn inspect(&self, hash: &str) -> Result<CatalogEntry, EditorError> {
        valid_hash(hash)?;
        let bytes = match fs::read(self.root.join(format!("{hash}.json"))) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(io(e)),
        };
        Ok(CatalogEntry {
            content_hash: hash.into(),
            metadata_hash: bytes.as_deref().map(digest),
            asset: bytes
                .as_deref()
                .and_then(|bytes| serde_json::from_slice(bytes).ok()),
        })
    }
    fn lock(&self, hash: &str) -> Result<File, EditorError> {
        valid_hash(hash)?;
        let locks = self.root.join(".catalog-locks");
        fs::create_dir_all(&locks).map_err(io)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(locks.join(hash))
            .map_err(io)?;
        file.lock_exclusive().map_err(io)?;
        Ok(file)
    }
    fn write(&self, hash: &str, asset: &Asset) -> Result<CatalogEntry, EditorError> {
        let bytes = serde_json::to_vec(asset).map_err(io)?;
        let temp = Temporary(
            self.root
                .join(format!(".metadata-{}", uuid::Uuid::new_v4())),
        );
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp.0)
            .map_err(io)?;
        file.write_all(&bytes).map_err(io)?;
        file.sync_all().map_err(io)?;
        fs::rename(&temp.0, self.root.join(format!("{hash}.json"))).map_err(io)?;
        #[cfg(unix)]
        File::open(&self.root)
            .and_then(|file| file.sync_all())
            .map_err(io)?;
        self.inspect(hash)
    }
    /// Normal imports never replace conflicting metadata. All cooperating
    /// imports/repairs use the same per-content lock and atomic publication.
    pub fn publish(&self, hash: &str, asset: &Asset) -> Result<(), EditorError> {
        self.publish_observed(hash, asset, &UntrackedPublication)
    }
    pub fn publish_observed(
        &self,
        hash: &str,
        asset: &Asset,
        journal: &dyn PublicationJournal,
    ) -> Result<(), EditorError> {
        let _lock = self.lock(hash)?;
        self.publish_locked(hash, asset, journal)
    }
    fn publish_locked(
        &self,
        hash: &str,
        asset: &Asset,
        journal: &dyn PublicationJournal,
    ) -> Result<(), EditorError> {
        let current = self.inspect(hash)?;
        let object = MaintenanceObject::MediaMetadata {
            content_hash: hash.into(),
        };
        journal.reconcile(&object, current.metadata_hash.as_deref())?;
        if current.metadata_hash.is_some() {
            if !current.asset.as_ref().is_some_and(|old| {
                old.kind == asset.kind
                    && old.metadata == asset.metadata
                    && old.duration == asset.duration
                    && old.id == asset.id
            }) {
                return Err(EditorError::new(
                    "MEDIA_METADATA_CONFLICT",
                    "Inspect and repair conflicting media metadata before importing again",
                )
                .with_details(serde_json::json!({"content_hash": hash,
                    "metadata_hash": current.metadata_hash,
                    "next_operation": "opencut_media_inspect"})));
            }
        } else {
            self.write_observed(
                hash,
                asset,
                current.metadata_hash,
                MaintenanceAction::MediaMetadataPublish,
                journal,
            )?;
        }
        Ok(())
    }
    pub fn repair(
        &self,
        hash: &str,
        expected_metadata_hash: Option<&str>,
        detect: impl FnOnce(&Path, &str, u64) -> Result<Asset, EditorError>,
    ) -> Result<CatalogEntry, EditorError> {
        self.repair_observed(hash, expected_metadata_hash, detect, &UntrackedPublication)
    }
    pub fn repair_observed(
        &self,
        hash: &str,
        expected_metadata_hash: Option<&str>,
        detect: impl FnOnce(&Path, &str, u64) -> Result<Asset, EditorError>,
        journal: &dyn PublicationJournal,
    ) -> Result<CatalogEntry, EditorError> {
        let _lock = self.lock(hash)?;
        let current = self.inspect(hash)?;
        journal.reconcile(
            &MaintenanceObject::MediaMetadata {
                content_hash: hash.into(),
            },
            current.metadata_hash.as_deref(),
        )?;
        if current.metadata_hash.as_deref() != expected_metadata_hash {
            return Err(EditorError::new(
                "MEDIA_METADATA_CONFLICT",
                "Metadata changed since inspection; inspect again before repairing",
            )
            .with_details(serde_json::json!({"content_hash": hash,
                "expected_metadata_hash": expected_metadata_hash,
                "actual_metadata_hash": current.metadata_hash,
                "next_operation": "opencut_media_inspect"})));
        }
        let source = self.root.join(hash);
        let source_path = fs::canonicalize(&source).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                EditorError::new("NOT_FOUND", "Media content is missing")
            } else {
                io(e)
            }
        })?;
        if !source_path.starts_with(&self.root) {
            return Err(EditorError::new(
                "NOT_AUTHORIZED",
                "Media content is outside the catalog",
            ));
        }
        let mut source = File::open(&source_path).map_err(io)?;
        let mut reservation = self
            .budget
            .as_ref()
            .map(|budget| budget.reserve("media_inspection", source.metadata().map_err(io)?.len()))
            .transpose()?;
        let temp = Temporary(self.root.join(format!(".inspect-{}", uuid::Uuid::new_v4())));
        let mut copy = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp.0)
            .map_err(io)?;
        let mut hasher = Sha256::new();
        let mut size = 0_u64;
        let mut buffer = [0_u8; 65536];
        loop {
            let count = source.read(&mut buffer).map_err(io)?;
            if count == 0 {
                break;
            }
            size += count as u64;
            if let Some(reservation) = &mut reservation {
                reservation.ensure(size)?;
            }
            hasher.update(&buffer[..count]);
            copy.write_all(&buffer[..count]).map_err(io)?;
        }
        if format!("{:x}", hasher.finalize()) != hash {
            return Err(EditorError::new(
                "ASSET_INTEGRITY_ERROR",
                "Media bytes do not match their content identity; metadata was not repaired",
            ));
        }
        copy.sync_all().map_err(io)?;
        drop(copy);
        let name = current
            .asset
            .as_ref()
            .map(|asset| asset.name.as_str())
            .unwrap_or(hash);
        let mut detected = detect(&temp.0, name, size)?;
        detected.uri = source_path
            .to_str()
            .ok_or_else(|| EditorError::new("INVALID_ARGUMENT", "Media path is not valid UTF-8"))?
            .into();
        self.write_observed(
            hash,
            &detected,
            current.metadata_hash,
            MaintenanceAction::MediaMetadataRepair,
            journal,
        )
    }
    fn write_observed(
        &self,
        hash: &str,
        asset: &Asset,
        before_hash: Option<String>,
        action: MaintenanceAction,
        journal: &dyn PublicationJournal,
    ) -> Result<CatalogEntry, EditorError> {
        let bytes = serde_json::to_vec(asset).map_err(io)?;
        let after_hash = digest(&bytes);
        if before_hash.as_deref() == Some(after_hash.as_str()) {
            return self.inspect(hash);
        }
        let _reservation = self
            .budget
            .as_ref()
            .map(|budget| budget.reserve("media_metadata", bytes.len() as u64))
            .transpose()?;
        maintenance::publish(
            journal,
            PublicationIntent {
                action,
                object: MaintenanceObject::MediaMetadata {
                    content_hash: hash.into(),
                },
                before_hash,
                after_hash,
            },
            || self.write(hash, asset),
        )
    }
    fn content_digest(&self, hash: &str) -> Result<Option<String>, EditorError> {
        let path = self.root.join(hash);
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io(e)),
            Ok(meta) if !meta.is_file() => {
                return Err(EditorError::new(
                    "NOT_AUTHORIZED",
                    "Media content must be a regular file",
                ));
            }
            _ => {}
        }
        file_digest(&path).map(Some)
    }
    /// Upload publication uses the same lock as repair, verifies deduplicated
    /// bytes and journals content/metadata separately under the same request.
    pub fn import_observed(
        &self,
        staging: &Path,
        hash: &str,
        asset: &Asset,
        journal: &dyn PublicationJournal,
    ) -> Result<(), EditorError> {
        let _lock = self.lock(hash)?;
        if file_digest(staging)? != hash {
            return Err(EditorError::new(
                "ASSET_INTEGRITY_ERROR",
                "Staged upload changed before publication",
            ));
        }
        let object = MaintenanceObject::MediaContent {
            content_hash: hash.into(),
        };
        let before = self.content_digest(hash)?;
        journal.reconcile(&object, before.as_deref())?;
        if before.as_deref().is_some_and(|observed| observed != hash) {
            return Err(EditorError::new(
                "ASSET_INTEGRITY_ERROR",
                "Stored content does not match its identity; upload cannot silently reuse or replace corrupted bytes",
            ));
        }
        if before.is_none() {
            maintenance::publish(
                journal,
                PublicationIntent {
                    action: MaintenanceAction::MediaContentPublish,
                    object,
                    before_hash: None,
                    after_hash: hash.into(),
                },
                || {
                    fs::rename(staging, self.root.join(hash)).map_err(io)?;
                    #[cfg(unix)]
                    File::open(&self.root)
                        .and_then(|f| f.sync_all())
                        .map_err(io)?;
                    Ok(())
                },
            )?;
        }
        self.publish_locked(hash, asset, journal)
    }
    pub fn reconcile(
        &self,
        object: MaintenanceObject,
        journal: &dyn PublicationJournal,
    ) -> Result<ReconcileResult, EditorError> {
        maintenance::validate_object(&object)?;
        let hash = match &object {
            MaintenanceObject::MediaContent { content_hash }
            | MaintenanceObject::MediaMetadata { content_hash } => content_hash,
        };
        let _lock = self.lock(hash)?;
        let observed_hash = match object {
            MaintenanceObject::MediaContent { .. } => self.content_digest(hash)?,
            MaintenanceObject::MediaMetadata { .. } => self.inspect(hash)?.metadata_hash,
        };
        let resolved_publication_ids = journal.reconcile(&object, observed_hash.as_deref())?;
        Ok(ReconcileResult {
            object,
            observed_hash,
            resolved_publication_ids,
        })
    }
}
fn file_digest(path: &Path) -> Result<String, EditorError> {
    let mut file = File::open(path).map_err(io)?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(io)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
