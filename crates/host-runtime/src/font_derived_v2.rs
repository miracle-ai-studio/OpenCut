//! Host-owned runtime and policy for reviewed variable-font recipes. Replay and
//! authority precede runtime discovery; a saved receipt never needs Python again.
use super::*;
use opencut_contracts::font_derived_v1::*;
use opencut_editor_application::{
    font_derived_v2::{self as application, DerivedFontSourceV1, PinnedDerivedFontV1},
    font_import_v1::{FontImportCheckpointV1, FontImportPolicyV1, catalog::FontFetchPolicyV1},
    project_v4::{ProjectV4Read, V4Intent},
};
use opencut_native_services::font_derived_v2::NativeConfiguredDerivedFontSourceV2;
#[cfg(test)]
mod tests;

fn policy() -> FontFetchPolicyV1 {
    FontFetchPolicyV1 {
        max_bytes: 64 * 1024 * 1024,
        allow_public_font_download: std::env::var("OPENCUT_ALLOW_PUBLIC_FONT_DOWNLOAD").as_deref()
            == Ok("1"),
        ..FontFetchPolicyV1::default()
    }
}
pub(super) fn configured() -> Result<NativeConfiguredDerivedFontSourceV2, EditorError> {
    let unavailable = || {
        EditorError::new(
            "FONT_DERIVATION_UNAVAILABLE",
            "A pinned variable-font derivation runtime is not configured on this host",
        )
    };
    let python = match std::env::var_os("OPENCUT_FONT_DERIVE_PYTHON") {
        Some(path) if !path.is_empty() => std::path::PathBuf::from(path),
        Some(_) => return Err(unavailable()),
        None => crate::bundled_runtime::executable("python")?.ok_or_else(unavailable)?,
    };
    let script = match std::env::var_os("OPENCUT_FONT_DERIVE_V2_SCRIPT") {
        Some(path) if !path.is_empty() => std::path::PathBuf::from(path),
        Some(_) => return Err(unavailable()),
        None => match std::env::var_os("OPENCUT_FONT_DERIVE_SCRIPT") {
            Some(path) if !path.is_empty() => std::path::PathBuf::from(path)
                .parent()
                .map(|parent| parent.join("run_derived_v2.py"))
                .ok_or_else(unavailable)?,
            Some(_) => return Err(unavailable()),
            None => crate::bundled_runtime::script("run_derived_v2.py")?.ok_or_else(unavailable)?,
        },
    };
    let cache = std::env::var_os("OPENCUT_FONT_DERIVE_CACHE")
        .filter(|v| !v.is_empty())
        .map(Into::into);
    Ok(NativeConfiguredDerivedFontSourceV2::new(
        python.into(),
        script.into(),
        cache,
    ))
}
struct HostDerivedFontSource;
impl DerivedFontSourceV1 for HostDerivedFontSource {
    fn obtain(
        &self,
        source: &PinnedDerivedFontV1,
        fetch_policy: &FontFetchPolicyV1,
        checkpoint: &mut dyn FontImportCheckpointV1,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        configured()?.obtain(source, fetch_policy, checkpoint)
    }
}

pub(super) fn catalog() -> Result<FontDerivedCatalogV1, EditorError> {
    let sources = application::catalog()?;
    let runtime = configured();
    let available = match &runtime {
        Ok(runtime) => runtime.availability(),
        Err(error) => Err(error.clone()),
    };
    let can_acquire = policy().allow_public_font_download
        || runtime
            .as_ref()
            .is_ok_and(|r| sources.iter().any(|s| r.cached(s)));
    let reason = match available {
        Err(error) => Some(error.message),
        Ok(()) if !can_acquire => Some("Public font downloads are disabled and no exact derived output is cached on this host.".into()),
        Ok(()) => None,
    };
    Ok(FontDerivedCatalogV1 {
        schema_version: 2,
        import_available: reason.is_none(),
        unavailable_reason: reason,
        source_commit: opencut_editor_application::font_import_v1::catalog::GOOGLE_FONTS_COMMIT_V1
            .into(),
        fonts: sources
            .into_iter()
            .map(|source| source.source.face)
            .collect(),
    })
}
pub(super) fn source(
    request: &FontDerivedSourceRequestV1,
) -> Result<FontDerivedSourceV1, EditorError> {
    Ok(application::source(&request.catalog_id)?.source)
}
pub(super) fn proof(
    host: &Automation,
    request: &ReadDerivedFontProofV1,
) -> Result<DerivedFontProofV1, EditorError> {
    application::proof::read(&host.app, request)
}
pub(super) fn import(
    host: &mut Automation,
    request: &ImportDerivedFontV1,
) -> Result<opencut_contracts::TransactionResult, EditorError> {
    application::validate_import_request_v2(request)?;
    let fetch_policy = policy();
    host.app
        .authorize_v4(V4Intent::DerivedFontImport(request))?;
    let mut authority = EditorApplication::open_managed(&host.database)?;
    authority.set_execution_context(host.app.execution_context().clone());
    let started = std::time::Instant::now();
    let mut checkpoint = || {
        authority.authorize_v4(V4Intent::DerivedFontImport(request))?;
        if started.elapsed() > std::time::Duration::from_millis(fetch_policy.deadline_ms) {
            return Err(EditorError::new(
                "FONT_SOURCE_TIMEOUT",
                "Derived font import exceeded its host deadline",
            ));
        }
        Ok(())
    };
    let publisher = opencut_asset_store::font_import_v1::ManagedFontPublisherV1::new(
        host.app.asset_store_root(),
    );
    opencut_editor_application::project_v4::import_derived_font_v1(
        &mut host.app,
        &HostDerivedFontSource,
        &opencut_native_services::font_import_v2::CatalogFontInspectorV2,
        &publisher,
        request,
        &fetch_policy,
        &FontImportPolicyV1::default(),
        &mut checkpoint,
    )
}
