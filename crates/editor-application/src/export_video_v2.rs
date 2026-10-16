//! Configurable output shares fixed scene/frame/audio admission, authority and
//! resource capture with the frozen MP4 tools. Options are part of durable input.
use crate::project_v4_frame_jobs::{valid_hash, valid_id, validate_authority};
use crate::{JobRecord, JobRequest, project_v4_video_jobs as video};
use opencut_contracts::{EditorError, project_v4::*};
use opencut_job_contracts::ArtifactDescriptor;
pub use opencut_job_contracts::export_video_v2::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedExportVideoV2 {
    pub options: ExportVideoOptionsV2,
    pub visual_profile: ExportVisualProfileV2,
    pub execution: video::PreparedV4VideoMp4V1,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredExportVideoV2 {
    pub public: ExportVideoResultV2,
    pub artifact: ArtifactDescriptor,
}
pub trait ExportVideoStoreV2: video::V4VideoJobSubmissionStore {
    fn accept_export_video_v2(
        &mut self,
        request: JobRequest,
        prepared: PreparedExportVideoV2,
    ) -> Result<JobRecord, EditorError>;
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
pub fn frame_profile(profile: ExportVisualProfileV2) -> video::VideoFrameProfile {
    match profile {
        ExportVisualProfileV2::SceneV1 => video::VideoFrameProfile::SceneV1,
        ExportVisualProfileV2::TextV2 => video::VideoFrameProfile::TextV2,
        ExportVisualProfileV2::CaptionV3 => video::VideoFrameProfile::CaptionV3,
        ExportVisualProfileV2::GraphicV1 => video::VideoFrameProfile::GraphicV1,
        ExportVisualProfileV2::BlendV1 => video::VideoFrameProfile::BlendV1,
        ExportVisualProfileV2::MaskV1 => video::VideoFrameProfile::MaskV1,
    }
}
pub fn select_profile(
    snapshot: &ProjectSnapshotV4,
    sequence: &str,
) -> Result<ExportVisualProfileV2, EditorError> {
    let scene = snapshot
        .project
        .sequences
        .iter()
        .find(|s| s.id == sequence)
        .ok_or_else(|| invalid("Export scene does not exist"))?;
    // Admission checks authored nodes too; disabled content cannot select a
    // narrower profile that would silently omit an unsupported kind later.
    let elements = || scene.tracks.iter().flat_map(|t| &t.elements);
    let blurred = matches!(&snapshot.project.settings, SettingsProfileV4::ClassicGlobal { settings } if matches!(settings.background, BackgroundV4::MainTrackBlur { .. }));
    Ok(
        if blurred
            || elements().any(|e| {
                matches!(e.content, ElementContentV4::EffectLayer { .. })
                    || e.visual.as_ref().is_some_and(|v| {
                        !v.effects.is_empty()
                            || v.masks.iter().filter(|m| m.enabled).count() > 1
                            || v.masks.iter().any(|m| {
                                m.enabled
                                    && (m.inverted
                                        || m.stroke.width_px > 0.
                                        || !matches!(
                                            m.shape,
                                            MaskShapeV4::Rectangle { .. }
                                                | MaskShapeV4::Ellipse { .. }
                                        ))
                            })
                    })
            })
        {
            ExportVisualProfileV2::SceneV1
        } else if elements().any(|e| e.visual.as_ref().is_some_and(|v| !v.masks.is_empty())) {
            ExportVisualProfileV2::MaskV1
        } else if elements().any(|e| {
            e.visual
                .as_ref()
                .is_some_and(|v| v.blend_mode != BlendModeV4::Normal)
        }) {
            ExportVisualProfileV2::BlendV1
        } else if elements().any(|e| matches!(e.content, ElementContentV4::Graphic { .. })) {
            ExportVisualProfileV2::GraphicV1
        } else if !scene.captions.is_empty() {
            ExportVisualProfileV2::CaptionV3
        } else {
            ExportVisualProfileV2::TextV2
        },
    )
}
pub fn validate_submission(input: &SubmitExportVideoV2) -> Result<(), EditorError> {
    video::validate_submission(&video::SubmitV4VideoMp4V1 {
        idempotency_key: input.idempotency_key.clone(),
        project_id: input.project_id.clone(),
        revision: input.revision.clone(),
        sequence_id: input.sequence_id.clone(),
    })
}
pub fn prepare(
    input: &PreparedExportVideoV2,
) -> Result<video::PreparedVideoSelectionV2, EditorError> {
    let fixed = &input.execution;
    validate_authority(&fixed.authority)?;
    if [
        &fixed.snapshot.project.id,
        &fixed.snapshot.revision,
        &fixed.sequence_id,
    ]
    .iter()
    .any(|id| !valid_id(id))
        || select_profile(&fixed.snapshot, &fixed.sequence_id)? != input.visual_profile
    {
        return Err(invalid("Export fixed identity/profile changed"));
    }
    let plan = video::prepare_with_audio_selection(
        &fixed.snapshot,
        &fixed.sequence_id,
        &fixed.processing_policy,
        frame_profile(input.visual_profile),
        input.options.include_audio,
    )?;
    if classic_export_rates_v2(&input.options, plan.identity.width, plan.identity.height).is_none()
    {
        return Err(invalid("Export quality has invalid dimensions"));
    }
    let assets = plan
        .asset_ids
        .iter()
        .map(|id| {
            let a = fixed
                .snapshot
                .project
                .assets
                .iter()
                .find(|a| &a.id == id)
                .ok_or_else(|| invalid("Export asset missing"))?;
            Ok(opencut_contracts::LockedAsset {
                asset_id: id.clone(),
                content_hash: a.version.content_hash.clone(),
                size: a.version.size,
            })
        })
        .collect::<Result<Vec<_>, EditorError>>()?;
    let fonts = plan
        .font_ids
        .iter()
        .map(|id| {
            let f = fixed
                .snapshot
                .project
                .fonts
                .iter()
                .find(|f| &f.id == id)
                .ok_or_else(|| invalid("Export font missing"))?;
            Ok(opencut_contracts::text_frame_v2::FontLockV2 {
                font_id: id.clone(),
                content_hash: f.content.content_hash.clone(),
                size: f.content.size,
            })
        })
        .collect::<Result<Vec<_>, EditorError>>()?;
    if fixed.asset_lock
        != (opencut_contracts::AssetLockManifest {
            version: 1,
            revision: fixed.snapshot.revision.clone(),
            sequence_id: fixed.sequence_id.clone(),
            assets,
        })
        || fixed.font_lock != fonts
    {
        return Err(EditorError::new(
            "ASSET_LOCK_INVALID",
            "Export inventory differs from its fixed participating resources",
        ));
    }
    Ok(plan)
}
pub fn validate_prepared_request(
    request: &JobRequest,
    prepared: &PreparedExportVideoV2,
) -> Result<(), EditorError> {
    let public: SubmitExportVideoV2 = serde_json::from_value(request.input.clone())
        .map_err(|_| invalid("Invalid public export input"))?;
    validate_submission(&public)?;
    if request.kind != EXPORT_VIDEO_V2_KIND
        || request.idempotency_key != public.idempotency_key
        || public.options != prepared.options
        || public.project_id != prepared.execution.snapshot.project.id
        || public.revision != prepared.execution.snapshot.revision
        || public.sequence_id != prepared.execution.sequence_id
    {
        return Err(invalid(
            "Export request differs from its durable options and fixed scene",
        ));
    }
    prepare(prepared)?;
    Ok(())
}
pub fn submit(
    store: &mut impl ExportVideoStoreV2,
    assets: &impl video::V4VideoAssetCapture,
    input: SubmitExportVideoV2,
) -> Result<JobRecord, EditorError> {
    validate_submission(&input)?;
    let authorized = store.fixed_v4_video_snapshot(&input.project_id, &input.revision)?;
    if authorized.snapshot.project.id != input.project_id
        || authorized.snapshot.revision != input.revision
    {
        return Err(invalid("Authorized export revision changed"));
    }
    let request = JobRequest {
        idempotency_key: input.idempotency_key.clone(),
        kind: EXPORT_VIDEO_V2_KIND.into(),
        input: serde_json::to_value(&input)
            .map_err(|_| invalid("Cannot serialize export input"))?,
    };
    if let Some(existing) = store.find_accepted_v4_video_job(&request)? {
        return Ok(existing);
    }
    validate_authority(&authorized.authority)?;
    let profile = select_profile(&authorized.snapshot, &input.sequence_id)?;
    let plan = video::prepare_with_audio_selection(
        &authorized.snapshot,
        &input.sequence_id,
        &authorized.processing_policy,
        frame_profile(profile),
        input.options.include_audio,
    )?;
    let (asset_lock, font_lock) = assets.capture(
        &authorized.snapshot,
        &input.sequence_id,
        &plan.asset_ids,
        &plan.font_ids,
    )?;
    let prepared = PreparedExportVideoV2 {
        options: input.options,
        visual_profile: profile,
        execution: video::PreparedV4VideoMp4V1 {
            snapshot: authorized.snapshot,
            sequence_id: input.sequence_id,
            asset_lock,
            font_lock,
            authority: authorized.authority,
            processing_policy: authorized.processing_policy,
        },
    };
    validate_prepared_request(&request, &prepared)?;
    store.accept_export_video_v2(request, prepared)
}
pub fn public_identity(
    input: &PreparedExportVideoV2,
    plan: &video::PreparedVideoSelectionV2,
) -> ExportVideoIdentityV2 {
    let i = &plan.identity;
    ExportVideoIdentityV2 {
        project_id: i.project_id.clone(),
        revision: i.revision.clone(),
        sequence_id: i.sequence_id.clone(),
        visual_profile: input.visual_profile,
        options: input.options.clone(),
        width: i.width,
        height: i.height,
        fps: i.fps.clone(),
        frame_count: i.frame_count.clone(),
        timeline_duration: i.timeline_duration.clone(),
        video_duration: i.video_duration.clone(),
        audio_frame_count: plan
            .audio
            .as_ref()
            .map(|a| a.duration_samples().to_string()),
    }
}
pub fn render_input_sha256(input: &PreparedExportVideoV2) -> Result<String, EditorError> {
    let bytes = serde_json::to_vec(&(
        2u32,
        &input.execution.snapshot,
        &input.execution.sequence_id,
        &input.options,
        input.visual_profile,
        48000u32,
        2u32,
    ))
    .map_err(|_| invalid("Cannot hash fixed export options"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
pub fn validate_stored_result(
    input: &PreparedExportVideoV2,
    stored: &StoredExportVideoV2,
) -> Result<ExportVideoResultV2, EditorError> {
    let bad = || {
        EditorError::new(
            "INVALID_STORED_RESULT",
            "Export result differs from fixed revision/options/encoder evidence",
        )
    };
    let plan = prepare(input).map_err(|_| bad())?;
    let p = &stored.public;
    let e = &p.execution;
    let enc = &e.encoder;
    let v = &e.visual;
    let format = input.options.format;
    let rates = classic_export_rates_v2(&input.options, plan.identity.width, plan.identity.height)
        .ok_or_else(bad)?;
    let frames = crate::classic_audio::decimal(&plan.identity.frame_count)?;
    let rgba = frames
        .checked_mul(u64::from(plan.identity.width))
        .and_then(|n| n.checked_mul(u64::from(plan.identity.height)))
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(bad)?;
    let duration = opencut_time::RationalTime::try_from(&plan.identity.video_duration)?;
    let seconds = duration.ticks() as f64 / f64::from(duration.timebase());
    // WebM timestamps have millisecond precision and include Opus codec delay.
    let video_tolerance = if format == ExportFormatV2::Webm {
        0.05
    } else {
        0.002
    };
    if p.schema_version != 2
        || p.identity != public_identity(input, &plan)
        || p.asset_lock != input.execution.asset_lock
        || p.font_lock != input.execution.font_lock
        || p.render_input_sha256 != render_input_sha256(input)?
        || enc.rates != rates
        || enc.video_codec != format.video_codec()
        || enc.audio_codec.as_deref() != input.options.include_audio.then(|| format.audio_codec())
        || enc.pixel_format != "yuv420p"
        || enc.observed_video_frames != plan.identity.frame_count
        || e.frames_rendered != plan.identity.frame_count
        || e.rgba_bytes != rgba.to_string()
        || !enc.observed_video_duration_seconds.is_finite()
        || (enc.observed_video_duration_seconds - seconds).abs() > video_tolerance
        || v.playwright_version != "1.63.0"
        || v.mediabunny_version != "1.41.0"
        || v.gpu_backend != "webgpu"
        || v.texture_format != "bgra8unorm"
        || v.decoder_profile != crate::project_v4_frame_jobs::CLASSIC_VISUAL_DECODER_PROFILE_V4
        || !valid_id(&v.browser_version)
        || !valid_id(&v.gpu_device)
        || [
            &v.host_sha256,
            &v.browser_sha256,
            &v.module_sha256,
            &v.shader_sha256,
            &enc.ffmpeg_sha256,
            &enc.ffprobe_sha256,
            &e.frame_receipts_sha256,
        ]
        .into_iter()
        .any(|s| !valid_hash(s))
        || p.artifact.mime_type != format.mime_type()
        || stored.artifact.mime_type != format.mime_type()
        || stored.artifact.size == 0
        || stored.artifact.size > input.execution.processing_policy.max_output_bytes
        || p.artifact.size_bytes != stored.artifact.size.to_string()
        || p.artifact.content_hash != stored.artifact.content_hash
        || !valid_hash(&stored.artifact.content_hash)
    {
        return Err(bad());
    }
    match (&plan.audio, &e.audio, enc.observed_audio_duration_seconds) {
        (None, None, None) => {}
        (Some(audio), Some(summary), Some(observed)) => {
            crate::project_v4_audio_jobs::validate_execution_summary(audio, summary)
                .map_err(|_| bad())?;
            if v.host_sha256 != summary.host_sha256 || v.browser_sha256 != summary.browser_sha256 || v.browser_version != summary.browser_version
                || !observed.is_finite() || (observed - audio.duration_samples() as f64 / 48000.0).abs() > 0.05
                || summary.mastering_profile != crate::project_v4_audio_jobs::V4AudioMasteringProfileV1::ClassicExportWholeSceneWebAudioV31 { return Err(bad()); }
        }
        _ => return Err(bad()),
    }
    Ok(p.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> PreparedExportVideoV2 {
        PreparedExportVideoV2 {
            options: ExportVideoOptionsV2::default(),
            visual_profile: ExportVisualProfileV2::TextV2,
            execution: crate::project_v4_video_jobs::tests::input(),
        }
    }
    #[test]
    fn audio_off_skips_even_audio_admission_and_keeps_exact_visual_inventory() {
        let mut input = input();
        input.execution.processing_policy.audio.max_pcm_bytes = 1;
        assert!(prepare(&input).is_err());
        input.options.include_audio = false;
        let plan = prepare(&input).unwrap();
        assert!(plan.audio.is_none());
        assert_eq!(plan.asset_ids, vec!["image"]);
        assert_eq!(plan.identity.frame_count, "60");
        assert_eq!(public_identity(&input, &plan).audio_frame_count, None);
    }
    #[test]
    fn options_are_part_of_durable_request_identity_and_render_digest() {
        let input = input();
        let mut changed = input.clone();
        changed.options.format = ExportFormatV2::Webm;
        assert_ne!(
            render_input_sha256(&input).unwrap(),
            render_input_sha256(&changed).unwrap()
        );
        let public = SubmitExportVideoV2 {
            idempotency_key: "same-key".into(),
            project_id: input.execution.snapshot.project.id.clone(),
            revision: input.execution.snapshot.revision.clone(),
            sequence_id: input.execution.sequence_id.clone(),
            options: input.options.clone(),
        };
        let request = JobRequest {
            idempotency_key: public.idempotency_key.clone(),
            kind: EXPORT_VIDEO_V2_KIND.into(),
            input: serde_json::to_value(public).unwrap(),
        };
        validate_prepared_request(&request, &input).unwrap();
        assert!(validate_prepared_request(&request, &changed).is_err());
        changed = input.clone();
        changed.options.include_audio = false;
        assert!(validate_prepared_request(&request, &changed).is_err());
        changed = input.clone();
        changed.visual_profile = ExportVisualProfileV2::GraphicV1;
        assert!(prepare(&changed).is_err());
    }
    #[test]
    fn stored_output_binds_format_quality_audio_absence_and_private_artifact() {
        use opencut_job_contracts::project_v4_frame::{
            V4FrameArtifactMetadataV1, V4FrameExecutionSummaryV1,
        };
        let mut input = input();
        input.options = ExportVideoOptionsV2 {
            format: ExportFormatV2::Webm,
            quality: ExportQualityV2::Low,
            include_audio: false,
        };
        let plan = prepare(&input).unwrap();
        let hash = "d".repeat(64);
        let stored = StoredExportVideoV2 {
            public: ExportVideoResultV2 {
                schema_version: 2,
                identity: public_identity(&input, &plan),
                asset_lock: input.execution.asset_lock.clone(),
                font_lock: input.execution.font_lock.clone(),
                render_input_sha256: render_input_sha256(&input).unwrap(),
                execution: ExportVideoExecutionV2 {
                    visual: V4FrameExecutionSummaryV1 {
                        host_sha256: hash.clone(),
                        browser_version: "test-browser".into(),
                        browser_sha256: hash.clone(),
                        module_sha256: hash.clone(),
                        shader_sha256: hash.clone(),
                        playwright_version: "1.63.0".into(),
                        mediabunny_version: "1.41.0".into(),
                        gpu_backend: "webgpu".into(),
                        gpu_device: "test-gpu".into(),
                        texture_format: "bgra8unorm".into(),
                        decoder_profile:
                            crate::project_v4_frame_jobs::CLASSIC_VISUAL_DECODER_PROFILE_V4.into(),
                    },
                    audio: None,
                    encoder: ExportVideoEncoderSummaryV2 {
                        ffmpeg_sha256: hash.clone(),
                        ffprobe_sha256: hash.clone(),
                        video_codec: "vp9".into(),
                        audio_codec: None,
                        pixel_format: "yuv420p".into(),
                        rates: classic_export_rates_v2(&input.options, 64, 48).unwrap(),
                        observed_video_frames: "60".into(),
                        observed_video_duration_seconds: 2.0,
                        observed_audio_duration_seconds: None,
                    },
                    frame_receipts_sha256: hash.clone(),
                    frames_rendered: "60".into(),
                    rgba_bytes: (60u64 * 64 * 48 * 4).to_string(),
                },
                artifact: V4FrameArtifactMetadataV1 {
                    mime_type: "video/webm".into(),
                    size_bytes: "100".into(),
                    content_hash: hash.clone(),
                },
            },
            artifact: ArtifactDescriptor {
                path: "/private/attempt/video.webm".into(),
                mime_type: "video/webm".into(),
                size: 100,
                content_hash: hash,
            },
        };
        let public = validate_stored_result(&input, &stored).unwrap();
        assert!(
            !serde_json::to_string(&public)
                .unwrap()
                .contains("/private/")
        );
        let mut changed = stored.clone();
        changed.public.identity.options.quality = ExportQualityV2::High;
        assert!(validate_stored_result(&input, &changed).is_err());
        changed = stored.clone();
        changed.public.execution.encoder.rates.video_bitrate += 1000;
        assert!(validate_stored_result(&input, &changed).is_err());
        changed = stored.clone();
        changed
            .public
            .execution
            .encoder
            .observed_audio_duration_seconds = Some(2.0);
        assert!(validate_stored_result(&input, &changed).is_err());
        changed = stored.clone();
        changed.public.execution.encoder.audio_codec = Some("opus".into());
        assert!(validate_stored_result(&input, &changed).is_err());
        changed = stored.clone();
        changed.artifact.mime_type = "video/mp4".into();
        assert!(validate_stored_result(&input, &changed).is_err());
        changed = stored.clone();
        changed.public.execution.frames_rendered = "59".into();
        assert!(validate_stored_result(&input, &changed).is_err());
        changed = stored.clone();
        changed
            .public
            .execution
            .encoder
            .observed_video_duration_seconds = f64::NAN;
        assert!(validate_stored_result(&input, &changed).is_err());
    }
}
