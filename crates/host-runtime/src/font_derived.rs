//! Host-owned runtime and policy for reviewed variable-font recipes. Replay and
//! authority precede runtime discovery; a saved receipt never needs Python again.
use super::*;
use opencut_contracts::font_derived::*;
use opencut_editor_application::{
    font_derived::{self as application, DerivedFontSource, PinnedDerivedFont},
    font_import::{FontImportCheckpoint, FontImportPolicy, catalog::FontFetchPolicy},
    scene::{SceneCommitIntent, SceneRead},
};
use opencut_native_services::font_derived::NativeConfiguredOutlineFontSource;

fn policy() -> FontFetchPolicy {
    FontFetchPolicy {
        allow_public_font_download: std::env::var("OPENCUT_ALLOW_PUBLIC_FONT_DOWNLOAD").as_deref()
            == Ok("1"),
        ..FontFetchPolicy::default()
    }
}
pub(super) fn configured() -> Result<NativeConfiguredOutlineFontSource, EditorError> {
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
    let script = match std::env::var_os("OPENCUT_FONT_DERIVE_SCRIPT") {
        Some(path) if !path.is_empty() => std::path::PathBuf::from(path),
        Some(_) => return Err(unavailable()),
        None => crate::bundled_runtime::script("run_derived.py")?.ok_or_else(unavailable)?,
    };
    let cache = std::env::var_os("OPENCUT_FONT_DERIVE_CACHE")
        .filter(|v| !v.is_empty())
        .map(Into::into);
    Ok(NativeConfiguredOutlineFontSource::new(
        python.into(),
        script.into(),
        cache,
    ))
}
struct HostDerivedFontSource;
impl DerivedFontSource for HostDerivedFontSource {
    fn obtain(
        &self,
        source: &PinnedDerivedFont,
        fetch_policy: &FontFetchPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        configured()?.obtain(source, fetch_policy, checkpoint)
    }
}

pub(super) fn import(
    host: &mut Automation,
    request: &ImportFont,
) -> Result<opencut_contracts::TransactionResult, EditorError> {
    application::validate_import_request(request)?;
    let fetch_policy = policy();
    host.app
        .authorize_project(SceneCommitIntent::DerivedFontImport(request))?;
    let mut authority = EditorApplication::open_managed(&host.database)?;
    authority.set_execution_context(host.app.execution_context().clone());
    let started = std::time::Instant::now();
    let mut checkpoint = || {
        authority.authorize_project(SceneCommitIntent::DerivedFontImport(request))?;
        if started.elapsed() > std::time::Duration::from_millis(fetch_policy.deadline_ms) {
            return Err(EditorError::new(
                "FONT_SOURCE_TIMEOUT",
                "Derived font import exceeded its host deadline",
            ));
        }
        Ok(())
    };
    let publisher =
        opencut_asset_store::font_import::ManagedFontPublisher::new(host.app.asset_store_root());
    opencut_editor_application::scene::import_derived_font(
        &mut host.app,
        &HostDerivedFontSource,
        &opencut_native_services::font_import::StaticFontInspector,
        &publisher,
        request,
        &fetch_policy,
        &FontImportPolicy::default(),
        &mut checkpoint,
    )
}
