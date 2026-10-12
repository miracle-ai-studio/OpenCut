//! Durable worker scheduling through injected storage and execution protocols.
//! Native paths, media backends and resource admission belong to host adapters.
use opencut_contracts::EditorError;
use opencut_editor_application::{JobRecord, JobState};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
pub const LEASE_MS: i64 = 30_000;
const PUBLICATION_BUSY_RUNWAY_MS: u64 = 5_000;

/// Each connection must preserve atomic attempt/lease checks on every mutation.
pub trait JobStore {
    fn recover(&mut self) -> Result<usize, EditorError>;
    fn claim(&mut self, worker: &str, lease_ms: i64) -> Result<Option<JobRecord>, EditorError>;
    fn get_job(&self, id: &str) -> Result<JobRecord, EditorError>;
    fn complete(
        &mut self,
        id: &str,
        worker: &str,
        attempt: u32,
        value: Value,
    ) -> Result<JobRecord, EditorError>;
    fn fail(
        &mut self,
        id: &str,
        worker: &str,
        attempt: u32,
        error: Value,
    ) -> Result<JobRecord, EditorError>;
    fn finish_cancelled(
        &mut self,
        id: &str,
        worker: &str,
        attempt: u32,
    ) -> Result<JobRecord, EditorError>;
}
pub trait JobStoreFactory {
    fn open(&self) -> Result<Box<dyn JobStore>, EditorError>;
}
/// Executor owns monitoring/progress and private attempt resources. Returning
/// success never publishes: the scheduler rechecks durable lease ownership.
pub trait ExecutorPort {
    fn clean_abandoned_inputs(&self) -> Result<(), EditorError>;
    /// Release resources held through publication, including failed fences.
    fn publication_finished(&self) {}
    fn execute(
        &self,
        worker: &str,
        job: &JobRecord,
        stop: Arc<AtomicBool>,
    ) -> Result<Value, EditorError>;
}
pub enum Publication {
    Published,
    AlreadyTerminal,
    LostLease,
    HostShutdown,
}

pub fn publish_with_busy_retry(
    factory: &impl JobStoreFactory,
    worker: &str,
    job: &JobRecord,
    result: &Result<Value, EditorError>,
    stop: &AtomicBool,
) -> Result<Publication, EditorError> {
    let deadline =
        Instant::now() + Duration::from_millis(LEASE_MS as u64 - PUBLICATION_BUSY_RUNWAY_MS);
    loop {
        // A BUSY result may leave the previous connection's pager in a failed
        // transaction. Reopen before inspecting the durable state or retrying
        // the attempt, including when a commit's outcome was not observed.
        let publication: Result<Publication, EditorError> = (|| {
            let mut app = factory.open()?;
            let state = app.get_job(&job.job_id)?;
            if state.state != JobState::Running {
                return Ok(
                    if state.attempt == job.attempt
                        && matches!(
                            state.state,
                            JobState::Succeeded | JobState::Failed | JobState::Cancelled
                        )
                    {
                        Publication::AlreadyTerminal
                    } else {
                        Publication::LostLease
                    },
                );
            }
            if state.lease_owner.as_deref() != Some(worker) || state.attempt != job.attempt {
                return Ok(Publication::LostLease);
            }
            if stop.load(Ordering::SeqCst) && !state.cancel_requested {
                return Ok(Publication::HostShutdown);
            }
            match result {
                Ok(value) => app.complete(&job.job_id, worker, job.attempt, value.clone())?,
                Err(_) if state.cancel_requested => {
                    app.finish_cancelled(&job.job_id, worker, job.attempt)?
                }
                Err(error) => app.fail(&job.job_id, worker, job.attempt, json!(error))?,
            };
            Ok(Publication::Published)
        })();
        match publication {
            // The atomic publication check can lose a lease after the earlier
            // observation (including expiration with the same owner/attempt).
            // This is a normal fence outcome, not a broken storage host.
            Err(error) if error.code == "JOB_LEASE_LOST" => return Ok(Publication::LostLease),
            Err(error) if error.code == "DATABASE_BUSY" && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(100));
            }
            other => return other,
        }
    }
}

pub fn run(
    factory: &impl JobStoreFactory,
    executor: &impl ExecutorPort,
    once: bool,
    stop: Arc<AtomicBool>,
) -> Result<(), EditorError> {
    let mut app = factory.open()?;
    let worker = format!("worker_{}", uuid::Uuid::new_v4().simple());
    let mut initial_cleanup = true;
    loop {
        if stop.load(Ordering::SeqCst) {
            return Ok(());
        }
        let recovered = match app.recover() {
            Ok(recovered) => recovered,
            Err(error) if error.code == "DATABASE_BUSY" => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(error) => return Err(error),
        };
        if initial_cleanup || recovered > 0 {
            if let Err(error) = executor.clean_abandoned_inputs() {
                eprintln!("Could not clean abandoned private inputs: {error}");
            }
            initial_cleanup = false;
        }
        let claimed = match app.claim(&worker, LEASE_MS) {
            Ok(claimed) => claimed,
            Err(error) if error.code == "DATABASE_BUSY" => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(error) => return Err(error),
        };
        if let Some(job) = claimed {
            let result = executor.execute(&worker, &job, stop.clone());
            let capacity_fault = result.as_ref().err().is_some_and(|error| {
                matches!(
                    error.code.as_str(),
                    "DATABASE_CAPACITY_EXHAUSTED" | "STORAGE_CAPACITY_LOW"
                )
            });
            let publication = publish_with_busy_retry(factory, &worker, &job, &result, &stop);
            executor.publication_finished();
            match publication {
                Ok(Publication::Published | Publication::AlreadyTerminal) => (),
                Ok(Publication::HostShutdown) => return Ok(()),
                Ok(Publication::LostLease) => eprintln!(
                    "Job {} attempt {} lost its lease; private output was not published",
                    job.job_id, job.attempt
                ),
                Err(error) if error.code == "DATABASE_BUSY" && !capacity_fault && !once => {
                    eprintln!(
                        "Could not publish job {} before the lease retry window ended: {error}; awaiting durable lease recovery",
                        job.job_id
                    );
                    continue;
                }
                Err(error) => {
                    eprintln!("Could not publish job {}: {error}", job.job_id);
                    // A worker that cannot persist its outcome must release all
                    // managed SQLite connections so the owner can enter repair.
                    return Err(error);
                }
            }
            if capacity_fault {
                return Err(EditorError::new(
                    "DATABASE_CAPACITY_EXHAUSTED",
                    "Worker stopped after storage capacity failure; owner repair may be required",
                ));
            }
            if once {
                return Ok(());
            }
        } else {
            if once {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

#[cfg(test)]
mod port_tests {
    use super::*;
    use std::sync::Mutex;
    #[derive(Clone)]
    struct Memory(Arc<Mutex<JobRecord>>);
    impl JobStoreFactory for Memory {
        fn open(&self) -> Result<Box<dyn JobStore>, EditorError> {
            Ok(Box::new(self.clone()))
        }
    }
    impl JobStore for Memory {
        fn recover(&mut self) -> Result<usize, EditorError> {
            Ok(0)
        }
        fn claim(&mut self, worker: &str, _: i64) -> Result<Option<JobRecord>, EditorError> {
            let mut job = self.0.lock().unwrap();
            if job.state != JobState::Queued {
                return Ok(None);
            }
            job.state = JobState::Running;
            job.attempt += 1;
            job.lease_owner = Some(worker.into());
            Ok(Some(job.clone()))
        }
        fn get_job(&self, _: &str) -> Result<JobRecord, EditorError> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn complete(
            &mut self,
            _: &str,
            worker: &str,
            attempt: u32,
            value: Value,
        ) -> Result<JobRecord, EditorError> {
            let mut job = self.0.lock().unwrap();
            assert_eq!(job.lease_owner.as_deref(), Some(worker));
            assert_eq!(job.attempt, attempt);
            job.state = JobState::Succeeded;
            job.result = Some(value);
            Ok(job.clone())
        }
        fn fail(
            &mut self,
            _: &str,
            _: &str,
            _: u32,
            error: Value,
        ) -> Result<JobRecord, EditorError> {
            let mut j = self.0.lock().unwrap();
            j.state = JobState::Failed;
            j.error = Some(error);
            Ok(j.clone())
        }
        fn finish_cancelled(&mut self, _: &str, _: &str, _: u32) -> Result<JobRecord, EditorError> {
            let mut j = self.0.lock().unwrap();
            j.state = JobState::Cancelled;
            Ok(j.clone())
        }
    }
    struct Executor {
        store: Memory,
        steal: bool,
        released: AtomicBool,
    }
    impl ExecutorPort for Executor {
        fn publication_finished(&self) {
            self.released.store(true, Ordering::SeqCst);
        }
        fn clean_abandoned_inputs(&self) -> Result<(), EditorError> {
            Ok(())
        }
        fn execute(
            &self,
            _: &str,
            _: &JobRecord,
            _: Arc<AtomicBool>,
        ) -> Result<Value, EditorError> {
            assert!(!self.released.load(Ordering::SeqCst));
            if self.steal {
                let mut j = self.store.0.lock().unwrap();
                j.attempt += 1;
                j.lease_owner = Some("replacement".into());
            }
            Ok(json!({"answer":42}))
        }
    }
    fn memory() -> Memory {
        Memory(Arc::new(Mutex::new(JobRecord {
            job_id: "j".into(),
            state: JobState::Queued,
            attempt: 0,
            kind: "fake".into(),
            input: Value::Null,
            result: None,
            error: None,
            lease_owner: None,
            lease_until: None,
            cancel_requested: false,
            progress: None,
        })))
    }
    #[test]
    fn injected_executor_publishes_once_and_cannot_publish_after_lease_replacement() {
        for steal in [false, true] {
            let store = memory();
            let executor = Executor {
                store: store.clone(),
                steal,
                released: AtomicBool::new(false),
            };
            run(&store, &executor, true, Arc::new(AtomicBool::new(false))).unwrap();
            assert!(executor.released.load(Ordering::SeqCst));
            let job = store.0.lock().unwrap();
            if steal {
                assert_eq!(job.state, JobState::Running);
                assert!(job.result.is_none());
            } else {
                assert_eq!(job.state, JobState::Succeeded);
                assert_eq!(job.result, Some(json!({"answer":42})));
            }
        }
    }
    #[test]
    fn shutdown_before_claim_has_no_execution_or_publication() {
        let store = memory();
        let executor = Executor {
            store: store.clone(),
            steal: false,
            released: AtomicBool::new(false),
        };
        run(&store, &executor, true, Arc::new(AtomicBool::new(true))).unwrap();
        assert!(!executor.released.load(Ordering::SeqCst));
        assert_eq!(store.0.lock().unwrap().state, JobState::Queued);
    }
    #[derive(Clone)]
    struct FencedStore {
        inner: Memory,
        code: &'static str,
    }
    impl JobStoreFactory for FencedStore {
        fn open(&self) -> Result<Box<dyn JobStore>, EditorError> {
            Ok(Box::new(self.clone()))
        }
    }
    impl JobStore for FencedStore {
        fn recover(&mut self) -> Result<usize, EditorError> {
            self.inner.recover()
        }
        fn claim(&mut self, w: &str, l: i64) -> Result<Option<JobRecord>, EditorError> {
            self.inner.claim(w, l)
        }
        fn get_job(&self, id: &str) -> Result<JobRecord, EditorError> {
            self.inner.get_job(id)
        }
        fn complete(
            &mut self,
            _: &str,
            _: &str,
            _: u32,
            _: Value,
        ) -> Result<JobRecord, EditorError> {
            Err(EditorError::new(self.code, "atomic publication rejected"))
        }
        fn fail(&mut self, _: &str, _: &str, _: u32, _: Value) -> Result<JobRecord, EditorError> {
            Err(EditorError::new(self.code, "atomic publication rejected"))
        }
        fn finish_cancelled(&mut self, _: &str, _: &str, _: u32) -> Result<JobRecord, EditorError> {
            Err(EditorError::new(self.code, "atomic publication rejected"))
        }
    }
    #[test]
    fn expiration_between_observation_and_publication_is_a_normal_fence_for_every_outcome() {
        for outcome in [
            Ok(json!({"answer":42})),
            Err(EditorError::new("EXECUTION_FAILED", "failed")),
        ] {
            for cancelled in [false, true] {
                let inner = memory();
                let job = inner.clone().claim("worker", LEASE_MS).unwrap().unwrap();
                inner.0.lock().unwrap().cancel_requested = cancelled;
                let store = FencedStore {
                    inner: inner.clone(),
                    code: "JOB_LEASE_LOST",
                };
                assert!(matches!(
                    publish_with_busy_retry(
                        &store,
                        "worker",
                        &job,
                        &outcome,
                        &AtomicBool::new(false)
                    )
                    .unwrap(),
                    Publication::LostLease
                ));
                let state = inner.0.lock().unwrap();
                assert_eq!(state.state, JobState::Running);
                assert!(state.result.is_none());
            }
        }
        let inner = memory();
        run(
            &FencedStore {
                inner: inner.clone(),
                code: "JOB_LEASE_LOST",
            },
            &Executor {
                store: inner.clone(),
                steal: false,
                released: AtomicBool::new(false),
            },
            true,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert!(inner.0.lock().unwrap().result.is_none());
    }
    #[test]
    fn real_capacity_faults_are_not_suppressed_as_lease_loss() {
        let inner = memory();
        let store = FencedStore {
            inner: inner.clone(),
            code: "DATABASE_CAPACITY_EXHAUSTED",
        };
        let error = run(
            &store,
            &Executor {
                store: inner.clone(),
                steal: false,
                released: AtomicBool::new(false),
            },
            true,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap_err();
        assert_eq!(error.code, "DATABASE_CAPACITY_EXHAUSTED");
        assert!(inner.0.lock().unwrap().result.is_none());
    }
}
