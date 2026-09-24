#![cfg(unix)]
use fs2::FileExt;
use opencut_editor_application::{
    audit::{CallSource, ExecutionContext},
    budget::{BudgetNative, Configure, RepairAction, RepairRequest, RepairState},
};
use opencut_storage_budget::Budget;
use std::{
    fs,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
fn configure(root: &Path, bytes: u64) {
    Budget::new(root)
        .configure(
            Configure {
                media_output_limit_bytes: None,
                control_limit_bytes: None,
                expected_revision: 0,
                idempotency_key: "policy".into(),
                payload_limit_bytes: bytes.to_string(),
                minimum_available_bytes: "0".into(),
            },
            &ExecutionContext::local(CallSource::Cli),
        )
        .unwrap();
}
fn wait(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !fs::read(path).is_ok_and(|contents| !contents.is_empty()) {
        assert!(Instant::now() < deadline, "missing {}", path.display());
        thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn bound_database_connection_does_not_need_an_exclusive_admission_gate() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let budget = Budget::new(root);
    drop(
        budget
            .database_connection_lease("workspace.sqlite", false)
            .unwrap(),
    );
    let gate = fs::File::open(root.join(".storage-budget")).unwrap();
    FileExt::lock_shared(&gate).unwrap();
    let started = Instant::now();
    let lease = budget
        .database_connection_lease("workspace.sqlite", false)
        .unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    drop(lease);
    FileExt::unlock(&gate).unwrap();
}
#[test]
fn physical_repair_reserve_fences_normal_writes_and_replays_transitions() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    configure(root, 1024);
    let budget = Budget::new(root);
    drop(
        budget
            .reserve_database_control("workspace.sqlite", 0)
            .unwrap(),
    );
    let ready = budget
        .inspect(false)
        .unwrap()
        .control
        .unwrap()
        .repair
        .unwrap();
    assert_eq!(ready.state, RepairState::Ready);
    assert_eq!(ready.allocated_file_bytes, ready.target_bytes);
    assert!(
        ready.allocated_blocks_bytes.parse::<u64>().unwrap()
            >= ready.target_bytes.parse::<u64>().unwrap()
    );
    let context = ExecutionContext::local(CallSource::Cli);
    let enter = RepairRequest {
        expected_revision: 0,
        idempotency_key: "enter-repair".into(),
    };
    let entered = budget
        .repair_transition(RepairAction::Enter, enter.clone(), &context)
        .unwrap();
    assert_eq!(entered.state, RepairState::Active);
    assert_eq!(
        fs::metadata(root.join(".storage-budget/repair.reserve"))
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        budget.reserve("cas_capture", 1).err().unwrap().code,
        "STORAGE_CAPACITY_LOW"
    );
    assert_eq!(
        budget
            .reserve_database_control("workspace.sqlite", 1)
            .err()
            .unwrap()
            .code,
        "DATABASE_CAPACITY_EXHAUSTED"
    );
    let repair = budget
        .reserve_database_control_repair("workspace.sqlite", 1024)
        .unwrap();
    let exit = RepairRequest {
        expected_revision: 1,
        idempotency_key: "exit-repair".into(),
    };
    assert_eq!(
        budget
            .repair_transition(RepairAction::Exit, exit.clone(), &context)
            .unwrap_err()
            .code,
        "STORAGE_BUDGET_BUSY"
    );
    drop(repair);
    let exited = budget
        .repair_transition(RepairAction::Exit, exit, &context)
        .unwrap();
    assert_eq!(exited.state, RepairState::Ready);
    assert_eq!(
        budget
            .repair_transition(RepairAction::Enter, enter, &context)
            .unwrap(),
        entered
    );
    assert_eq!(
        budget
            .inspect(false)
            .unwrap()
            .control
            .unwrap()
            .repair
            .unwrap()
            .state,
        RepairState::Ready
    );
    drop(budget.reserve("cas_capture", 1).unwrap());
    fs::OpenOptions::new()
        .write(true)
        .open(root.join(".storage-budget/repair.reserve"))
        .unwrap()
        .set_len(0)
        .unwrap();
    assert_eq!(
        budget.reserve("cas_capture", 1).err().unwrap().code,
        "STORAGE_BUDGET_ERROR"
    );
    assert_eq!(
        budget
            .inspect(false)
            .unwrap()
            .control
            .unwrap()
            .repair
            .unwrap()
            .state,
        RepairState::Degraded
    );
    // A crash after truncating the file but before committing the transition
    // leaves a ready ledger with a short file. Normal writes fail closed; the
    // owner can finish the intended transition with the next exact revision.
    budget
        .repair_transition(
            RepairAction::Enter,
            RepairRequest {
                expected_revision: 2,
                idempotency_key: "resume-enter".into(),
            },
            &context,
        )
        .unwrap();
    // A crash after refilling the file but before recording exit leaves an
    // active ledger. Repair writes pause and exit can finish idempotently.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(root.join(".storage-budget/repair.reserve"))
        .unwrap();
    let random = fs::File::open("/dev/urandom").unwrap();
    assert_eq!(
        std::io::copy(&mut random.take(8 * 1024 * 1024), &mut file).unwrap(),
        8 * 1024 * 1024
    );
    file.sync_all().unwrap();
    drop(file);
    assert_eq!(
        budget
            .reserve_database_control_repair("workspace.sqlite", 1)
            .err()
            .unwrap()
            .code,
        "STORAGE_BUDGET_ERROR"
    );
    budget
        .repair_transition(
            RepairAction::Exit,
            RepairRequest {
                expected_revision: 3,
                idempotency_key: "resume-exit".into(),
            },
            &context,
        )
        .unwrap();
    assert_eq!(
        budget
            .inspect(false)
            .unwrap()
            .control
            .unwrap()
            .repair
            .unwrap()
            .state,
        RepairState::Ready
    );
    fs::remove_file(root.join(".storage-budget/connections.lock")).unwrap();
    assert_eq!(
        budget
            .inspect(false)
            .unwrap()
            .control
            .unwrap()
            .repair
            .unwrap()
            .state,
        RepairState::Degraded
    );
    assert_eq!(
        budget.reserve("cas_capture", 1).err().unwrap().code,
        "STORAGE_BUDGET_ERROR"
    );
}

#[test]
fn missing_ledger_with_allocated_repair_file_never_resets_policy() {
    let directory = tempfile::tempdir().unwrap();
    let budget = Budget::new(directory.path());
    configure(directory.path(), 1024);
    drop(
        budget
            .reserve_database_control("workspace.sqlite", 0)
            .unwrap(),
    );
    fs::remove_file(directory.path().join(".storage-budget/ledger.json")).unwrap();
    assert_eq!(
        budget
            .reserve_database_control("workspace.sqlite", 1)
            .err()
            .unwrap()
            .code,
        "STORAGE_BUDGET_ERROR"
    );
    assert_eq!(
        budget.inspect(false).unwrap_err().code,
        "STORAGE_BUDGET_ERROR"
    );
}
#[test]
#[ignore = "child process launched by lifecycle tests"]
fn writer() {
    let Some(root) = std::env::var_os("OPENCUT_BUDGET_TEST_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    let id = std::env::var("OPENCUT_BUDGET_TEST_ID").unwrap();
    let budget = Budget::new(root);
    let result = if std::env::var_os("OPENCUT_BUDGET_TEST_CONTROL").is_some() {
        budget.reserve_database_control("workspace.sqlite", 150)
    } else {
        budget.reserve("cas_capture", 150)
    };
    match result {
        Ok(_reservation) => {
            if std::env::var_os("OPENCUT_BUDGET_TEST_BYTES").is_some() {
                fs::write(root.join("asset-store/partial"), [0; 64]).unwrap();
            }
            fs::write(root.join(format!("{id}.ready")), "held").unwrap();
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        }
        Err(e) => {
            fs::write(root.join(format!("{id}.ready")), e.code).unwrap();
        }
    }
}
struct ChildGuard(std::process::Child);
impl std::ops::Deref for ChildGuard {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for ChildGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn child(root: &Path, id: &str, bytes: bool) -> ChildGuard {
    let mut c = Command::new(std::env::current_exe().unwrap());
    c.args(["--ignored", "--exact", "writer", "--nocapture"])
        .env("OPENCUT_BUDGET_TEST_ROOT", root)
        .env("OPENCUT_BUDGET_TEST_ID", id)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    if bytes {
        c.env("OPENCUT_BUDGET_TEST_BYTES", "yes");
    }
    ChildGuard(c.spawn().unwrap())
}
fn control_child(root: &Path, id: &str) -> ChildGuard {
    let mut c = Command::new(std::env::current_exe().unwrap());
    c.args(["--ignored", "--exact", "writer", "--nocapture"])
        .env("OPENCUT_BUDGET_TEST_ROOT", root)
        .env("OPENCUT_BUDGET_TEST_ID", id)
        .env("OPENCUT_BUDGET_TEST_CONTROL", "yes")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    ChildGuard(c.spawn().unwrap())
}
#[test]
fn simultaneous_processes_cannot_double_admit_and_sigkill_keeps_actual_usage() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir(root.join("asset-store")).unwrap();
    configure(root, 256);
    let mut a = child(root, "a", true);
    let mut b = child(root, "b", true);
    wait(&root.join("a.ready"));
    wait(&root.join("b.ready"));
    let statuses = [
        fs::read_to_string(root.join("a.ready")).unwrap(),
        fs::read_to_string(root.join("b.ready")).unwrap(),
    ];
    assert_eq!(statuses.iter().filter(|s| s.as_str() == "held").count(), 1);
    assert_eq!(
        statuses
            .iter()
            .filter(|s| s.as_str() == "STORAGE_BUDGET_EXCEEDED")
            .count(),
        1
    );
    let budget = Budget::new(root);
    let live = budget.inspect(true).unwrap();
    assert_eq!(live.reserved_bytes, "150");
    assert_eq!(live.recovered_reservations, 0);
    for c in [&mut a, &mut b] {
        if c.try_wait().unwrap().is_none() {
            c.kill().unwrap();
        }
        c.wait().unwrap();
    }
    let pending = budget.inspect(false).unwrap();
    assert_eq!(pending.reserved_bytes, "150");
    assert_eq!(pending.observed_payload_bytes, "64");
    let recovered = budget.inspect(true).unwrap();
    assert_eq!(recovered.recovered_reservations, 1);
    assert_eq!(recovered.reserved_bytes, "0");
    assert_eq!(recovered.observed_payload_bytes, "64");
    assert!(root.join("asset-store/partial").exists());
    assert_eq!(
        budget.reserve("cas_capture", 193).err().unwrap().code,
        "STORAGE_BUDGET_EXCEEDED"
    );
    let guard = budget.reserve("cas_capture", 192).unwrap();
    drop(guard);
    assert_eq!(budget.inspect(false).unwrap().reserved_bytes, "0");
}
#[test]
fn observation_does_not_create_state_and_policy_replay_is_exact() {
    let d = tempfile::tempdir().unwrap();
    let budget = Budget::new(d.path());
    let initial = budget.inspect(false).unwrap();
    assert_eq!(initial.policy.revision, 0);
    assert_eq!(fs::read_dir(d.path()).unwrap().count(), 0);
    let context = ExecutionContext::local(CallSource::Cli);
    let input = Configure {
        media_output_limit_bytes: None,
        control_limit_bytes: None,
        expected_revision: 0,
        idempotency_key: "first".into(),
        payload_limit_bytes: "128".into(),
        minimum_available_bytes: "0".into(),
    };
    let policy = budget.configure(input.clone(), &context).unwrap();
    assert_eq!(budget.configure(input.clone(), &context).unwrap(), policy);
    let mut conflict = input.clone();
    conflict.idempotency_key = "second".into();
    assert_eq!(
        budget.configure(conflict, &context).unwrap_err().code,
        "STORAGE_POLICY_CONFLICT"
    );
    let mut changed = input;
    changed.payload_limit_bytes = "256".into();
    assert_eq!(
        budget.configure(changed, &context).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    let mut held = budget.reserve("cas_capture", 64).unwrap();
    held.ensure(128).unwrap();
    assert_eq!(
        held.ensure(129).unwrap_err().code,
        "STORAGE_BUDGET_EXCEEDED"
    );
    drop(held);
    assert_eq!(budget.inspect(false).unwrap().reserved_bytes, "0");
}
#[test]
fn ambiguous_namespace_and_corrupt_journal_fail_closed() {
    use std::os::unix::fs::symlink;
    let d = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let budget = Budget::new(d.path());
    symlink(outside.path(), d.path().join("asset-store")).unwrap();
    assert_eq!(
        budget.reserve("cas_capture", 1).err().unwrap().code,
        "STORAGE_BUDGET_ERROR"
    );
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    fs::remove_file(d.path().join("asset-store")).unwrap();
    configure(d.path(), 128);
    let ledger = d.path().join(".storage-budget/ledger.json");
    fs::write(&ledger, b"{").unwrap();
    assert_eq!(
        budget.reserve("cas_capture", 1).err().unwrap().code,
        "STORAGE_BUDGET_ERROR"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn automatic_output_ceiling_keeps_room_for_its_own_durable_ledger() {
    use opencut_editor_application::budget::Writer;
    let root = tempfile::tempdir().unwrap();
    let image = root.path().join("volume.dmg");
    let mount = root.path().join("volume");
    fs::create_dir(&mount).unwrap();
    assert!(
        Command::new("hdiutil")
            .args([
                "create",
                "-size",
                "64m",
                "-fs",
                "HFS+",
                "-volname",
                "OpenCutCeilingQA"
            ])
            .arg(&image)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("hdiutil")
            .args(["attach", "-nobrowse", "-mountpoint"])
            .arg(&mount)
            .arg(&image)
            .status()
            .unwrap()
            .success()
    );
    struct Mounted(std::path::PathBuf);
    impl Drop for Mounted {
        fn drop(&mut self) {
            for _ in 0..10 {
                if Command::new("hdiutil")
                    .arg("detach")
                    .arg(&self.0)
                    .output()
                    .is_ok_and(|result| result.status.success())
                {
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
            panic!("could not detach test volume");
        }
    }
    let _mounted = Mounted(mount.clone());
    let budget = Budget::new(&mount);
    budget
        .configure(
            Configure {
                expected_revision: 0,
                idempotency_key: "physical-ceiling".into(),
                payload_limit_bytes: i64::MAX.to_string(),
                minimum_available_bytes: "0".into(),
                media_output_limit_bytes: Some(i64::MAX.to_string()),
                control_limit_bytes: None,
            },
            &ExecutionContext::local(CallSource::Cli),
        )
        .unwrap();
    let reservation = budget
        .reserve_output(Writer {
            job_id: "job_00000000000000000000000000000001".into(),
            attempt: 1,
        })
        .unwrap();
    assert!(reservation.reserved_bytes() > 0);
    assert!(reservation.reserved_bytes() < 64 << 20);
    assert!(budget.volume_available_bytes().unwrap() > reservation.reserved_bytes());
    drop(reservation);
    assert_eq!(budget.inspect(false).unwrap().reserved_bytes, "0");
}

#[test]
fn media_ceiling_shares_headroom_and_policy_changes_do_not_revoke_live_writers() {
    use opencut_editor_application::budget::Writer;
    let root = tempfile::tempdir().unwrap();
    let budget = Budget::new(root.path());
    let context = ExecutionContext::local(CallSource::Cli);
    budget
        .configure(
            Configure {
                expected_revision: 0,
                idempotency_key: "media".into(),
                payload_limit_bytes: "100".into(),
                minimum_available_bytes: "0".into(),
                media_output_limit_bytes: Some("80".into()),
                control_limit_bytes: None,
            },
            &context,
        )
        .unwrap();
    let writer = Writer {
        job_id: "job_00000000000000000000000000000001".into(),
        attempt: 1,
    };
    let first = budget.reserve_output(writer.clone()).unwrap();
    assert_eq!(first.reserved_bytes(), 80);
    let second = budget
        .reserve_output(Writer {
            attempt: 2,
            ..writer.clone()
        })
        .unwrap();
    assert_eq!(second.reserved_bytes(), 20);
    assert_eq!(
        budget.reserve_output(writer.clone()).err().unwrap().code,
        "STORAGE_BUDGET_EXCEEDED"
    );
    assert_eq!(
        budget
            .reserve_caption(1, writer.clone())
            .err()
            .unwrap()
            .code,
        "STORAGE_BUDGET_EXCEEDED"
    );
    budget
        .configure(
            Configure {
                expected_revision: 1,
                idempotency_key: "smaller".into(),
                payload_limit_bytes: "100".into(),
                minimum_available_bytes: "0".into(),
                media_output_limit_bytes: Some("10".into()),
                control_limit_bytes: None,
            },
            &context,
        )
        .unwrap();
    assert_eq!(first.reserved_bytes(), 80);
    assert_eq!(budget.inspect(true).unwrap().reserved_bytes, "100");
    drop(second);
    let replacement = budget.reserve_output(writer.clone()).unwrap();
    assert_eq!(replacement.reserved_bytes(), 10);
    let caption = budget.reserve_caption(10, writer.clone()).unwrap();
    assert_eq!(budget.inspect(false).unwrap().reserved_bytes, "100");
    assert!(
        budget
            .inspect(false)
            .unwrap()
            .reservations
            .iter()
            .all(|r| r.writer.as_ref() == Some(&writer))
    );
    drop((first, replacement, caption));
    assert_eq!(budget.inspect(false).unwrap().reserved_bytes, "0");
}

#[test]
fn control_ledger_is_independent_of_payload_and_recovers_a_killed_writer() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    configure(root, 20);
    fs::write(root.join("workspace.sqlite"), vec![0; 32]).unwrap();
    fs::write(root.join("workspace.sqlite-wal"), vec![0; 16]).unwrap();
    let budget = Budget::new(root);
    let mut control = budget
        .reserve_database_control("workspace.sqlite", 100)
        .unwrap();
    control.ensure(120).unwrap();
    let payload = budget.reserve("cas_capture", 20).unwrap();
    let report = budget.inspect(false).unwrap();
    assert_eq!(report.reserved_bytes, "20");
    let control_report = report.control.unwrap();
    assert_eq!(control_report.reserved_control_bytes, "120");
    assert!(
        control_report
            .observed_control_bytes
            .parse::<u64>()
            .unwrap()
            >= 48
    );
    assert!(!control_report.write_interception_enabled);
    assert_eq!(
        budget
            .reserve_database_control("other.sqlite", 1)
            .err()
            .unwrap()
            .code,
        "STORAGE_BUDGET_ERROR"
    );
    assert_eq!(
        budget
            .reserve_database_control("../workspace.sqlite", 1)
            .err()
            .unwrap()
            .code,
        "STORAGE_BUDGET_ERROR"
    );
    drop((control, payload));
    assert_eq!(
        budget
            .inspect(false)
            .unwrap()
            .control
            .unwrap()
            .reserved_control_bytes,
        "0"
    );
    let ledger: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join(".storage-budget/ledger.json")).unwrap())
            .unwrap();
    assert_eq!(ledger["version"], 3);
    assert_eq!(ledger["repair"]["state"], "ready");
    assert_eq!(ledger["repair"]["target_bytes"], "8388608");
    let allocated = fs::metadata(root.join(".storage-budget/repair.reserve")).unwrap();
    assert_eq!(allocated.len(), 8 * 1024 * 1024);
    use std::os::unix::fs::MetadataExt;
    assert!(allocated.blocks() * 512 >= allocated.len());
    assert_eq!(ledger["policy"]["payload_limit_bytes"], "20");
    assert_eq!(ledger["changes"][0]["input"]["idempotency_key"], "policy");
    assert_eq!(ledger["control_database_name"], "workspace.sqlite");
    assert!(ledger.get("control_held").is_none());

    let mut writer = control_child(root, "control");
    wait(&root.join("control.ready"));
    assert_eq!(
        fs::read_to_string(root.join("control.ready")).unwrap(),
        "held"
    );
    assert_eq!(
        budget
            .inspect(true)
            .unwrap()
            .control
            .unwrap()
            .reserved_control_bytes,
        "150"
    );
    writer.kill().unwrap();
    writer.wait().unwrap();
    assert_eq!(
        budget
            .inspect(false)
            .unwrap()
            .control
            .unwrap()
            .reserved_control_bytes,
        "150"
    );
    let recovered = budget.inspect(true).unwrap();
    assert_eq!(recovered.recovered_reservations, 1);
    assert_eq!(recovered.control.unwrap().reserved_control_bytes, "0");
}

#[test]
fn control_observation_rejects_ambiguous_sidecars() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    fs::write(elsewhere.path().join("wal"), b"outside").unwrap();
    symlink(
        elsewhere.path().join("wal"),
        root.path().join("workspace.sqlite-wal"),
    )
    .unwrap();
    let budget = Budget::new(root.path());
    assert_eq!(
        budget
            .reserve_database_control("workspace.sqlite", 1)
            .err()
            .unwrap()
            .code,
        "STORAGE_BUDGET_ERROR"
    );
    assert_eq!(fs::read(elsewhere.path().join("wal")).unwrap(), b"outside");
}

#[test]
fn owner_control_policy_is_idempotent_and_old_style_reconfigure_preserves_it() {
    let root = tempfile::tempdir().unwrap();
    let budget = Budget::new(root.path());
    let context = ExecutionContext::local(CallSource::Cli);
    let initial = budget.inspect(false).unwrap();
    assert!(initial.control.is_none());
    let first = Configure {
        expected_revision: 0,
        idempotency_key: "control-limit".into(),
        payload_limit_bytes: "20".into(),
        minimum_available_bytes: "0".into(),
        media_output_limit_bytes: None,
        control_limit_bytes: Some("4096".into()),
    };
    let changed = budget.configure(first.clone(), &context).unwrap();
    assert_eq!(changed.control_limit_bytes.as_deref(), Some("4096"));
    assert_eq!(budget.configure(first, &context).unwrap(), changed);
    let control = budget.inspect(false).unwrap().control.unwrap();
    assert_eq!(control.database_name, None);
    assert_eq!(control.control_limit_bytes, "4096");
    assert!(!control.write_interception_enabled);
    budget
        .configure(
            Configure {
                expected_revision: 1,
                idempotency_key: "payload-only".into(),
                payload_limit_bytes: "30".into(),
                minimum_available_bytes: "0".into(),
                media_output_limit_bytes: None,
                control_limit_bytes: None,
            },
            &context,
        )
        .unwrap();
    assert_eq!(
        budget
            .inspect(false)
            .unwrap()
            .control
            .unwrap()
            .control_limit_bytes,
        "4096"
    );
    assert_eq!(
        budget
            .reserve_database_control("workspace.sqlite", 4096)
            .err()
            .unwrap()
            .code,
        "DATABASE_CAPACITY_EXHAUSTED"
    );
    let payload = budget.reserve("cas_capture", 30).unwrap();
    drop(payload);
}
