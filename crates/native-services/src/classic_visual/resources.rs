use crate::browser_resources::VerifiedBrowserResources;
use opencut_composition::project_v4::PreparedVisualFrameV4;
use opencut_contracts::EditorError;
use opencut_editor_application::classic_visual::{
    ClassicVisualResourceHandle, ClassicVisualResources, VisualResource,
};
use std::path::Path;
pub struct VerifiedClassicVisualResources<F> {
    inner: VerifiedBrowserResources<F>,
}
impl<F: FnMut() -> Result<(), EditorError>> VerifiedClassicVisualResources<F> {
    pub fn new(
        root: &Path,
        prepared: &PreparedVisualFrameV4,
        checkpoint: F,
    ) -> Result<Self, EditorError> {
        Ok(Self {
            inner: VerifiedBrowserResources::new(
                root,
                prepared.resources(),
                checkpoint,
                "INVALID_VISUAL_PLAN",
            )?,
        })
    }
    pub fn handles(&self) -> Vec<ClassicVisualResourceHandle> {
        self.inner
            .handles()
            .map(|(id, handle)| ClassicVisualResourceHandle {
                asset_id: id.into(),
                handle: handle.into(),
            })
            .collect()
    }
}
impl<F: FnMut() -> Result<(), EditorError>> ClassicVisualResources
    for VerifiedClassicVisualResources<F>
{
    fn checkpoint(&mut self) -> Result<(), EditorError> {
        self.inner.checkpoint()
    }
    fn resolve(&mut self, r: &VisualResource, handle: &str) -> Result<(String, u64), EditorError> {
        let size = r
            .size
            .parse::<u64>()
            .map_err(|_| EditorError::new("INVALID_VISUAL_PLAN", "Invalid encoded byte count"))?;
        self.inner
            .resolve(&r.asset_id, &r.content_hash, size, handle)
    }
    fn read(&mut self, id: &str, offset: u64, count: u32) -> Result<(Vec<u8>, bool), EditorError> {
        self.inner.read(id, offset, count)
    }
    fn release(&mut self, id: &str) -> Result<(), EditorError> {
        self.inner.release(id)
    }
}
