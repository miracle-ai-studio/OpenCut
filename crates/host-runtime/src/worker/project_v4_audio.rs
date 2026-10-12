//! Actual fixed-revision Classic browser PCM -> private WAV worker execution.
use super::output_attempt::{
    attempt_directory, cancelled, check_attempt, check_output_name, error, file_hash, io_error,
};
use crate::classic_audio_runtime;
use opencut_composition::project_v4::ClassicAudioPreparationModeV4;
use opencut_contracts::EditorError;
use opencut_editor_application::{
    budget::Writer,
    classic_audio::{
        ClassicAudioExecutor, ClassicAudioRange, ClassicAudioResources, ClassicAudioSession,
    },
    project_v4_audio_jobs::{
        self as audio, PreparedV4AudioWav, StoredV4AudioRangeWavResultV1, StoredV4AudioWavResultV1,
    },
};
use opencut_job_contracts::{ArtifactDescriptor, project_v4_audio::*};
use opencut_media::CancellationToken;
use opencut_native_services::classic_audio::{
    ClassicAudioProcess, ClassicAudioWavSink, VerifiedClassicAudioResources,
};
use opencut_sqlite_store::{EditorApplication, JobRecord};
use opencut_storage_budget::{AttemptLease, Budget};
use serde_json::Value;
use std::{collections::BTreeSet, ffi::OsStr, path::Path, sync::Mutex};

pub(super) fn execute(
    database: &Path,
    root: &Path,
    worker: &str,
    job: &JobRecord,
    cancel: &CancellationToken,
    held: &Mutex<Option<AttemptLease>>,
) -> Result<Value, EditorError> {
    cancelled(cancel)?;
    let input: PreparedV4AudioWav = serde_json::from_value(job.input.clone())
        .map_err(|_| error("INVALID_ARGUMENT", "Invalid private V4 audio preparation"))?;
    let prepared = audio::validate_prepared(&input)?;
    if (job.kind == V4_AUDIO_RANGE_WAV_KIND) != input.output_range.is_some() {
        return Err(error(
            "INVALID_ARGUMENT",
            "Audio kind differs from its fixed output selection",
        ));
    }
    let mut session = ClassicAudioSession::new_with_range(
        &prepared,
        input.processing_policy.max_pcm_bytes,
        input.processing_policy.max_encoded_bytes,
        input.output_range.as_ref().map(|r| ClassicAudioRange {
            start_sample: r.start_sample.clone(),
            frame_count: r.frame_count.clone(),
        }),
    )?;
    let output_frames = session.output_frame_count();
    let app = EditorApplication::open_managed(database)?;
    app.revalidate_v4_audio_job(&job.job_id, worker, job.attempt, &input.authority)?;
    let config = classic_audio_runtime::configured(&input.processing_policy)?;
    let expected_bytes = ClassicAudioWavSink::checked_bytes(input.sample_rate, output_frames)?;
    let workspace = root
        .parent()
        .ok_or_else(|| error("STORAGE_BUDGET_ERROR", "Missing managed output workspace"))?;
    let budget = Budget::new(workspace);
    let _reservation = budget.reserve_audio_wav(
        expected_bytes,
        Writer {
            job_id: job.job_id.clone(),
            attempt: job.attempt,
        },
    )?;
    let directory = attempt_directory(root, job)?;
    let attempt = AttemptLease::acquire(&directory).map_err(io_error)?;
    check_attempt(&attempt, &directory)?;
    {
        let mut guard = held
            .lock()
            .map_err(|_| error("INTERNAL_ERROR", "Attempt lifetime guard is unavailable"))?;
        if guard.is_some() {
            return Err(error("INTERNAL_ERROR", "An output attempt is already held"));
        }
        *guard = Some(attempt.clone());
    }
    // The original caller, current lease, captured policy and cancellation are
    // checked on every CAS operation and after browser termination. The CAS
    // collection gate remains held until all private output verification ends.
    let mut resources =
        VerifiedClassicAudioResources::new(app.asset_store_root(), &prepared, || {
            cancelled(cancel)?;
            check_attempt(&attempt, &directory)?;
            app.revalidate_v4_audio_job(&job.job_id, worker, job.attempt, &input.authority)
        })?;
    let handles = resources.handles();
    resources.checkpoint()?;
    let filename = OsStr::new("audio.wav");
    let file = attempt.create_readwrite_child(filename).map_err(io_error)?;
    let mut sink = ClassicAudioWavSink::new(file, input.sample_rate, output_frames)?;
    let host_sha256 =
        opencut_native_services::reproducibility::execution_identity(vec![])?.process_binary_sha256;
    resources.checkpoint()?;
    let execution = ClassicAudioProcess { config }.execute(
        &mut session,
        &mut resources,
        &handles,
        &mut sink,
    )?;
    resources.checkpoint()?;
    let mut file = sink.finalize()?;
    let content_hash = file_hash(&mut file, expected_bytes, cancel)?;
    attempt.sync_directory().map_err(io_error)?;
    check_output_name(&attempt, filename, &file)?;
    resources.checkpoint()?;

    let decoder_profiles = execution
        .receipt
        .decoders
        .iter()
        .map(|decoder| match decoder.profile.as_str() {
            "web-audio-decode-at-target-rate" => {
                Ok(V4AudioDecoderProfileV1::WebAudioDecodeAtTargetRate)
            }
            "mediabunny-1.41.0-concatenate-offline-resample" => {
                Ok(V4AudioDecoderProfileV1::Mediabunny141ConcatenateOfflineResample)
            }
            _ => Err(error(
                "ARTIFACT_VERIFICATION_FAILED",
                "Unknown actual audio decoder profile",
            )),
        })
        .collect::<Result<BTreeSet<_>, _>>()?
        .into_iter()
        .collect();
    let pitch_profiles = execution
        .receipt
        .preparation
        .iter()
        .map(|receipt| match receipt.facts.mode {
            ClassicAudioPreparationModeV4::UnpreparedLinear => {
                V4AudioPitchProfileV1::UnpreparedLinear
            }
            ClassicAudioPreparationModeV4::PreparedLinear => V4AudioPitchProfileV1::PreparedLinear,
            ClassicAudioPreparationModeV4::PreparedSoundtouch => {
                V4AudioPitchProfileV1::PreparedSoundtouch
            }
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let identity = execution.runtime_identity.identity;
    let summary = V4AudioExecutionSummaryV1 {
        host_sha256: host_sha256.clone(),
        browser_version: identity.browser_version,
        browser_sha256: identity.browser_sha256,
        module_sha256: identity.module_sha256,
        playwright_version: identity.playwright_version,
        mediabunny_version: identity.mediabunny_version,
        soundtouch_version: "0.3.0".into(),
        decoder_profiles,
        pitch_profiles,
        mastering_profile: V4AudioMasteringProfileV1::ClassicExportWholeSceneWebAudioV31,
        mastering_applied: execution.receipt.mastering.applied,
    };
    let public_artifact = V4AudioArtifactMetadataV1 {
        mime_type: "audio/wav".into(),
        size_bytes: expected_bytes.to_string(),
        content_hash: content_hash.clone(),
    };
    let artifact = ArtifactDescriptor {
        path: directory
            .join(filename)
            .into_os_string()
            .into_string()
            .map_err(|_| {
                error(
                    "ARTIFACT_VERIFICATION_FAILED",
                    "Private output location is not UTF-8",
                )
            })?,
        mime_type: "audio/wav".into(),
        size: expected_bytes,
        content_hash,
    };
    let value = if let Some(range) = &input.output_range {
        let selection = V4AudioSampleRangeV1 {
            start_sample: range.start_sample.clone(),
            frame_count: range.frame_count.clone(),
        };
        let stored = StoredV4AudioRangeWavResultV1 {
            public: V4AudioRangeWavResultV1 {
                schema_version: 1,
                identity: V4AudioRangeWavIdentityV1 {
                    project_id: input.snapshot.project.id.clone(),
                    revision: input.snapshot.revision.clone(),
                    sequence_id: input.sequence_id.clone(),
                    profile: V4AudioProfileV1::ClassicExportV31WebAudio,
                    sample_rate: input.sample_rate,
                    channels: 2,
                    start_sample: selection.start_sample.clone(),
                    frame_count: selection.frame_count.clone(),
                    whole_scene_frame_count: prepared.duration_samples().to_string(),
                    sample_format: V4AudioSampleFormatV1::IeeeFloat32Le,
                },
                asset_lock: input.asset_lock.clone(),
                render_input_sha256: audio::render_range_input_sha256(
                    &input.snapshot,
                    &input.sequence_id,
                    input.sample_rate,
                    &selection,
                )?,
                execution: summary,
                artifact: public_artifact,
            },
            artifact,
        };
        audio::validate_stored_range_result(&input, &stored)?;
        serde_json::to_value(stored)
    } else {
        let stored = StoredV4AudioWavResultV1 {
            public: V4AudioWavResultV1 {
                schema_version: 1,
                identity: V4AudioWavIdentityV1 {
                    project_id: input.snapshot.project.id.clone(),
                    revision: input.snapshot.revision.clone(),
                    sequence_id: input.sequence_id.clone(),
                    profile: V4AudioProfileV1::ClassicExportV31WebAudio,
                    sample_rate: input.sample_rate,
                    channels: 2,
                    frame_count: output_frames.to_string(),
                    sample_format: V4AudioSampleFormatV1::IeeeFloat32Le,
                },
                asset_lock: input.asset_lock.clone(),
                render_input_sha256: audio::render_input_sha256(
                    &input.snapshot,
                    &input.sequence_id,
                    input.sample_rate,
                )?,
                execution: summary,
                artifact: public_artifact,
            },
            artifact,
        };
        audio::validate_stored_result(&input, &stored)?;
        serde_json::to_value(stored)
    }
    .map_err(|_| {
        error(
            "SERIALIZATION_ERROR",
            "Could not serialize private audio result",
        )
    })?;
    resources.checkpoint()?;
    if opencut_native_services::reproducibility::execution_identity(vec![])?.process_binary_sha256
        != host_sha256
    {
        return Err(error(
            "EXECUTION_IDENTITY_CHANGED",
            "Executing host identity changed during audio processing",
        ));
    }
    check_output_name(&attempt, filename, &file)?;
    Ok(value)
    // `held` owns a cloned inode lease after return, through queue publication.
}
