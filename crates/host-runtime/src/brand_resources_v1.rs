//! No download/runtime configuration: approved SVG sources are compiled and offline.
use super::*;
use opencut_contracts::brand_resources_v1::*;
use opencut_editor_application::project_v4::{ProjectV4Read, V4Intent};
pub(super) fn catalog(input: &ListBrandResourcesV1) -> Result<BrandCatalogV1, EditorError> {
    opencut_editor_application::brand_resources_v1::catalog(input)
}
pub(super) fn source(input: &GetBrandSourceV1) -> Result<BrandSourceV1, EditorError> {
    opencut_editor_application::brand_resources_v1::source(input)
}
pub(super) fn import(
    host: &mut Automation,
    input: &ImportBrandResourceV1,
) -> Result<opencut_contracts::TransactionResult, EditorError> {
    host.app.authorize_v4(V4Intent::BrandImport(input))?;
    let mut authority = EditorApplication::open_managed(&host.database)?;
    authority.set_execution_context(host.app.execution_context().clone());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let mut checkpoint = || {
        authority.authorize_v4(V4Intent::BrandImport(input))?;
        if std::time::Instant::now() >= deadline {
            return Err(EditorError::new(
                "RESOURCE_SOURCE_TIMEOUT",
                "Brand acquisition exceeded the host deadline",
            ));
        }
        Ok(())
    };
    opencut_editor_application::project_v4::brand_resources_v1::import(
        &mut host.app,
        input,
        &mut checkpoint,
    )
}
