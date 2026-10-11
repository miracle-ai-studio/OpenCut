//! Schema 3 jobs use the existing durable queue, receipt, lease and asset lock ports.
use crate::{
    JobRecord, JobRequest,
    access::SourceAccess,
    jobs::{JobAssetCapture, JobSubmissionStore},
    project_v3::AuthorizedSnapshotV3,
};
pub use opencut_api_contracts::project_v3_jobs::{SubmitV3Frame, SubmitV3Range, SubmitV3Render};
use opencut_contracts::{
    EditorError, ProjectSnapshot, Time, TimeRange, project_v3::ProjectSnapshotV3,
};
use opencut_editor_core::dependencies::{self, AssetScope};
use opencut_job_execution::project_v3::{PreparedV3Frame, PreparedV3Range, PreparedV3Render};
use opencut_time::{RationalTime, validate_range};
use serde::Serialize;
use std::collections::BTreeSet;

pub trait V3JobSubmissionStore: JobSubmissionStore {
    /// Must authorize RunJobs and load exactly this project's immutable revision.
    fn fixed_v3_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<AuthorizedSnapshotV3, EditorError>;
}

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn serialize(value: &impl Serialize) -> Result<serde_json::Value, EditorError> {
    serde_json::to_value(value)
        .map_err(|_| EditorError::new("SERIALIZATION_ERROR", "Could not serialize schema 3 job"))
}
fn fixed(
    store: &impl V3JobSubmissionStore,
    project_id: &str,
    revision: &str,
    sequence_id: &str,
) -> Result<AuthorizedSnapshotV3, EditorError> {
    let authorized = store.fixed_v3_snapshot(project_id, revision)?;
    let snapshot = &authorized.snapshot;
    if snapshot.project.base.id != project_id
        || snapshot.revision != revision
        || !snapshot
            .project
            .base
            .sequences
            .iter()
            .any(|sequence| sequence.id == sequence_id)
    {
        return Err(invalid(
            "Sequence or revision does not belong to the requested project",
        ));
    }
    opencut_editor_core::project_v3::validate_project_v3(&snapshot.project)?;
    Ok(authorized)
}
fn lock_assets(
    assets: &impl JobAssetCapture,
    authorized: &AuthorizedSnapshotV3,
    sequence_id: &str,
    scope: AssetScope,
) -> Result<opencut_contracts::AssetLockManifest, EditorError> {
    let snapshot = &authorized.snapshot;
    let base = ProjectSnapshot {
        project: snapshot.project.base.clone(),
        revision: snapshot.revision.clone(),
    };
    let ids = dependencies::required_asset_ids(&base, sequence_id, scope)?;
    if authorized.sources == SourceAccess::ProjectContent
        && ids.iter().any(|id| {
            base.project
                .assets
                .iter()
                .find(|asset| &asset.id == id)
                .is_none_or(|asset| asset.version.is_none())
        })
    {
        return Err(EditorError::new(
            "NOT_AUTHORIZED",
            "Project job requires pinned project content",
        ));
    }
    let lock = assets.capture(&base, sequence_id, &ids)?;
    let expected: BTreeSet<_> = ids.iter().map(String::as_str).collect();
    let observed: BTreeSet<_> = lock
        .assets
        .iter()
        .map(|asset| asset.asset_id.as_str())
        .collect();
    if lock.revision != snapshot.revision
        || lock.sequence_id != sequence_id
        || lock.assets.len() != ids.len()
        || lock.version != 1
        || observed != expected
        || lock.assets.iter().any(|locked| {
            base.project
                .assets
                .iter()
                .find(|asset| asset.id == locked.asset_id)
                .and_then(|asset| asset.version.as_ref())
                .is_some_and(|version| {
                    version.content_hash != locked.content_hash || version.size != locked.size
                })
        })
    {
        return Err(EditorError::new(
            "ASSET_LOCK_INVALID",
            "Schema 3 capture differs from fixed revision",
        ));
    }
    Ok(lock)
}
fn accept<P: Serialize>(
    store: &mut impl V3JobSubmissionStore,
    assets: &impl JobAssetCapture,
    request: JobRequest,
    authorized: AuthorizedSnapshotV3,
    sequence_id: String,
    scope: AssetScope,
    prepared: impl FnOnce(ProjectSnapshotV3, opencut_contracts::AssetLockManifest) -> P,
) -> Result<JobRecord, EditorError> {
    if let Some(existing) = store.find_accepted_job(&request)? {
        return Ok(existing);
    }
    let lock = lock_assets(assets, &authorized, &sequence_id, scope)?;
    store.accept_job(request, serialize(&prepared(authorized.snapshot, lock))?)
}

fn valid_time(value: &Time) -> Result<(), EditorError> {
    if RationalTime::try_from(value)?.ticks() < 0 {
        return Err(invalid("Requested time must be nonnegative"));
    }
    Ok(())
}
fn valid_range(value: &TimeRange) -> Result<(), EditorError> {
    validate_range(value, true)?;
    valid_time(&value.start)
}

pub fn submit_sequence_transcribe(
    store: &mut impl V3JobSubmissionStore,
    assets: &impl JobAssetCapture,
    input: opencut_job_contracts::SequenceTranscribeInput,
) -> Result<JobRecord, EditorError> {
    if input.language.as_ref().is_some_and(|language| {
        language != "auto"
            && (!(2..=3).contains(&language.len())
                || !language.bytes().all(|byte| byte.is_ascii_lowercase()))
    }) {
        return Err(invalid("Invalid transcription language"));
    }
    let authorized = fixed(
        store,
        &input.project_id,
        &input.revision,
        &input.sequence_id,
    )?;
    let request = JobRequest {
        idempotency_key: input.idempotency_key,
        kind: "analysis.sequence_transcribe".into(),
        input: serialize(&serde_json::json!({"snapshot": authorized.snapshot,
            "sequence_id": input.sequence_id, "language": input.language}))?,
    };
    accept(
        store,
        assets,
        request,
        authorized,
        input.sequence_id.clone(),
        AssetScope::All,
        |snapshot, asset_lock| opencut_job_execution::project_v3::PreparedSequenceTranscribe {
            snapshot,
            asset_lock,
            sequence_id: input.sequence_id,
            language: input.language,
        },
    )
}

pub fn submit_render(
    store: &mut impl V3JobSubmissionStore,
    assets: &impl JobAssetCapture,
    input: SubmitV3Render,
) -> Result<JobRecord, EditorError> {
    let authorized = fixed(
        store,
        &input.project_id,
        &input.revision,
        &input.sequence_id,
    )?;
    let identity = serde_json::json!({"snapshot":authorized.snapshot,"sequence_id":input.sequence_id,
        "container":input.container,"video_codec":input.video_codec});
    let request = JobRequest {
        idempotency_key: input.idempotency_key,
        kind: "render.project.v3".into(),
        input: serialize(&identity)?,
    };
    accept(
        store,
        assets,
        request,
        authorized,
        input.sequence_id.clone(),
        AssetScope::All,
        |snapshot, asset_lock| PreparedV3Render {
            snapshot,
            sequence_id: input.sequence_id,
            container: input.container,
            video_codec: input.video_codec,
            asset_lock,
        },
    )
}
pub fn submit_frame(
    store: &mut impl V3JobSubmissionStore,
    assets: &impl JobAssetCapture,
    input: SubmitV3Frame,
) -> Result<JobRecord, EditorError> {
    valid_time(&input.requested_time)?;
    let authorized = fixed(
        store,
        &input.project_id,
        &input.revision,
        &input.sequence_id,
    )?;
    let identity = serde_json::json!({"snapshot":authorized.snapshot,"sequence_id":input.sequence_id,
        "requested_time":input.requested_time,"alignment":input.alignment});
    let request = JobRequest {
        idempotency_key: input.idempotency_key,
        kind: "preview.project_frame.v3".into(),
        input: serialize(&identity)?,
    };
    accept(
        store,
        assets,
        request,
        authorized,
        input.sequence_id.clone(),
        AssetScope::Visual,
        |snapshot, asset_lock| PreparedV3Frame {
            snapshot,
            sequence_id: input.sequence_id,
            requested_time: input.requested_time,
            alignment: input.alignment,
            asset_lock,
        },
    )
}
pub fn submit_range(
    store: &mut impl V3JobSubmissionStore,
    assets: &impl JobAssetCapture,
    input: SubmitV3Range,
) -> Result<JobRecord, EditorError> {
    valid_range(&input.requested_range)?;
    let authorized = fixed(
        store,
        &input.project_id,
        &input.revision,
        &input.sequence_id,
    )?;
    let identity = serde_json::json!({"snapshot":authorized.snapshot,"sequence_id":input.sequence_id,
        "requested_range":input.requested_range,"alignment":input.alignment});
    let request = JobRequest {
        idempotency_key: input.idempotency_key,
        kind: "preview.project_range.v3".into(),
        input: serialize(&identity)?,
    };
    accept(
        store,
        assets,
        request,
        authorized,
        input.sequence_id.clone(),
        AssetScope::All,
        |snapshot, asset_lock| PreparedV3Range {
            snapshot,
            sequence_id: input.sequence_id,
            requested_range: input.requested_range,
            alignment: input.alignment,
            asset_lock,
        },
    )
}
