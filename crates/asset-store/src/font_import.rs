//! Durable publication of inspected font bytes. The returned store retains the
//! shared CAS gate through the application's atomic project commit.
use super::*;
use opencut_editor_application::font_import::{
    FontCasPublish, FontImportCheckpoint, MAX_FONT_BYTES, PreparedFont,
};

pub struct ManagedFontPublisher {
    root: PathBuf,
}
impl ManagedFontPublisher {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }
}

fn verify(
    store: &AssetStore,
    version: &AssetVersion,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<(), EditorError> {
    let path = store.object_path(&version.content_hash)?;
    if !fs::symlink_metadata(&path)
        .map_err(io_error)?
        .file_type()
        .is_file()
    {
        return Err(error(
            "ASSET_INTEGRITY_ERROR",
            "Font CAS object must be a regular file",
        ));
    }
    let mut file = regular_file(&path)?;
    let mut hash = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0; 65536];
    loop {
        checkpoint.check()?;
        let count = file.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        size += count as u64;
        if size > version.size {
            return Err(error("ASSET_INTEGRITY_ERROR", "Font CAS object grew"));
        }
        hash.update(&buffer[..count]);
    }
    if size != version.size || format!("{:x}", hash.finalize()) != version.content_hash {
        return Err(error(
            "ASSET_INTEGRITY_ERROR",
            "Font CAS bytes differ from the pinned resource",
        ));
    }
    file.sync_all().map_err(io_error)?;
    sync_directory(path.parent().expect("CAS object parent"))?;
    checkpoint.check()
}

impl FontCasPublish for ManagedFontPublisher {
    type Lease = AssetStore;
    fn publish(
        &self,
        font: &PreparedFont,
        bytes: &[u8],
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<AssetStore, EditorError> {
        checkpoint.check()?;
        let version = &font.resource.content;
        if version.id != format!("av_{}", version.content_hash)
            || version.size == 0
            || version.size > MAX_FONT_BYTES
            || version.size != bytes.len() as u64
        {
            return Err(error(
                "ASSET_VERSION_INVALID",
                "Font bytes do not match their admitted resource",
            ));
        }
        let store = AssetStore::open_managed(&self.root)?;
        let path = store.object_path(&version.content_hash)?;
        // Validate the supplied bytes even when another importer already won.
        let mut hash = Sha256::new();
        for chunk in bytes.chunks(65536) {
            checkpoint.check()?;
            hash.update(chunk);
        }
        if format!("{:x}", hash.finalize()) != version.content_hash {
            return Err(error(
                "ASSET_INTEGRITY_ERROR",
                "Inspected font bytes changed before publication",
            ));
        }
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                verify(&store, version, checkpoint)?;
                return Ok(store);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_error(e)),
        }
        let _reservation = store.reserve("cas_capture", version.size)?;
        let temporary = Temporary(
            store
                .root
                .join(format!(".capture-{}", uuid::Uuid::new_v4())),
        );
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary.0)
            .map_err(io_error)?;
        for chunk in bytes.chunks(65536) {
            checkpoint.check()?;
            file.write_all(chunk).map_err(io_error)?;
        }
        file.sync_all().map_err(io_error)?;
        make_read_only(&file)?;
        drop(file);
        checkpoint.check()?;
        fs::create_dir_all(path.parent().expect("CAS object parent")).map_err(io_error)?;
        sync_directory(&store.root)?;
        match fs::hard_link(&temporary.0, &path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                verify(&store, version, checkpoint)?
            }
            Err(e) => return Err(io_error(e)),
        }
        sync_directory(path.parent().expect("CAS object parent"))?;
        checkpoint.check()?;
        Ok(store)
    }
}

#[cfg(test)]
mod tests;
