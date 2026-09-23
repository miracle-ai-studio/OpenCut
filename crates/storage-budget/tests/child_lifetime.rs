#![cfg(unix)]
use opencut_editor_application::budget::BudgetNative;
use opencut_storage_budget::{AttemptLease, Budget};
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn ffmpeg(output: &Path) -> Command {
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-v",
            "error",
            "-re",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=640x360:rate=5",
            "-t",
            "60",
            "-c:v",
            "rawvideo",
            "-threads",
            "1",
            "-f",
            "nut",
        ])
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}
fn until(mut condition: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(20);
    while !condition() {
        assert!(Instant::now() < end, "child lifecycle deadline");
        thread::sleep(Duration::from_millis(5));
    }
}
struct Supervisor(std::process::Child);
impl Drop for Supervisor {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Pid(Option<i32>);
impl Pid {
    fn signal(&self, signal: i32) {
        assert_eq!(
            unsafe { libc::kill(self.0.expect("live test child"), signal) },
            0
        );
    }
}
impl Drop for Pid {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
        }
    }
}

#[test]
#[ignore = "spawned by actual orphan FFmpeg acceptance"]
fn child_supervisor() {
    let Some(root) = std::env::var_os("OPENCUT_CHILD_ADMISSION_TEST") else {
        return;
    };
    let root = Path::new(&root);
    let held = Budget::new(root)
        .reserve("private_asset_copy", 4 << 20)
        .unwrap();
    let mut writer = held
        .spawn_in_attempt(
            ffmpeg(&root.join("artifacts/output.nut")),
            AttemptLease::acquire(&root.join("artifacts")).unwrap(),
        )
        .unwrap();
    let reader_dir = root.join("reader-attempt");
    fs::create_dir(&reader_dir).unwrap();
    let mut reader_command = Command::new("sleep");
    reader_command
        .arg("60")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut reader = AttemptLease::acquire(&reader_dir)
        .unwrap()
        .spawn_readonly(reader_command)
        .unwrap();
    // This unrelated long-lived process must not inherit the lifetime lock.
    let unrelated = Supervisor(
        Command::new("sleep")
            .arg("60")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    fs::write(
        root.join("children.tmp"),
        format!(
            "[{},{},{}]",
            writer.process().id(),
            unrelated.0.id(),
            reader.process().id()
        ),
    )
    .unwrap();
    fs::rename(root.join("children.tmp"), root.join("children.json")).unwrap();
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

#[test]
fn orphan_ffmpeg_retains_admission_and_kernel_ceiling_after_supervisor_sigkill() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("artifacts")).unwrap();
    let budget = Budget::new(root.path());
    let mut supervisor = Supervisor(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "child_supervisor", "--ignored", "--nocapture"])
            .env("OPENCUT_CHILD_ADMISSION_TEST", root.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let children = root.path().join("children.json");
    until(|| children.exists());
    let pids: Vec<i32> = serde_json::from_slice(&fs::read(&children).unwrap()).unwrap();
    let mut writer = Pid(Some(pids[0]));
    let unrelated = Pid(Some(pids[1]));
    let mut reader = Pid(Some(pids[2]));
    let output = root.path().join("artifacts/output.nut");
    until(|| output.metadata().is_ok_and(|m| m.len() > 0));
    writer.signal(libc::SIGSTOP);
    supervisor.0.kill().unwrap();
    supervisor.0.wait().unwrap();
    assert!(
        AttemptLease::try_cleanup(&root.path().join("artifacts"))
            .unwrap()
            .is_none()
    );
    assert!(
        AttemptLease::try_cleanup(&root.path().join("reader-attempt"))
            .unwrap()
            .is_none()
    );
    reader.signal(libc::SIGKILL);
    until(|| {
        AttemptLease::try_cleanup(&root.path().join("reader-attempt"))
            .unwrap()
            .is_some()
    });
    reader.0 = None;
    let stopped_size = output.metadata().unwrap().len();
    let recovered = budget.inspect(true).unwrap();
    assert_eq!(recovered.reserved_bytes, (4_u64 << 20).to_string());
    assert_eq!(recovered.recovered_reservations, 0);
    assert!(budget.reserve("private_asset_copy", 64 << 30).is_err());
    writer.signal(libc::SIGCONT);
    until(|| output.metadata().unwrap().len() > stopped_size);
    // The original writer would run 60 seconds. It must instead stop within the
    // deadline due to the inherited 4 MiB kernel ceiling, with no parent alive.
    until(|| budget.inspect(true).unwrap().reserved_bytes == "0");
    let size = output.metadata().unwrap().len();
    assert!(
        size > stopped_size && size <= 4 << 20,
        "actual bytes {size}"
    );
    assert!(
        AttemptLease::try_cleanup(&root.path().join("artifacts"))
            .unwrap()
            .is_some()
    );
    unrelated.signal(0); // a leaked lock here would prevent the recovery above
    assert_eq!(
        budget.inspect(false).unwrap().observed_payload_bytes,
        size.to_string()
    );
    assert!(!root.path().join("core").exists());
    writer.0 = None; // proven finished; do not signal a subsequently reused PID
    println!(
        "orphan_ffmpeg_evidence={}",
        serde_json::json!({
            "supervisor_sigkill": true, "writer_and_readonly_attempt_inheritance": true, "reserved_while_orphan_alive": recovered.reserved_bytes,
            "stopped_bytes": stopped_size, "final_bytes": size,
            "configured_ceiling": 4_u64 << 20, "unrelated_child_still_alive": true,
            "recovered_reserved_bytes": budget.inspect(false).unwrap().reserved_bytes,
        })
    );
}

#[test]
fn normal_child_exit_and_failed_spawn_leave_no_reservation() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("artifacts")).unwrap();
    let budget = Budget::new(root.path());
    let held = budget.reserve("private_asset_copy", 1024).unwrap();
    assert!(
        held.spawn_single_file(Command::new(root.path().join("no-such-executable")))
            .is_err()
    );
    assert_eq!(budget.inspect(false).unwrap().reserved_bytes, "0");
    let held = budget.reserve("private_asset_copy", 1024).unwrap();
    let output = root.path().join("artifacts/small.nut");
    let mut child = held.spawn_single_file(ffmpeg(&output)).unwrap();
    assert!(!child.process().wait().unwrap().success());
    drop(child);
    assert_eq!(budget.inspect(false).unwrap().reserved_bytes, "0");
    assert!(output.metadata().unwrap().len() <= 1024);
    let held = budget.reserve("private_asset_copy", 4 << 20).unwrap();
    let mut abandoned = held
        .spawn_single_file(ffmpeg(&root.path().join("artifacts/abandoned.nut")))
        .unwrap();
    let pid = abandoned.process().id() as i32;
    drop(abandoned);
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert_eq!(budget.inspect(false).unwrap().reserved_bytes, "0");
}

#[test]
fn cleanup_tolerates_another_cleaner_and_rechecks_directory_identity() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("attempt");
    fs::create_dir(&path).unwrap();
    let lease = AttemptLease::acquire(&path).unwrap();
    assert!(AttemptLease::try_cleanup(&path).unwrap().is_none());
    drop(lease);
    let guard = AttemptLease::try_cleanup(&path).unwrap().unwrap();
    assert!(guard.still_names(&path).unwrap());
    fs::remove_dir(&path).unwrap();
    assert!(!guard.still_names(&path).unwrap());
    assert!(AttemptLease::try_cleanup(&path).unwrap().is_none());
    fs::create_dir(&path).unwrap();
    assert!(!guard.still_names(&path).unwrap());
    let alias = root.path().join("alias");
    std::os::unix::fs::symlink(&path, &alias).unwrap();
    assert!(AttemptLease::try_cleanup(&alias).is_err());
}
