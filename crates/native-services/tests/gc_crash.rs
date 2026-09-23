#![cfg(unix)]
//! Real SIGKILL at both durable collection boundaries; hooks exist only in this test executable.
use opencut_contracts::EditorError;
use opencut_editor_application::gc::{
    self, Files, ItemState, Native, Observation, PlanInput, ReferenceStore,
};
use opencut_native_services::gc::{Locked, NativeGc};
use opencut_sqlite_store::EditorApplication;
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
struct Paused {
    inner: Locked,
    root: PathBuf,
    after: bool,
}
struct Instrumented {
    root: PathBuf,
    after: bool,
}
impl Native for Instrumented {
    type Locked = Paused;
    fn exclusive(&self) -> Result<Paused, EditorError> {
        Ok(Paused {
            inner: NativeGc(
                &self.root.join("asset-store"),
                &self.root.join("workspace.sqlite"),
            )
            .exclusive()?,
            root: self.root.clone(),
            after: self.after,
        })
    }
}
impl Paused {
    fn pause(&self) {
        std::fs::write(self.root.join("at-boundary"), b"durable").unwrap();
        loop {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Files for Paused {
    fn protect_sources(
        &self,
        objects: &[Observation],
        paths: &std::collections::BTreeSet<String>,
    ) -> Result<std::collections::BTreeSet<String>, EditorError> {
        self.inner.protect_sources(objects, paths)
    }
    fn list(&self, input: &PlanInput) -> Result<(Vec<Observation>, Option<String>), EditorError> {
        self.inner.list(input)
    }
    fn remove(&self, object: &Observation) -> Result<ItemState, EditorError> {
        if !self.after {
            self.pause();
        }
        let result = self.inner.remove(object)?;
        self.pause();
        #[allow(unreachable_code)]
        Ok(result)
    }
}
#[test]
#[ignore = "spawned only by durable_collection_recovers_after_sigkill"]
fn child_crash_window() {
    let root = PathBuf::from(std::env::var_os("OPENCUT_GC_TEST_ROOT").unwrap());
    let input = serde_json::from_slice(&std::fs::read(root.join("execute.json")).unwrap()).unwrap();
    let mut app = EditorApplication::open(root.join("workspace.sqlite")).unwrap();
    let native = Instrumented {
        root,
        after: std::env::var("OPENCUT_GC_TEST_WINDOW").unwrap() == "after",
    };
    gc::execute(&mut app, &native, input).unwrap();
    panic!("parent should kill this process");
}
#[test]
fn durable_collection_recovers_after_sigkill() {
    use std::os::unix::process::ExitStatusExt;
    for window in ["before", "after"] {
        let root = tempfile::tempdir().unwrap().keep();
        let db = root.join("workspace.sqlite");
        let cas = root.join("asset-store");
        std::fs::create_dir(&cas).unwrap();
        let object = cas.join(".capture-12345678-1234-1234-1234-123456789012");
        std::fs::write(&object, b"unfinished capture").unwrap();
        let mut app = EditorApplication::open(&db).unwrap();
        let plan = gc::plan(
            &mut app,
            &NativeGc(&cas, &db),
            PlanInput {
                idempotency_key: window.into(),
                after: None,
                limit: 100,
            },
        )
        .unwrap();
        let input = gc::ExecuteInput {
            plan_id: plan.plan_id.clone(),
            expected_plan_hash: plan.plan_hash,
        };
        std::fs::write(
            root.join("execute.json"),
            serde_json::to_vec(&input).unwrap(),
        )
        .unwrap();
        drop(app);
        let log = std::fs::File::create(root.join("child.log")).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "child_crash_window", "--ignored", "--nocapture"])
            .env("OPENCUT_GC_TEST_ROOT", &root)
            .env("OPENCUT_GC_TEST_WINDOW", window)
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap();
        let until = Instant::now() + Duration::from_secs(15);
        while !root.join("at-boundary").exists() {
            if Instant::now() >= until || child.try_wait().unwrap().is_some() {
                let _ = child.kill();
                panic!(
                    "child did not reach {window} boundary: {}",
                    std::fs::read_to_string(root.join("child.log")).unwrap()
                );
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        child.kill().unwrap();
        assert_eq!(child.wait().unwrap().signal(), Some(9));
        assert_eq!(object.exists(), window == "before");
        let mut app = EditorApplication::open(&db).unwrap();
        let pending = app.gc_load(&plan.plan_id).unwrap();
        assert_eq!(pending.view.items[0].state, ItemState::Removing);
        let completed = gc::execute(&mut app, &NativeGc(&cas, &db), input.clone()).unwrap();
        assert_eq!(
            completed.items[0].state,
            if window == "before" {
                ItemState::Removed
            } else {
                ItemState::AlreadyAbsent
            }
        );
        assert!(!object.exists());
        assert_eq!(
            serde_json::to_value(&completed).unwrap(),
            serde_json::to_value(gc::execute(&mut app, &NativeGc(&cas, &db), input).unwrap())
                .unwrap()
        );
        println!("SIGKILL {window}: {}", root.display());
    }
}
