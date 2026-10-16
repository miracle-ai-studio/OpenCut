//! Host assembly for unchanged fixed font sources and explicit browser synthesis.
use super::*;
use opencut_contracts::{font_derived_v1::*, font_synthesis_v1::*};
use opencut_editor_application::{
    font_derived_v1::{DerivedFontSourceV1, PinnedDerivedFontV1},
    font_import_v1::{FontImportCheckpointV1, FontImportPolicyV1, catalog::FontFetchPolicyV1},
    font_synthesis_v1 as application,
    project_v4::{ProjectV4Read, V4Intent},
};

pub(super) fn source(
    request: &FontDerivedSourceRequestV1,
) -> Result<FontSynthesisSourceV1, EditorError> {
    application::source(&request.catalog_id)
}
pub(super) fn proof(
    host: &Automation,
    request: &ReadDerivedFontProofV1,
) -> Result<FontSynthesisProofV1, EditorError> {
    application::proof::read(&host.app, request)
}
// Native synthesis first checks the exact existing output cache. These adapters
// discover only the selected trusted runtime, and only after that cache misses.
struct HostBaseDerivedV1;
struct HostBaseDerivedV2;
impl DerivedFontSourceV1 for HostBaseDerivedV1 {
    fn obtain(
        &self,
        pin: &PinnedDerivedFontV1,
        policy: &FontFetchPolicyV1,
        checkpoint: &mut dyn FontImportCheckpointV1,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        super::font_derived_v1::configured()?.obtain(pin, policy, checkpoint)
    }
}
impl DerivedFontSourceV1 for HostBaseDerivedV2 {
    fn obtain(
        &self,
        pin: &PinnedDerivedFontV1,
        policy: &FontFetchPolicyV1,
        checkpoint: &mut dyn FontImportCheckpointV1,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        super::font_derived_v2::configured()?.obtain(pin, policy, checkpoint)
    }
}
pub(super) fn import(
    host: &mut Automation,
    request: &ImportDerivedFontV1,
) -> Result<opencut_contracts::TransactionResult, EditorError> {
    application::validate_import_request(request)?;
    host.app
        .authorize_v4(V4Intent::DerivedFontImport(request))?;
    let limits = application::acquisition_limits(&request.catalog_id)?;
    let fetch_policy = FontFetchPolicyV1 {
        max_bytes: limits.max_source_bytes,
        deadline_ms: limits.max_deadline_ms.min(90_000),
        allow_public_font_download: std::env::var("OPENCUT_ALLOW_PUBLIC_FONT_DOWNLOAD").as_deref()
            == Ok("1"),
        ..FontFetchPolicyV1::default()
    };
    let mut authority = EditorApplication::open_managed(&host.database)?;
    authority.set_execution_context(host.app.execution_context().clone());
    let started = std::time::Instant::now();
    let mut checkpoint = || {
        authority.authorize_v4(V4Intent::DerivedFontImport(request))?;
        if started.elapsed() > std::time::Duration::from_millis(fetch_policy.deadline_ms) {
            return Err(EditorError::new(
                "FONT_SOURCE_TIMEOUT",
                "Font synthesis import exceeded its host deadline",
            ));
        }
        Ok(())
    };
    let base_v1 = HostBaseDerivedV1;
    let base_v2 = HostBaseDerivedV2;
    let source = opencut_native_services::font_synthesis_v1::RoutedSynthesisFontSourceV1::new(
        std::env::var_os("OPENCUT_FONT_DERIVE_CACHE")
            .filter(|v| !v.is_empty())
            .map(Into::into),
        Some(&base_v1),
        Some(&base_v2),
    );
    let publisher = opencut_asset_store::font_import_v1::ManagedFontPublisherV1::new(
        host.app.asset_store_root(),
    );
    opencut_editor_application::project_v4::import_derived_font_v1(
        &mut host.app,
        &source,
        &opencut_native_services::font_import_v2::CatalogFontInspectorV2,
        &publisher,
        request,
        &fetch_policy,
        &FontImportPolicyV1 {
            max_font_bytes: limits.max_output_bytes,
            ..FontImportPolicyV1::default()
        },
        &mut checkpoint,
    )
}
