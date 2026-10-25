use crate::{EditorApplication, JobProgress, JobRecord, JobRequest, JobState};
use opencut_contracts::*;
use opencut_editor_application::lifecycle::*;
use serde_json::json;
use std::cell::RefCell;
use std::sync::{
    Arc,
    atomic::{AtomicI64, AtomicUsize, Ordering},
    mpsc,
};
use std::time::Duration;

fn enqueue(app: &mut EditorApplication, key: &str) -> JobRecord {
    app.create_job(JobRequest {
        idempotency_key: key.into(),
        kind: "media.probe".into(),
        input: json!({"input":"qa.wav"}),
    })
    .unwrap()
}
#[test]
fn queued_jobs_with_equal_timestamp_keep_admission_order_instead_of_uuid_order() {
    let mut app=EditorApplication::open(":memory:").unwrap();
    let expected=(0..32).map(|i|enqueue(&mut app,&format!("nearby-{i}"))).collect::<Vec<_>>();
    for job in expected {
        assert_eq!(app.claim_job("worker",0,30_000).unwrap().unwrap().job_id,job.job_id);
    }
}
fn sample() -> JobProgress {
    JobProgress {
        stage: "encoding".into(),
        fraction: Some(0.5),
        media_time: None,
    }
}
#[test]
fn preferred_claim_keeps_eligibility_lease_and_original_fifo_order() {
    let mut app=EditorApplication::open(":memory:").unwrap();
    let old=enqueue(&mut app,"old-background");
    let cancelled=enqueue(&mut app,"cancelled-preview");
    let current=enqueue(&mut app,"current-preview");
    app.cancel_job(&cancelled.job_id).unwrap();
    let preferred=vec![cancelled.job_id.clone(),current.job_id.clone()];
    let first=app.live_jobs().claim_preferred("foreground",30_000,&preferred).unwrap().unwrap();
    assert_eq!(first.job_id,current.job_id);assert_eq!(first.attempt,1);
    assert_eq!(first.lease_owner.as_deref(),Some("foreground"));
    let next=app.live_jobs().claim_preferred("other-worker",30_000,&preferred).unwrap().unwrap();
    assert_eq!(next.job_id,old.job_id);
    assert_eq!(app.get_job(&current.job_id).unwrap().lease_owner.as_deref(),Some("foreground"));
    assert!(app.live_jobs().claim_preferred("other-worker",30_000,&preferred).unwrap().is_none());
}
thread_local! { static BUSY_NOTICE: RefCell<Option<mpsc::Sender<()>>> = const { RefCell::new(None) }; }
fn busy(attempt: i32) -> bool {
    BUSY_NOTICE.with(|notice| {
        if let Some(sender) = notice.borrow_mut().take() {
            let _ = sender.send(());
        }
    });
    std::thread::sleep(Duration::from_millis(1));
    attempt < 5000
}
struct ObservedClock {
    now: Arc<AtomicI64>,
    reads: Arc<AtomicUsize>,
}
impl JobClock for ObservedClock {
    fn now_ms(&self) -> Result<i64, EditorError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(self.now.load(Ordering::SeqCst))
    }
}
#[test]
fn every_worker_mutation_checks_time_after_the_sqlite_write_lock() {
    for operation in 0..5 {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db.sqlite");
        let mut owner = EditorApplication::open(&path).unwrap();
        let job = enqueue(&mut owner, "job");
        owner.claim_job("worker", 0, 10).unwrap().unwrap();
        if operation == 4 {
            owner.cancel_job(&job.job_id).unwrap();
        }
        let mut contender = EditorApplication::open(&path).unwrap();
        let expected = owner.get_job(&job.job_id).unwrap();
        let events = owner.job_events_after(0, 100, None).unwrap();
        let now = Arc::new(AtomicI64::new(1));
        let reads = Arc::new(AtomicUsize::new(0));
        let clock = ObservedClock {
            now: now.clone(),
            reads: reads.clone(),
        };
        let (sender, receiver) = mpsc::channel();
        contender.connection.busy_handler(Some(busy)).unwrap();
        let tx = owner
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let id = job.job_id.clone();
        let handle = std::thread::spawn(move || {
            BUSY_NOTICE.with(|notice| *notice.borrow_mut() = Some(sender));
            let mut service = JobService::new(&mut contender.connection, clock);
            match operation {
                0 => service.heartbeat(&id, "worker", 1, 10),
                1 => service.progress(&id, "worker", 1, sample()),
                2 => service.complete(&id, "worker", 1, json!({"bad":true})),
                3 => service.fail(&id, "worker", 1, json!({"bad":true})),
                _ => service.finish_cancelled(&id, "worker", 1),
            }
        });
        receiver.recv_timeout(Duration::from_secs(5)).unwrap(); // proves the same live operation is waiting on SQLite
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        now.store(10, Ordering::SeqCst); // equality is expired, not one millisecond later
        tx.commit().unwrap();
        assert_eq!(handle.join().unwrap().unwrap_err().code, "JOB_LEASE_LOST");
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        let independent = EditorApplication::open(&path).unwrap();
        assert_eq!(independent.get_job(&job.job_id).unwrap(), expected);
        assert_eq!(independent.job_events_after(0, 100, None).unwrap(), events);
    }
}
fn reject(app: &EditorApplication, event: &str) {
    app.connection.execute_batch(&format!("CREATE TRIGGER reject_event BEFORE INSERT ON job_events WHEN NEW.event_type='{event}' BEGIN SELECT RAISE(ABORT,'injected event failure'); END;")).unwrap();
}
fn allow(app: &EditorApplication) {
    app.connection
        .execute_batch("DROP TRIGGER reject_event")
        .unwrap();
}
#[test]
fn state_and_events_roll_back_together_for_every_lifecycle_transition() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite");
    let mut app = EditorApplication::open(&path).unwrap();
    let job = enqueue(&mut app, "job");
    reject(&app, "running");
    assert_eq!(
        app.claim_job("w", 0, 100).unwrap_err().code,
        "STORAGE_ERROR"
    );
    assert_eq!(
        EditorApplication::open(&path)
            .unwrap()
            .get_job(&job.job_id)
            .unwrap(),
        job
    );
    allow(&app);
    app.claim_job("w", 0, 100).unwrap();
    for event in ["progress", "cancel_requested", "succeeded", "failed"] {
        let before = app.get_job(&job.job_id).unwrap();
        let events = app.job_events_after(0, 100, None).unwrap();
        reject(&app, event);
        let result = match event {
            "progress" => app.update_job_progress(&job.job_id, "w", 1, 1, sample()),
            "cancel_requested" => app.cancel_job(&job.job_id),
            "succeeded" => app.complete_job(&job.job_id, "w", 1, 1, json!({"ok":true})),
            _ => app.fail_job(&job.job_id, "w", 1, 1, json!({"code":"INTERNAL_ERROR"})),
        };
        assert_eq!(result.unwrap_err().code, "STORAGE_ERROR");
        let other = EditorApplication::open(&path).unwrap();
        assert_eq!(other.get_job(&job.job_id).unwrap(), before);
        assert_eq!(other.job_events_after(0, 100, None).unwrap(), events);
        allow(&app);
    }
    app.cancel_job(&job.job_id).unwrap();
    let before = app.get_job(&job.job_id).unwrap();
    reject(&app, "cancelled");
    assert_eq!(
        app.finish_cancelled(&job.job_id, "w", 1, 1)
            .unwrap_err()
            .code,
        "STORAGE_ERROR"
    );
    assert_eq!(
        EditorApplication::open(&path)
            .unwrap()
            .get_job(&job.job_id)
            .unwrap(),
        before
    );
    allow(&app);
    assert_eq!(
        app.finish_cancelled(&job.job_id, "w", 1, 1).unwrap().state,
        JobState::Cancelled
    );
}
#[test]
fn recovery_batch_rolls_back_even_after_an_earlier_recovery_event_was_written() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite");
    let mut app = EditorApplication::open(&path).unwrap();
    let first = enqueue(&mut app, "one");
    let second = enqueue(&mut app, "two");
    app.claim_job("w1", 0, 10).unwrap();
    app.claim_job("w2", 0, 10).unwrap();
    app.cancel_job(&first.job_id).unwrap();
    let records = app.list_jobs().unwrap();
    let events = app.job_events_after(0, 100, None).unwrap();
    app.connection.execute_batch("CREATE TRIGGER reject_second BEFORE INSERT ON job_events WHEN NEW.event_type='recovered' AND (SELECT COUNT(*) FROM job_events WHERE event_type='recovered')=1 BEGIN SELECT RAISE(ABORT,'fail second recovery'); END;").unwrap();
    assert_eq!(app.recover_expired(10).unwrap_err().code, "STORAGE_ERROR");
    let other = EditorApplication::open(&path).unwrap();
    assert_eq!(other.list_jobs().unwrap(), records);
    assert_eq!(other.job_events_after(0, 100, None).unwrap(), events);
    app.connection
        .execute_batch("DROP TRIGGER reject_second")
        .unwrap();
    assert_eq!(app.recover_expired(10).unwrap(), 2);
    assert_eq!(app.recover_expired(10).unwrap(), 0);
    assert_eq!(
        app.get_job(&first.job_id).unwrap().state,
        JobState::Cancelled
    );
    assert_eq!(app.get_job(&second.job_id).unwrap().state, JobState::Queued);
    assert_eq!(app.claim_job("new", 11, 10).unwrap().unwrap().attempt, 2);
}
#[test]
fn cancellation_is_cooperative_and_terminal_records_are_immutable() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let job = enqueue(&mut app, "race");
    app.claim_job("w", 0, 100).unwrap();
    app.cancel_job(&job.job_id).unwrap();
    // A successful backend may finish before observing cancellation. Preserve v1 semantics.
    let completed = app
        .complete_job(&job.job_id, "w", 1, 1, json!({"ok":true}))
        .unwrap();
    assert_eq!(completed.state, JobState::Succeeded);
    assert!(completed.cancel_requested);
    assert_eq!(app.cancel_job(&job.job_id).unwrap(), completed);
    assert_eq!(
        app.finish_cancelled(&job.job_id, "w", 1, 2)
            .unwrap_err()
            .code,
        "JOB_LEASE_LOST"
    );
    assert_eq!(app.get_job(&job.job_id).unwrap(), completed);
}

#[cfg(unix)]
#[test]
fn owner_repair_cannot_cancel_a_live_writer_but_fences_an_abandoned_attempt() {
    use opencut_editor_application::{
        audit::{CallSource, ExecutionContext},
        budget::{BudgetNative, RepairAction, RepairRequest},
    };
    use opencut_storage_budget::{AttemptLease, Budget};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace.sqlite");
    let mut worker = EditorApplication::open_managed(&path).unwrap();
    let job = enqueue(&mut worker, "repair-running");
    let claimed = worker.claim_job("worker", 0, 30_000).unwrap().unwrap();
    let attempt = dir
        .path()
        .join("artifacts")
        .join(&job.job_id)
        .join(format!("attempt-{}", claimed.attempt));
    std::fs::create_dir_all(&attempt).unwrap();
    let live_writer = AttemptLease::acquire(&attempt).unwrap();
    assert_eq!(
        worker.repair_cancel_job(&job.job_id).unwrap_err().code,
        "CAPABILITY_UNAVAILABLE"
    );
    drop(worker);

    let budget = Budget::new(dir.path());
    let context = ExecutionContext::local(CallSource::Cli);
    budget
        .repair_transition(
            RepairAction::Enter,
            RepairRequest {
                expected_revision: 0,
                idempotency_key: "enter".into(),
            },
            &context,
        )
        .unwrap();
    let mut repair = EditorApplication::open_managed_repair(&path).unwrap();
    assert_eq!(
        repair.repair_cancel_job(&job.job_id).unwrap_err().code,
        "STORAGE_BUDGET_BUSY"
    );
    assert_eq!(
        repair.get_job(&job.job_id).unwrap().state,
        JobState::Running
    );
    drop(live_writer);
    let cancelled = repair.repair_cancel_job(&job.job_id).unwrap();
    assert_eq!(cancelled.state, JobState::Cancelled);
    assert!(cancelled.cancel_requested);
    assert!(cancelled.lease_owner.is_none());
    assert!(cancelled.lease_until.is_none());
    assert_eq!(repair.repair_cancel_job(&job.job_id).unwrap(), cancelled);
    let events = repair.job_events_after(0, 100, Some(&job.job_id)).unwrap();
    assert_eq!(events.last().unwrap().event_type, "cancelled");
    drop(repair);
    budget
        .repair_transition(
            RepairAction::Exit,
            RepairRequest {
                expected_revision: 1,
                idempotency_key: "exit".into(),
            },
            &context,
        )
        .unwrap();
    let mut reopened = EditorApplication::open_managed(&path).unwrap();
    assert_eq!(reopened.get_job(&job.job_id).unwrap(), cancelled);
    assert_eq!(
        reopened
            .fail_job(
                &job.job_id,
                "worker",
                claimed.attempt,
                1,
                json!({"late":true})
            )
            .unwrap_err()
            .code,
        "JOB_LEASE_LOST"
    );
}
#[test]
fn a_lease_identity_cannot_be_used_for_another_job_with_the_same_worker_and_attempt() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    enqueue(&mut app, "one");
    enqueue(&mut app, "two");
    let first = app.claim_job("w", 0, 100).unwrap().unwrap();
    let second = app.claim_job("w", 0, 100).unwrap().unwrap();
    let result = transition(
        &second,
        JobAction::Worker {
            lease: LeaseIdentity {
                job_id: &first.job_id,
                worker: "w",
                attempt: 1,
            },
            update: WorkerUpdate::Complete(json!({})),
        },
        1,
    );
    assert!(matches!(result,Err(EditorError {code,..}) if code=="JOB_LEASE_LOST"));
}
