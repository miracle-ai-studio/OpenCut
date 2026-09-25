use crate::{
    JobRecord, JobRequest,
    jobs::{JobAssetCapture, JobSubmissionStore},
};
use opencut_contracts::{AssetKind, EditorError};
use opencut_job_contracts::{ShotsJobInput, SilenceJobInput, WaveformJobInput};
use opencut_job_execution::{AnalysisOptions, PreparedAnalysisInput};
use serde_json::json;

pub fn submit_silence(
    app: &mut impl JobSubmissionStore,
    assets: &impl JobAssetCapture,
    input: SilenceJobInput,
) -> Result<JobRecord, EditorError> {
    if !(-80..=-5).contains(&input.noise_db) || !(100..=10_000).contains(&input.min_duration_ms) {
        return Err(invalid());
    }
    submit(
        app,
        assets,
        AnalysisSubmission {
            kind: "analysis.silence",
            idempotency_key: input.idempotency_key,
            project_id: input.project_id,
            revision: input.revision,
            asset_id: input.asset_id,
            options: AnalysisOptions::Silence {
                noise_db: input.noise_db,
                min_duration_ms: input.min_duration_ms,
            },
        },
    )
}
pub fn submit_shots(
    app: &mut impl JobSubmissionStore,
    assets: &impl JobAssetCapture,
    input: ShotsJobInput,
) -> Result<JobRecord, EditorError> {
    if !input.threshold.is_finite() || !(0.05..=0.95).contains(&input.threshold) {
        return Err(invalid());
    }
    submit(
        app,
        assets,
        AnalysisSubmission {
            kind: "analysis.shots",
            idempotency_key: input.idempotency_key,
            project_id: input.project_id,
            revision: input.revision,
            asset_id: input.asset_id,
            options: AnalysisOptions::Shots {
                threshold: input.threshold,
            },
        },
    )
}
pub fn submit_waveform(
    app: &mut impl JobSubmissionStore,
    assets: &impl JobAssetCapture,
    input: WaveformJobInput,
) -> Result<JobRecord, EditorError> {
    if !(16..=2048).contains(&input.buckets) {
        return Err(invalid());
    }
    submit(
        app,
        assets,
        AnalysisSubmission {
            kind: "analysis.waveform",
            idempotency_key: input.idempotency_key,
            project_id: input.project_id,
            revision: input.revision,
            asset_id: input.asset_id,
            options: AnalysisOptions::Waveform {
                buckets: input.buckets,
            },
        },
    )
}
fn invalid() -> EditorError {
    EditorError::new(
        "INVALID_ARGUMENT",
        "Analysis options outside supported bounds",
    )
}
struct AnalysisSubmission {
    kind: &'static str,
    idempotency_key: String,
    project_id: String,
    revision: String,
    asset_id: String,
    options: AnalysisOptions,
}
fn submit(
    app: &mut impl JobSubmissionStore,
    assets: &impl JobAssetCapture,
    request: AnalysisSubmission,
) -> Result<JobRecord, EditorError> {
    let AnalysisSubmission {
        kind,
        idempotency_key,
        project_id,
        revision,
        asset_id,
        options,
    } = request;
    let snapshot = app.fixed_analysis_snapshot(&project_id, &revision)?;
    let asset = snapshot
        .project
        .assets
        .iter()
        .find(|asset| asset.id == asset_id)
        .ok_or_else(|| {
            EditorError::new(
                "NOT_FOUND",
                "Asset does not exist in the requested revision",
            )
        })?;
    let accepted_kind = if kind == "analysis.shots" {
        matches!(asset.kind, AssetKind::Video)
    } else {
        matches!(asset.kind, AssetKind::Audio | AssetKind::Video)
    };
    if !accepted_kind {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Asset kind is unsupported for this analysis",
        ));
    }
    let sequence_id = snapshot.project.default_sequence_id.clone();
    let request = JobRequest {
        idempotency_key,
        kind: kind.into(),
        input: json!({"snapshot":snapshot,"sequence_id":sequence_id,"asset_id":asset_id,"options":options}),
    };
    if let Some(existing) = app.find_accepted_job(&request)? {
        return Ok(existing);
    }
    let lock = assets.capture(&snapshot, &sequence_id, std::slice::from_ref(&asset_id))?;
    if lock.assets.len() != 1 || lock.assets[0].asset_id != asset_id {
        return Err(EditorError::new(
            "ASSET_LOCK_INVALID",
            "Analysis asset capture did not match request",
        ));
    }
    let prepared = PreparedAnalysisInput {
        snapshot,
        sequence_id,
        asset_id,
        options,
        asset_lock: lock,
    };
    let prepared = serde_json::to_value(prepared)
        .map_err(|_| EditorError::new("SERIALIZATION_ERROR", "Could not encode analysis input"))?;
    app.accept_job(request, prepared)
}
