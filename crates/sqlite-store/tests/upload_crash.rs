#![cfg(unix)]
//! Native upload persistence under real process death and competing OS locks.
//! Pauses belong to this test's port adapter, never production code.
use opencut_asset_store::uploads::UploadChunks;
use opencut_contracts::{CreateProject, EditorError, FrameRate};
use opencut_editor_application::{access::*, uploads::*};
use opencut_sqlite_store::EditorApplication;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Paused {
    app: EditorApplication,
    root: PathBuf,
    mode: String,
}
impl Paused {
    fn pause(&self) {
        std::fs::write(self.root.join("ready"), b"ready").unwrap();
        while !self.root.join("release").exists() {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl UploadStore for Paused {
    fn create_upload(&mut self, input: &CreateUpload) -> Result<UploadView, EditorError> {
        self.app.create_upload(input)
    }
    fn inspect_upload(&self, id: &str) -> Result<UploadView, EditorError> {
        self.app.inspect_upload(id)
    }
    fn authorize_chunk(&self, input: &ChunkDescriptor) -> Result<UploadView, EditorError> {
        self.app.authorize_chunk(input)
    }
    fn acknowledge_chunk(&mut self, input: &ChunkDescriptor) -> Result<UploadView, EditorError> {
        if self.mode == "before" {
            self.pause();
        }
        let view = self.app.acknowledge_chunk(input)?;
        if self.mode == "after" {
            self.pause();
        }
        Ok(view)
    }
    fn abort_upload(&mut self, id: &str) -> Result<UploadView, EditorError> {
        self.app.abort_upload(id)
    }
}
impl UploadRetentionStore for Paused {
    fn read_upload_page(
        &self,
        project: &str,
        retained_only: bool,
        upper: Option<&str>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<UploadPageSlice, EditorError> {
        self.app
            .read_upload_page(project, retained_only, upper, after, limit)
    }

    fn upload_retention(&self, id: &str) -> Result<UploadRetentionView, EditorError> {
        self.app.upload_retention(id)
    }
    fn upload_quota(&self, id: &str) -> Result<UploadQuotaView, EditorError> {
        self.app.upload_quota(id)
    }
    fn authorize_upload_release(&self, id: &str) -> Result<UploadRetentionView, EditorError> {
        self.app.authorize_upload_release(id)
    }
    fn finish_upload_release(&mut self, id: &str) -> Result<UploadRetentionView, EditorError> {
        if self.mode == "release_before" {
            self.pause();
        }
        let result = self.app.finish_upload_release(id)?;
        if self.mode == "release_after" {
            self.pause();
        }
        Ok(result)
    }
}
fn descriptor(id: &str, data: &[u8]) -> ChunkDescriptor {
    ChunkDescriptor {
        upload_id: id.into(),
        offset: "0".into(),
        size_bytes: data.len().to_string(),
        content_hash: format!("{:x}", Sha256::digest(data)),
    }
}
#[test]
#[ignore = "spawned by the real upload recovery tests"]
fn upload_child() {
    let root = PathBuf::from(std::env::var_os("OPENCUT_UPLOAD_TEST_ROOT").unwrap());
    let mode = std::env::var("OPENCUT_UPLOAD_TEST_MODE").unwrap();
    let id = std::fs::read_to_string(root.join("upload-id")).unwrap();
    let data = if mode == "conflict" { b"xyz" } else { b"abc" };
    let mut app = EditorApplication::open(root.join("db")).unwrap();
    if let Ok(secret) = std::fs::read_to_string(root.join("credential")) {
        app.set_caller(app.authenticate(&secret).unwrap());
    }
    let mut store = Paused {
        app,
        root: root.clone(),
        mode: mode.clone(),
    };
    std::fs::write(root.join(format!("{mode}-started")), b"started").unwrap();
    // Resume through a relative path to the same workspace. This also checks
    // parent-directory durability and lock identity across path spellings.
    let upload_root = if mode == "retry" {
        PathBuf::from("uploads")
    } else {
        root.join("uploads")
    };
    let native = UploadChunks::new(upload_root);
    let result = if mode.starts_with("release_") {
        release_upload(
            &mut store,
            &native,
            UploadInput {
                upload_id: id.clone(),
            },
        )
        .map(|_| store.inspect_upload(&id).unwrap())
    } else if mode == "abort" {
        abort_with_cleanup(
            &mut store,
            &native,
            UploadInput {
                upload_id: id.clone(),
            },
        )
    } else {
        native.put(&mut store, &descriptor(&id, data), data)
    };
    std::fs::write(
        root.join(format!("{mode}-result.json")),
        serde_json::to_vec(&result).unwrap(),
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
            .args(["--exact", "upload_child", "--ignored", "--nocapture"])
            .current_dir(root)
            .env("OPENCUT_UPLOAD_TEST_ROOT", root)
            .env("OPENCUT_UPLOAD_TEST_MODE", mode)
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    )
}
fn wait(child: &mut Child, file: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !file.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "child exited before {}",
            file.display()
        );
        assert!(Instant::now() < deadline, "child readiness deadline");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn setup(root: &Path) -> (EditorApplication, UploadView) {
    let mut app = EditorApplication::open(root.join("db")).unwrap();
    app.create_project(
        CreateProject {
            id: "a".into(),
            name: "a".into(),
            sequence_id: "main".into(),
            width: 64,
            height: 48,
            fps: FrameRate {
                numerator: 10,
                denominator: 1,
            },
        },
        "create",
    )
    .unwrap();
    let view = app
        .create_upload(&CreateUpload {
            project_id: "a".into(),
            idempotency_key: "upload".into(),
            name: "file".into(),
            size_bytes: "6".into(),
            content_hash: format!("{:x}", Sha256::digest(b"abcdef")),
        })
        .unwrap();
    std::fs::write(root.join("upload-id"), &view.upload_id).unwrap();
    (app, view)
}
fn result(root: &Path, mode: &str) -> Result<UploadView, EditorError> {
    serde_json::from_slice(&std::fs::read(root.join(format!("{mode}-result.json"))).unwrap())
        .unwrap()
}
fn chunk_count(root: &Path) -> i64 {
    rusqlite::Connection::open(root.join("db"))
        .unwrap()
        .query_row("SELECT count(*) FROM upload_chunks", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn abort_crash_while_waiting_for_writer_lock_is_recoverable_without_late_ack() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (app, view) = setup(root);
    let mut writer = spawn(root, "before");
    wait(&mut writer, &root.join("ready"));
    let mut aborter = spawn(root, "abort");
    wait(&mut aborter, &root.join("abort-started"));
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.inspect_upload(&view.upload_id).unwrap().state != UploadState::Aborted {
        assert!(aborter.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!root.join("abort-result.json").exists());
    assert!(root.join("uploads").join(&view.upload_id).exists());
    aborter.kill().unwrap();
    aborter.wait().unwrap();
    std::fs::write(root.join("release"), b"release").unwrap();
    wait(&mut writer, &root.join("before-result.json"));
    assert!(writer.wait().unwrap().success());
    assert_eq!(
        result(root, "before").unwrap_err().code,
        "UPLOAD_STATE_CONFLICT"
    );
    assert_eq!(chunk_count(root), 0);
    let mut retry = spawn(root, "abort");
    wait(&mut retry, &root.join("abort-result.json"));
    assert!(retry.wait().unwrap().success());
    assert_eq!(result(root, "abort").unwrap().state, UploadState::Aborted);
    assert!(!root.join("uploads").join(&view.upload_id).exists());
    assert_eq!(
        app.inspect_upload(&view.upload_id).unwrap().next_offset,
        "0"
    );
}
#[test]
fn sigkill_before_and_after_ack_keeps_confirmed_offset_and_releases_the_upload_lock() {
    use std::os::unix::process::ExitStatusExt;
    for (mode, confirmed) in [("before", "0"), ("after", "3")] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (app, view) = setup(root);
        let head = app.snapshot("a").unwrap();
        let mut writer = spawn(root, mode);
        wait(&mut writer, &root.join("ready"));
        assert_eq!(
            std::fs::read(root.join("uploads").join(&view.upload_id).join("0.chunk")).unwrap(),
            b"abc"
        );
        assert_eq!(
            app.inspect_upload(&view.upload_id).unwrap().next_offset,
            confirmed
        );
        let mut retry = spawn(root, "retry");
        wait(&mut retry, &root.join("retry-started"));
        std::thread::sleep(Duration::from_millis(100));
        assert!(retry.try_wait().unwrap().is_none());
        assert!(!root.join("retry-result.json").exists());
        writer.kill().unwrap();
        assert_eq!(writer.wait().unwrap().signal(), Some(9));
        wait(&mut retry, &root.join("retry-result.json"));
        assert!(retry.wait().unwrap().success());
        assert_eq!(result(root, "retry").unwrap().next_offset, "3");
        assert_eq!(chunk_count(root), 1);
        drop(app);
        let reopened = EditorApplication::open(root.join("db")).unwrap();
        assert_eq!(
            reopened
                .inspect_upload(&view.upload_id)
                .unwrap()
                .next_offset,
            "3"
        );
        assert_eq!(reopened.snapshot("a").unwrap(), head);
        let conn = rusqlite::Connection::open(root.join("db")).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM outbox WHERE event_type='upload.chunk_accepted'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
#[test]
fn different_concurrent_chunks_cannot_both_claim_the_same_offset() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (app, view) = setup(root);
    let mut first = spawn(root, "before");
    wait(&mut first, &root.join("ready"));
    let mut second = spawn(root, "conflict");
    wait(&mut second, &root.join("conflict-started"));
    std::thread::sleep(Duration::from_millis(100));
    assert!(!root.join("conflict-result.json").exists());
    std::fs::write(root.join("release"), b"release").unwrap();
    wait(&mut first, &root.join("before-result.json"));
    wait(&mut second, &root.join("conflict-result.json"));
    assert!(first.wait().unwrap().success());
    assert!(second.wait().unwrap().success());
    assert!(result(root, "before").is_ok());
    assert_eq!(
        result(root, "conflict").unwrap_err().code,
        "UPLOAD_CHUNK_CONFLICT"
    );
    assert_eq!(chunk_count(root), 1);
    assert_eq!(
        app.inspect_upload(&view.upload_id).unwrap().next_offset,
        "3"
    );
    assert_eq!(
        std::fs::read(root.join("uploads").join(&view.upload_id).join("0.chunk")).unwrap(),
        b"abc"
    );
}
#[test]
fn revoke_or_abort_after_durable_bytes_prevents_acknowledgement() {
    for revoke in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (mut app, view) = setup(root);
        app.set_principal_policy(PrincipalPolicy {
            principal_id: "uploader".into(),
            expected_revision: "0".into(),
            enabled: true,
            grants: vec![ProjectGrant {
                project_id: "a".into(),
                actions: vec![
                    ProjectAction::Read,
                    ProjectAction::Edit,
                    ProjectAction::RunJobs,
                ],
            }],
        })
        .unwrap();
        let issued = app.issue_credential("uploader").unwrap();
        std::fs::write(root.join("credential"), &issued.secret).unwrap();
        let mut writer = spawn(root, "before");
        wait(&mut writer, &root.join("ready"));
        if revoke {
            app.revoke_credential(&issued.credential_id).unwrap();
        } else {
            app.abort_upload(&view.upload_id).unwrap();
        }
        std::fs::write(root.join("release"), b"release").unwrap();
        wait(&mut writer, &root.join("before-result.json"));
        assert!(writer.wait().unwrap().success());
        assert_eq!(
            result(root, "before").unwrap_err().code,
            if revoke {
                "NOT_AUTHORIZED"
            } else {
                "UPLOAD_STATE_CONFLICT"
            }
        );
        assert_eq!(chunk_count(root), 0);
        assert_eq!(
            app.inspect_upload(&view.upload_id).unwrap().next_offset,
            "0"
        );
    }
}

#[test]
fn release_sigkill_before_and_after_reservation_commit_never_frees_unremoved_chunks() {
    for (mode, reserved) in [("release_before", "6"), ("release_after", "0")] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (mut app, view) = setup(root);
        let native = UploadChunks::new(root.join("uploads"));
        native
            .put(&mut app, &descriptor(&view.upload_id, b"abc"), b"abc")
            .unwrap();
        app.abort_upload(&view.upload_id).unwrap();
        let mut cleaner = spawn(root, mode);
        wait(&mut cleaner, &root.join("ready"));
        assert!(!root.join("uploads").join(&view.upload_id).exists());
        assert_eq!(
            app.upload_quota("a").unwrap().project_reserved_bytes,
            reserved
        );
        cleaner.kill().unwrap();
        cleaner.wait().unwrap();
        release_upload(
            &mut app,
            &native,
            UploadInput {
                upload_id: view.upload_id.clone(),
            },
        )
        .unwrap();
        assert_eq!(app.upload_quota("a").unwrap().project_reserved_bytes, "0");
        assert_eq!(
            app.upload_retention(&view.upload_id).unwrap().state,
            UploadRetentionState::Released
        );
        let c = rusqlite::Connection::open(root.join("db")).unwrap();
        assert_eq!(
            c.query_row::<i64, _, _>(
                "SELECT count(*) FROM audit_records WHERE action='upload.staging_released'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            1
        );
    }
}
