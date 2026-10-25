//! Public views and edits share revision authority with internal use cases.
//! Resource selectors recover complete authorized records, never public views.
use super::*;
use opencut_contracts::project_public::*;
mod edits;
mod projection;
pub(crate) use edits::plan_public_snapshot;
pub use edits::{apply_public, plan_public};
pub use projection::project;
#[cfg(test)]
mod tests;

pub trait ProjectPublicResources: SceneRead {
    /// None preserves old header-only observations. An explicit unknown packet
    /// observation suppresses the header fallback and remains version-scoped.
    fn authorized_project_media_packet_rate_scene(
        &self,
        _project_id: &str,
        _media_id: &str,
        _version: &opencut_contracts::AssetVersion,
    ) -> Result<Option<opencut_media_contracts::ClassicPacketRate>, EditorError> {
        Ok(None)
    }
    /// Read only from the authorized fixed revision, never a moving head.
    fn project_asset_packet_rate_scene(
        &self,
        _project_id: &str,
        _revision: &str,
        _asset_id: &str,
    ) -> Result<Option<opencut_media_contracts::ClassicPacketRate>, EditorError> {
        Ok(None)
    }
    /// Read the authorized available-project-media observation and verify this
    /// exact immutable identity. Never capture a locator or trust hash alone.
    /// Commit must recheck resource scope and hold CAS protection through commit.
    fn authorized_project_media_scene(
        &self,
        project_id: &str,
        media_id: &str,
        version: &opencut_contracts::AssetVersion,
    ) -> Result<ProjectAsset, EditorError>;
}
