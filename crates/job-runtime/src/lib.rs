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
            match publish_with_busy_retry(factory, &worker, &job, &result, &stop) {
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
    }
    impl ExecutorPort for Executor {
        fn clean_abandoned_inputs(&self) -> Result<(), EditorError> {
            Ok(())
        }
        fn execute(
            &self,
            _: &str,
            _: &JobRecord,
            _: Arc<AtomicBool>,
        ) -> Result<Value, EditorError> {
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
            };
            run(&store, &executor, true, Arc::new(AtomicBool::new(false))).unwrap();
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
        };
        run(&store, &executor, true, Arc::new(AtomicBool::new(true))).unwrap();
        assert_eq!(store.0.lock().unwrap().state, JobState::Queued);
    }
}
