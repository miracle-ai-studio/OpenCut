pub mod project_scope;
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Component as PathComponent, Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

use fs2::FileExt;
use opencut_api_contracts::plugin_publication::{
    Action as PluginAction, Object as PluginObject, ReconcileResult,
};
use opencut_contracts::{EditorError, Operation, ProjectSnapshot};
use opencut_editor_application::maintenance::{
    self, PublicationIntent, PublicationJournal, UntrackedPublication,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({ path: "wit", world: "planner" });

pub const PLANNER_WORLD: &str = "opencut:planner/planner@1.0.0";
pub const KNOWN_PERMISSIONS: &[&str] = &["project.snapshot.read", "edit.plan.propose"];

pub use opencut_api_contracts::plugins::{
    Contributions, PlanInvocation, PluginLimits, PluginManifest, PluginOperation, RuntimeManifest,
};
pub type InstalledPlugin = opencut_api_contracts::plugins::InstalledPlugin<PathBuf>;

struct RuntimeState {
    limits: StoreLimits,
    wasi: WasiCtx,
    table: ResourceTable,
}

impl WasiView for RuntimeState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

pub struct PluginHost {
    root: PathBuf,
    engine: Engine,
    budget: Option<opencut_storage_budget::Budget>,
}

impl PluginHost {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, EditorError> {
        fs::create_dir_all(root.as_ref()).map_err(io_error)?;
        Self::read_only(root)
    }
    pub fn new_managed(root: impl AsRef<Path>) -> Result<Self, EditorError> {
        let mut host = Self::new(root)?;
        #[cfg(unix)]
        {
            host.root = host.root.canonicalize().map_err(io_error)?;
            host.budget = host.root.parent().map(opencut_storage_budget::Budget::new);
        }
        Ok(host)
    }

    /// Open capabilities without creating the plugin root. Safe for list, inspect and plan.
    pub fn read_only(root: impl AsRef<Path>) -> Result<Self, EditorError> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config).map_err(runtime_error)?;
        Ok(Self {
            root: root.as_ref().to_path_buf(),
            engine,
            budget: None,
        })
    }

    pub fn install(
        &self,
        package_root: impl AsRef<Path>,
        granted_permissions: impl IntoIterator<Item = String>,
    ) -> Result<InstalledPlugin, EditorError> {
        self.install_observed(package_root, granted_permissions, &UntrackedPublication)
    }

    pub fn install_observed(
        &self,
        package_root: impl AsRef<Path>,
        granted_permissions: impl IntoIterator<Item = String>,
        journal: &dyn PublicationJournal<PluginObject>,
    ) -> Result<InstalledPlugin, EditorError> {
        let package_root = package_root.as_ref().canonicalize().map_err(io_error)?;
        if !package_root.is_dir() {
            return Err(invalid("plugin package must be a directory"));
        }
        reject_symlinks(&package_root, &package_root)?;
        let manifest_bytes = fs::read(package_root.join("manifest.json")).map_err(io_error)?;
        let manifest: PluginManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|e| invalid(format!("invalid manifest schema: {e}")))?;
        validate_manifest(&manifest, &package_root)?;
        if package_root.join(".opencut-grants.json").exists()
            || package_root.join(".opencut-grants.json.tmp").exists()
        {
            return Err(invalid("plugin package uses a reserved host metadata path"));
        }
        let granted_permissions: BTreeSet<String> = granted_permissions.into_iter().collect();
        for permission in &granted_permissions {
            if !manifest.permissions.contains(permission) {
                return Err(EditorError::new(
                    "PERMISSION_DENIED",
                    format!("permission {permission} was not requested"),
                ));
            }
        }
        let package_sha256 = hash_package(&package_root)?;
        let package_object = PluginObject::PluginPackage {
            plugin_id: manifest.id.clone(),
            version: manifest.version.clone(),
            package_sha256: package_sha256.clone(),
        };
        maintenance::validate_object(&package_object)?;
        let _lock = self.install_lock(&manifest.id, &manifest.version)?;
        let version_root = self.root.join(&manifest.id).join(&manifest.version);
        if version_root.exists()
            && read_dirs(&version_root)?
                .iter()
                .any(|p| p.file_name().and_then(|n| n.to_str()) != Some(package_sha256.as_str()))
        {
            return Err(EditorError::new(
                "PLUGIN_VERSION_CONFLICT",
                "the plugin version is already installed with different package content",
            ));
        }
        let install_path = version_root.join(&package_sha256);
        let before = package_digest(&install_path)?;
        journal.reconcile(&package_object, before.as_deref())?;
        if before.as_deref().is_some_and(|h| h != package_sha256) {
            return Err(EditorError::new(
                "PLUGIN_INTEGRITY",
                "Installed package bytes differ from their content identity",
            ));
        }
        if before.is_none() {
            let staging = Staging(version_root.join(".install-staging"));
            if staging.0.exists() {
                fs::remove_dir_all(&staging.0).map_err(io_error)?;
            }
            copy_package(&package_root, &staging.0, self.budget.as_ref())?;
            reject_symlinks(&staging.0, &staging.0)?;
            if fs::read(staging.0.join("manifest.json")).map_err(io_error)? != manifest_bytes
                || hash_package(&staging.0)? != package_sha256
            {
                return Err(EditorError::new(
                    "PLUGIN_INTEGRITY",
                    "Plugin source changed during snapshot preparation",
                ));
            }
            validate_manifest(&manifest, &staging.0.canonicalize().map_err(io_error)?)?;
            maintenance::publish(
                journal,
                PublicationIntent {
                    action: PluginAction::PluginPackagePublish,
                    object: package_object,
                    before_hash: None,
                    after_hash: package_sha256.clone(),
                },
                || {
                    fs::rename(&staging.0, &install_path).map_err(io_error)?;
                    sync_dir(&version_root)?;
                    sync_dir(&self.root.join(&manifest.id))?;
                    sync_dir(&self.root)?;
                    Ok(())
                },
            )?;
        }
        let grant_file = install_path.join(".opencut-grants.json");
        let before = grant_digest(&grant_file)?;
        let grants = serde_json::to_vec(&granted_permissions).map_err(json_error)?;
        let after = format!("{:x}", Sha256::digest(&grants));
        let object = PluginObject::PluginGrants {
            plugin_id: manifest.id.clone(),
            version: manifest.version.clone(),
            package_sha256: package_sha256.clone(),
        };
        journal.reconcile(&object, before.as_deref())?;
        if before.as_deref() != Some(after.as_str()) {
            let _reservation = self
                .budget
                .as_ref()
                .map(|budget| budget.reserve("plugin_grants", grants.len() as u64))
                .transpose()?;
            maintenance::publish(
                journal,
                PublicationIntent {
                    action: PluginAction::PluginGrantsUpdate,
                    object,
                    before_hash: before,
                    after_hash: after,
                },
                || {
                    let temporary = install_path.join(".opencut-grants.json.tmp");
                    match fs::remove_file(&temporary) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => return Err(io_error(e)),
                    }
                    let mut file = fs::OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(&temporary)
                        .map_err(io_error)?;
                    file.write_all(&grants).map_err(io_error)?;
                    file.sync_all().map_err(io_error)?;
                    drop(file);
                    fs::rename(&temporary, &grant_file).map_err(io_error)?;
                    sync_dir(&install_path)?;
                    Ok(())
                },
            )?;
        }
        load_installed(&install_path)
    }
    fn install_lock(&self, id: &str, version: &str) -> Result<fs::File, EditorError> {
        let root = self.root.join(".install-locks");
        fs::create_dir_all(&root).map_err(io_error)?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join(format!("{id}@{version}.lock")))
            .map_err(io_error)?;
        file.lock_exclusive().map_err(io_error)?;
        Ok(file)
    }
    fn read_lock(&self, id: &str, version: &str) -> Result<Option<fs::File>, EditorError> {
        match fs::File::open(
            self.root
                .join(".install-locks")
                .join(format!("{id}@{version}.lock")),
        ) {
            Ok(file) => {
                FileExt::lock_shared(&file).map_err(io_error)?;
                Ok(Some(file))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_error(e)),
        }
    }
    pub fn reconcile(
        &self,
        object: PluginObject,
        journal: &dyn PublicationJournal<PluginObject>,
    ) -> Result<ReconcileResult, EditorError> {
        maintenance::validate_object(&object)?;
        let (id, version, hash) = match &object {
            PluginObject::PluginPackage {
                plugin_id,
                version,
                package_sha256,
            }
            | PluginObject::PluginGrants {
                plugin_id,
                version,
                package_sha256,
            } => (plugin_id, version, package_sha256),
        };
        let _lock = self.install_lock(id, version)?;
        let path = self.root.join(id).join(version).join(hash);
        let observed_hash = match &object {
            PluginObject::PluginPackage { .. } => package_digest(&path)?,
            PluginObject::PluginGrants { .. } => grant_digest(&path.join(".opencut-grants.json"))?,
        };
        let resolved_publication_ids = journal.reconcile(&object, observed_hash.as_deref())?;
        Ok(ReconcileResult {
            object,
            observed_hash,
            resolved_publication_ids,
        })
    }

    pub fn list(&self) -> Result<Vec<InstalledPlugin>, EditorError> {
        let mut plugins = Vec::new();
        if !self.root.exists() {
            return Ok(plugins);
        }
        for id in read_dirs(&self.root)? {
            for version in read_dirs(&id)? {
                let _lock = self.read_lock(
                    id.file_name()
                        .and_then(|s| s.to_str())
                        .ok_or_else(|| invalid("Invalid installed id"))?,
                    version
                        .file_name()
                        .and_then(|s| s.to_str())
                        .ok_or_else(|| invalid("Invalid installed version"))?,
                )?;
                for package in read_dirs(&version)? {
                    if package.join("manifest.json").is_file()
                        && package.join(".opencut-grants.json").is_file()
                    {
                        plugins.push(load_installed(&package)?);
                    }
                }
            }
        }
        plugins.sort_by(|a, b| {
            (&a.manifest.id, &a.manifest.version).cmp(&(&b.manifest.id, &b.manifest.version))
        });
        Ok(plugins)
    }

    pub fn inspect(&self, id: &str, version: &str) -> Result<InstalledPlugin, EditorError> {
        maintenance::validate_object(&PluginObject::PluginPackage {
            plugin_id: id.into(),
            version: version.into(),
            package_sha256: "0".repeat(64),
        })?;
        let _lock = self.read_lock(id, version)?;
        let root = self.root.join(id).join(version);
        if root.is_dir() {
            let packages = read_dirs(&root)?;
            if packages.len() > 1 {
                return Err(EditorError::new(
                    "PLUGIN_VERSION_CONFLICT",
                    "Multiple package identities exist for this plugin version",
                ));
            }
            if let Some(path) = packages.into_iter().next() {
                if !path.join("manifest.json").is_file()
                    || !path.join(".opencut-grants.json").is_file()
                {
                    return Err(EditorError::new(
                        "PLUGIN_INSTALL_INCOMPLETE",
                        "Plugin package publication is incomplete; inspect publication history and retry the original install",
                    ));
                }
                let plugin = load_installed(&path)?;
                if plugin.manifest.id == id && plugin.manifest.version == version {
                    return Ok(plugin);
                }
                return Err(EditorError::new(
                    "PLUGIN_INTEGRITY",
                    "Installed manifest does not match its location",
                ));
            }
        }
        Err(EditorError::new(
            "NOT_FOUND",
            format!("plugin {id}@{version} is not installed"),
        ))
    }

    pub fn invoke_plan(
        &self,
        id: &str,
        version: &str,
        operation_name: &str,
        snapshot: &ProjectSnapshot,
        parameters: &Value,
    ) -> Result<PlanInvocation, EditorError> {
        let operations =
            self.invoke_operations(id, version, operation_name, snapshot, parameters)?;
        opencut_editor_application::plugins::plan_operations(
            id,
            version,
            operation_name,
            snapshot,
            operations,
        )
    }

    pub fn invoke_operations(
        &self,
        id: &str,
        version: &str,
        operation_name: &str,
        snapshot: &ProjectSnapshot,
        parameters: &Value,
    ) -> Result<Vec<Operation>, EditorError> {
        self.invoke_checked(
            &opencut_api_contracts::plugins::PlanInput {
                id: id.into(),
                version: version.into(),
                operation_name: operation_name.into(),
                project_id: snapshot.project.id.clone(),
                revision: snapshot.revision.clone(),
                parameters: parameters.clone(),
            },
            snapshot,
            None,
        )
    }

    pub fn invoke_checked(
        &self,
        input: &opencut_api_contracts::plugins::PlanInput,
        snapshot: &ProjectSnapshot,
        policy: Option<&project_scope::ProjectPolicy<'_>>,
    ) -> Result<Vec<Operation>, EditorError> {
        let id = input.id.as_str();
        let version = input.version.as_str();
        let operation_name = input.operation_name.as_str();
        let parameters = &input.parameters;
        if let Some(policy) = policy {
            use opencut_editor_application::project_plugins as app;
            if policy.grant.target.project_id != input.project_id
                || snapshot.project.id != input.project_id
                || snapshot.revision != input.revision
                || !policy.grant.enabled
                || !policy.grant.operations.contains(operation_name)
            {
                return Err(opencut_editor_application::access::denied());
            }
            if serde_json::to_vec(snapshot).map_err(json_error)?.len() > app::MAX_SNAPSHOT
                || serde_json::to_vec(parameters).map_err(json_error)?.len() > app::MAX_PARAMETERS
            {
                return Err(invalid("Project plugin input exceeds host byte ceiling"));
            }
        }
        maintenance::validate_object(&PluginObject::PluginPackage {
            plugin_id: id.into(),
            version: version.into(),
            package_sha256: "0".repeat(64),
        })?;
        // Retain the grant snapshot throughout execution; concurrent installs/revocations
        // take the exclusive version lock and become effective for the next invocation.
        let _execution_lock = self.read_lock(id, version)?;
        if let Some(policy) = policy {
            (policy.authorize)()?;
        }
        let mut plugin = self.inspect(id, version)?;
        if let Some(policy) = policy {
            project_scope::limits(&mut plugin, policy)?;
        }
        // Packages installed by an older Host must satisfy today's callable
        // ABI as well; listing historical metadata is not an execution grant.
        validate_manifest(
            &plugin.manifest,
            &plugin.install_path.canonicalize().map_err(io_error)?,
        )?;
        require_grant(&plugin, "project.snapshot.read")?;
        require_grant(&plugin, "edit.plan.propose")?;
        let operation = plugin
            .manifest
            .contributes
            .operations
            .iter()
            .find(|op| op.name == operation_name)
            .ok_or_else(|| {
                EditorError::new(
                    "NOT_FOUND",
                    format!("plugin operation {operation_name} was not found"),
                )
            })?;
        let component = if let Some(policy) = policy {
            let input_schema =
                project_scope::schema(&plugin.install_path.join(&operation.input_schema))?;
            project_scope::schema(&plugin.install_path.join(&operation.output_schema))?;
            jsonschema::validator_for(&input_schema)
                .map_err(|_| invalid("Invalid schema"))?
                .validate(parameters)
                .map_err(|_| {
                    EditorError::new("INVALID_ARGUMENT", "Plugin parameters do not match schema")
                })?;
            let bytes = project_scope::bounded_read(
                &plugin.install_path.join(&plugin.manifest.runtime.entry),
                opencut_editor_application::project_plugins::MAX_COMPONENT,
            )?;
            (policy.authorize)()?;
            Component::new(&self.engine, bytes).map_err(runtime_error)?
        } else {
            validate_json_schema(
                &plugin.install_path.join(&operation.input_schema),
                parameters,
                "INVALID_ARGUMENT",
            )?;
            Component::from_file(
                &self.engine,
                plugin.install_path.join(&plugin.manifest.runtime.entry),
            )
            .map_err(runtime_error)?
        };
        if let Some(policy) = policy {
            (policy.authorize)()?;
        }
        let mut linker = Linker::new(&self.engine);
        // An empty WASI context satisfies language runtime imports without
        // granting filesystem, network, environment, arguments, or stdio.
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(runtime_error)?;
        let memory_bytes = plugin
            .manifest
            .limits
            .memory_mib
            .saturating_mul(1024 * 1024) as usize;
        let limits = StoreLimitsBuilder::new().memory_size(memory_bytes).build();
        let mut store = Store::new(
            &self.engine,
            RuntimeState {
                limits,
                wasi: WasiCtxBuilder::new().build(),
                table: ResourceTable::new(),
            },
        );
        store.limiter(|state| &mut state.limits);
        store
            .set_fuel(plugin.manifest.limits.fuel)
            .map_err(runtime_error)?;
        store.set_epoch_deadline(1);
        let (stop_tx, stop_rx) = mpsc::channel();
        let engine = self.engine.clone();
        let wall_time = Duration::from_millis(plugin.manifest.limits.wall_time_ms);
        let interrupter = thread::spawn(move || {
            if stop_rx.recv_timeout(wall_time).is_err() {
                engine.increment_epoch();
            }
        });
        let call = (|| {
            let bindings = Planner::instantiate(&mut store, &component, &linker)?;
            let snapshot_json = serde_json::to_string(snapshot)?;
            let parameters_json = serde_json::to_string(parameters)?;
            bindings.call_plan(&mut store, &snapshot_json, &parameters_json)
        })();
        let _ = stop_tx.send(());
        let _ = interrupter.join();
        let output = call.map_err(runtime_error)?;
        if output.len() > plugin.manifest.limits.output_bytes {
            return Err(EditorError::new(
                "PLUGIN_OUTPUT_LIMIT",
                "plugin output exceeded its byte limit",
            ));
        }
        let output_value: Value = serde_json::from_str(&output)
            .map_err(|e| invalid(format!("plugin returned invalid JSON: {e}")))?;
        validate_json_schema(
            &plugin.install_path.join(&operation.output_schema),
            &output_value,
            "INVALID_PLUGIN",
        )?;
        let operations: Vec<Operation> = serde_json::from_value(output_value)
            .map_err(|e| invalid(format!("plugin returned invalid operations: {e}")))?;
        Ok(operations)
    }
}

fn validate_manifest(manifest: &PluginManifest, root: &Path) -> Result<(), EditorError> {
    if manifest.manifest_version != 1 {
        return Err(invalid("manifest_version must be 1"));
    }
    if !valid_plugin_id(&manifest.id) {
        return Err(invalid("plugin id must be a reverse-domain identifier"));
    }
    semver::Version::parse(&manifest.version)
        .map_err(|e| invalid(format!("invalid plugin version: {e}")))?;
    let requirement = semver::VersionReq::parse(&manifest.plugin_api)
        .map_err(|e| invalid(format!("invalid plugin_api: {e}")))?;
    if !requirement.matches(&semver::Version::new(1, 0, 0)) {
        return Err(invalid("plugin_api does not support host API 1.0.0"));
    }
    if manifest.runtime.kind != "wasm-component" || manifest.runtime.world != PLANNER_WORLD {
        return Err(invalid(
            "runtime must target the OpenCut planner component world",
        ));
    }
    if !manifest.targets.iter().any(|target| target == "native") {
        return Err(invalid("plugin does not target native hosts"));
    }
    for permission in &manifest.permissions {
        if permission == "analysis.read" {
            return Err(EditorError::new(
                "CAPABILITY_UNAVAILABLE",
                "analysis.read is reserved but this planner Host has no analysis ABI",
            ));
        }
        if !KNOWN_PERMISSIONS.contains(&permission.as_str()) {
            return Err(invalid(format!("unknown permission {permission}")));
        }
    }
    if manifest.limits.memory_mib == 0
        || manifest.limits.wall_time_ms == 0
        || manifest.limits.output_bytes == 0
        || manifest.limits.fuel == 0
    {
        return Err(invalid("plugin limits must be positive"));
    }
    validate_relative_file(root, &manifest.runtime.entry)?;
    let mut operation_names = BTreeSet::new();
    for operation in &manifest.contributes.operations {
        let namespace = format!("{}.", manifest.id);
        if !operation
            .name
            .strip_prefix(&namespace)
            .is_some_and(|suffix| {
                !suffix.is_empty()
                    && suffix.split('.').all(|part| {
                        !part.is_empty()
                            && part
                                .bytes()
                                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                    })
            })
        {
            return Err(invalid(
                "plugin operations must use their package namespace",
            ));
        }
        if !operation_names.insert(&operation.name) {
            return Err(invalid(format!(
                "duplicate plugin operation {}",
                operation.name
            )));
        }
        if operation.kind != "job" || operation.version == 0 {
            return Err(invalid("plugin operation kind/version is invalid"));
        }
        validate_relative_file(root, &operation.input_schema)?;
        validate_relative_file(root, &operation.output_schema)?;
    }
    Ok(())
}

fn validate_relative_file(root: &Path, relative: &str) -> Result<PathBuf, EditorError> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, PathComponent::Normal(_)))
    {
        return Err(invalid(format!("unsafe package path {relative}")));
    }
    let resolved = root.join(path).canonicalize().map_err(io_error)?;
    if !resolved.starts_with(root) || !resolved.is_file() {
        return Err(invalid(format!("package path {relative} is invalid")));
    }
    Ok(resolved)
}

fn valid_plugin_id(id: &str) -> bool {
    id.contains('.')
        && id.split('.').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

fn require_grant(plugin: &InstalledPlugin, permission: &str) -> Result<(), EditorError> {
    if plugin.granted_permissions.contains(permission) {
        Ok(())
    } else {
        Err(EditorError::new(
            "PERMISSION_DENIED",
            format!("plugin lacks grant {permission}"),
        ))
    }
}

fn validate_json_schema(
    path: &Path,
    instance: &Value,
    instance_error_code: &str,
) -> Result<(), EditorError> {
    let schema: Value =
        serde_json::from_slice(&fs::read(path).map_err(io_error)?).map_err(json_error)?;
    let validator = jsonschema::validator_for(&schema)
        .map_err(|e| invalid(format!("invalid JSON schema: {e}")))?;
    if let Err(error) = validator.validate(instance) {
        return Err(EditorError::new(
            instance_error_code,
            format!("JSON schema validation failed: {error}"),
        ));
    }
    Ok(())
}

fn reject_symlinks(root: &Path, path: &Path) -> Result<(), EditorError> {
    for entry in fs::read_dir(path).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let ty = entry.file_type().map_err(io_error)?;
        if ty.is_symlink() || (!ty.is_dir() && !ty.is_file()) {
            return Err(invalid(format!(
                "Only regular files and directories are allowed in plugin packages: {}",
                entry.path().display()
            )));
        }
        if ty.is_dir() {
            reject_symlinks(root, &entry.path())?;
        }
        if !entry.path().starts_with(root) {
            return Err(invalid("package entry escaped root"));
        }
    }
    Ok(())
}

fn hash_package(root: &Path) -> Result<String, EditorError> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();
    let mut hash = Sha256::new();
    for relative in files {
        hash.update(relative.to_string_lossy().as_bytes());
        hash.update([0]);
        hash.update(fs::read(root.join(relative)).map_err(io_error)?);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn collect_files(root: &Path, path: &Path, files: &mut Vec<PathBuf>) -> Result<(), EditorError> {
    for entry in fs::read_dir(path).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        if path == root
            && matches!(
                entry.file_name().to_str(),
                Some(".opencut-grants.json" | ".opencut-grants.json.tmp")
            )
        {
            continue;
        }
        if entry
            .path()
            .strip_prefix(root)
            .ok()
            .and_then(|p| p.to_str())
            .is_none()
        {
            return Err(invalid("Package paths must be UTF-8"));
        }
        if entry.file_type().map_err(io_error)?.is_dir() {
            collect_files(root, &entry.path(), files)?;
        } else {
            files.push(
                entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|_| invalid("package path escaped root"))?
                    .to_path_buf(),
            );
        }
    }
    Ok(())
}

fn copy_package(
    source: &Path,
    destination: &Path,
    budget: Option<&opencut_storage_budget::Budget>,
) -> Result<(), EditorError> {
    fs::create_dir_all(destination).map_err(io_error)?;
    for entry in fs::read_dir(source).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let target = destination.join(entry.file_name());
        if entry.file_type().map_err(io_error)?.is_dir() {
            copy_package(&entry.path(), &target, budget)?;
        } else {
            if !entry.file_type().map_err(io_error)?.is_file() {
                return Err(invalid(
                    "Package entries must be regular files or directories",
                ));
            }
            let mut input = fs::File::open(entry.path()).map_err(io_error)?;
            let mut reservation = budget
                .map(|budget| {
                    budget.reserve("plugin_package", input.metadata().map_err(io_error)?.len())
                })
                .transpose()?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(io_error)?;
            let mut copied = 0_u64;
            let mut buffer = [0_u8; 65536];
            loop {
                let count = input.read(&mut buffer).map_err(io_error)?;
                if count == 0 {
                    break;
                }
                copied = copied
                    .checked_add(count as u64)
                    .ok_or_else(|| invalid("Package is too large"))?;
                if let Some(reservation) = &mut reservation {
                    reservation.ensure(copied)?;
                }
                output.write_all(&buffer[..count]).map_err(io_error)?;
            }
            output.sync_all().map_err(io_error)?;
        }
    }
    sync_dir(destination)
}

struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn sync_dir(path: &Path) -> Result<(), EditorError> {
    #[cfg(unix)]
    {
        fs::File::open(path)
            .and_then(|f| f.sync_all())
            .map_err(io_error)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
fn package_digest(path: &Path) -> Result<Option<String>, EditorError> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error(e)),
        Ok(m) if !m.is_dir() => return Err(invalid("Installed package must be a directory")),
        _ => {}
    }
    reject_symlinks(path, path)?;
    hash_package(path).map(Some)
}
fn grant_digest(path: &Path) -> Result<Option<String>, EditorError> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error(e)),
        Ok(m) if !m.is_file() => return Err(invalid("Plugin grants must be a regular file")),
        _ => {}
    }
    Ok(Some(format!(
        "{:x}",
        Sha256::digest(fs::read(path).map_err(io_error)?)
    )))
}

fn read_dirs(path: &Path) -> Result<Vec<PathBuf>, EditorError> {
    fs::read_dir(path)
        .map_err(io_error)?
        .filter_map(|entry| match entry {
            Ok(entry) => entry
                .file_type()
                .ok()
                .filter(|ty| ty.is_dir())
                .and_then(|_| {
                    (!entry.file_name().to_string_lossy().starts_with('.'))
                        .then(|| Ok(entry.path()))
                }),
            Err(e) => Some(Err(io_error(e))),
        })
        .collect()
}

fn load_installed(path: &Path) -> Result<InstalledPlugin, EditorError> {
    reject_symlinks(path, path)?;
    let manifest = serde_json::from_slice(&fs::read(path.join("manifest.json")).map_err(io_error)?)
        .map_err(json_error)?;
    let grants =
        serde_json::from_slice(&fs::read(path.join(".opencut-grants.json")).map_err(io_error)?)
            .map_err(json_error)?;
    let package_sha256: String = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| invalid("invalid install path"))?
        .into();
    let actual_sha256 = hash_package(path)?;
    if actual_sha256 != package_sha256 {
        return Err(EditorError::new(
            "PLUGIN_INTEGRITY",
            "installed plugin content does not match its package digest",
        ));
    }
    Ok(InstalledPlugin {
        manifest,
        package_sha256,
        install_path: path.to_path_buf(),
        granted_permissions: grants,
    })
}

fn invalid(message: impl Into<String>) -> EditorError {
    EditorError::new("INVALID_PLUGIN", message)
}
fn io_error(error: std::io::Error) -> EditorError {
    EditorError::new("PLUGIN_IO", error.to_string())
}
fn json_error(error: serde_json::Error) -> EditorError {
    invalid(error.to_string())
}
fn runtime_error(error: impl std::fmt::Display) -> EditorError {
    EditorError::new("PLUGIN_RUNTIME", error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{CreateProject, FrameRate};
    use std::{process::Command, sync::OnceLock};
    use tempfile::TempDir;

    #[test]
    fn similarly_named_and_nested_grant_files_are_part_of_the_package_identity() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();
        let first = hash_package(dir.path()).unwrap();
        for name in [".opencut-grants.json.extra", "nested/.opencut-grants.json"] {
            fs::write(dir.path().join(name), b"executable package content").unwrap();
            assert_ne!(first, hash_package(dir.path()).unwrap());
            fs::remove_file(dir.path().join(name)).unwrap();
        }
        fs::write(dir.path().join(".opencut-grants.json"), b"host metadata").unwrap();
        assert_eq!(first, hash_package(dir.path()).unwrap());
    }

    #[test]
    fn publication_uses_verified_private_snapshot_even_if_source_changes_after_prepare() {
        struct ChangeSource(PathBuf);
        impl PublicationJournal<PluginObject> for ChangeSource {
            fn reconcile(
                &self,
                _: &PluginObject,
                _: Option<&str>,
            ) -> Result<Vec<String>, EditorError> {
                Ok(vec![])
            }
            fn prepare(
                &self,
                intent: &opencut_api_contracts::plugin_publication::Intent,
            ) -> Result<String, EditorError> {
                if matches!(intent.object, PluginObject::PluginPackage { .. }) {
                    fs::write(
                        self.0.join("input.json"),
                        b"changed after verified snapshot",
                    )
                    .unwrap();
                }
                Ok("test-publication".into())
            }
            fn complete(&self, _: &str) -> Result<(), EditorError> {
                Ok(())
            }
        }
        let package = package(|_| {});
        let expected = hash_package(package.path()).unwrap();
        let root = tempfile::tempdir().unwrap();
        let installed = PluginHost::new(root.path())
            .unwrap()
            .install_observed(
                package.path(),
                Vec::<String>::new(),
                &ChangeSource(package.path().into()),
            )
            .unwrap();
        assert_eq!(installed.package_sha256, expected);
        assert_eq!(hash_package(&installed.install_path).unwrap(), expected);
        assert_ne!(hash_package(package.path()).unwrap(), expected);
    }

    static GUEST: OnceLock<PathBuf> = OnceLock::new();

    fn guest_component() -> PathBuf {
        GUEST
            .get_or_init(|| {
                let manifest =
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/planner-guest/Cargo.toml");
                let status = Command::new("cargo")
                    .args(["build", "--manifest-path"])
                    .arg(&manifest)
                    .args(["--target", "wasm32-wasip2", "--release"])
                    .status()
                    .unwrap();
                assert!(status.success(), "example component must compile");
                manifest
                    .parent()
                    .unwrap()
                    .join("target/wasm32-wasip2/release/opencut_example_planner_guest.wasm")
            })
            .clone()
    }

    fn package(manifest_patch: impl FnOnce(&mut Value)) -> TempDir {
        let directory = tempfile::tempdir().unwrap();
        fs::copy(guest_component(), directory.path().join("plugin.wasm")).unwrap();
        fs::create_dir(directory.path().join("schemas")).unwrap();
        fs::write(
            directory.path().join("schemas/input.json"),
            r#"{"type":"object"}"#,
        )
        .unwrap();
        fs::write(
            directory.path().join("schemas/output.json"),
            r#"{"type":"array"}"#,
        )
        .unwrap();
        let mut manifest = serde_json::json!({
            "manifest_version": 1,
            "id": "org.example.planner",
            "version": "1.0.0",
            "display_name": "Example Planner",
            "plugin_api": ">=1.0.0, <2.0.0",
            "runtime": {"kind":"wasm-component", "entry":"plugin.wasm", "world":PLANNER_WORLD},
            "targets": ["native"],
            "permissions": ["project.snapshot.read", "edit.plan.propose"],
            "contributes": {"operations":[{
                "name":"org.example.planner.plan", "version":1, "kind":"job",
                "input_schema":"schemas/input.json", "output_schema":"schemas/output.json"
            }]},
            "limits": {"memory_mib":16, "wall_time_ms":1000, "output_bytes":4096, "fuel":1000000}
        });
        manifest_patch(&mut manifest);
        fs::write(
            directory.path().join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        directory
    }

    fn snapshot() -> ProjectSnapshot {
        let project = opencut_editor_core::create_project(&CreateProject {
            id: "project-1".into(),
            name: "Project".into(),
            sequence_id: "sequence-1".into(),
            width: 1920,
            height: 1080,
            fps: FrameRate {
                numerator: 30,
                denominator: 1,
            },
        })
        .unwrap();
        ProjectSnapshot {
            project,
            revision: "rev-1".into(),
        }
    }

    #[test]
    fn installs_inspects_and_executes_a_real_component() {
        let package = package(|_| {});
        let install_root = tempfile::tempdir().unwrap();
        let host = PluginHost::new(install_root.path()).unwrap();
        let installed = host
            .install(
                package.path(),
                ["project.snapshot.read".into(), "edit.plan.propose".into()],
            )
            .unwrap();
        let original = snapshot();

        let invocation = host
            .invoke_plan(
                &installed.manifest.id,
                &installed.manifest.version,
                "org.example.planner.plan",
                &original,
                &serde_json::json!({"track_id":"from-plugin"}),
            )
            .unwrap();

        assert_eq!(invocation.operations.len(), 1);
        assert_eq!(invocation.plan.base_revision, "rev-1");
        assert_eq!(
            invocation.plan.proposed_project.sequences[0].tracks[0].id,
            "from-plugin"
        );
        assert!(original.project.sequences[0].tracks.is_empty());
        assert_eq!(host.list().unwrap().len(), 1);
        assert_eq!(
            host.inspect("org.example.planner", "1.0.0")
                .unwrap()
                .package_sha256,
            installed.package_sha256
        );
    }

    #[test]
    fn fuel_exhaustion_traps_a_looping_component() {
        let package = package(|manifest| manifest["limits"]["fuel"] = 1000.into());
        let install_root = tempfile::tempdir().unwrap();
        let host = PluginHost::new(install_root.path()).unwrap();
        host.install(
            package.path(),
            ["project.snapshot.read".into(), "edit.plan.propose".into()],
        )
        .unwrap();

        let error = host
            .invoke_plan(
                "org.example.planner",
                "1.0.0",
                "org.example.planner.plan",
                &snapshot(),
                &serde_json::json!({"loop":true}),
            )
            .err()
            .unwrap();

        assert_eq!(error.code, "PLUGIN_RUNTIME");
    }

    #[test]
    fn rejects_unsafe_package_paths() {
        let package = package(|manifest| manifest["runtime"]["entry"] = "../escape.wasm".into());
        let host_root = tempfile::tempdir().unwrap();
        let host = PluginHost::new(host_root.path()).unwrap();
        assert_eq!(
            host.install(package.path(), Vec::new()).unwrap_err().code,
            "INVALID_PLUGIN"
        );
    }

    #[test]
    fn rejects_unknown_permissions() {
        let package =
            package(|manifest| manifest["permissions"] = serde_json::json!(["shell.execute"]));
        let host_root = tempfile::tempdir().unwrap();
        let host = PluginHost::new(host_root.path()).unwrap();
        assert_eq!(
            host.install(package.path(), Vec::new()).unwrap_err().code,
            "INVALID_PLUGIN"
        );
    }

    #[test]
    fn rejects_manifest_schema_errors() {
        let package = package(|manifest| manifest["unexpected"] = true.into());
        let host_root = tempfile::tempdir().unwrap();
        let host = PluginHost::new(host_root.path()).unwrap();
        assert_eq!(
            host.install(package.path(), Vec::new()).unwrap_err().code,
            "INVALID_PLUGIN"
        );
    }

    #[test]
    fn requires_runtime_capability_grants() {
        let package = package(|_| {});
        let host_root = tempfile::tempdir().unwrap();
        let host = PluginHost::new(host_root.path()).unwrap();
        host.install(package.path(), ["project.snapshot.read".into()])
            .unwrap();
        assert_eq!(
            host.invoke_plan(
                "org.example.planner",
                "1.0.0",
                "org.example.planner.plan",
                &snapshot(),
                &serde_json::json!({})
            )
            .err()
            .unwrap()
            .code,
            "PERMISSION_DENIED"
        );
    }

    #[test]
    fn rejects_different_content_for_an_installed_version() {
        let first = package(|_| {});
        let second = package(|_| {});
        fs::write(
            second.path().join("schemas/input.json"),
            r#"{"type":"object","maxProperties":3}"#,
        )
        .unwrap();
        let host_root = tempfile::tempdir().unwrap();
        let host = PluginHost::new(host_root.path()).unwrap();
        host.install(first.path(), Vec::new()).unwrap();

        let error = host.install(second.path(), Vec::new()).unwrap_err();

        assert_eq!(error.code, "PLUGIN_VERSION_CONFLICT");
        assert_eq!(host.list().unwrap().len(), 1);
    }

    #[test]
    fn concurrent_install_publishes_only_one_hash_per_version() {
        let first = package(|_| {});
        let second = package(|_| {});
        fs::write(
            second.path().join("schemas/input.json"),
            r#"{"type":"object","maxProperties":4}"#,
        )
        .unwrap();
        let host_root = tempfile::tempdir().unwrap();
        let root_a = host_root.path().to_path_buf();
        let root_b = root_a.clone();
        let first_path = first.path().to_path_buf();
        let second_path = second.path().to_path_buf();
        let a = thread::spawn(move || {
            PluginHost::new(root_a)
                .unwrap()
                .install(first_path, Vec::new())
        });
        let b = thread::spawn(move || {
            PluginHost::new(root_b)
                .unwrap()
                .install(second_path, Vec::new())
        });
        let results = [a.join().unwrap(), b.join().unwrap()];

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(
                    |result| matches!(result, Err(error) if error.code == "PLUGIN_VERSION_CONFLICT")
                )
                .count(),
            1
        );
        assert_eq!(
            PluginHost::new(host_root.path())
                .unwrap()
                .list()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn inspect_detects_installed_package_tampering() {
        let package = package(|_| {});
        let host_root = tempfile::tempdir().unwrap();
        let host = PluginHost::new(host_root.path()).unwrap();
        let installed = host.install(package.path(), Vec::new()).unwrap();
        fs::write(installed.install_path.join("schemas/input.json"), "{}").unwrap();

        let error = host.inspect("org.example.planner", "1.0.0").unwrap_err();

        assert_eq!(error.code, "PLUGIN_INTEGRITY");
    }
}
