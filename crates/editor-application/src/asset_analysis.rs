//! Fixed V4 resource analysis without projecting a V4 aggregate into a legacy document.
use crate::{
    JobRecord, JobRequest,
    source_art::{SourceArtPolicy, SourceArtRead},
};
use opencut_contracts::{AssetKind, EditorError, LockedAsset, scene::EditorSnapshot};
pub use opencut_job_contracts::asset_analysis::*;
use opencut_job_execution::scene_audio::TrustedJobAuthority;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedAssetAnalysis {
    pub snapshot: EditorSnapshot,
    pub asset_id: String,
    pub options: AssetAnalysisOptions,
    pub authority: TrustedJobAuthority,
    pub processing_policy: SourceArtPolicy,
}
pub trait AssetAnalysisStore: SourceArtRead {
    fn find_asset_analysis(&self, request: &JobRequest) -> Result<Option<JobRecord>, EditorError>;
    fn accept_asset_analysis(
        &mut self,
        request: JobRequest,
        prepared: PreparedAssetAnalysis,
    ) -> Result<JobRecord, EditorError>;
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
pub fn validate(input: &SubmitAssetAnalysis) -> Result<(), EditorError> {
    crate::require_idempotency_key(&input.idempotency_key)?;
    if [&input.project_id, &input.revision, &input.asset_id]
        .iter()
        .any(|s| s.trim().is_empty() || s.len() > 1024)
    {
        return Err(invalid(
            "Asset analysis requires fixed project/revision/asset IDs",
        ));
    }
    match &input.options {
        AssetAnalysisOptions::Silence {
            noise_db,
            min_duration_ms,
        } if !(-80..=-5).contains(noise_db) || !(100..=10000).contains(min_duration_ms) => {
            return Err(invalid("Invalid silence parameters"));
        }
        AssetAnalysisOptions::Shots { threshold }
            if !threshold.is_finite() || !(0.05..=0.95).contains(threshold) =>
        {
            return Err(invalid("Invalid shot threshold"));
        }
        AssetAnalysisOptions::Waveform { buckets } if !(16..=2048).contains(buckets) => {
            return Err(invalid("Invalid waveform bucket count"));
        }
        AssetAnalysisOptions::Transcribe { language, .. } => {
            crate::scene_transcription::validate_language(language.as_deref())?
        }
        _ => {}
    }
    Ok(())
}
pub fn prepare(input: &PreparedAssetAnalysis) -> Result<LockedAsset, EditorError> {
    crate::media_frame_jobs::validate_authority(&input.authority)?;
    crate::source_art::validate_policy(&input.processing_policy)?;
    opencut_editor_core::scene::validate_project_scene(&input.snapshot.project)?;
    let asset = input
        .snapshot
        .project
        .assets
        .iter()
        .find(|a| a.id == input.asset_id)
        .ok_or_else(|| invalid("Analysis resource absent"))?;
    if !matches!(asset.kind, AssetKind::Video | AssetKind::Audio)
        || matches!(input.options, AssetAnalysisOptions::Shots { .. })
            && asset.kind != AssetKind::Video
        || asset.version.size > input.processing_policy.max_encoded_bytes
    {
        return Err(invalid(
            "Analysis resource exceeds its kind or captured byte admission",
        ));
    }
    Ok(LockedAsset {
        asset_id: asset.id.clone(),
        content_hash: asset.version.content_hash.clone(),
        size: asset.version.size,
    })
}
pub fn validate_request(
    request: &JobRequest,
    prepared: &PreparedAssetAnalysis,
) -> Result<(), EditorError> {
    let public: SubmitAssetAnalysis = serde_json::from_value(request.input.clone())
        .map_err(|_| invalid("Invalid V4 analysis input"))?;
    validate(&public)?;
    if request.kind != KIND
        || request.idempotency_key != public.idempotency_key
        || public.project_id != prepared.snapshot.project.id
        || public.revision != prepared.snapshot.revision
        || public.asset_id != prepared.asset_id
        || public.options != prepared.options
    {
        return Err(invalid(
            "Prepared analysis differs from the accepted request",
        ));
    }
    prepare(prepared)?;
    Ok(())
}
pub fn submit(
    store: &mut impl AssetAnalysisStore,
    input: SubmitAssetAnalysis,
) -> Result<JobRecord, EditorError> {
    validate(&input)?;
    let fixed = store.fixed_source_art_snapshot(&input.project_id, &input.revision)?;
    let request = JobRequest {
        idempotency_key: input.idempotency_key.clone(),
        kind: KIND.into(),
        input: serde_json::to_value(&input).map_err(|_| invalid("Cannot bind analysis request"))?,
    };
    if let Some(existing) = store.find_asset_analysis(&request)? {
        return Ok(existing);
    }
    let prepared = PreparedAssetAnalysis {
        snapshot: fixed.snapshot,
        asset_id: input.asset_id,
        options: input.options,
        authority: fixed.authority,
        processing_policy: fixed.policy,
    };
    validate_request(&request, &prepared)?;
    store.accept_asset_analysis(request, prepared)
}
pub fn validate_result(
    input: &PreparedAssetAnalysis,
    result: &AssetAnalysisResult,
) -> Result<(), EditorError> {
    let lock = prepare(input)?;
    let valid = match (&input.options, &result.data) {
        (AssetAnalysisOptions::Silence { .. }, AssetAnalysisData::Silence(v)) => {
            opencut_media_contracts::validate_public_silence_result(v)
        }
        (AssetAnalysisOptions::Shots { .. }, AssetAnalysisData::Shots(v)) => {
            opencut_media_contracts::validate_public_shots_result(v)
        }
        (AssetAnalysisOptions::Waveform { .. }, AssetAnalysisData::Waveform(v)) => {
            opencut_media_contracts::validate_public_waveform_result(v)
        }
        (AssetAnalysisOptions::Transcribe { .. }, AssetAnalysisData::Transcribe(v)) => {
            opencut_media_contracts::validate_public_transcription_result(v)
        }
        (AssetAnalysisOptions::SpeakerTurns {}, AssetAnalysisData::SpeakerTurns(v)) => {
            opencut_media_contracts::validate_public_speaker_turns_result(v)
        }
        _ => false,
    };
    if !valid
        || result.schema_version != 4
        || result.project_id != input.snapshot.project.id
        || result.revision != input.snapshot.revision
        || result.asset_id != input.asset_id
        || result.asset_version != lock
        || result.options != input.options
        || serde_json::to_vec(result)
            .map_err(|_| invalid("Cannot encode analysis result"))?
            .len()
            > 2 * 1024 * 1024
    {
        return Err(EditorError::new(
            "INVALID_STORED_RESULT",
            "V4 analysis evidence differs from the fixed resource/options",
        ));
    }
    Ok(())
}
