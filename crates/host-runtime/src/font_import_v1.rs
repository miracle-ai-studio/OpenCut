//! Trusted assembly for the pinned public font source. This permission is a
//! host setting, never derived from request JSON or a project text field.
use super::*;
use opencut_contracts::font_import_v1::{FontCatalogEntryV1, FontCatalogV1, ImportFontV1};
use opencut_editor_application::{
    font_import_v1::{
        FontImportPolicyV1,
        catalog::{FontFetchPolicyV1, font_catalog_v1},
    },
    project_v4::{ProjectV4Read, V4Intent},
};

fn policy() -> FontFetchPolicyV1 {
    FontFetchPolicyV1 {
        allow_public_font_download: std::env::var("OPENCUT_ALLOW_PUBLIC_FONT_DOWNLOAD").as_deref()
            == Ok("1"),
        ..FontFetchPolicyV1::default()
    }
}
pub(super) fn catalog() -> FontCatalogV1 {
    let allowed = policy().allow_public_font_download;
    FontCatalogV1 {
        schema_version: 1,
        download_available: allowed,
        unavailable_reason: (!allowed)
            .then(|| "Public font downloads are disabled by this host.".into()),
        fonts: font_catalog_v1()
            .iter()
            .map(|font| FontCatalogEntryV1 {
                id: font.catalog_id.into(),
                family: font.family.into(),
                weight: font.weight,
                style: font.style,
                source_url: font.source_url.into(),
                source_commit: font.source_commit.into(),
                content_hash: font.content_hash.into(),
                size_bytes: font.size_bytes.to_string(),
                license_id: "OFL-1.1".into(),
                license_notice: font.license_notice.into(),
            })
            .collect(),
    }
}

pub(super) fn import(
    host: &mut Automation,
    request: &ImportFontV1,
) -> Result<opencut_contracts::TransactionResult, EditorError> {
    let policy = policy();
    // A second connection checks live authority between bounded native reads;
    // no transaction or database write lock is held across network I/O.
    host.app.authorize_v4(V4Intent::FontImport(request))?;
    let mut authority = EditorApplication::open_managed(&host.database)?;
    authority.set_execution_context(host.app.execution_context().clone());
    let started = std::time::Instant::now();
    let mut checkpoint = || {
        authority.authorize_v4(V4Intent::FontImport(request))?;
        if started.elapsed() > std::time::Duration::from_millis(policy.deadline_ms) {
            return Err(EditorError::new(
                "FONT_SOURCE_TIMEOUT",
                "Font import exceeded its host deadline",
            ));
        }
        Ok(())
    };
    let publisher = opencut_asset_store::font_import_v1::ManagedFontPublisherV1::new(
        host.app.asset_store_root(),
    );
    opencut_editor_application::project_v4::import_font_v1(
        &mut host.app,
        &opencut_native_services::font_import_v1::GoogleFontsFetchV1,
        &opencut_native_services::font_import_v1::StaticFontInspectorV1,
        &publisher,
        request,
        &policy,
        &FontImportPolicyV1::default(),
        &mut checkpoint,
    )
}
