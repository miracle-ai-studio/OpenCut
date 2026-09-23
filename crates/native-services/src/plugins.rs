use opencut_api_contracts::plugin_publication;
pub use opencut_api_contracts::plugins::{
    InstallInput, InstalledPlugin, PlanInput, PlanInvocation, PluginInput,
};
use opencut_contracts::{EditorError, Operation, ProjectSnapshot};
use opencut_editor_application::maintenance::{PublicationJournal, UntrackedPublication};
use opencut_editor_application::{plugins as application, projects::ProjectRead};
use opencut_plugin_host::PluginHost;
use std::path::Path;
pub struct NativePlugins<'a>(
    pub &'a Path,
    pub Option<&'a dyn PublicationJournal<plugin_publication::Object>>,
);
fn portable(value: opencut_plugin_host::InstalledPlugin) -> Result<InstalledPlugin, EditorError> {
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
    Ok(InstalledPlugin {
        manifest: value.manifest,
        package_sha256: value.package_sha256,
        granted_permissions: value.granted_permissions,
        install_path,
    })
}
impl application::PluginCatalog for NativePlugins<'_> {
    fn list(&self) -> Result<Vec<InstalledPlugin>, EditorError> {
        PluginHost::read_only(self.0)?
            .list()?
            .into_iter()
            .map(portable)
            .collect()
    }
    fn inspect(&self, input: &PluginInput) -> Result<InstalledPlugin, EditorError> {
        portable(PluginHost::read_only(self.0)?.inspect(&input.id, &input.version)?)
    }
    fn install(&self, input: InstallInput) -> Result<InstalledPlugin, EditorError> {
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
}
impl application::PluginRuntime for NativePlugins<'_> {
    fn propose(
        &self,
        input: &PlanInput,
        snapshot: &ProjectSnapshot,
    ) -> Result<Vec<Operation>, EditorError> {
        PluginHost::read_only(self.0)?.invoke_operations(
            &input.id,
            &input.version,
            &input.operation_name,
            snapshot,
            &input.parameters,
        )
    }
}
pub fn list(root: &Path) -> Result<Vec<InstalledPlugin>, EditorError> {
    application::list(&NativePlugins(root, None))
}
pub fn inspect(root: &Path, input: PluginInput) -> Result<InstalledPlugin, EditorError> {
    application::inspect(&NativePlugins(root, None), input)
}
pub fn install(root: &Path, input: InstallInput) -> Result<InstalledPlugin, EditorError> {
    application::install(&NativePlugins(root, None), input)
}
pub fn plan(
    projects: &impl ProjectRead,
    root: &Path,
    input: PlanInput,
) -> Result<PlanInvocation, EditorError> {
    application::plan(projects, &NativePlugins(root, None), input)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    #[test]
    fn unrepresentable_install_location_fails_before_creating_directories() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join(OsString::from_vec(vec![b'p', 0xff]));
        let error = install(
            &root,
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

pub fn install_observed(
    root: &Path,
    input: InstallInput,
    journal: &dyn PublicationJournal<plugin_publication::Object>,
) -> Result<InstalledPlugin, EditorError> {
    application::install(&NativePlugins(root, Some(journal)), input)
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
        snapshot: &ProjectSnapshot,
        grant: &scoped::Grant,
        authorize: &dyn Fn() -> Result<(), EditorError>,
    ) -> Result<Vec<Operation>, EditorError> {
        let _slot = ProjectCall::acquire()?;
        PluginHost::read_only(self.0)?.invoke_checked(
            input,
            snapshot,
            Some(&opencut_plugin_host::project_scope::ProjectPolicy { grant, authorize }),
        )
    }
}
