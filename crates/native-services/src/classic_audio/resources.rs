use crate::browser_resources::VerifiedBrowserResources;
use opencut_composition::project_v4::PreparedAudioV4;
use opencut_contracts::EditorError;
use opencut_editor_application::classic_audio::{
    AudioResource, ClassicAudioResourceHandle, ClassicAudioResources,
};
use std::path::Path;
/// Verified descriptors and the CAS gate outlive all browser work. The live host
/// checkpoint runs before each read and after native process teardown.
pub struct VerifiedClassicAudioResources<F> {
    inner: VerifiedBrowserResources<F>,
}
impl<F: FnMut() -> Result<(), EditorError>> VerifiedClassicAudioResources<F> {
    pub fn new(
        root: &Path,
        prepared: &PreparedAudioV4,
        checkpoint: F,
    ) -> Result<Self, EditorError> {
        Ok(Self {
            inner: VerifiedBrowserResources::new(
                root,
                prepared.resources(),
                checkpoint,
                "INVALID_AUDIO_PLAN",
            )?,
        })
    }
    pub fn handles(&self) -> Vec<ClassicAudioResourceHandle> {
        self.inner
            .handles()
            .map(|(id, handle)| ClassicAudioResourceHandle {
                asset_id: id.into(),
                handle: handle.into(),
            })
            .collect()
    }
}
impl<F: FnMut() -> Result<(), EditorError>> ClassicAudioResources
    for VerifiedClassicAudioResources<F>
{
    fn checkpoint(&mut self) -> Result<(), EditorError> {
        self.inner.checkpoint()
    }
    fn resolve(&mut self, r: &AudioResource, handle: &str) -> Result<(String, u64), EditorError> {
        self.inner
            .resolve(&r.asset_id, &r.content_hash, r.size, handle)
    }
    fn read(&mut self, id: &str, offset: u64, count: u32) -> Result<(Vec<u8>, bool), EditorError> {
        self.inner.read(id, offset, count)
    }
    fn release(&mut self, id: &str) -> Result<(), EditorError> {
        self.inner.release(id)
    }
}
