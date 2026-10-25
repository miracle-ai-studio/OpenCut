pub use opencut_api_contracts::plugin_packages::PinnedPluginPackage;
use opencut_api_contracts::plugin_publication;
pub use opencut_api_contracts::plugins::{InstallInput, PlanInput, PluginInput};
use opencut_contracts::EditorError;
use opencut_editor_application::maintenance::{PublicationJournal, UntrackedPublication};
use opencut_editor_application::plugins as application;
pub use opencut_editor_application::plugins::InstalledWasmPlugin;
use opencut_plugin_host::PluginHost;
use std::path::Path;
pub struct NativePlugins<'a>(
    pub &'a Path,
    pub Option<&'a dyn PublicationJournal<plugin_publication::Object>>,
);
fn portable(
    value: opencut_plugin_host::InstalledWasmPlugin,
) -> Result<InstalledWasmPlugin, EditorError> {
    let install_path = value
        .install_path
        .into_os_string()
        .into_string()
        .map_err(|_| {
            EditorError::new(
                "INVALID_PLUGIN",
                "Plugin installation path is not valid UTF-8",
            )
        })?;
    Ok(InstalledWasmPlugin {
        manifest: value.manifest,
        package_sha256: value.package_sha256,
        granted_permissions: value.granted_permissions,
        install_path,
    })
}
impl application::PluginCatalog for NativePlugins<'_> {
    fn list(&self) -> Result<Vec<InstalledWasmPlugin>, EditorError> {
        PluginHost::read_only(self.0)?
            .list()?
            .into_iter()
            .map(portable)
            .collect()
    }
    fn inspect(&self, input: &PluginInput) -> Result<InstalledWasmPlugin, EditorError> {
        portable(PluginHost::read_only(self.0)?.inspect(&input.id, &input.version)?)
    }
    fn install(&self, input: InstallInput) -> Result<InstalledWasmPlugin, EditorError> {
        // Reject an unrepresentable public location before publishing a package.
        if self.0.to_str().is_none() {
            return Err(EditorError::new(
                "INVALID_PLUGIN",
                "Plugin installation path is not valid UTF-8",
            ));
        }
        portable(PluginHost::new_managed(self.0)?.install_observed(
            input.package_root,
            input.granted_permissions,
            self.1.unwrap_or(&UntrackedPublication),
        )?)
    }
    fn install_pinned(
        &self,
        input: PinnedPluginPackage,
    ) -> Result<InstalledWasmPlugin, EditorError> {
        if self.0.to_str().is_none() {
            return Err(EditorError::new(
                "INVALID_PLUGIN",
                "Plugin installation path is not valid UTF-8",
            ));
        }
        portable(PluginHost::new_managed(self.0)?.install_pinned_observed(
            input.package_root,
            input.granted_permissions,
            Some(&input.expected_package_sha256),
            self.1.unwrap_or(&UntrackedPublication),
        )?)
    }
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    #[test]
    fn unrepresentable_install_location_fails_before_creating_directories() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join(OsString::from_vec(vec![b'p', 0xff]));
        let error = application::install(
            &NativePlugins(&root, None),
            InstallInput {
                package_root: "unused".into(),
                granted_permissions: vec![],
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "INVALID_PLUGIN");
        assert!(!root.exists());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}

pub fn reconcile(
    root: &Path,
    input: plugin_publication::ReconcileInput,
    journal: &dyn PublicationJournal<plugin_publication::Object>,
) -> Result<plugin_publication::ReconcileResult, EditorError> {
    opencut_editor_application::maintenance::validate_object(&input.object)?;
    PluginHost::read_only(root)?.reconcile(input.object, journal)
}

use opencut_editor_application::project_plugins as scoped;
use std::sync::atomic::{AtomicUsize, Ordering};
static PROJECT_CALLS: AtomicUsize = AtomicUsize::new(0);
struct ProjectCall;
impl ProjectCall {
    fn acquire() -> Result<Self, EditorError> {
        PROJECT_CALLS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < scoped::MAX_CONCURRENT).then_some(n + 1)
            })
            .map_err(|_| {
                EditorError::new(
                    "CAPABILITY_UNAVAILABLE",
                    "Project plugin concurrency limit reached",
                )
            })?;
        Ok(Self)
    }
}
impl Drop for ProjectCall {
    fn drop(&mut self) {
        PROJECT_CALLS.fetch_sub(1, Ordering::AcqRel);
    }
}
impl scoped::ProjectPluginRuntime for NativePlugins<'_> {
    fn inspect_project_operation(
        &self,
        input: &scoped::InspectInput,
        grant: &scoped::Grant,
        authorize: &dyn Fn() -> Result<(), EditorError>,
    ) -> Result<scoped::OperationView, EditorError> {
        let _slot = ProjectCall::acquire()?;
        PluginHost::read_only(self.0)?.inspect_project_operation(
            input,
            &opencut_plugin_host::project_scope::ProjectPolicy { grant, authorize },
        )
    }
    fn propose_project_operations(
        &self,
        input: &PlanInput,
        snapshot: &opencut_contracts::project_public::PublicProjectSnapshot,
        grant: &scoped::Grant,
        authorize: &dyn Fn() -> Result<(), EditorError>,
    ) -> Result<Vec<opencut_contracts::project_public::ProjectEditOperation>, EditorError> {
        let _slot = ProjectCall::acquire()?;
        PluginHost::read_only(self.0)?.invoke_project_operations(
            input,
            snapshot,
            &opencut_plugin_host::project_scope::ProjectPolicy { grant, authorize },
        )
    }
}

impl opencut_editor_application::project_readonly::EffectAvailability for NativePlugins<'_> {
    fn unavailable_reason(
        &self,
        plugin: &opencut_contracts::project_readonly::EffectPluginRef,
    ) -> Result<opencut_contracts::project_readonly::UnavailableEffectReason, EditorError> {
        use opencut_contracts::project_readonly::UnavailableEffectReason as R;
        let host = match PluginHost::read_only(self.0) {
            Ok(host) => host,
            Err(_) => return Ok(R::InspectionUnavailable),
        };
        match host.inspect(&plugin.id, &plugin.version) {
            Ok(installed) if installed.package_sha256 == plugin.package_sha256 => {
                Ok(R::UnsupportedByHost)
            }
            Ok(_) => Ok(R::InvalidPackage),
            Err(error) if error.code == "NOT_FOUND" => Ok(R::NotInstalled),
            Err(error) if matches!(error.code.as_str(), "INVALID_PLUGIN" | "PERMISSION_DENIED") => {
                Ok(R::InvalidPackage)
            }
            Err(_) => Ok(R::InspectionUnavailable),
        }
    }
}
