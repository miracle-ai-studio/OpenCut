//! No download/runtime configuration: approved SVG sources are compiled and offline.
use super::*;
use opencut_contracts::brand_resources::*;
use opencut_editor_application::scene::{SceneCommitIntent, SceneRead};
pub(super) fn catalog(input: &ListBrandResources) -> Result<BrandCatalog, EditorError> {
    opencut_editor_application::brand_resources::catalog(input)
}
pub(super) fn source(input: &GetBrandSource) -> Result<BrandSource, EditorError> {
    opencut_editor_application::brand_resources::source(input)
}
pub(super) fn import(
    host: &mut Automation,
    input: &ImportBrandResource,
) -> Result<opencut_contracts::TransactionResult, EditorError> {
    host.app
        .authorize_project(SceneCommitIntent::BrandImport(input))?;
    let mut authority = EditorApplication::open_managed(&host.database)?;
    authority.set_execution_context(host.app.execution_context().clone());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let mut checkpoint = || {
        authority.authorize_project(SceneCommitIntent::BrandImport(input))?;
        if std::time::Instant::now() >= deadline {
            return Err(EditorError::new(
                "RESOURCE_SOURCE_TIMEOUT",
                "Brand acquisition exceeded the host deadline",
            ));
        }
        Ok(())
    };
    opencut_editor_application::scene::brand_resources::import(
        &mut host.app,
        input,
        &mut checkpoint,
    )
}
