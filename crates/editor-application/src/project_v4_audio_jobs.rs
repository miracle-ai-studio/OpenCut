//! Fixed V4 audio submission and safe result validation; no files or processes.
use crate::{JobRecord, JobRequest};
use opencut_composition::project_v4::{
    AudioExecutionProfileV4, AudioProcessingV4, PcmFormatV4, PreparedAudioV4, prepare_audio_v4,
};
use opencut_contracts::{AssetKind, AssetLockManifest, EditorError, project_v4::ProjectSnapshotV4};
use opencut_job_contracts::ArtifactDescriptor;
pub use opencut_job_contracts::project_v4_audio::*;
pub use opencut_job_execution::project_v4_audio::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub struct AuthorizedV4AudioSnapshot {
    pub snapshot: ProjectSnapshotV4,
    pub authority: TrustedJobAuthorityV1,
    pub processing_policy: LocalAudioExecutionPolicyV1,
}
pub trait V4AudioJobSubmissionStore {
    /// Read + RunJobs, current live project and this exact V4 revision share a
    /// consistent read boundary. Authority and admission come from the host.
    fn fixed_v4_audio_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<AuthorizedV4AudioSnapshot, EditorError>;
    /// Must reauthorize before inspecting a workspace-scoped idempotent receipt.
    fn find_accepted_v4_audio_job(
        &self,
        request: &JobRequest,
    ) -> Result<Option<JobRecord>, EditorError>;
    /// Recheck request/prepared binding and live authority under the write lock;
    /// atomically persist job, project binding, receipt, event and audit.
    fn accept_v4_audio_job(
        &mut self,
        request: JobRequest,
        prepared: PreparedV4AudioWav,
    ) -> Result<JobRecord, EditorError>;
}
pub trait V4AudioAssetCapture {
    /// Verify only the requested participating V4 CAS resources. Keep the shared
    /// collection gate through queue acceptance; do not use legacy locators.
    fn capture(
        &self,
        snapshot: &ProjectSnapshotV4,
        sequence_id: &str,
        asset_ids: &[String],
    ) -> Result<AssetLockManifest, EditorError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredV4AudioWavResultV1 {
    pub public: V4AudioWavResultV1,
    /// Attempt-local storage location. Never serialize this record to a client.
    pub artifact: ArtifactDescriptor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredV4AudioRangeWavResultV1 {
    pub public: V4AudioRangeWavResultV1,
    /// Attempt-local descriptor; only the public metadata is projected.
    pub artifact: ArtifactDescriptor,
}

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn stored_invalid() -> EditorError {
    EditorError::new(
        "INVALID_STORED_RESULT",
        "Stored V4 audio result does not match its fixed input",
    )
}
fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn hash(value: &impl Serialize) -> Result<String, EditorError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|_| EditorError::new("SERIALIZATION_ERROR", "Could not hash V4 audio input"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn validate_submission(input: &SubmitV4AudioWav) -> Result<(), EditorError> {
    if ![
        &input.idempotency_key,
        &input.project_id,
        &input.revision,
        &input.sequence_id,
    ]
    .into_iter()
    .all(|id| valid_id(id))
        || !matches!(input.sample_rate, 44_100 | 48_000)
    {
        return Err(invalid(
            "V4 audio requires bounded identifiers and 44100 or 48000 Hz",
        ));
    }
    Ok(())
}
pub fn validate_range_submission(input: &SubmitV4AudioRangeWav) -> Result<(), EditorError> {
    validate_submission(&range_base(input))?;
    let start = crate::classic_audio::decimal(&input.range.start_sample)?;
    let count = crate::classic_audio::decimal(&input.range.frame_count)?;
    if count == 0 || start.checked_add(count).is_none() {
        return Err(invalid(
            "Audio range requires a nonempty bounded sample interval",
        ));
    }
    Ok(())
}
fn range_base(input: &SubmitV4AudioRangeWav) -> SubmitV4AudioWav {
    SubmitV4AudioWav {
        idempotency_key: input.idempotency_key.clone(),
        project_id: input.project_id.clone(),
        revision: input.revision.clone(),
        sequence_id: input.sequence_id.clone(),
        sample_rate: input.sample_rate,
    }
}
/// Canonical conversion does not change sample indices or quantize time.
pub fn classic_output_range(
    input: &PreparedV4AudioWav,
) -> Option<crate::classic_audio::ClassicAudioRange> {
    input
        .output_range
        .as_ref()
        .map(|r| crate::classic_audio::ClassicAudioRange {
            start_sample: r.start_sample.clone(),
            frame_count: r.frame_count.clone(),
        })
}

pub fn validate_policy(policy: &LocalAudioExecutionPolicyV1) -> Result<(), EditorError> {
    if policy.max_pcm_bytes == 0
        || policy.max_pcm_bytes > 512 * 1024 * 1024
        || policy.max_encoded_bytes == 0
        || policy.max_encoded_bytes > 32 * 1024 * 1024
        || policy.max_rss_bytes == 0
        || !(1..=300_000).contains(&policy.deadline_ms)
    {
        return Err(invalid("Invalid host Classic audio admission"));
    }
    Ok(())
}
fn validate_authority(authority: &TrustedJobAuthorityV1) -> Result<(), EditorError> {
    if let TrustedJobAuthorityV1::Credential {
        principal_id,
        credential_id,
    } = authority
        && (!valid_id(principal_id) || !valid_id(credential_id))
    {
        return Err(invalid("Invalid trusted job authority"));
    }
    Ok(())
}
fn prepare(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    sample_rate: u32,
) -> Result<PreparedAudioV4, EditorError> {
    if !matches!(sample_rate, 44_100 | 48_000)
        || !valid_id(sequence_id)
        || !valid_id(&snapshot.revision)
    {
        return Err(invalid("Invalid V4 audio preparation identity"));
    }
    prepare_audio_v4(
        snapshot,
        sequence_id,
        AudioExecutionProfileV4::ClassicV31,
        PcmFormatV4 {
            sample_rate,
            channels: 2,
        },
    )
    .map_err(|e| EditorError::new(e.code, e.message))
}
fn expected_assets(prepared: &PreparedAudioV4) -> BTreeMap<String, (String, u64)> {
    prepared
        .resources()
        .map(|r| {
            (
                r.asset_id.clone(),
                (r.version.content_hash.clone(), r.version.size),
            )
        })
        .collect()
}
fn validate_lock(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    prepared: &PreparedAudioV4,
    lock: &AssetLockManifest,
) -> Result<(), EditorError> {
    let expected = expected_assets(prepared);
    let observed: BTreeMap<_, _> = lock
        .assets
        .iter()
        .map(|a| (a.asset_id.clone(), (a.content_hash.clone(), a.size)))
        .collect();
    if lock.version != 1
        || lock.revision != snapshot.revision
        || lock.sequence_id != sequence_id
        || observed.len() != lock.assets.len()
        || observed != expected
    {
        return Err(EditorError::new(
            "ASSET_LOCK_INVALID",
            "Audio lock differs from fixed V4 participating resources",
        ));
    }
    Ok(())
}
/// Complete snapshot serialization preserves authored values/timebases and all
/// non-audio fields. Hashing a reduced voice plan would not identify the revision.
pub fn render_input_sha256(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    sample_rate: u32,
) -> Result<String, EditorError> {
    #[derive(Serialize)]
    struct Input<'a> {
        schema_version: u32,
        snapshot: &'a ProjectSnapshotV4,
        sequence_id: &'a str,
        sample_rate: u32,
        channels: u32,
        profile: V4AudioProfileV1,
        sample_format: V4AudioSampleFormatV1,
    }
    hash(&Input {
        schema_version: 1,
        snapshot,
        sequence_id,
        sample_rate,
        channels: 2,
        profile: V4AudioProfileV1::ClassicExportV31WebAudio,
        sample_format: V4AudioSampleFormatV1::IeeeFloat32Le,
    })
}
pub fn render_range_input_sha256(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    sample_rate: u32,
    range: &V4AudioSampleRangeV1,
) -> Result<String, EditorError> {
    // Keep the established whole-scene digest untouched and bind the range in a
    // separate versioned domain. Neither authority nor machine policy is public.
    #[derive(Serialize)]
    struct RangeInput<'a> {
        schema_version: u32,
        kind: &'static str,
        whole_scene_sha256: String,
        range: &'a V4AudioSampleRangeV1,
    }
    hash(&RangeInput {
        schema_version: 1,
        kind: V4_AUDIO_RANGE_WAV_KIND,
        whole_scene_sha256: render_input_sha256(snapshot, sequence_id, sample_rate)?,
        range,
    })
}

pub fn validate_prepared(input: &PreparedV4AudioWav) -> Result<PreparedAudioV4, EditorError> {
    validate_policy(&input.processing_policy)?;
    validate_authority(&input.authority)?;
    let prepared = prepare(&input.snapshot, &input.sequence_id, input.sample_rate)?;
    crate::classic_audio::ClassicAudioSession::new_with_range(
        &prepared,
        input.processing_policy.max_pcm_bytes,
        input.processing_policy.max_encoded_bytes,
        classic_output_range(input),
    )?;
    validate_lock(
        &input.snapshot,
        &input.sequence_id,
        &prepared,
        &input.asset_lock,
    )?;
    Ok(prepared)
}
pub fn validate_prepared_request(
    request: &JobRequest,
    prepared: &PreparedV4AudioWav,
) -> Result<(), EditorError> {
    let input = match request.kind.as_str() {
        V4_AUDIO_WAV_KIND => {
            let input: SubmitV4AudioWav = serde_json::from_value(request.input.clone())
                .map_err(|_| invalid("Invalid public V4 audio request"))?;
            validate_submission(&input)?;
            if prepared.output_range.is_some() {
                return Err(invalid("Whole-scene input cannot contain an output range"));
            }
            input
        }
        V4_AUDIO_RANGE_WAV_KIND => {
            let input: SubmitV4AudioRangeWav = serde_json::from_value(request.input.clone())
                .map_err(|_| invalid("Invalid public V4 audio range request"))?;
            validate_range_submission(&input)?;
            if prepared.output_range.as_ref().is_none_or(|r| {
                r.start_sample != input.range.start_sample
                    || r.frame_count != input.range.frame_count
            }) {
                return Err(invalid(
                    "Range request differs from its private preparation",
                ));
            }
            range_base(&input)
        }
        _ => return Err(invalid("Unsupported V4 audio job kind")),
    };
    if request.idempotency_key != input.idempotency_key
        || input.project_id != prepared.snapshot.project.id
        || input.revision != prepared.snapshot.revision
        || input.sequence_id != prepared.sequence_id
        || input.sample_rate != prepared.sample_rate
    {
        return Err(invalid(
            "V4 audio request differs from its private preparation",
        ));
    }
    validate_prepared(prepared)?;
    Ok(())
}
pub fn submit_audio_wav(
    store: &mut impl V4AudioJobSubmissionStore,
    assets: &impl V4AudioAssetCapture,
    input: SubmitV4AudioWav,
) -> Result<JobRecord, EditorError> {
    validate_submission(&input)?;
    let request = JobRequest {
        idempotency_key: input.idempotency_key.clone(),
        kind: V4_AUDIO_WAV_KIND.into(),
        input: serde_json::to_value(&input)
            .map_err(|_| invalid("Could not serialize V4 audio request"))?,
    };
    submit(store, assets, input, request, None)
}
pub fn submit_audio_range_wav(
    store: &mut impl V4AudioJobSubmissionStore,
    assets: &impl V4AudioAssetCapture,
    input: SubmitV4AudioRangeWav,
) -> Result<JobRecord, EditorError> {
    validate_range_submission(&input)?;
    let request = JobRequest {
        idempotency_key: input.idempotency_key.clone(),
        kind: V4_AUDIO_RANGE_WAV_KIND.into(),
        input: serde_json::to_value(&input)
            .map_err(|_| invalid("Could not serialize V4 audio range request"))?,
    };
    let range = PreparedV4AudioSampleRangeV1 {
        start_sample: input.range.start_sample.clone(),
        frame_count: input.range.frame_count.clone(),
    };
    submit(store, assets, range_base(&input), request, Some(range))
}
fn submit(
    store: &mut impl V4AudioJobSubmissionStore,
    assets: &impl V4AudioAssetCapture,
    input: SubmitV4AudioWav,
    request: JobRequest,
    output_range: Option<PreparedV4AudioSampleRangeV1>,
) -> Result<JobRecord, EditorError> {
    let authorized = store.fixed_v4_audio_snapshot(&input.project_id, &input.revision)?;
    if authorized.snapshot.project.id != input.project_id
        || authorized.snapshot.revision != input.revision
    {
        return Err(invalid(
            "Authorized snapshot differs from requested fixed revision",
        ));
    }
    opencut_editor_core::project_v4::validate_project_v4(&authorized.snapshot.project)?;
    if let Some(existing) = store.find_accepted_v4_audio_job(&request)? {
        return Ok(existing);
    }
    validate_policy(&authorized.processing_policy)?;
    validate_authority(&authorized.authority)?;
    let prepared = prepare(&authorized.snapshot, &input.sequence_id, input.sample_rate)?;
    crate::classic_audio::ClassicAudioSession::new_with_range(
        &prepared,
        authorized.processing_policy.max_pcm_bytes,
        authorized.processing_policy.max_encoded_bytes,
        output_range
            .as_ref()
            .map(|r| crate::classic_audio::ClassicAudioRange {
                start_sample: r.start_sample.clone(),
                frame_count: r.frame_count.clone(),
            }),
    )?;
    let ids: Vec<_> = expected_assets(&prepared).into_keys().collect();
    let asset_lock = assets.capture(&authorized.snapshot, &input.sequence_id, &ids)?;
    validate_lock(
        &authorized.snapshot,
        &input.sequence_id,
        &prepared,
        &asset_lock,
    )?;
    store.accept_v4_audio_job(
        request,
        PreparedV4AudioWav {
            snapshot: authorized.snapshot,
            sequence_id: input.sequence_id,
            sample_rate: input.sample_rate,
            asset_lock,
            authority: authorized.authority,
            processing_policy: authorized.processing_policy,
            output_range,
        },
    )
}

pub fn validate_stored_result(
    input: &PreparedV4AudioWav,
    stored: &StoredV4AudioWavResultV1,
) -> Result<V4AudioWavResultV1, EditorError> {
    if input.output_range.is_some() {
        return Err(stored_invalid());
    }
    let prepared = validate_prepared(input).map_err(|_| stored_invalid())?;
    let public = &stored.public;
    let identity = &public.identity;
    if public.schema_version != 1
        || identity.project_id != input.snapshot.project.id
        || identity.revision != input.snapshot.revision
        || identity.sequence_id != input.sequence_id
        || identity.sample_rate != input.sample_rate
        || identity.channels != 2
        || identity.frame_count != prepared.duration_samples().to_string()
    {
        return Err(stored_invalid());
    }
    validate_result_common(
        input,
        &prepared,
        ResultEvidence {
            asset_lock: &public.asset_lock,
            input_sha256: &public.render_input_sha256,
            expected_sha256: &render_input_sha256(
                &input.snapshot,
                &input.sequence_id,
                input.sample_rate,
            )?,
            execution: &public.execution,
            public_artifact: &public.artifact,
            artifact: &stored.artifact,
        },
        prepared.duration_samples(),
    )?;
    Ok(public.clone())
}
pub fn validate_stored_range_result(
    input: &PreparedV4AudioWav,
    stored: &StoredV4AudioRangeWavResultV1,
) -> Result<V4AudioRangeWavResultV1, EditorError> {
    let prepared = validate_prepared(input).map_err(|_| stored_invalid())?;
    let range = input.output_range.as_ref().ok_or_else(stored_invalid)?;
    let public = &stored.public;
    let identity = &public.identity;
    if public.schema_version != 1
        || identity.project_id != input.snapshot.project.id
        || identity.revision != input.snapshot.revision
        || identity.sequence_id != input.sequence_id
        || identity.sample_rate != input.sample_rate
        || identity.channels != 2
        || identity.start_sample != range.start_sample
        || identity.frame_count != range.frame_count
        || identity.whole_scene_frame_count != prepared.duration_samples().to_string()
    {
        return Err(stored_invalid());
    }
    let public_range = V4AudioSampleRangeV1 {
        start_sample: range.start_sample.clone(),
        frame_count: range.frame_count.clone(),
    };
    validate_result_common(
        input,
        &prepared,
        ResultEvidence {
            asset_lock: &public.asset_lock,
            input_sha256: &public.render_input_sha256,
            expected_sha256: &render_range_input_sha256(
                &input.snapshot,
                &input.sequence_id,
                input.sample_rate,
                &public_range,
            )?,
            execution: &public.execution,
            public_artifact: &public.artifact,
            artifact: &stored.artifact,
        },
        crate::classic_audio::decimal(&range.frame_count)?,
    )?;
    Ok(public.clone())
}
struct ResultEvidence<'a> {
    asset_lock: &'a AssetLockManifest,
    input_sha256: &'a str,
    expected_sha256: &'a str,
    execution: &'a V4AudioExecutionSummaryV1,
    public_artifact: &'a V4AudioArtifactMetadataV1,
    artifact: &'a ArtifactDescriptor,
}
fn validate_result_common(
    input: &PreparedV4AudioWav,
    prepared: &PreparedAudioV4,
    evidence: ResultEvidence<'_>,
    output_frames: u64,
) -> Result<(), EditorError> {
    let ResultEvidence {
        asset_lock,
        input_sha256,
        expected_sha256,
        execution,
        public_artifact,
        artifact,
    } = evidence;
    // Independently check the native WAV writer's fixed Float32 stereo layout:
    // 58 bytes of RIFF/fmt/fact/data headers and eight bytes per exact frame.
    // The RIFF length excludes its initial eight bytes; both chunks use u32.
    let expected_wav_size = output_frames
        .checked_mul(8)
        .filter(|&bytes| bytes <= u64::from(u32::MAX))
        .and_then(|bytes| bytes.checked_add(58))
        .filter(|&bytes| bytes - 8 <= u64::from(u32::MAX))
        .ok_or_else(stored_invalid)?;
    validate_execution_summary(prepared, execution)?;
    if asset_lock != &input.asset_lock
        || input_sha256 != expected_sha256
        || artifact.mime_type != "audio/wav"
        || public_artifact.mime_type != "audio/wav"
        || artifact.size != expected_wav_size
        || public_artifact.size_bytes != artifact.size.to_string()
        || public_artifact.content_hash != artifact.content_hash
        || !valid_hash(&artifact.content_hash)
    {
        return Err(stored_invalid());
    }
    Ok(())
}

/// Shared whole-scene Classic execution receipt validation for audio and video.
pub(crate) fn validate_execution_summary(
    prepared: &PreparedAudioV4,
    execution: &V4AudioExecutionSummaryV1,
) -> Result<(), EditorError> {
    let decoder_profiles: BTreeSet<_> = prepared
        .resources()
        .map(|r| match r.kind {
            AssetKind::Audio => Ok(V4AudioDecoderProfileV1::WebAudioDecodeAtTargetRate),
            AssetKind::Video => {
                Ok(V4AudioDecoderProfileV1::Mediabunny141ConcatenateOfflineResample)
            }
            _ => Err(stored_invalid()),
        })
        .collect::<Result<_, _>>()?;
    let pitch_profiles: BTreeSet<_> = prepared
        .voices()
        .map(|v| match v.processing {
            AudioProcessingV4::LinearInterpolationZeroTail => {
                Ok(V4AudioPitchProfileV1::UnpreparedLinear)
            }
            AudioProcessingV4::ClassicPreparedLinear => Ok(V4AudioPitchProfileV1::PreparedLinear),
            AudioProcessingV4::NativeMaintainPitch => Ok(V4AudioPitchProfileV1::PreparedSoundtouch),
            _ => Err(stored_invalid()),
        })
        .collect::<Result<_, _>>()?;
    let observed_decoders: BTreeSet<_> = execution.decoder_profiles.iter().copied().collect();
    let observed_pitch: BTreeSet<_> = execution.pitch_profiles.iter().copied().collect();
    if execution.playwright_version != "1.63.0"
        || execution.mediabunny_version != "1.41.0"
        || execution.soundtouch_version != "0.3.0"
        || !valid_id(&execution.browser_version)
        || [
            &execution.host_sha256,
            &execution.browser_sha256,
            &execution.module_sha256,
        ]
        .into_iter()
        .any(|h| !valid_hash(h))
        || observed_decoders != decoder_profiles
        || observed_decoders.len() != execution.decoder_profiles.len()
        || observed_pitch != pitch_profiles
        || observed_pitch.len() != execution.pitch_profiles.len()
    {
        return Err(stored_invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
