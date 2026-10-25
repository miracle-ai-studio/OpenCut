//! Plugin application use cases. A runtime can propose operations, never a trusted plan or a write.
pub use opencut_api_contracts::plugin_packages::PinnedPluginPackage;
pub use opencut_api_contracts::plugins::{InstallInput, PluginInput};
use opencut_contracts::EditorError;
pub trait PluginCatalog {
    fn list(&self) -> Result<Vec<InstalledWasmPlugin>, EditorError>;
    fn inspect(&self, input: &PluginInput) -> Result<InstalledWasmPlugin, EditorError>;
    fn install(&self, input: InstallInput) -> Result<InstalledWasmPlugin, EditorError>;
    fn install_pinned(
        &self,
        input: PinnedPluginPackage,
    ) -> Result<InstalledWasmPlugin, EditorError>;
}
pub fn list(catalog: &impl PluginCatalog) -> Result<Vec<InstalledWasmPlugin>, EditorError> {
    catalog.list()
}
pub fn inspect(
    catalog: &impl PluginCatalog,
    input: PluginInput,
) -> Result<InstalledWasmPlugin, EditorError> {
    catalog.inspect(&input)
}
pub fn install(
    catalog: &impl PluginCatalog,
    input: InstallInput,
) -> Result<InstalledWasmPlugin, EditorError> {
    catalog.install(input)
}
pub fn install_pinned(
    catalog: &impl PluginCatalog,
    input: PinnedPluginPackage,
) -> Result<InstalledWasmPlugin, EditorError> {
    catalog.install_pinned(input)
}
/// Internal installed resource. Public tools expose portable package metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstalledWasmPlugin<P = String> {
    pub manifest: opencut_api_contracts::plugins::PluginManifest,
    pub package_sha256: String,
    pub install_path: P,
    pub granted_permissions: std::collections::BTreeSet<String>,
}
