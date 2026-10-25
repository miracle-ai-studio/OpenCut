//! Host-owned runtime and policy for reviewed variable-font recipes. Replay and
//! authority precede runtime discovery; a saved receipt never needs Python again.
use super::*;
use opencut_contracts::font_derived::*;
use opencut_editor_application::{
    font_import::{FontImportCheckpoint, FontImportPolicy, catalog::FontFetchPolicy},
    font_named::{self as application, DerivedFontSource, PinnedDerivedFont},
    scene::{SceneCommitIntent, SceneRead},
};
use opencut_native_services::font_named::NativeConfiguredNamedFontSource;
#[cfg(test)]
mod tests;

fn policy() -> FontFetchPolicy {
    FontFetchPolicy {
        max_bytes: 64 * 1024 * 1024,
        // V2's approved child CPU envelope is90s; its wall envelope is120s.
        deadline_ms: 120_000,
        allow_public_font_download: std::env::var("OPENCUT_ALLOW_PUBLIC_FONT_DOWNLOAD").as_deref()
            == Ok("1"),
        ..FontFetchPolicy::default()
    }
}
pub(super) fn configured_paths(
    script_env: &str,
    script_name: &str,
) -> Result<
    (
        std::path::PathBuf,
        std::path::PathBuf,
        Option<std::path::PathBuf>,
    ),
    EditorError,
> {
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
    let script = match std::env::var_os(script_env) {
        Some(path) if !path.is_empty() => std::path::PathBuf::from(path),
        Some(_) => return Err(unavailable()),
        None => match std::env::var_os("OPENCUT_FONT_DERIVE_SCRIPT") {
            Some(path) if !path.is_empty() => std::path::PathBuf::from(path)
                .parent()
                .map(|parent| parent.join(script_name))
                .ok_or_else(unavailable)?,
            Some(_) => return Err(unavailable()),
            None => crate::bundled_runtime::script(script_name)?.ok_or_else(unavailable)?,
        },
    };
    let cache = std::env::var_os("OPENCUT_FONT_DERIVE_CACHE")
        .filter(|v| !v.is_empty())
        .map(Into::into);
    Ok((python, script, cache))
}
pub(super) fn configured() -> Result<NativeConfiguredNamedFontSource, EditorError> {
    let (python, script, cache) =
        configured_paths("OPENCUT_FONT_NAMED_SCRIPT", "run_derived_v2.py")?;
    Ok(NativeConfiguredNamedFontSource::new(python, script, cache))
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
    import_with_source(host, request, &HostDerivedFontSource)
}
pub(super) fn import_with_source(
    host: &mut Automation,
    request: &ImportFont,
    source: &impl DerivedFontSource,
) -> Result<opencut_contracts::TransactionResult, EditorError> {
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
        source,
        &opencut_native_services::font_inspection::CatalogFontInspector,
        &publisher,
        request,
        &fetch_policy,
        &FontImportPolicy::default(),
        &mut checkpoint,
    )
}
