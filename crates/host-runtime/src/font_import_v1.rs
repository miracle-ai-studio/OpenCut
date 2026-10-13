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
        fonts: font_catalog_v1().iter().map(catalog_entry).collect(),
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

fn catalog_entry(
    font: &opencut_editor_application::font_import_v1::catalog::PinnedFontSourceV1,
) -> FontCatalogEntryV1 {
    FontCatalogEntryV1 {
        id: font.catalog_id.into(),
        family: font.family.into(),
        weight: font.weight,
        style: font.style,
        source_url: font.source_url.into(),
        source_commit: font.source_commit.into(),
        content_hash: font.content_hash.into(),
        size_bytes: font.size_bytes.to_string(),
        license_id: font.license_identifier.into(),
        license_notice: font.license_notice.into(),
    }
}
pub(super) fn source(
    request: &opencut_contracts::font_browser_v2::FontSourceRequestV2,
) -> Result<FontCatalogEntryV1, EditorError> {
    let source = opencut_editor_application::font_import_v1::catalog::pinned_font_source_v1(
        &request.catalog_id,
    )?;
    Ok(catalog_entry(source))
}
pub(super) fn browser_catalog() -> opencut_contracts::font_browser_v2::FontBrowserCatalogV2 {
    use opencut_contracts::font_browser_v2 as wire;
    use opencut_editor_application::font_import_v1::browser_catalog as app;
    let data = app::font_browser_catalog_v2();
    let allowed = policy().allow_public_font_download;
    wire::FontBrowserCatalogV2 {
        schema_version: 2,
        download_available: allowed,
        unavailable_reason: (!allowed)
            .then(|| "Public font downloads are disabled by this host.".into()),
        source_commit: data.source_commit.clone(),
        classic_atlas_sha256: data.classic_atlas_sha256.clone(),
        classic_family_count: data.classic_family_count as u32,
        pinned_face_count: data.pinned_face_count as u32,
        families: data
            .families
            .iter()
            .map(|family| wire::FontBrowserFamilyV2 {
                family: family.family.clone(),
                category: match family.category {
                    app::FontBrowserCategoryV2::Google => wire::FontBrowserCategoryV2::Google,
                    app::FontBrowserCategoryV2::System => wire::FontBrowserCategoryV2::System,
                },
                faces: family
                    .faces
                    .iter()
                    .map(|face| wire::FontBrowserFaceV2 {
                        weight: face.weight,
                        style: face.style,
                        catalog_id: face.catalog_id.clone(),
                        unavailable_reason: face.unavailable_reason.map(|reason| match reason {
                            app::FontBrowserUnavailableReasonV2::WeightNotExpressedByTextV4 => {
                                wire::FontBrowserUnavailableReasonV2::WeightNotExpressedByTextV4
                            }
                            app::FontBrowserUnavailableReasonV2::NoPinnedStaticSource => {
                                wire::FontBrowserUnavailableReasonV2::NoPinnedStaticSource
                            }
                            app::FontBrowserUnavailableReasonV2::SystemFontNotRedistributable => {
                                wire::FontBrowserUnavailableReasonV2::SystemFontNotRedistributable
                            }
                        }),
                    })
                    .collect(),
            })
            .collect(),
    }
}
