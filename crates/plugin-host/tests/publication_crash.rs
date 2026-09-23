#![cfg(unix)]
//! OS process death, actual filesystem publication and actual database recovery.
use opencut_api_contracts::{maintenance::PublicationPhase, plugin_publication::*};
use opencut_contracts::EditorError;
use opencut_editor_application::maintenance::PublicationJournal;
use opencut_plugin_host::PluginHost;
use opencut_sqlite_store::EditorApplication;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command},
    time::{Duration, Instant},
};
struct Pause<'a> {
    app: &'a EditorApplication,
    root: PathBuf,
    mode: String,
}
impl Pause<'_> {
    fn stop(&self) {
        std::fs::write(self.root.join("ready"), b"ready").unwrap();
        loop {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
impl PublicationJournal<Object> for Pause<'_> {
    fn reconcile(&self, o: &Object, h: Option<&str>) -> Result<Vec<String>, EditorError> {
        self.app.reconcile(o, h)
    }
    fn prepare(&self, i: &Intent) -> Result<String, EditorError> {
        let id = self.app.prepare(i)?;
        let kind = if matches!(i.object, Object::PluginPackage { .. }) {
            "package"
        } else {
            "grants"
        };
        if self.mode.starts_with(kind) {
            std::fs::write(
                self.root.join("object.json"),
                serde_json::to_vec(&i.object).unwrap(),
            )
            .unwrap();
            if self.mode.ends_with("before") {
                self.stop();
            }
        }
        Ok(id)
    }
    fn complete(&self, id: &str) -> Result<(), EditorError> {
        if self.mode.ends_with("after") && self.root.join("object.json").exists() {
            self.stop();
        }
        PublicationJournal::<Object>::complete(self.app, id)
    }
}
#[test]
#[ignore = "spawned by real SIGKILL acceptance"]
fn plugin_publication_child() {
    let root = PathBuf::from(std::env::var_os("OPENCUT_PLUGIN_CRASH_ROOT").unwrap());
    let mode = std::env::var("OPENCUT_PLUGIN_CRASH_MODE").unwrap();
    let app = EditorApplication::open(root.join("db")).unwrap();
    let host = PluginHost::new(root.join("plugins")).unwrap();
    if mode == "reconcile" {
        let object: Object =
            serde_json::from_slice(&std::fs::read(root.join("object.json")).unwrap()).unwrap();
        std::fs::write(root.join("reconcile-started"), b"ready").unwrap();
        let result = host.reconcile(object, &app).unwrap();
        std::fs::write(
            root.join("result.json"),
            serde_json::to_vec(&result).unwrap(),
        )
        .unwrap();
    } else {
        host.install_observed(
            root.join("package"),
            Vec::<String>::new(),
            &Pause {
                app: &app,
                root,
                mode,
            },
        )
        .unwrap();
    }
}
struct Running(Child);
impl std::ops::Deref for Running {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}
impl std::ops::DerefMut for Running {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn spawn(root: &Path, mode: &str) -> Running {
    Running(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "plugin_publication_child",
                "--ignored",
                "--nocapture",
            ])
            .env("OPENCUT_PLUGIN_CRASH_ROOT", root)
            .env("OPENCUT_PLUGIN_CRASH_MODE", mode)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    )
}
fn wait(child: &mut Child, path: &Path) {
    let until = Instant::now() + Duration::from_secs(15);
    while !path.exists() {
        assert!(child.try_wait().unwrap().is_none());
        assert!(Instant::now() < until, "readiness timeout");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn events(app: &EditorApplication) -> Vec<Event> {
    app.plugin_publication_query(Query {
        target: Target::Workspace,
        after_sequence: "0".into(),
        limit: 100,
    })
    .unwrap()
    .events
}
fn package(root: &Path) {
    std::fs::create_dir(root).unwrap();
    // Installation validates ABI metadata and fingerprints; invocation is covered by the real WASM journey.
    std::fs::write(root.join("plugin.wasm"), b"crash test publication bytes").unwrap();
    std::fs::write(root.join("schema.json"), b"{}").unwrap();
    std::fs::write(root.join("manifest.json"),serde_json::to_vec(&serde_json::json!({"manifest_version":1,"id":"org.example.crash","version":"1.0.0","display_name":"crash","plugin_api":">=1.0.0, <2.0.0","runtime":{"kind":"wasm-component","entry":"plugin.wasm","world":"opencut:planner/planner@1.0.0"},"targets":["native"],"permissions":[],"contributes":{"operations":[{"name":"org.example.crash.plan","version":1,"kind":"job","input_schema":"schema.json","output_schema":"schema.json"}]},"limits":{"memory_mib":16,"wall_time_ms":1000,"output_bytes":4096,"fuel":10000}})).unwrap()).unwrap();
}
#[test]
fn package_and_grants_crash_windows_wait_for_the_real_version_lock_and_recover() {
    use std::os::unix::process::ExitStatusExt;
    for family in ["package", "grants"] {
        for (phase, expected) in [
            ("before", PublicationPhase::ObservedUnchanged),
            ("after", PublicationPhase::ObservedApplied),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            package(&root.join("package"));
            let app = EditorApplication::open(root.join("db")).unwrap();
            let mut publisher = spawn(root, &format!("{family}-{phase}"));
            wait(&mut publisher, &root.join("ready"));
            let initial = events(&app);
            let pending = initial.last().unwrap();
            assert_eq!(pending.phase, PublicationPhase::Prepared);
            let mut recovery = spawn(root, "reconcile");
            wait(&mut recovery, &root.join("reconcile-started"));
            std::thread::sleep(Duration::from_millis(150));
            assert!(recovery.try_wait().unwrap().is_none());
            assert!(!root.join("result.json").exists());
            assert_eq!(events(&app).len(), initial.len());
            publisher.kill().unwrap();
            assert_eq!(publisher.wait().unwrap().signal(), Some(9));
            wait(&mut recovery, &root.join("result.json"));
            assert!(recovery.wait().unwrap().success());
            let recovered = events(&app);
            let end = recovered.last().unwrap();
            assert_eq!(end.phase, expected);
            assert_eq!(end.publication_id, pending.publication_id);
            assert_eq!(end.origin_request_id, pending.request_id);
            assert_ne!(end.request_id, end.origin_request_id);
            let host = PluginHost::read_only(root.join("plugins")).unwrap();
            let result: ReconcileResult =
                serde_json::from_slice(&std::fs::read(root.join("result.json")).unwrap()).unwrap();
            assert_eq!(
                result.resolved_publication_ids,
                vec![pending.publication_id.clone()]
            );
            assert!(
                host.reconcile(result.object, &app)
                    .unwrap()
                    .resolved_publication_ids
                    .is_empty()
            );
            if (family == "package" && phase == "after")
                || (family == "grants" && phase == "before")
            {
                assert!(host.list().unwrap().is_empty());
                assert_eq!(
                    host.inspect("org.example.crash", "1.0.0").unwrap_err().code,
                    "PLUGIN_INSTALL_INCOMPLETE"
                );
            }
            host.install_observed(root.join("package"), Vec::<String>::new(), &app)
                .unwrap();
            assert_eq!(host.list().unwrap().len(), 1);
            assert!(
                app.maintenance_query(opencut_api_contracts::maintenance::MaintenanceQuery {
                    target: opencut_api_contracts::maintenance::MaintenanceTarget::Workspace,
                    after_sequence: "0".into(),
                    limit: 100
                })
                .unwrap()
                .events
                .is_empty()
            );
        }
    }
}
