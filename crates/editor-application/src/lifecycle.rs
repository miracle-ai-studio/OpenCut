//! Durable job transitions and atomic lifecycle ports. No storage or wall-clock I/O.
use opencut_api_contracts::{JobProgress, JobRecord, JobState};
use opencut_contracts::EditorError;
use serde_json::{Value, json};

/// Storage must sample this clock AFTER obtaining its serialization boundary.
/// It must use one sample for both transition validation and the resulting deadline.
pub trait JobClock {
    fn now_ms(&self) -> Result<i64, EditorError>;
}
/// Deterministic logical time for embedded callers and tests; not the native Worker clock.
pub struct FixedClock(pub i64);
impl JobClock for FixedClock {
    fn now_ms(&self) -> Result<i64, EditorError> {
        Ok(self.0)
    }
}

#[derive(Clone, Copy)]
pub struct LeaseIdentity<'a> {
    pub job_id: &'a str,
    pub worker: &'a str,
    pub attempt: u32,
}
pub enum WorkerUpdate {
    Heartbeat { lease_ms: i64 },
    Progress(JobProgress),
    Complete(Value),
    Fail(Value),
    Cancel,
}
pub enum JobAction<'a> {
    RequestCancel,
    /// Owner repair has fenced normal database connections and verified that
    /// the attempt has no live writer before abandoning its durable lease.
    RepairCancel,
    Claim {
        worker: &'a str,
        lease_ms: i64,
    },
    Worker {
        lease: LeaseIdentity<'a>,
        update: WorkerUpdate,
    },
    Recover,
}
pub struct EventDraft {
    pub event_type: &'static str,
    pub payload: Value,
}
pub struct Transition {
    pub record: JobRecord,
    pub event: Option<EventDraft>,
    pub changed: bool,
}

/// Aggregate persistence: read current state, sample clock, evaluate transition,
/// persist state AND event in one atomic boundary. A failed event write rolls
/// back the state. Recovery of a selected expired batch must also be atomic.
/// Event-producing transitions also persist trusted mutation provenance, linking
/// the original accepted request when known; never invent an actor for old jobs.
pub trait JobLifecycleStore {
    fn claim_atomic(
        &mut self,
        worker: &str,
        lease_ms: i64,
        clock: &dyn JobClock,
    ) -> Result<Option<JobRecord>, EditorError>;
    fn update_atomic(
        &mut self,
        job_id: &str,
        action: JobAction<'_>,
        clock: &dyn JobClock,
    ) -> Result<JobRecord, EditorError>;
    fn recover_atomic(&mut self, clock: &dyn JobClock) -> Result<usize, EditorError>;
}
pub struct JobService<'a, S, C> {
    store: &'a mut S,
    clock: C,
}
impl<'a, S: JobLifecycleStore, C: JobClock> JobService<'a, S, C> {
    pub fn new(store: &'a mut S, clock: C) -> Self {
        Self { store, clock }
    }
    pub fn claim(&mut self, worker: &str, lease_ms: i64) -> Result<Option<JobRecord>, EditorError> {
        validate_lease(worker, lease_ms)?;
        self.store.claim_atomic(worker, lease_ms, &self.clock)
    }
    pub fn cancel(&mut self, job_id: &str) -> Result<JobRecord, EditorError> {
        self.store
            .update_atomic(job_id, JobAction::RequestCancel, &self.clock)
    }
    pub fn repair_cancel(&mut self, job_id: &str) -> Result<JobRecord, EditorError> {
        self.store
            .update_atomic(job_id, JobAction::RepairCancel, &self.clock)
    }
    pub fn recover(&mut self) -> Result<usize, EditorError> {
        self.store.recover_atomic(&self.clock)
    }
    fn worker(
        &mut self,
        job_id: &str,
        worker: &str,
        attempt: u32,
        update: WorkerUpdate,
    ) -> Result<JobRecord, EditorError> {
        self.store.update_atomic(
            job_id,
            JobAction::Worker {
                lease: LeaseIdentity {
                    job_id,
                    worker,
                    attempt,
                },
                update,
            },
            &self.clock,
        )
    }
    pub fn heartbeat(
        &mut self,
        job_id: &str,
        worker: &str,
        attempt: u32,
        lease_ms: i64,
    ) -> Result<JobRecord, EditorError> {
        validate_lease(worker, lease_ms)?;
        self.worker(
            job_id,
            worker,
            attempt,
            WorkerUpdate::Heartbeat { lease_ms },
        )
    }
    pub fn progress(
        &mut self,
        job_id: &str,
        worker: &str,
        attempt: u32,
        progress: JobProgress,
    ) -> Result<JobRecord, EditorError> {
        validate_progress(&progress)?;
        self.worker(job_id, worker, attempt, WorkerUpdate::Progress(progress))
    }
    pub fn complete(
        &mut self,
        job_id: &str,
        worker: &str,
        attempt: u32,
        result: Value,
    ) -> Result<JobRecord, EditorError> {
        self.worker(job_id, worker, attempt, WorkerUpdate::Complete(result))
    }
    pub fn fail(
        &mut self,
        job_id: &str,
        worker: &str,
        attempt: u32,
        error: Value,
    ) -> Result<JobRecord, EditorError> {
        self.worker(job_id, worker, attempt, WorkerUpdate::Fail(error))
    }
    pub fn finish_cancelled(
        &mut self,
        job_id: &str,
        worker: &str,
        attempt: u32,
    ) -> Result<JobRecord, EditorError> {
        self.worker(job_id, worker, attempt, WorkerUpdate::Cancel)
    }
}
pub fn deadline(worker: &str, now_ms: i64, lease_ms: i64) -> Result<i64, EditorError> {
    validate_lease(worker, lease_ms)?;
    now_ms
        .checked_add(lease_ms)
        .ok_or_else(|| EditorError::new("INVALID_ARGUMENT", "lease timestamp overflowed"))
}
fn event(event_type: &'static str, payload: Value) -> Option<EventDraft> {
    Some(EventDraft {
        event_type,
        payload,
    })
}

/// One canonical transition function for every persistence adapter. It never
/// mutates input and cannot publish events; the adapter owns atomic publication.
pub fn transition(
    current: &JobRecord,
    action: JobAction<'_>,
    now_ms: i64,
) -> Result<Transition, EditorError> {
    let mut next = current.clone();
    let mut changed = true;
    let event = match action {
        JobAction::RequestCancel => match current.state {
            JobState::Queued => {
                next.state = JobState::Cancelled;
                next.cancel_requested = true;
                event("cancelled", json!({}))
            }
            JobState::Running if !current.cancel_requested => {
                next.cancel_requested = true;
                event("cancel_requested", json!({}))
            }
            _ => {
                changed = false;
                None
            }
        },
        JobAction::RepairCancel => {
            if current.state != JobState::Running {
                changed = false;
                None
            } else {
                next.state = JobState::Cancelled;
                next.cancel_requested = true;
                next.lease_owner = None;
                next.lease_until = None;
                next.result = None;
                next.error = None;
                event("cancelled", json!({"repair":true}))
            }
        }
        JobAction::Claim { worker, lease_ms } => {
            let until = deadline(worker, now_ms, lease_ms)?;
            if current.state != JobState::Queued || current.cancel_requested {
                return Err(EditorError::new(
                    "INVALID_JOB_STATE",
                    "job is not available for claiming",
                ));
            }
            next.attempt = current.attempt.checked_add(1).ok_or_else(|| {
                EditorError::new("INVALID_JOB_STATE", "job attempt counter exhausted")
            })?;
            next.state = JobState::Running;
            next.lease_owner = Some(worker.into());
            next.lease_until = Some(until);
            next.result = None;
            next.error = None;
            next.progress = None;
            event("running", json!({"worker":worker,"lease_until":until}))
        }
        JobAction::Recover => {
            if current.state != JobState::Running
                || current.lease_until.is_none_or(|until| until > now_ms)
            {
                changed = false;
                None
            } else {
                next.state = if current.cancel_requested {
                    JobState::Cancelled
                } else {
                    JobState::Queued
                };
                next.lease_owner = None;
                next.lease_until = None;
                event("recovered", json!({"recovered_at":now_ms}))
            }
        }
        JobAction::Worker { lease, update } => {
            if matches!(update, WorkerUpdate::Cancel) && !current.cancel_requested {
                return Err(EditorError::new(
                    "INVALID_JOB_STATE",
                    "job has no cancellation request",
                ));
            }
            if current.job_id != lease.job_id {
                return Err(EditorError::new(
                    "JOB_LEASE_LOST",
                    "lease belongs to another job",
                ));
            }
            require_active_lease(current, lease.worker, lease.attempt, now_ms)?;
            match update {
                WorkerUpdate::Heartbeat { lease_ms } => {
                    next.lease_until = Some(deadline(lease.worker, now_ms, lease_ms)?);
                    None
                }
                WorkerUpdate::Progress(progress) => {
                    validate_progress(&progress)?;
                    next.progress = Some(progress.clone());
                    event("progress", json!(progress))
                }
                // Import publishes a new project resource. Its contract gives an
                // already committed cancellation priority over publication.
                // Legacy render/probe jobs retain their cooperative v1 semantics.
                WorkerUpdate::Complete(_) | WorkerUpdate::Fail(_)
                    if current.cancel_requested
                        && (matches!(
                            current.kind.as_str(),
                            "render.project_audio.v4"
                                | "render.project_audio_range.v4"
                                | "render.project_frame.v4"
                        | "render.project_text_frame.v4"
                        ) || current.kind == "media.import"
                            && matches!(update, WorkerUpdate::Complete(_))) =>
                {
                    next.state = JobState::Cancelled;
                    next.result = None;
                    next.error = None;
                    next.lease_owner = None;
                    next.lease_until = None;
                    event("cancelled", json!({}))
                }
                WorkerUpdate::Complete(result) => {
                    next.state = JobState::Succeeded;
                    next.result = Some(result);
                    next.error = None;
                    next.lease_owner = None;
                    next.lease_until = None;
                    event("succeeded", json!({}))
                }
                WorkerUpdate::Fail(error) => {
                    next.state = JobState::Failed;
                    next.error = Some(error);
                    next.result = None;
                    next.lease_owner = None;
                    next.lease_until = None;
                    event("failed", json!({}))
                }
                WorkerUpdate::Cancel => {
                    next.state = JobState::Cancelled;
                    next.lease_owner = None;
                    next.lease_until = None;
                    event("cancelled", json!({}))
                }
            }
        }
    };
    Ok(Transition {
        record: next,
        event,
        changed,
    })
}

fn require_active_lease(
    job: &JobRecord,
    worker: &str,
    attempt: u32,
    now_ms: i64,
) -> Result<(), EditorError> {
    if job.state != JobState::Running
        || job.lease_owner.as_deref() != Some(worker)
        || job.attempt != attempt
        || job.lease_until.is_none_or(|until| until <= now_ms)
    {
        Err(EditorError::new(
            "JOB_LEASE_LOST",
            "worker no longer owns an active lease for this job attempt",
        ))
    } else {
        Ok(())
    }
}

fn validate_progress(progress: &JobProgress) -> Result<(), EditorError> {
    if progress.stage.trim().is_empty() || progress.stage.len() > 128 {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "progress stage must contain 1 to 128 bytes",
        ));
    }
    if let Some(fraction) = progress.fraction
        && (!fraction.is_finite() || !(0.0..=1.0).contains(&fraction))
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "progress fraction must be finite and between 0 and 1",
        ));
    }
    if let Some(media_time) = &progress.media_time
        && (media_time.timebase == 0 || media_time.ticks.parse::<i64>().is_err())
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "progress media_time must use i64 ticks and a positive timebase",
        ));
    }
    Ok(())
}

fn validate_lease(worker: &str, lease_ms: i64) -> Result<(), EditorError> {
    if worker.trim().is_empty() {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "worker identity must not be empty",
        ));
    }
    if lease_ms <= 0 {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "lease duration must be positive",
        ));
    }
    Ok(())
}
