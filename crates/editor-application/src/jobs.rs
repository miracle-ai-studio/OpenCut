use crate::{JobRecord, JobRequest};
use opencut_contracts::AssetLockManifest;
use opencut_contracts::{EditorError, ProjectSnapshot, Time};
pub use opencut_job_contracts::{
    ExportJobInput, FrameJobInput, JobInput, JobStartInput, ProjectExportInput, ProjectFrameInput,
    ProjectRangeInput,
};
use opencut_job_execution::{
    PreparedProjectFrameInput, PreparedProjectRangeInput, PreparedProjectRenderInput,
};
use serde::Serialize;
use serde_json::json;

/// All persistence methods must meet the atomicity requirements of JobSubmissionStore.
/// Asset capture occurs outside the store transaction and may leave unreferenced CAS
/// bytes after a conflict; it must never publish an accepted task on its own.
pub fn submit(
    app: &mut impl JobSubmissionStore,
    assets: &impl JobAssetCapture,
    request: JobStartInput,
) -> Result<JobRecord, EditorError> {
    let (idempotency_key, kind, input) = match request {
        JobStartInput::Probe {
            idempotency_key,
            input,
        } => (idempotency_key, "media.probe", json!(input)),
        JobStartInput::Frame {
            idempotency_key,
            input,
        } => (idempotency_key, "preview.frame", json!(input)),
        JobStartInput::ProjectFrame {
            idempotency_key,
            input,
        } => {
            validate_requested_time(&input.requested_time)?;
            let snapshot =
                fixed_snapshot(app, &input.project_id, &input.revision, &input.sequence_id)?;
            let prepared = PreparedProjectFrameInput {
                snapshot,
                sequence_id: input.sequence_id,
                requested_time: input.requested_time,
                alignment: input.alignment,
                asset_lock: None,
            };
            return create_locked_project_job(
                app,
                assets,
                idempotency_key,
                "preview.project_frame",
                prepared,
                AssetScope::All,
            );
        }
        JobStartInput::CompositedFrame {
            idempotency_key,
            input,
        } => {
            validate_requested_time(&input.requested_time)?;
            let snapshot =
                fixed_snapshot(app, &input.project_id, &input.revision, &input.sequence_id)?;
            let prepared = PreparedProjectFrameInput {
                snapshot,
                sequence_id: input.sequence_id,
                requested_time: input.requested_time,
                alignment: input.alignment,
                asset_lock: None,
            };
            return create_locked_project_job(
                app,
                assets,
                idempotency_key,
                "preview.composited_frame",
                prepared,
                AssetScope::Visual,
            );
        }
        JobStartInput::ProjectRange {
            idempotency_key,
            input,
        } => {
            validate_requested_range(&input.requested_range)?;
            let snapshot =
                fixed_snapshot(app, &input.project_id, &input.revision, &input.sequence_id)?;
            let prepared = PreparedProjectRangeInput {
                snapshot,
                sequence_id: input.sequence_id,
                requested_range: input.requested_range,
                alignment: input.alignment,
                asset_lock: None,
            };
            return create_locked_project_job(
                app,
                assets,
                idempotency_key,
                "preview.project_range",
                prepared,
                AssetScope::All,
            );
        }
        JobStartInput::Export {
            idempotency_key,
            input,
        } => (idempotency_key, "render.export", json!(input)),
        JobStartInput::Verify {
            idempotency_key,
            input,
        } => (idempotency_key, "artifact.verify", json!(input)),
        JobStartInput::Project {
            idempotency_key,
            input,
        } => {
            let snapshot =
                fixed_snapshot(app, &input.project_id, &input.revision, &input.sequence_id)?;
            let prepared = PreparedProjectRenderInput {
                snapshot,
                sequence_id: input.sequence_id,
                container: input.container,
                video_codec: input.video_codec,
                asset_lock: None,
            };
            return create_locked_project_job(
                app,
                assets,
                idempotency_key,
                "render.project",
                prepared,
                AssetScope::All,
            );
        }
    };
    app.accept_job(
        JobRequest {
            idempotency_key,
            kind: kind.into(),
            input: input.clone(),
        },
        input,
    )
}

/// Internal application port. It does not expose SQL, connections or independent
/// writes of jobs, idempotency receipts and queued events.
pub trait JobSubmissionStore {
    /// Return an immutable revision belonging to the requested project.
    fn fixed_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<ProjectSnapshot, EditorError>;
    /// Check the public identity before expensive preparation. A receipt must
    /// return the current record of the original task; key reuse must fail.
    fn find_accepted_job(&self, request: &JobRequest) -> Result<Option<JobRecord>, EditorError>;
    /// Atomically recheck idempotency, insert execution input, receipt and queued
    /// event. On any error none of these writes may become visible. A concurrent
    /// identical acceptance returns the original task; it must not replace its
    /// execution input, even if this caller prepared different local bytes.
    fn accept_job(
        &mut self,
        request: JobRequest,
        execution_input: serde_json::Value,
    ) -> Result<JobRecord, EditorError>;
}

/// Preparation capability: produces a durable lock for exactly the fixed
/// revision, sequence and required asset IDs. No queue write is permitted here.
pub trait JobAssetCapture {
    fn capture(
        &self,
        snapshot: &ProjectSnapshot,
        sequence_id: &str,
        asset_ids: &[String],
    ) -> Result<AssetLockManifest, EditorError>;
}

trait PreparedProjectInput: Clone + Serialize {
    fn snapshot(&self) -> &ProjectSnapshot;
    fn sequence_id(&self) -> &str;
    fn set_asset_lock(&mut self, asset_lock: AssetLockManifest);
}

impl PreparedProjectInput for PreparedProjectRenderInput {
    fn snapshot(&self) -> &ProjectSnapshot {
        &self.snapshot
    }

    fn sequence_id(&self) -> &str {
        &self.sequence_id
    }

    fn set_asset_lock(&mut self, asset_lock: AssetLockManifest) {
        self.asset_lock = Some(asset_lock);
    }
}

impl PreparedProjectInput for PreparedProjectFrameInput {
    fn snapshot(&self) -> &ProjectSnapshot {
        &self.snapshot
    }

    fn sequence_id(&self) -> &str {
        &self.sequence_id
    }

    fn set_asset_lock(&mut self, asset_lock: AssetLockManifest) {
        self.asset_lock = Some(asset_lock);
    }
}

impl PreparedProjectInput for PreparedProjectRangeInput {
    fn snapshot(&self) -> &ProjectSnapshot {
        &self.snapshot
    }

    fn sequence_id(&self) -> &str {
        &self.sequence_id
    }

    fn set_asset_lock(&mut self, asset_lock: AssetLockManifest) {
        self.asset_lock = Some(asset_lock);
    }
}

fn create_locked_project_job<T: PreparedProjectInput>(
    app: &mut impl JobSubmissionStore,
    assets: &impl JobAssetCapture,
    idempotency_key: String,
    kind: &str,
    mut prepared: T,
    asset_scope: AssetScope,
) -> Result<JobRecord, EditorError> {
    let identity_input = serialize_prepared(&prepared)?;
    let request = JobRequest {
        idempotency_key,
        kind: kind.into(),
        input: identity_input,
    };
    if let Some(existing) = app.find_accepted_job(&request)? {
        return Ok(existing);
    }
    let asset_ids = opencut_editor_core::dependencies::required_asset_ids(
        prepared.snapshot(),
        prepared.sequence_id(),
        asset_scope,
    )?;
    let asset_lock = assets.capture(prepared.snapshot(), prepared.sequence_id(), &asset_ids)?;
    prepared.set_asset_lock(asset_lock);
    let execution_input = serialize_prepared(&prepared)?;
    app.accept_job(request, execution_input)
}

use opencut_editor_core::dependencies::AssetScope;

fn serialize_prepared(input: &impl Serialize) -> Result<serde_json::Value, EditorError> {
    serde_json::to_value(input).map_err(|error| {
        EditorError::new(
            "SERIALIZATION_ERROR",
            format!("could not serialize prepared job input: {error}"),
        )
    })
}

fn fixed_snapshot(
    app: &impl JobSubmissionStore,
    project_id: &str,
    revision: &str,
    sequence_id: &str,
) -> Result<opencut_contracts::ProjectSnapshot, EditorError> {
    let snapshot = app.fixed_snapshot(project_id, revision)?;
    if !snapshot
        .project
        .sequences
        .iter()
        .any(|sequence| sequence.id == sequence_id)
    {
        return Err(EditorError::new(
            "NOT_FOUND",
            "Sequence does not exist in the requested revision",
        ));
    }
    Ok(snapshot)
}

fn validate_requested_time(time: &Time) -> Result<(), EditorError> {
    let ticks = time.ticks.parse::<i64>().map_err(|_| {
        EditorError::new(
            "INVALID_ARGUMENT",
            "requested_time ticks must be a signed 64-bit decimal integer",
        )
    })?;
    if time.timebase == 0 || ticks < 0 {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "requested_time must be nonnegative and use a positive timebase",
        ));
    }
    Ok(())
}

fn validate_requested_range(range: &opencut_contracts::TimeRange) -> Result<(), EditorError> {
    validate_requested_time(&range.start)?;
    let duration = range.duration.ticks.parse::<i64>().map_err(|_| {
        EditorError::new(
            "INVALID_ARGUMENT",
            "requested_range duration ticks must be a signed 64-bit decimal integer",
        )
    })?;
    if range.duration.timebase == 0 || duration <= 0 {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "requested_range duration must be positive and use a positive timebase",
        ));
    }
    Ok(())
}
