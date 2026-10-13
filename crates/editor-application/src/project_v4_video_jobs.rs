//! Fixed-revision CFR video admission and result validation. Native execution
//! uses the same complete text/media frames and whole-scene Classic master.
use crate::project_v4_frame_jobs::{valid_hash, valid_id, validate_authority};
use crate::{JobRecord, JobRequest};
use opencut_composition::{CompositionScope, project_v4::*};
pub use opencut_contracts::text_frame_v2::FontLockV2;
use opencut_contracts::{AssetLockManifest, EditorError, Time, project_v4::*};
use opencut_job_contracts::ArtifactDescriptor;
pub use opencut_job_contracts::project_v4_video::*;
pub use opencut_job_execution::project_v4_audio::TrustedJobAuthorityV1;
pub use opencut_job_execution::project_v4_video::*;
use opencut_time::RationalTime;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub struct AuthorizedV4VideoSnapshot {
    pub snapshot: ProjectSnapshotV4,
    pub authority: TrustedJobAuthorityV1,
    pub processing_policy: LocalVideoExecutionPolicyV1,
}
pub trait V4VideoJobSubmissionStore {
    fn fixed_v4_video_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<AuthorizedV4VideoSnapshot, EditorError>;
    fn find_accepted_v4_video_job(
        &self,
        request: &JobRequest,
    ) -> Result<Option<JobRecord>, EditorError>;
    fn accept_v4_video_job(
        &mut self,
        request: JobRequest,
        prepared: PreparedV4VideoMp4V1,
    ) -> Result<JobRecord, EditorError>;
}
pub trait V4VideoAssetCapture {
    /// Verify participating CAS versions under the collection gate held through acceptance.
    fn capture(
        &self,
        snapshot: &ProjectSnapshotV4,
        sequence_id: &str,
        asset_ids: &[String],
        font_ids: &[String],
    ) -> Result<(AssetLockManifest, Vec<FontLockV2>), EditorError>;
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredV4VideoMp4ResultV1 {
    pub public: V4VideoMp4ResultV1,
    pub artifact: ArtifactDescriptor,
}
/// One fixed audio preparation plus bounded frame-index planning, never N copies
/// of a complete snapshot or another video-specific visual layout.
pub struct PreparedVideoV4 {
    pub identity: V4VideoIdentityV1,
    pub audio: PreparedAudioV4,
    pub asset_ids: Vec<String>,
    pub font_ids: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoFrameProfile {
    TextV2,
    CaptionV3,
}
impl VideoFrameProfile {
    pub fn kind(self) -> &'static str {
        match self {
            Self::TextV2 => V4_VIDEO_MP4_KIND,
            Self::CaptionV3 => {
                opencut_job_contracts::project_v4_caption_video::V4_CAPTION_VIDEO_MP4_KIND
            }
        }
    }
}
pub enum PreparedVideoFrame {
    Text(PreparedTextFrameV2),
    Caption(PreparedTextFrameV3),
}
impl PreparedVideoFrame {
    pub fn execution(&self) -> &PreparedTextFrameV2 {
        match self {
            Self::Text(frame) => frame,
            Self::Caption(frame) => frame.execution(),
        }
    }
}
pub fn prepare_profiled_frame(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    at: &Time,
    profile: VideoFrameProfile,
) -> Result<PreparedVideoFrame, EditorError> {
    match profile {
        VideoFrameProfile::TextV2 => {
            prepare_frame(snapshot, sequence_id, at).map(PreparedVideoFrame::Text)
        }
        VideoFrameProfile::CaptionV3 => {
            crate::project_v4_caption_frame_jobs::prepare(snapshot, sequence_id, at)
                .map(PreparedVideoFrame::Caption)
        }
    }
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn stored_invalid() -> EditorError {
    EditorError::new(
        "INVALID_STORED_RESULT",
        "Stored V4 video result differs from its fixed input",
    )
}
fn budget() -> EditorError {
    EditorError::new(
        "VIDEO_BUDGET_EXCEEDED",
        "Whole-scene video exceeds host admission",
    )
}
fn unsupported() -> EditorError {
    EditorError::new(
        "UNSUPPORTED_VIDEO_PROFILE",
        "Basic CFR video requires Classic global settings and supported media/free-text/audio normal elements",
    )
}
pub fn validate_submission(input: &SubmitV4VideoMp4V1) -> Result<(), EditorError> {
    if [
        &input.idempotency_key,
        &input.project_id,
        &input.revision,
        &input.sequence_id,
    ]
    .into_iter()
    .all(|v| valid_id(v))
    {
        Ok(())
    } else {
        Err(invalid(
            "Video submission requires bounded fixed identities",
        ))
    }
}
pub fn validate_policy(policy: &LocalVideoExecutionPolicyV1) -> Result<(), EditorError> {
    crate::project_v4_frame_jobs::validate_policy(&policy.visual)?;
    crate::project_v4_audio_jobs::validate_policy(&policy.audio)?;
    if policy.max_frames == 0
        || policy.max_frames > 1800
        || policy.max_output_bytes == 0
        || policy.max_output_bytes > 512 * 1024 * 1024
        || !(1..=300_000).contains(&policy.deadline_ms)
    {
        return Err(budget());
    }
    Ok(())
}
pub fn frame_time(identity: &V4VideoIdentityV1, index: u64) -> Result<Time, EditorError> {
    let count = crate::classic_audio::decimal(&identity.frame_count)?;
    if index >= count {
        return Err(invalid("CFR frame index is outside the fixed scene"));
    }
    grid_time(&identity.fps, index)
}
fn grid_time(fps: &opencut_contracts::FrameRate, index: u64) -> Result<Time, EditorError> {
    let ticks = u128::from(index)
        .checked_mul(u128::from(fps.denominator))
        .ok_or_else(budget)?;
    Ok(RationalTime::new(i64::try_from(ticks).map_err(|_| budget())?, fps.numerator)?.into())
}
pub fn prepare_frame(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    at: &Time,
) -> Result<PreparedTextFrameV2, EditorError> {
    let graph = compile_frame_v4(snapshot, sequence_id, at, CompositionScope::Visual)
        .map_err(|e| EditorError::new(e.code, e.message))?;
    prepare_text_frame_v2(&graph).map_err(|e| EditorError::new(e.code, e.message))
}
pub fn prepare(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    policy: &LocalVideoExecutionPolicyV1,
) -> Result<PreparedVideoV4, EditorError> {
    prepare_with_profile(snapshot, sequence_id, policy, VideoFrameProfile::TextV2)
}
pub fn prepare_with_profile(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    policy: &LocalVideoExecutionPolicyV1,
    profile: VideoFrameProfile,
) -> Result<PreparedVideoV4, EditorError> {
    validate_policy(policy)?;
    opencut_editor_core::project_v4::validate_project_v4(&snapshot.project)?;
    let SettingsProfileV4::ClassicGlobal { settings } = &snapshot.project.settings else {
        return Err(unsupported());
    };
    let sequence = snapshot
        .project
        .sequences
        .iter()
        .find(|s| s.id == sequence_id)
        .ok_or_else(|| invalid("Video sequence does not exist"))?;
    if (profile == VideoFrameProfile::TextV2 && !sequence.captions.is_empty())
        || !sequence.transitions.is_empty()
    {
        return Err(unsupported());
    }
    let mut duration = RationalTime::new(0, 1)?;
    for track in &sequence.tracks {
        for element in &track.elements {
            let end = RationalTime::try_from(&opencut_time::range_end(&element.timeline_range)?)?;
            if end.cmp_exact(duration).is_gt() {
                duration = end;
            }
            // Reject unsupported authored kinds even if shorter than one output
            // frame; sampling must never silently make an unsupported node vanish.
            if !matches!(
                element.content,
                ElementContentV4::Video { .. }
                    | ElementContentV4::Image { .. }
                    | ElementContentV4::Audio { .. }
                    | ElementContentV4::Text { .. }
            ) {
                return Err(unsupported());
            }
            if element.visual.as_ref().is_some_and(|visual| {
                visual.profile != VisualProfileV4::ClassicCanvasV31
                    || visual.blend_mode != BlendModeV4::Normal
                    || !visual.effects.is_empty()
                    || !visual.masks.is_empty()
            }) || matches!(&element.content, ElementContentV4::Text { text } if !matches!(text.font, FontBindingV4::Locked { .. }))
            {
                return Err(unsupported());
            }
        }
    }
    if profile == VideoFrameProfile::CaptionV3 {
        for caption in &sequence.captions {
            if caption.style.layout != CaptionLayoutProfileV4::PinnedCaptionV4
                || !matches!(caption.style.font, FontBindingV4::Locked { .. })
            {
                return Err(unsupported());
            }
            let end = RationalTime::try_from(&opencut_time::range_end(&caption.timeline_range)?)?;
            if end.cmp_exact(duration).is_gt() {
                duration = end;
            }
        }
    }
    if duration.ticks() <= 0 {
        return Err(invalid("Video requires a nonempty complete scene"));
    }
    let numerator = i128::from(duration.ticks()) * i128::from(settings.fps.numerator);
    let denominator = i128::from(duration.timebase()) * i128::from(settings.fps.denominator);
    let frame_count =
        u64::try_from((numerator + denominator - 1) / denominator).map_err(|_| budget())?;
    if frame_count == 0 || frame_count > policy.max_frames {
        return Err(budget());
    }
    let audio = prepare_audio_v4(
        snapshot,
        sequence_id,
        AudioExecutionProfileV4::ClassicV31,
        PcmFormatV4 {
            sample_rate: 48_000,
            channels: 2,
        },
    )
    .map_err(|e| EditorError::new(e.code, e.message))?;
    crate::classic_audio::ClassicAudioSession::new(
        &audio,
        policy.audio.max_pcm_bytes,
        policy.audio.max_encoded_bytes,
    )?;
    let identity = V4VideoIdentityV1 {
        project_id: snapshot.project.id.clone(),
        revision: snapshot.revision.clone(),
        sequence_id: sequence_id.into(),
        profile: V4VideoProfileV1::ClassicV31CfrH264AacV1,
        width: settings.canvas_size.width,
        height: settings.canvas_size.height,
        fps: settings.fps.clone(),
        frame_count: frame_count.to_string(),
        timeline_duration: duration.into(),
        video_duration: grid_time(&settings.fps, frame_count)?,
        sample_rate: 48_000,
        channels: 2,
        audio_frame_count: audio.duration_samples().to_string(),
    };
    if identity.width % 2 != 0 || identity.height % 2 != 0 {
        return Err(unsupported());
    }
    let mut assets: BTreeSet<String> = audio.resources().map(|r| r.asset_id.clone()).collect();
    let mut fonts = BTreeSet::new();
    for index in 0..frame_count {
        let selected = prepare_profiled_frame(
            snapshot,
            sequence_id,
            &frame_time(&identity, index)?,
            profile,
        )?;
        let frame = selected.execution();
        crate::classic_text_frame_v2::admit_text_frame_v2(frame, &policy.visual)?;
        assets.extend(frame.media().resources().iter().map(|r| r.asset_id.clone()));
        fonts.extend(frame.graph().fonts.iter().map(|f| f.id.clone()));
    }
    if assets.iter().any(|id| fonts.contains(id)) {
        return Err(invalid("Font and media identities must be distinct"));
    }
    let encoded_bytes = assets.iter().try_fold(0u64, |n, id| {
        snapshot
            .project
            .assets
            .iter()
            .find(|a| &a.id == id)
            .and_then(|a| n.checked_add(a.version.size))
            .ok_or_else(budget)
    })?;
    let font_bytes = fonts.iter().try_fold(0u64, |n, id| {
        snapshot
            .project
            .fonts
            .iter()
            .find(|f| &f.id == id)
            .and_then(|f| n.checked_add(f.content.size))
            .ok_or_else(budget)
    })?;
    if assets.len() + fonts.len() > 32
        || encoded_bytes
            .checked_add(font_bytes)
            .is_none_or(|n| n > policy.visual.max_encoded_bytes)
    {
        return Err(budget());
    }
    Ok(PreparedVideoV4 {
        identity,
        audio,
        asset_ids: assets.into_iter().collect(),
        font_ids: fonts.into_iter().collect(),
    })
}
pub(crate) fn validate_lock(
    input: &PreparedV4VideoMp4V1,
    plan: &PreparedVideoV4,
) -> Result<(), EditorError> {
    let expected: BTreeMap<_, _> = plan
        .asset_ids
        .iter()
        .map(|id| {
            let a = input
                .snapshot
                .project
                .assets
                .iter()
                .find(|a| &a.id == id)
                .expect("admitted fixed resource");
            (id.clone(), (a.version.content_hash.clone(), a.version.size))
        })
        .collect();
    let observed: BTreeMap<_, _> = input
        .asset_lock
        .assets
        .iter()
        .map(|a| (a.asset_id.clone(), (a.content_hash.clone(), a.size)))
        .collect();
    if input.asset_lock.version != 1
        || input.asset_lock.revision != input.snapshot.revision
        || input.asset_lock.sequence_id != input.sequence_id
        || observed.len() != input.asset_lock.assets.len()
        || observed != expected
    {
        return Err(EditorError::new(
            "ASSET_LOCK_INVALID",
            "Video lock differs from all fixed participating media",
        ));
    }
    let expected: BTreeMap<_, _> = plan
        .font_ids
        .iter()
        .map(|id| {
            let f = input
                .snapshot
                .project
                .fonts
                .iter()
                .find(|f| &f.id == id)
                .expect("admitted fixed font");
            (id.clone(), (f.content.content_hash.clone(), f.content.size))
        })
        .collect();
    let observed: BTreeMap<_, _> = input
        .font_lock
        .iter()
        .map(|f| (f.font_id.clone(), (f.content_hash.clone(), f.size)))
        .collect();
    if observed.len() != input.font_lock.len() || observed != expected {
        return Err(EditorError::new(
            "FONT_LOCK_INVALID",
            "Video font lock differs from all fixed participating fonts",
        ));
    }
    Ok(())
}
pub fn validate_prepared(input: &PreparedV4VideoMp4V1) -> Result<PreparedVideoV4, EditorError> {
    validate_prepared_with_profile(input, VideoFrameProfile::TextV2)
}
pub fn validate_prepared_with_profile(
    input: &PreparedV4VideoMp4V1,
    profile: VideoFrameProfile,
) -> Result<PreparedVideoV4, EditorError> {
    validate_authority(&input.authority)?;
    if ![
        &input.snapshot.project.id,
        &input.snapshot.revision,
        &input.sequence_id,
    ]
    .into_iter()
    .all(|v| valid_id(v))
    {
        return Err(invalid("Invalid fixed video scope"));
    }
    let plan = prepare_with_profile(
        &input.snapshot,
        &input.sequence_id,
        &input.processing_policy,
        profile,
    )?;
    validate_lock(input, &plan)?;
    Ok(plan)
}
pub fn validate_prepared_request(
    request: &JobRequest,
    input: &PreparedV4VideoMp4V1,
) -> Result<(), EditorError> {
    validate_prepared_request_with_profile(request, input, VideoFrameProfile::TextV2)
}
pub fn validate_prepared_request_with_profile(
    request: &JobRequest,
    input: &PreparedV4VideoMp4V1,
    profile: VideoFrameProfile,
) -> Result<(), EditorError> {
    let public: SubmitV4VideoMp4V1 = serde_json::from_value(request.input.clone())
        .map_err(|_| invalid("Invalid public video request"))?;
    validate_submission(&public)?;
    if request.kind != profile.kind()
        || request.idempotency_key != public.idempotency_key
        || public.project_id != input.snapshot.project.id
        || public.revision != input.snapshot.revision
        || public.sequence_id != input.sequence_id
    {
        return Err(invalid("Video request differs from private preparation"));
    }
    validate_prepared_with_profile(input, profile)?;
    Ok(())
}
pub fn submit_video_mp4(
    store: &mut impl V4VideoJobSubmissionStore,
    assets: &impl V4VideoAssetCapture,
    input: SubmitV4VideoMp4V1,
) -> Result<JobRecord, EditorError> {
    submit_video_mp4_with_profile(store, assets, input, VideoFrameProfile::TextV2)
}
pub fn submit_video_mp4_with_profile(
    store: &mut impl V4VideoJobSubmissionStore,
    assets: &impl V4VideoAssetCapture,
    input: SubmitV4VideoMp4V1,
    profile: VideoFrameProfile,
) -> Result<JobRecord, EditorError> {
    validate_submission(&input)?;
    let authorized = store.fixed_v4_video_snapshot(&input.project_id, &input.revision)?;
    if authorized.snapshot.project.id != input.project_id
        || authorized.snapshot.revision != input.revision
    {
        return Err(invalid("Authorized revision differs from video request"));
    }
    let request = JobRequest {
        idempotency_key: input.idempotency_key.clone(),
        kind: profile.kind().into(),
        input: serde_json::to_value(&input)
            .map_err(|_| invalid("Cannot serialize video request"))?,
    };
    if let Some(existing) = store.find_accepted_v4_video_job(&request)? {
        return Ok(existing);
    }
    validate_authority(&authorized.authority)?;
    let plan = prepare_with_profile(
        &authorized.snapshot,
        &input.sequence_id,
        &authorized.processing_policy,
        profile,
    )?;
    let (asset_lock, font_lock) = assets.capture(
        &authorized.snapshot,
        &input.sequence_id,
        &plan.asset_ids,
        &plan.font_ids,
    )?;
    let prepared = PreparedV4VideoMp4V1 {
        snapshot: authorized.snapshot,
        sequence_id: input.sequence_id,
        asset_lock,
        font_lock,
        authority: authorized.authority,
        processing_policy: authorized.processing_policy,
    };
    validate_lock(&prepared, &plan)?;
    store.accept_v4_video_job(request, prepared)
}
pub fn render_input_sha256(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
) -> Result<String, EditorError> {
    let bytes = serde_json::to_vec(&(
        1u32,
        snapshot,
        sequence_id,
        V4VideoProfileV1::ClassicV31CfrH264AacV1,
        48_000u32,
        2u32,
    ))
    .map_err(|_| invalid("Cannot hash fixed video input"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
pub fn validate_stored_result(
    input: &PreparedV4VideoMp4V1,
    stored: &StoredV4VideoMp4ResultV1,
) -> Result<V4VideoMp4ResultV1, EditorError> {
    let plan = validate_prepared(input).map_err(|_| stored_invalid())?;
    let public = &stored.public;
    if public.schema_version != 1
        || public.identity != plan.identity
        || public.asset_lock != input.asset_lock
        || public.font_lock != input.font_lock
        || public.render_input_sha256 != render_input_sha256(&input.snapshot, &input.sequence_id)?
    {
        return Err(stored_invalid());
    }
    validate_result_evidence(
        input,
        &plan,
        &public.execution,
        &public.artifact,
        &stored.artifact,
    )?;
    Ok(public.clone())
}
pub(crate) fn validate_result_evidence(
    input: &PreparedV4VideoMp4V1,
    plan: &PreparedVideoV4,
    execution: &V4VideoExecutionSummaryV1,
    artifact: &opencut_job_contracts::project_v4_frame::V4FrameArtifactMetadataV1,
    descriptor: &ArtifactDescriptor,
) -> Result<(), EditorError> {
    let visual = &execution.visual;
    let encoder = &execution.encoder;
    crate::project_v4_audio_jobs::validate_execution_summary(&plan.audio, &execution.audio)
        .map_err(|_| stored_invalid())?;
    let expected_rgba = crate::classic_audio::decimal(&plan.identity.frame_count)?
        .checked_mul(u64::from(plan.identity.width))
        .and_then(|n| n.checked_mul(u64::from(plan.identity.height)))
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(stored_invalid)?;
    let seconds = |t: &Time| -> Result<f64, EditorError> {
        let r = RationalTime::try_from(t)?;
        Ok(r.ticks() as f64 / f64::from(r.timebase()))
    };
    let video_seconds = seconds(&plan.identity.video_duration)?;
    let audio_seconds = plan.audio.duration_samples() as f64 / 48_000.0;
    if visual.host_sha256 != execution.audio.host_sha256
        || visual.browser_sha256 != execution.audio.browser_sha256
        || visual.browser_version != execution.audio.browser_version
        || visual.playwright_version != "1.63.0" || visual.mediabunny_version != "1.41.0" || visual.gpu_backend != "webgpu" || visual.texture_format != "bgra8unorm" || visual.decoder_profile != crate::project_v4_frame_jobs::CLASSIC_VISUAL_DECODER_PROFILE_V4 || !valid_id(&visual.browser_version) || !valid_id(&visual.gpu_device)
        || [&visual.host_sha256, &visual.browser_sha256, &visual.module_sha256, &visual.shader_sha256, &encoder.ffmpeg_sha256, &encoder.ffprobe_sha256, &execution.frame_receipts_sha256].into_iter().any(|h| !valid_hash(h))
        || execution.frames_rendered != plan.identity.frame_count || execution.rgba_bytes != expected_rgba.to_string() || encoder.observed_video_frames != plan.identity.frame_count || encoder.video_codec != "h264" || encoder.audio_codec != "aac" || encoder.pixel_format != "yuv420p" || encoder.crf != 18 || encoder.audio_bitrate != 192000
        || !encoder.observed_video_duration_seconds.is_finite() || (encoder.observed_video_duration_seconds - video_seconds).abs() > 0.002 || !encoder.observed_audio_duration_seconds.is_finite() || (encoder.observed_audio_duration_seconds - audio_seconds).abs() > 0.05
        || execution.audio.mastering_profile != crate::project_v4_audio_jobs::V4AudioMasteringProfileV1::ClassicExportWholeSceneWebAudioV31
        || descriptor.mime_type != "video/mp4" || artifact.mime_type != "video/mp4" || descriptor.size == 0 || descriptor.size > input.processing_policy.max_output_bytes || artifact.size_bytes != descriptor.size.to_string() || artifact.content_hash != descriptor.content_hash || !valid_hash(&descriptor.content_hash)
    { return Err(stored_invalid()); }
    Ok(())
}

#[cfg(test)]
mod tests;
