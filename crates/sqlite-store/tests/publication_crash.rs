#![cfg(unix)]
//! Real SIGKILL windows. The pause lives only in this test adapter, never the host.
use opencut_asset_store::catalog::MediaCatalog;
use opencut_contracts::{Asset, AssetKind, EditorError};
use opencut_editor_application::maintenance::*;
use opencut_sqlite_store::EditorApplication;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command},
    time::{Duration, Instant},
};
const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
fn object() -> MaintenanceObject {
    MaintenanceObject::MediaMetadata {
        content_hash: HASH.into(),
    }
}
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
impl PublicationJournal for Pause<'_> {
    fn reconcile(
        &self,
        o: &MaintenanceObject,
        h: Option<&str>,
    ) -> Result<Vec<String>, EditorError> {
        self.app.reconcile(o, h)
    }
    fn prepare(&self, i: &PublicationIntent) -> Result<String, EditorError> {
        let id = self.app.prepare(i)?;
        if self.mode == "before" {
            self.stop();
        }
        Ok(id)
    }
    fn complete(&self, id: &str) -> Result<(), EditorError> {
        if self.mode == "after" {
            self.stop();
        }
        PublicationJournal::<opencut_api_contracts::maintenance::MaintenanceObject>::complete(
            self.app, id,
        )
    }
}
#[test]
#[ignore = "spawned by the SIGKILL acceptance test"]
fn publication_child() {
    let root = PathBuf::from(std::env::var_os("OPENCUT_PUBLICATION_TEST_ROOT").unwrap());
    let mode = std::env::var("OPENCUT_PUBLICATION_TEST_MODE").unwrap();
    let app = EditorApplication::open(root.join("db")).unwrap();
    let catalog = MediaCatalog::open(root.join("media")).unwrap();
    if mode == "reconcile" {
        std::fs::write(root.join("reconcile-started"), b"ready").unwrap();
        let result = catalog.reconcile(object(), &app).unwrap();
        std::fs::write(
            root.join("result.json"),
            serde_json::to_vec(&result).unwrap(),
        )
        .unwrap();
        return;
    }
    let asset = Asset {
        id: format!("asset_{HASH}"),
        name: "crash-window".into(),
        kind: AssetKind::Image,
        uri: "managed:test".into(),
        duration: None,
        version: None,
        metadata: Default::default(),
    };
    catalog
        .publish_observed(
            HASH,
            &asset,
            &Pause {
                app: &app,
                root,
                mode,
            },
        )
        .unwrap();
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
            .args(["--exact", "publication_child", "--ignored", "--nocapture"])
            .env("OPENCUT_PUBLICATION_TEST_ROOT", root)
            .env("OPENCUT_PUBLICATION_TEST_MODE", mode)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    )
}
fn await_file(child: &mut Child, path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(child.try_wait().unwrap().is_none());
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("child readiness timeout");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn events(app: &EditorApplication) -> Vec<MaintenanceEvent> {
    app.maintenance_query(MaintenanceQuery {
        target: MaintenanceTarget::Workspace,
        after_sequence: "0".into(),
        limit: 100,
    })
    .unwrap()
    .events
}
#[test]
fn sigkill_before_and_after_publication_reconciles_only_after_the_object_lock_is_released() {
    use std::os::unix::process::ExitStatusExt;
    for (mode, expected) in [
        ("before", PublicationPhase::ObservedUnchanged),
        ("after", PublicationPhase::ObservedApplied),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let app = EditorApplication::open(root.join("db")).unwrap();
        let mut publisher = spawn(root, mode);
        await_file(&mut publisher, &root.join("ready"));
        let first = events(&app);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].phase, PublicationPhase::Prepared);
        let mut reconciliation = spawn(root, "reconcile");
        await_file(&mut reconciliation, &root.join("reconcile-started"));
        std::thread::sleep(Duration::from_millis(100));
        assert!(reconciliation.try_wait().unwrap().is_none());
        assert!(!root.join("result.json").exists());
        assert_eq!(events(&app).len(), 1);
        publisher.kill().unwrap();
        assert_eq!(publisher.wait().unwrap().signal(), Some(9));
        await_file(&mut reconciliation, &root.join("result.json"));
        assert!(reconciliation.wait().unwrap().success());
        let resolved = events(&app);
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[1].phase, expected);
        assert_eq!(resolved[0].publication_id, resolved[1].publication_id);
        assert_eq!(resolved[0].request_id, resolved[1].origin_request_id);
        assert_ne!(resolved[1].request_id, resolved[1].origin_request_id);
        let result: ReconcileResult =
            serde_json::from_slice(&std::fs::read(root.join("result.json")).unwrap()).unwrap();
        assert_eq!(
            result.resolved_publication_ids,
            vec![resolved[0].publication_id.clone()]
        );
        let catalog = MediaCatalog::open(root.join("media")).unwrap();
        assert!(
            catalog
                .reconcile(object(), &app)
                .unwrap()
                .resolved_publication_ids
                .is_empty()
        );
        assert_eq!(events(&app).len(), 2);
    }
}
