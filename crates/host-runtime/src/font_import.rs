//! Trusted assembly for the pinned public font source. This permission is a
//! host setting, never derived from request JSON or a project text field.
use super::*;
use opencut_contracts::font_import::ImportFont;
use opencut_editor_application::{
    font_import::{FontImportPolicy, catalog::FontFetchPolicy},
    scene::{SceneCommitIntent, SceneRead},
};

fn policy() -> FontFetchPolicy {
    FontFetchPolicy {
        allow_public_font_download: std::env::var("OPENCUT_ALLOW_PUBLIC_FONT_DOWNLOAD").as_deref()
            == Ok("1"),
        ..FontFetchPolicy::default()
    }
}

pub(super) fn import(
    host: &mut Automation,
    request: &ImportFont,
) -> Result<opencut_contracts::TransactionResult, EditorError> {
    // Selection pins this native acquisition to the reviewed repository source.
    opencut_editor_application::font_import::catalog::pinned_font_source(&request.catalog_id)?;
    let policy = policy();
    // A second connection checks live authority between bounded native reads;
    // no transaction or database write lock is held across network I/O.
    host.app
        .authorize_project(SceneCommitIntent::FontImport(request))?;
    let mut authority = EditorApplication::open_managed(&host.database)?;
    authority.set_execution_context(host.app.execution_context().clone());
    let started = std::time::Instant::now();
    let mut checkpoint = || {
        authority.authorize_project(SceneCommitIntent::FontImport(request))?;
        if started.elapsed() > std::time::Duration::from_millis(policy.deadline_ms) {
            return Err(EditorError::new(
                "FONT_SOURCE_TIMEOUT",
                "Font import exceeded its host deadline",
            ));
        }
        Ok(())
    };
    let publisher =
        opencut_asset_store::font_import::ManagedFontPublisher::new(host.app.asset_store_root());
    opencut_editor_application::scene::import_font_resource(
        &mut host.app,
        &opencut_native_services::font_import::GoogleFontsFetch,
        &opencut_native_services::font_import::StaticFontInspector,
        &publisher,
        request,
        &policy,
        &FontImportPolicy::default(),
        &mut checkpoint,
    )
}
