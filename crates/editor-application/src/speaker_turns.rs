//! Durable, fixed-version submission of English local speaker-turn analysis.
use crate::{
    JobRecord, JobRequest,
    jobs::{JobAssetCapture, JobSubmissionStore},
};
use opencut_contracts::{AssetKind, EditorError};
use opencut_job_contracts::SpeakerTurnsJobInput;
use opencut_job_execution::PreparedSpeakerTurnsInput;
use serde_json::json;

pub fn submit(
    app: &mut impl JobSubmissionStore,
    assets: &impl JobAssetCapture,
    input: SpeakerTurnsJobInput,
) -> Result<JobRecord, EditorError> {
    if input.language != "en" {
        return Err(EditorError::new(
            "CAPABILITY_UNAVAILABLE",
            "Speaker-turn model supports English only",
        ));
    }
    let snapshot = app.fixed_analysis_snapshot(&input.project_id, &input.revision)?;
    let asset = snapshot
        .project
        .assets
        .iter()
        .find(|asset| asset.id == input.asset_id)
        .ok_or_else(|| {
            EditorError::new(
                "NOT_FOUND",
                "Asset does not exist in the requested revision",
            )
        })?;
    if !matches!(asset.kind, AssetKind::Audio | AssetKind::Video) {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Asset must contain audio",
        ));
    }
    let sequence_id = snapshot.project.default_sequence_id.clone();
    let request = JobRequest {
        idempotency_key: input.idempotency_key,
        kind: "analysis.speaker_turns".into(),
        input: json!({"snapshot":snapshot,"sequence_id":sequence_id,"asset_id":input.asset_id,"language":input.language}),
    };
    if let Some(existing) = app.find_accepted_job(&request)? {
        return Ok(existing);
    }
    let lock = assets.capture(
        &snapshot,
        &sequence_id,
        std::slice::from_ref(&input.asset_id),
    )?;
    if lock.assets.len() != 1 || lock.assets[0].asset_id != input.asset_id {
        return Err(EditorError::new(
            "ASSET_LOCK_INVALID",
            "Speaker-turn asset capture did not match request",
        ));
    }
    let prepared = PreparedSpeakerTurnsInput {
        snapshot,
        sequence_id,
        asset_id: input.asset_id,
        language: input.language,
        asset_lock: lock,
    };
    app.accept_job(
        request,
        serde_json::to_value(prepared).map_err(|_| {
            EditorError::new("SERIALIZATION_ERROR", "Could not encode speaker-turn job")
        })?,
    )
}
