//! Trusted resource assembly. Requests cannot change a source URL, runtime or permission.
use super::*;
use opencut_contracts::catalog_resources_v1::*;
use opencut_editor_application::project_v4::{ProjectV4Read, V4Intent};
use opencut_native_services::catalog_resources_v1::{
    CatalogArchiveRuntimeV1, NativeCatalogSoundV1, cached_archive_candidate,
};

pub(super) fn runtime() -> Result<CatalogArchiveRuntimeV1, EditorError> {
    use crate::browser_runtime::{absolute, executable, metadata, package_file};
    let unavailable = || {
        EditorError::new(
            "CAPABILITY_UNAVAILABLE",
            "Pinned resource archive runtime is unavailable",
        )
    };
    if !cfg!(unix) {
        return Err(unavailable());
    }
    let root = std::env::var_os("OPENCUT_CLASSIC_VISUAL_PACKAGE")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()?
                .parent()
                .map(|p| p.join("libexec/classic-visual-renderer"))
        })
        .ok_or_else(unavailable)?;
    let root = absolute(&root).map_err(|_| unavailable())?;
    let manifest = metadata(&root.join("package.json")).map_err(|_| unavailable())?;
    if manifest["name"] != "@opencut/classic-visual-renderer"
        || manifest["version"] != "0.1.0"
        || manifest["type"] != "module"
    {
        return Err(unavailable());
    }
    let node = if let Some(path) = std::env::var_os("OPENCUT_CLASSIC_VISUAL_NODE") {
        executable(std::path::Path::new(&path)).map_err(|_| unavailable())?
    } else if let Some(path) = crate::bundled_runtime::executable("node")? {
        executable(&path).map_err(|_| unavailable())?
    } else {
        std::env::var_os("PATH")
            .as_deref()
            .into_iter()
            .flat_map(std::env::split_paths)
            .filter(|p| p.is_absolute())
            .find_map(|p| executable(&p.join("node")).ok())
            .ok_or_else(unavailable)?
    };
    let runner = package_file(&root, "host/catalog-archive-v1.mjs").map_err(|_| unavailable())?;
    // Source identity is compiled into the host. An older separately configured
    // package cannot silently implement a different extraction policy.
    let bytes = std::fs::read(&runner).map_err(|_| unavailable())?;
    if bytes
        != include_bytes!("../../../packages/classic-visual-renderer/host/catalog-archive-v1.mjs")
    {
        return Err(unavailable());
    }
    opencut_media::FfmpegBackend::discover().map_err(|_| unavailable())?;
    Ok(CatalogArchiveRuntimeV1 { node, runner })
}
fn allowed() -> bool {
    std::env::var("OPENCUT_ALLOW_PUBLIC_RESOURCE_DOWNLOAD").as_deref() == Ok("1")
}
pub(super) fn catalog(host: &Automation) -> ResourceCatalogV1 {
    catalog_report(
        allowed(),
        runtime().is_ok(),
        cached_archive_candidate(host.app.asset_store_root()),
    )
}
fn catalog_report(
    download_allowed: bool,
    runtime_available: bool,
    archive_candidate: bool,
) -> ResourceCatalogV1 {
    // The frozen field is an acquisition hint. A local candidate still requires
    // full hash/size verification in the original import transaction.
    let available = runtime_available && (download_allowed || archive_candidate);
    let reason = if !runtime_available {
        Some(
            "The pinned sound archive and media probe runtime are unavailable on this host.".into(),
        )
    } else if !available {
        Some(
            "Public sound downloads are disabled and no local archive candidate is available."
                .into(),
        )
    } else {
        None
    };
    ResourceCatalogV1 {
        schema_version: 1,
        sound_download_available: available,
        sound_unavailable_reason: reason,
        entries: opencut_editor_application::catalog_resources_v1::entries().to_vec(),
        brands_available: false,
    }
}
pub(super) fn import(
    host: &mut Automation,
    request: &ImportCatalogResourceV1,
) -> Result<opencut_contracts::TransactionResult, EditorError> {
    host.app.authorize_v4(V4Intent::CatalogImport(request))?;
    let runtime = if request.uploaded.is_none() {
        runtime().ok()
    } else {
        None
    };
    let native = NativeCatalogSoundV1 {
        root: host.app.asset_store_root().to_path_buf(),
        runtime,
        allow_download: allowed(),
    };
    let mut authority = EditorApplication::open_managed(&host.database)?;
    authority.set_execution_context(host.app.execution_context().clone());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
    let mut checkpoint = || {
        authority.authorize_v4(V4Intent::CatalogImport(request))?;
        if std::time::Instant::now() >= deadline {
            return Err(EditorError::new(
                "RESOURCE_SOURCE_TIMEOUT",
                "Catalog import exceeded the host deadline",
            ));
        }
        Ok(())
    };
    opencut_editor_application::project_v4::catalog_resources_v1::import(
        &mut host.app,
        &native,
        request,
        &mut checkpoint,
    )
}
pub(super) fn generate(
    host: &mut Automation,
    request: &GenerateCatalogSoundV1,
) -> Result<opencut_contracts::TransactionResult, EditorError> {
    host.app.authorize_v4(V4Intent::CatalogGenerate(request))?;
    let native = NativeCatalogSoundV1 {
        root: host.app.asset_store_root().to_path_buf(),
        runtime: None,
        allow_download: false,
    };
    let mut authority = EditorApplication::open_managed(&host.database)?;
    authority.set_execution_context(host.app.execution_context().clone());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut checkpoint = || {
        authority.authorize_v4(V4Intent::CatalogGenerate(request))?;
        if std::time::Instant::now() >= deadline {
            return Err(EditorError::new(
                "RESOURCE_SOURCE_TIMEOUT",
                "Local sound generation exceeded the host deadline",
            ));
        }
        Ok(())
    };
    opencut_editor_application::project_v4::catalog_resources_v1::generate(
        &mut host.app,
        &native,
        request,
        &mut checkpoint,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_archive_candidate_is_acquirable_only_with_the_complete_runtime() {
        let offline = catalog_report(false, true, true);
        assert!(offline.sound_download_available);
        assert!(offline.sound_unavailable_reason.is_none());
        assert_eq!(
            offline
                .entries
                .iter()
                .filter(|e| e.category == CatalogResourceCategoryV1::Sounds)
                .count(),
            51
        );
        assert!(!catalog_report(false, false, true).sound_download_available);
        assert!(!catalog_report(true, false, true).sound_download_available);
    }
    #[test]
    fn missing_offline_source_is_explicit_without_advertising_network_permission() {
        let missing = catalog_report(false, true, false);
        assert!(!missing.sound_download_available);
        assert!(
            missing
                .sound_unavailable_reason
                .unwrap()
                .contains("disabled and no local archive")
        );
        assert!(catalog_report(true, true, false).sound_download_available);
    }
}
