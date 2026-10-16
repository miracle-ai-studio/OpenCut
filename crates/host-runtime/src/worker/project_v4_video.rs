//! Whole-scene V4 shared media/text/caption/graphic RGBA + Classic PCM, encoded in one private
//! attempt. Output remains tentative until the normal queue publication fence.
use super::{
    media_output::TaskOutput,
    output_attempt::{
        attempt_directory, cancelled, check_attempt, check_output_name, error, file_hash, io_error,
    },
};
use crate::{classic_audio_runtime, classic_visual_runtime};
use opencut_composition::project_v4::ClassicAudioPreparationModeV4;
use opencut_contracts::EditorError;
use opencut_editor_application::{
    budget::Writer,
    classic_audio::{ClassicAudioExecutor, ClassicAudioResources, ClassicAudioSession},
    classic_blend_frame_v1::ClassicBlendFrameSessionV1,
    classic_graphic_frame_v1::ClassicGraphicFrameSessionV1,
    classic_mask_frame_v1::ClassicMaskFrameSessionV1,
    classic_scene_frame_v1::ClassicSceneFrameSessionV1,
    classic_text_frame_v2::ClassicTextFrameSessionV2,
    classic_text_frame_v3::ClassicCaptionFrameSessionV3,
    classic_visual::ClassicVisualResources,
    export_video_v2 as export,
    font_import_v1::FontImportPolicyV1,
    project_v4_blend_video_jobs as blend, project_v4_caption_video_jobs as caption,
    project_v4_graphic_video_jobs as graphic, project_v4_mask_video_jobs as mask,
    project_v4_scene_video_jobs as scene,
    project_v4_video_jobs::{self as video, PreparedV4VideoMp4V1, StoredV4VideoMp4ResultV1},
};
use opencut_job_contracts::{
    ArtifactDescriptor,
    project_v4_audio::*,
    project_v4_frame::{V4FrameArtifactMetadataV1, V4FrameExecutionSummaryV1},
    project_v4_video::*,
};
use opencut_media::CancellationToken;
use opencut_native_services::{
    classic_audio::{ClassicAudioProcess, ClassicAudioWavSink, VerifiedClassicAudioResources},
    classic_video::{
        ClassicVideoEncoder, ClassicVideoEncoderConfig, ClassicVideoProcess, resolve_tool,
    },
    classic_visual::VerifiedClassicTextResources,
};
use opencut_sqlite_store::{EditorApplication, JobRecord};
use opencut_storage_budget::{AttemptLease, Budget};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::OsStr,
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};
#[cfg(unix)]
pub(super) fn execute(
    database: &Path,
    root: &Path,
    worker: &str,
    job: &JobRecord,
    cancel: &CancellationToken,
    held: &Mutex<Option<AttemptLease>>,
) -> Result<Value, EditorError> {
    execute_with_profile(
        database,
        root,
        worker,
        job,
        cancel,
        held,
        video::VideoFrameProfile::TextV2,
    )
}
#[cfg(unix)]
pub(super) fn execute_with_profile(
    database: &Path,
    root: &Path,
    worker: &str,
    job: &JobRecord,
    cancel: &CancellationToken,
    held: &Mutex<Option<AttemptLease>>,
    profile: video::VideoFrameProfile,
) -> Result<Value, EditorError> {
    execute_kernel(database, root, worker, job, cancel, held, Some(profile))
}
#[cfg(unix)]
pub(super) fn execute_export_v2(
    database: &Path,
    root: &Path,
    worker: &str,
    job: &JobRecord,
    cancel: &CancellationToken,
    held: &Mutex<Option<AttemptLease>>,
) -> Result<Value, EditorError> {
    execute_kernel(database, root, worker, job, cancel, held, None)
}
#[cfg(unix)]
fn execute_kernel(
    database: &Path,
    root: &Path,
    worker: &str,
    job: &JobRecord,
    cancel: &CancellationToken,
    held: &Mutex<Option<AttemptLease>>,
    selected_profile: Option<video::VideoFrameProfile>,
) -> Result<Value, EditorError> {
    let export_input: Option<export::PreparedExportVideoV2> = if selected_profile.is_none() {
        if job.kind != export::EXPORT_VIDEO_V2_KIND {
            return Err(error(
                "INVALID_ARGUMENT",
                "Configurable export kind differs",
            ));
        }
        Some(
            serde_json::from_value(job.input.clone())
                .map_err(|_| error("INVALID_ARGUMENT", "Invalid private configurable export"))?,
        )
    } else {
        None
    };
    let profile = selected_profile
        .unwrap_or_else(|| export::frame_profile(export_input.as_ref().unwrap().visual_profile));
    if export_input.is_none() && job.kind != profile.kind() {
        return Err(error(
            "INVALID_ARGUMENT",
            "Video kind differs from selected execution profile",
        ));
    }
    cancelled(cancel)?;
    let started = Instant::now();
    let input: PreparedV4VideoMp4V1 = if let Some(input) = &export_input {
        input.execution.clone()
    } else {
        serde_json::from_value(job.input.clone())
            .map_err(|_| error("INVALID_ARGUMENT", "Invalid private V4 video preparation"))?
    };
    let prepared = if let Some(input) = &export_input {
        export::prepare(input)?
    } else {
        let fixed = video::validate_prepared_with_profile(&input, profile)?;
        video::PreparedVideoSelectionV2 {
            identity: fixed.identity,
            audio: Some(fixed.audio),
            asset_ids: fixed.asset_ids,
            font_ids: fixed.font_ids,
        }
    };
    let app = EditorApplication::open_managed(database)?;
    let revalidate = || {
        if export_input.is_some() {
            return app.revalidate_export_video_v2(
                &job.job_id,
                worker,
                job.attempt,
                &input.authority,
            );
        }
        match profile {
            video::VideoFrameProfile::TextV2 => {
                app.revalidate_v4_video_job(&job.job_id, worker, job.attempt, &input.authority)
            }
            video::VideoFrameProfile::CaptionV3 => app.revalidate_v4_caption_video_job(
                &job.job_id,
                worker,
                job.attempt,
                &input.authority,
            ),
            video::VideoFrameProfile::GraphicV1 => app.revalidate_v4_graphic_video_job(
                &job.job_id,
                worker,
                job.attempt,
                &input.authority,
            ),
            video::VideoFrameProfile::SceneV1 => app.revalidate_v4_scene_video_job(
                &job.job_id,
                worker,
                job.attempt,
                &input.authority,
            ),
            video::VideoFrameProfile::MaskV1 => {
                app.revalidate_v4_mask_video_job(&job.job_id, worker, job.attempt, &input.authority)
            }
            video::VideoFrameProfile::BlendV1 => app.revalidate_v4_blend_video_job(
                &job.job_id,
                worker,
                job.attempt,
                &input.authority,
            ),
        }
    };
    revalidate()?;
    let policy = &input.processing_policy;
    let deadline = Duration::from_millis(policy.deadline_ms);
    let count = prepared
        .identity
        .frame_count
        .parse::<u64>()
        .map_err(|_| error("INVALID_ARGUMENT", "Invalid fixed video frame count"))?;
    let host_sha256 =
        opencut_native_services::reproducibility::execution_identity(vec![])?.process_binary_sha256;
    let workspace = root
        .parent()
        .ok_or_else(|| error("STORAGE_BUDGET_ERROR", "Missing managed output workspace"))?;
    let directory = attempt_directory(root, job)?;
    let attempt = AttemptLease::acquire(&directory).map_err(io_error)?;
    check_attempt(&attempt, &directory)?;
    {
        let mut guard = held
            .lock()
            .map_err(|_| error("INTERNAL_ERROR", "Attempt lifetime guard unavailable"))?;
        if guard.is_some() {
            return Err(error("INTERNAL_ERROR", "Output attempt already held"));
        }
        *guard = Some(attempt.clone());
    }
    let checkpoint = || {
        cancelled(cancel)?;
        check_attempt(&attempt, &directory)?;
        if started.elapsed() >= deadline {
            return Err(error(
                "VIDEO_TIME_LIMIT",
                "Whole video attempt exceeded captured deadline",
            ));
        }
        revalidate()
    };
    checkpoint()?;
    let audio_name = OsStr::new("video-audio.wav");
    let budget = Budget::new(workspace);
    let writer = Writer {
        job_id: job.job_id.clone(),
        attempt: job.attempt,
    };
    let (audio_file, audio_summary, _audio_reservation) = if let Some(audio) = &prepared.audio {
        let audio_frames = audio.duration_samples();
        let audio_bytes = ClassicAudioWavSink::checked_bytes(48000, audio_frames)?;
        let _audio_reservation = budget.reserve_audio_wav(audio_bytes, writer.clone())?;
        let audio_file = attempt
            .create_readwrite_child(audio_name)
            .map_err(io_error)?;
        let mut audio_sink = ClassicAudioWavSink::new(audio_file, 48000, audio_frames)?;
        let mut audio_resources =
            VerifiedClassicAudioResources::new(app.asset_store_root(), audio, checkpoint)?;
        let audio_handles = audio_resources.handles();
        let mut audio_session = ClassicAudioSession::new(
            audio,
            policy.audio.max_pcm_bytes,
            policy.audio.max_encoded_bytes,
        )?;
        let audio_config = classic_audio_runtime::configured(&policy.audio)?;
        let audio_execution = ClassicAudioProcess {
            config: audio_config,
        }
        .execute(
            &mut audio_session,
            &mut audio_resources,
            &audio_handles,
            &mut audio_sink,
        )?;
        audio_resources.checkpoint()?;
        let audio_file = audio_sink.finalize()?;
        check_output_name(&attempt, audio_name, &audio_file)?;
        audio_file.sync_all().map_err(io_error)?;
        let decoder_profiles = audio_execution
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
        let pitch_profiles = audio_execution
            .receipt
            .preparation
            .iter()
            .map(|r| match r.facts.mode {
                ClassicAudioPreparationModeV4::UnpreparedLinear => {
                    V4AudioPitchProfileV1::UnpreparedLinear
                }
                ClassicAudioPreparationModeV4::PreparedLinear => {
                    V4AudioPitchProfileV1::PreparedLinear
                }
                ClassicAudioPreparationModeV4::PreparedSoundtouch => {
                    V4AudioPitchProfileV1::PreparedSoundtouch
                }
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let audio_identity = audio_execution.runtime_identity.identity;
        let audio_summary = V4AudioExecutionSummaryV1 {
            host_sha256: host_sha256.clone(),
            browser_version: audio_identity.browser_version,
            browser_sha256: audio_identity.browser_sha256,
            module_sha256: audio_identity.module_sha256,
            playwright_version: audio_identity.playwright_version,
            mediabunny_version: audio_identity.mediabunny_version,
            soundtouch_version: "0.3.0".into(),
            decoder_profiles,
            pitch_profiles,
            mastering_profile: V4AudioMasteringProfileV1::ClassicExportWholeSceneWebAudioV31,
            mastering_applied: audio_execution.receipt.mastering.applied,
        };
        drop(audio_resources);
        (
            Some(audio_file),
            Some(audio_summary),
            Some(_audio_reservation),
        )
    } else {
        (None, None, None)
    };
    checkpoint()?;
    let ffmpeg = resolve_tool(
        std::env::var_os("OPENCUT_FFMPEG")
            .map(Into::into)
            .unwrap_or_else(|| "ffmpeg".into()),
    )?;
    let ffprobe = resolve_tool(
        std::env::var_os("OPENCUT_FFPROBE")
            .map(Into::into)
            .unwrap_or_else(|| "ffprobe".into()),
    )?;
    let output_name = OsStr::new(match export_input.as_ref().map(|i| i.options.format) {
        Some(export::ExportFormatV2::Webm) => "video.webm",
        _ => "video.mp4",
    });
    let output_path = directory.join(output_name);
    let admission = TaskOutput {
        budget: budget.clone(),
        attempt: attempt.clone(),
        directory: directory.clone(),
        writer,
    };
    let encoder_config = ClassicVideoEncoderConfig {
        ffmpeg,
        ffprobe,
        output: output_path.clone(),
        audio: directory.join(audio_name),
        width: prepared.identity.width,
        height: prepared.identity.height,
        fps_numerator: prepared.identity.fps.numerator,
        fps_denominator: prepared.identity.fps.denominator,
        frame_count: count,
        max_output_bytes: policy.max_output_bytes,
        max_rss_bytes: policy.visual.max_rss_bytes,
        deadline: deadline.saturating_sub(started.elapsed()),
    };
    let mut encoder = if let Some(export) = &export_input {
        ClassicVideoEncoder::start_v2(
            encoder_config,
            export.options.clone(),
            &admission,
            checkpoint,
        )?
    } else {
        ClassicVideoEncoder::start(encoder_config, &admission, checkpoint)?
    };
    let mut config = match profile {
        video::VideoFrameProfile::TextV2 => {
            classic_visual_runtime::configured_text(&policy.visual)?
        }
        video::VideoFrameProfile::CaptionV3 => {
            classic_visual_runtime::configured_caption(&policy.visual)?
        }
        video::VideoFrameProfile::GraphicV1 => {
            classic_visual_runtime::configured_graphic(&policy.visual)?
        }
        video::VideoFrameProfile::SceneV1 => {
            classic_visual_runtime::configured_scene(&policy.visual)?
        }
        video::VideoFrameProfile::MaskV1 => {
            classic_visual_runtime::configured_mask(&policy.visual)?
        }
        video::VideoFrameProfile::BlendV1 => {
            classic_visual_runtime::configured_blend(&policy.visual)?
        }
    };
    config.runner = config
        .runner
        .parent()
        .ok_or_else(|| error("CAPABILITY_UNAVAILABLE", "Missing visual runner directory"))?
        .join("video-runner.mjs");
    if !config.runner.is_file() {
        return Err(error(
            "CAPABILITY_UNAVAILABLE",
            "Persistent video runner unavailable",
        ));
    }
    let mut process = match profile {
        video::VideoFrameProfile::TextV2 => {
            ClassicVideoProcess::start(config, count, &mut || checkpoint())?
        }
        video::VideoFrameProfile::CaptionV3 => {
            ClassicVideoProcess::start_caption(config, count, &mut || checkpoint())?
        }
        video::VideoFrameProfile::GraphicV1 => {
            ClassicVideoProcess::start_graphic(config, count, &mut || checkpoint())?
        }
        video::VideoFrameProfile::SceneV1 => {
            ClassicVideoProcess::start_scene(config, count, &mut || checkpoint())?
        }
        video::VideoFrameProfile::MaskV1 => {
            ClassicVideoProcess::start_mask(config, count, &mut || checkpoint())?
        }
        video::VideoFrameProfile::BlendV1 => {
            ClassicVideoProcess::start_blend(config, count, &mut || checkpoint())?
        }
    };
    let mut receipts = Sha256::new();
    let mut rgba_bytes = 0u64;
    for index in 0..count {
        checkpoint()?;
        let at = video::frame_time(&prepared.identity, index)?;
        let selected =
            video::prepare_profiled_frame(&input.snapshot, &input.sequence_id, &at, profile)?;
        selected.admit(&policy.visual)?;
        let mut resources = match &selected {
            video::PreparedVideoFrame::Text(frame) => VerifiedClassicTextResources::new(
                app.asset_store_root(),
                frame,
                &FontImportPolicyV1::default(),
                checkpoint,
            )?,
            video::PreparedVideoFrame::Caption(frame) => {
                VerifiedClassicTextResources::new_caption_frame_v3(
                    app.asset_store_root(),
                    frame,
                    &FontImportPolicyV1::default(),
                    checkpoint,
                )?
            }
            video::PreparedVideoFrame::Graphic(frame) => {
                VerifiedClassicTextResources::new_caption_frame_v3(
                    app.asset_store_root(),
                    frame.execution(),
                    &FontImportPolicyV1::default(),
                    checkpoint,
                )?
            }
            video::PreparedVideoFrame::Scene(frame) => {
                VerifiedClassicTextResources::new_scene_frame_v1(
                    app.asset_store_root(),
                    frame,
                    &FontImportPolicyV1::default(),
                    checkpoint,
                )?
            }
            video::PreparedVideoFrame::Mask(frame) => {
                VerifiedClassicTextResources::new_caption_frame_v3(
                    app.asset_store_root(),
                    frame.execution().execution(),
                    &FontImportPolicyV1::default(),
                    checkpoint,
                )?
            }
            video::PreparedVideoFrame::Blend(frame) => {
                VerifiedClassicTextResources::new_caption_frame_v3(
                    app.asset_store_root(),
                    frame.execution().execution(),
                    &FontImportPolicyV1::default(),
                    checkpoint,
                )?
            }
        };
        let handles = resources.handles();
        let fonts = resources.font_handles();
        let bytes = match &selected {
            video::PreparedVideoFrame::Text(frame) => {
                let mut session =
                    ClassicTextFrameSessionV2::new(frame, &policy.visual, resources.coverage())?;
                let receipt = process.render_frame(
                    &mut session,
                    &mut resources,
                    &handles,
                    &fonts,
                    &mut encoder,
                )?;
                serde_json::to_vec(&receipt)
            }
            video::PreparedVideoFrame::Caption(frame) => {
                let mut session =
                    ClassicCaptionFrameSessionV3::new(frame, &policy.visual, resources.coverage())?;
                let receipt = process.render_caption_frame(
                    &mut session,
                    &mut resources,
                    &handles,
                    &fonts,
                    &mut encoder,
                )?;
                serde_json::to_vec(&receipt)
            }
            video::PreparedVideoFrame::Graphic(frame) => {
                let mut session =
                    ClassicGraphicFrameSessionV1::new(frame, &policy.visual, resources.coverage())?;
                let receipt = process.render_graphic_frame(
                    &mut session,
                    &mut resources,
                    &handles,
                    &fonts,
                    &mut encoder,
                )?;
                serde_json::to_vec(&receipt)
            }
            video::PreparedVideoFrame::Scene(frame) => {
                let mut session =
                    ClassicSceneFrameSessionV1::new(frame, &policy.visual, resources.coverage())?;
                let receipt = process.render_scene_frame(
                    &mut session,
                    &mut resources,
                    &handles,
                    &fonts,
                    &mut encoder,
                )?;
                serde_json::to_vec(&receipt)
            }
            video::PreparedVideoFrame::Mask(frame) => {
                let mut session =
                    ClassicMaskFrameSessionV1::new(frame, &policy.visual, resources.coverage())?;
                let receipt = process.render_mask_frame(
                    &mut session,
                    &mut resources,
                    &handles,
                    &fonts,
                    &mut encoder,
                )?;
                serde_json::to_vec(&receipt)
            }
            video::PreparedVideoFrame::Blend(frame) => {
                let mut session =
                    ClassicBlendFrameSessionV1::new(frame, &policy.visual, resources.coverage())?;
                let receipt = process.render_blend_frame(
                    &mut session,
                    &mut resources,
                    &handles,
                    &fonts,
                    &mut encoder,
                )?;
                serde_json::to_vec(&receipt)
            }
        }
        .map_err(|_| {
            error(
                "SERIALIZATION_ERROR",
                "Cannot hash complete verified video frame receipt",
            )
        })?;
        encoder.finish_frame()?;
        resources.checkpoint()?;
        receipts.update(index.to_le_bytes());
        receipts.update((bytes.len() as u64).to_le_bytes());
        receipts.update(&bytes);
        rgba_bytes = rgba_bytes
            .checked_add(
                u64::from(prepared.identity.width) * u64::from(prepared.identity.height) * 4,
            )
            .ok_or_else(|| error("VIDEO_BUDGET_EXCEEDED", "Video byte counter overflow"))?;
    }
    let (runtime, _quota) = process.finish(&mut || checkpoint())?;
    checkpoint()?;
    if let Some(audio_file) = &audio_file {
        check_output_name(&attempt, audio_name, audio_file)?;
    }
    let observed = encoder.finish(&admission)?;
    checkpoint()?;
    let mut file = attempt.open_readonly_child(output_name).map_err(io_error)?;
    let size = file.metadata().map_err(io_error)?.len();
    if size == 0 || size > policy.max_output_bytes {
        return Err(error(
            "ARTIFACT_VERIFICATION_FAILED",
            "MP4 differs from captured output admission",
        ));
    }
    let content_hash = file_hash(&mut file, size, cancel)?;
    file.sync_all().map_err(io_error)?;
    attempt.sync_directory().map_err(io_error)?;
    check_output_name(&attempt, output_name, &file)?;
    checkpoint()?;
    let identity = runtime.identity;
    let visual_summary = V4FrameExecutionSummaryV1 {
        host_sha256: host_sha256.clone(),
        browser_version: identity.browser_version,
        browser_sha256: identity.browser_sha256,
        module_sha256: identity.module_sha256,
        shader_sha256: identity.shader_sha256,
        playwright_version: identity.playwright_version,
        mediabunny_version: identity.mediabunny_version,
        gpu_backend: identity.gpu_backend,
        gpu_device: identity.gpu_device,
        texture_format: identity.texture_format,
        decoder_profile: identity.decoder_profile,
    };
    let result = if let Some(export_input) = &export_input {
        let format = export_input.options.format;
        let stored = export::StoredExportVideoV2 {
            public: export::ExportVideoResultV2 {
                schema_version: 2,
                identity: export::public_identity(export_input, &prepared),
                asset_lock: input.asset_lock.clone(),
                font_lock: input.font_lock.clone(),
                render_input_sha256: export::render_input_sha256(export_input)?,
                execution: export::ExportVideoExecutionV2 {
                    visual: visual_summary,
                    audio: audio_summary,
                    encoder: export::ExportVideoEncoderSummaryV2 {
                        ffmpeg_sha256: observed.ffmpeg_sha256,
                        ffprobe_sha256: observed.ffprobe_sha256,
                        video_codec: format.video_codec().into(),
                        audio_codec: export_input
                            .options
                            .include_audio
                            .then(|| format.audio_codec().into()),
                        pixel_format: "yuv420p".into(),
                        rates: export::classic_export_rates_v2(
                            &export_input.options,
                            prepared.identity.width,
                            prepared.identity.height,
                        )
                        .ok_or_else(|| {
                            error("INVALID_ARGUMENT", "Invalid captured export bitrate")
                        })?,
                        observed_video_frames: observed.video_frames.to_string(),
                        observed_video_duration_seconds: observed.video_duration_seconds,
                        observed_audio_duration_seconds: export_input
                            .options
                            .include_audio
                            .then_some(observed.audio_duration_seconds),
                    },
                    frame_receipts_sha256: format!("{:x}", receipts.finalize()),
                    frames_rendered: count.to_string(),
                    rgba_bytes: rgba_bytes.to_string(),
                },
                artifact: V4FrameArtifactMetadataV1 {
                    mime_type: format.mime_type().into(),
                    size_bytes: size.to_string(),
                    content_hash: content_hash.clone(),
                },
            },
            artifact: ArtifactDescriptor {
                path: output_path.into_os_string().into_string().map_err(|_| {
                    error(
                        "ARTIFACT_VERIFICATION_FAILED",
                        "Private export output is not UTF-8",
                    )
                })?,
                mime_type: format.mime_type().into(),
                size,
                content_hash,
            },
        };
        export::validate_stored_result(export_input, &stored)?;
        serde_json::to_value(stored).map_err(|_| {
            error(
                "SERIALIZATION_ERROR",
                "Cannot serialize configurable export result",
            )
        })?
    } else {
        let stored = StoredV4VideoMp4ResultV1 {
            public: V4VideoMp4ResultV1 {
                schema_version: 1,
                identity: prepared.identity,
                asset_lock: input.asset_lock.clone(),
                font_lock: input.font_lock.clone(),
                render_input_sha256: video::render_input_sha256(
                    &input.snapshot,
                    &input.sequence_id,
                )?,
                execution: V4VideoExecutionSummaryV1 {
                    visual: visual_summary,
                    audio: audio_summary.expect("Legacy output always prepares audio"),
                    encoder: V4VideoEncoderSummaryV1 {
                        ffmpeg_sha256: observed.ffmpeg_sha256,
                        ffprobe_sha256: observed.ffprobe_sha256,
                        video_codec: "h264".into(),
                        audio_codec: "aac".into(),
                        pixel_format: "yuv420p".into(),
                        crf: 18,
                        audio_bitrate: 192000,
                        observed_video_frames: observed.video_frames.to_string(),
                        observed_video_duration_seconds: observed.video_duration_seconds,
                        observed_audio_duration_seconds: observed.audio_duration_seconds,
                    },
                    frame_receipts_sha256: format!("{:x}", receipts.finalize()),
                    frames_rendered: count.to_string(),
                    rgba_bytes: rgba_bytes.to_string(),
                },
                artifact: V4FrameArtifactMetadataV1 {
                    mime_type: "video/mp4".into(),
                    size_bytes: size.to_string(),
                    content_hash: content_hash.clone(),
                },
            },
            artifact: ArtifactDescriptor {
                path: output_path.into_os_string().into_string().map_err(|_| {
                    error(
                        "ARTIFACT_VERIFICATION_FAILED",
                        "Private video output is not UTF-8",
                    )
                })?,
                mime_type: "video/mp4".into(),
                size,
                content_hash,
            },
        };
        match profile {
            video::VideoFrameProfile::TextV2 => {
                video::validate_stored_result(&input, &stored)?;
                serde_json::to_value(stored)
            }
            video::VideoFrameProfile::CaptionV3 => {
                let public = stored.public;
                let stored = caption::StoredV4CaptionVideoMp4ResultV2 {
                    public: caption::V4CaptionVideoMp4ResultV2 {
                        schema_version: 2,
                        identity: caption::public_identity(&public.identity),
                        asset_lock: public.asset_lock,
                        font_lock: public.font_lock,
                        render_input_sha256: caption::render_input_sha256(
                            &input.snapshot,
                            &input.sequence_id,
                        )?,
                        execution: public.execution,
                        artifact: public.artifact,
                    },
                    artifact: stored.artifact,
                };
                caption::validate_stored_result(&input, &stored)?;
                serde_json::to_value(stored)
            }
            video::VideoFrameProfile::GraphicV1 => {
                let public = stored.public;
                let stored = graphic::StoredV4GraphicVideoMp4ResultV1 {
                    public: graphic::V4GraphicVideoMp4ResultV1 {
                        schema_version: 1,
                        identity: graphic::public_identity(&public.identity),
                        asset_lock: public.asset_lock,
                        font_lock: public.font_lock,
                        render_input_sha256: graphic::render_input_sha256(
                            &input.snapshot,
                            &input.sequence_id,
                        )?,
                        execution: public.execution,
                        artifact: public.artifact,
                    },
                    artifact: stored.artifact,
                };
                graphic::validate_stored_result(&input, &stored)?;
                serde_json::to_value(stored)
            }
            video::VideoFrameProfile::SceneV1 => {
                let public = stored.public;
                let stored = scene::StoredV4SceneVideoMp4ResultV1 {
                    public: scene::V4SceneVideoMp4ResultV1 {
                        schema_version: 1,
                        identity: scene::public_identity(&public.identity),
                        asset_lock: public.asset_lock,
                        font_lock: public.font_lock,
                        render_input_sha256: scene::render_input_sha256(
                            &input.snapshot,
                            &input.sequence_id,
                        )?,
                        execution: public.execution,
                        artifact: public.artifact,
                    },
                    artifact: stored.artifact,
                };
                scene::validate_stored_result(&input, &stored)?;
                serde_json::to_value(stored)
            }
            video::VideoFrameProfile::MaskV1 => {
                let public = stored.public;
                let stored = mask::StoredV4MaskVideoMp4ResultV1 {
                    public: mask::V4MaskVideoMp4ResultV1 {
                        schema_version: 1,
                        identity: mask::public_identity(&public.identity),
                        asset_lock: public.asset_lock,
                        font_lock: public.font_lock,
                        render_input_sha256: mask::render_input_sha256(
                            &input.snapshot,
                            &input.sequence_id,
                        )?,
                        execution: public.execution,
                        artifact: public.artifact,
                    },
                    artifact: stored.artifact,
                };
                mask::validate_stored_result(&input, &stored)?;
                serde_json::to_value(stored)
            }
            video::VideoFrameProfile::BlendV1 => {
                let public = stored.public;
                let stored = blend::StoredV4BlendVideoMp4ResultV1 {
                    public: blend::V4BlendVideoMp4ResultV1 {
                        schema_version: 1,
                        identity: blend::public_identity(&public.identity),
                        asset_lock: public.asset_lock,
                        font_lock: public.font_lock,
                        render_input_sha256: blend::render_input_sha256(
                            &input.snapshot,
                            &input.sequence_id,
                        )?,
                        execution: public.execution,
                        artifact: public.artifact,
                    },
                    artifact: stored.artifact,
                };
                blend::validate_stored_result(&input, &stored)?;
                serde_json::to_value(stored)
            }
        }
        .map_err(|_| {
            error(
                "SERIALIZATION_ERROR",
                "Cannot serialize private video result",
            )
        })?
    };
    checkpoint()?;
    if opencut_native_services::reproducibility::execution_identity(vec![])?.process_binary_sha256
        != host_sha256
    {
        return Err(error(
            "EXECUTION_IDENTITY_CHANGED",
            "Executing host changed during video processing",
        ));
    }
    check_output_name(&attempt, output_name, &file)?;
    Ok(result)
}

#[cfg(not(unix))]
pub(super) fn execute(
    _: &Path,
    _: &Path,
    _: &str,
    _: &JobRecord,
    _: &CancellationToken,
    _: &Mutex<Option<AttemptLease>>,
) -> Result<Value, EditorError> {
    Err(error(
        "CAPABILITY_UNAVAILABLE",
        "Persistent video output requires native Unix admission and process termination",
    ))
}

#[cfg(not(unix))]
pub(super) fn execute_with_profile(
    database: &Path,
    root: &Path,
    worker: &str,
    job: &JobRecord,
    cancel: &CancellationToken,
    held: &Mutex<Option<AttemptLease>>,
    _: video::VideoFrameProfile,
) -> Result<Value, EditorError> {
    execute(database, root, worker, job, cancel, held)
}

#[cfg(not(unix))]
pub(super) fn execute_export_v2(
    database: &Path,
    root: &Path,
    worker: &str,
    job: &JobRecord,
    cancel: &CancellationToken,
    held: &Mutex<Option<AttemptLease>>,
) -> Result<Value, EditorError> {
    execute(database, root, worker, job, cancel, held)
}
