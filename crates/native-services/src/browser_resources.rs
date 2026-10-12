//! Fixed verified CAS descriptors shared by isolated audio and visual adapters.
use opencut_asset_store::AssetStore;
use opencut_composition::project_v4::CasResourceV4;
use opencut_contracts::EditorError;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
struct Descriptor {
    handle: String,
    file: File,
    size: u64,
    hash: String,
    active: bool,
}
pub(crate) struct VerifiedBrowserResources<F> {
    _store: AssetStore,
    files: BTreeMap<String, Descriptor>,
    checkpoint: F,
    invalid_code: &'static str,
}
impl<F: FnMut() -> Result<(), EditorError>> VerifiedBrowserResources<F> {
    pub(crate) fn new<'a>(
        root: &Path,
        resources: impl IntoIterator<Item = &'a CasResourceV4>,
        mut checkpoint: F,
        invalid_code: &'static str,
    ) -> Result<Self, EditorError> {
        checkpoint()?;
        let store = AssetStore::read_only(root)?;
        let mut files = BTreeMap::new();
        for resource in resources {
            if files.contains_key(&resource.asset_id) {
                continue;
            }
            checkpoint()?;
            let (file, size) = store.open_version_verified(&resource.version)?;
            let descriptor = Descriptor {
                handle: format!("resource-{}", files.len()),
                file,
                size,
                hash: resource.version.content_hash.clone(),
                active: false,
            };
            files.insert(resource.asset_id.clone(), descriptor);
        }
        Ok(Self {
            _store: store,
            files,
            checkpoint,
            invalid_code,
        })
    }
    pub(crate) fn handles(&self) -> impl Iterator<Item = (&str, &str)> {
        self.files
            .iter()
            .map(|(id, d)| (id.as_str(), d.handle.as_str()))
    }
    pub(crate) fn checkpoint(&mut self) -> Result<(), EditorError> {
        (self.checkpoint)()
    }
    pub(crate) fn resolve(
        &mut self,
        id: &str,
        hash: &str,
        size: u64,
        handle: &str,
    ) -> Result<(String, u64), EditorError> {
        self.checkpoint()?;
        let descriptor = self.files.get_mut(id).ok_or_else(|| {
            EditorError::new("NOT_AUTHORIZED", "Resource is outside fixed execution")
        })?;
        if descriptor.handle != handle
            || descriptor.size != size
            || descriptor.hash != hash
            || descriptor.active
        {
            return Err(EditorError::new(
                self.invalid_code,
                "Resource handle is invalid or already active",
            ));
        }
        descriptor.active = true;
        Ok((handle.into(), size))
    }
    pub(crate) fn read(
        &mut self,
        transfer: &str,
        offset: u64,
        count: u32,
    ) -> Result<(Vec<u8>, bool), EditorError> {
        self.checkpoint()?;
        let d = self
            .files
            .values_mut()
            .find(|d| d.handle == transfer && d.active)
            .ok_or_else(|| {
                EditorError::new("NOT_AUTHORIZED", "Unknown or released resource transfer")
            })?;
        if count == 0 || count > 65536 || offset > d.size {
            return Err(EditorError::new(
                self.invalid_code,
                "Invalid bounded resource range",
            ));
        }
        let size = (d.size - offset).min(u64::from(count)) as usize;
        let mut bytes = vec![0; size];
        d.file
            .seek(SeekFrom::Start(offset))
            .and_then(|_| d.file.read_exact(&mut bytes))
            .map_err(|_| {
                EditorError::new(
                    "ASSET_RESOLUTION_FAILED",
                    "Verified descriptor could not be read",
                )
            })?;
        let eof = offset + size as u64 == d.size;
        self.checkpoint()?;
        Ok((bytes, eof))
    }
    pub(crate) fn release(&mut self, transfer: &str) -> Result<(), EditorError> {
        self.checkpoint()?;
        let d = self
            .files
            .values_mut()
            .find(|d| d.handle == transfer && d.active)
            .ok_or_else(|| {
                EditorError::new(self.invalid_code, "Unknown or released resource transfer")
            })?;
        d.active = false;
        Ok(())
    }
}
