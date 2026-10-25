//! Host assembly for unchanged fixed font sources and explicit browser synthesis.
use super::*;
use opencut_contracts::{font_derived::*, font_synthesis::*};
use opencut_editor_application::{
    font_derived::{DerivedFontSource, PinnedDerivedFont},
    font_import::{FontImportCheckpoint, FontImportPolicy, catalog::FontFetchPolicy},
    font_synthesis as application,
    scene::{SceneCommitIntent, SceneRead},
};

pub(super) fn source(request: &FontSourceRequest) -> Result<FontSynthesisSource, EditorError> {
    application::source(&request.catalog_id)
}
pub(super) fn proof(
    host: &Automation,
    request: &ReadDerivedFontProof,
) -> Result<FontSynthesisProof, EditorError> {
    application::proof::read(&host.app, request)
}
// Native synthesis first checks the exact existing output cache. These adapters
// discover only the selected trusted runtime, and only after that cache misses.
struct HostOutlineSource;
struct HostNamedSource;
struct HostBaseColorStatic;
impl DerivedFontSource for HostOutlineSource {
    fn obtain(
        &self,
        pin: &PinnedDerivedFont,
        policy: &FontFetchPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        super::font_derived::configured()?.obtain(pin, policy, checkpoint)
    }
}
impl DerivedFontSource for HostNamedSource {
    fn obtain(
        &self,
        pin: &PinnedDerivedFont,
        policy: &FontFetchPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        super::font_named::configured()?.obtain(pin, policy, checkpoint)
    }
}
impl DerivedFontSource for HostBaseColorStatic {
    fn obtain(
        &self,
        pin: &PinnedDerivedFont,
        policy: &FontFetchPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        super::font_color_static::configured()?.obtain(pin, policy, checkpoint)
    }
}
pub(super) fn import(
    host: &mut Automation,
    request: &ImportFont,
) -> Result<opencut_contracts::TransactionResult, EditorError> {
    application::validate_import_request(request)?;
    host.app
        .authorize_project(SceneCommitIntent::DerivedFontImport(request))?;
    let limits = application::acquisition_limits(&request.catalog_id)?;
    let fetch_policy = FontFetchPolicy {
        max_bytes: limits.max_source_bytes,
        // Preserve the selected base policy: static30s, derived120s.
        deadline_ms: limits.max_deadline_ms,
        allow_public_font_download: std::env::var("OPENCUT_ALLOW_PUBLIC_FONT_DOWNLOAD").as_deref()
            == Ok("1"),
        ..FontFetchPolicy::default()
    };
    let mut authority = EditorApplication::open_managed(&host.database)?;
    authority.set_execution_context(host.app.execution_context().clone());
    let started = std::time::Instant::now();
    let mut checkpoint = || {
        authority.authorize_project(SceneCommitIntent::DerivedFontImport(request))?;
        if started.elapsed() > std::time::Duration::from_millis(fetch_policy.deadline_ms) {
            return Err(EditorError::new(
                "FONT_SOURCE_TIMEOUT",
                "Font synthesis import exceeded its host deadline",
            ));
        }
        Ok(())
    };
    let outline = HostOutlineSource;
    let renamed = HostNamedSource;
    let base_color = HostBaseColorStatic;
    let source = opencut_native_services::font_synthesis::RoutedSynthesisFontSource::new(
        std::env::var_os("OPENCUT_FONT_DERIVE_CACHE")
            .filter(|v| !v.is_empty())
            .map(Into::into),
        Some(&outline),
        Some(&renamed),
    )
    .with_color_static(&base_color);
    let publisher =
        opencut_asset_store::font_import::ManagedFontPublisher::new(host.app.asset_store_root());
    opencut_editor_application::scene::import_derived_font(
        &mut host.app,
        &source,
        &opencut_native_services::font_inspection::CatalogFontInspector,
        &publisher,
        request,
        &fetch_policy,
        &FontImportPolicy {
            max_font_bytes: limits.max_output_bytes,
            ..FontImportPolicy::default()
        },
        &mut checkpoint,
    )
}
