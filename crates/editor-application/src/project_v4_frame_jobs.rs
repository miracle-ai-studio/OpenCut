//! Fixed-revision PNG submission and completion validation. No native I/O.
use crate::{JobRecord, JobRequest};
use opencut_composition::{CompositionScope, project_v4::*};
use opencut_contracts::{AssetLockManifest, EditorError, Time, project_v4::ProjectSnapshotV4};
use opencut_job_contracts::ArtifactDescriptor;
pub use opencut_job_contracts::project_v4_frame::*;
pub use opencut_job_execution::project_v4_audio::TrustedJobAuthorityV1;
pub use opencut_job_execution::project_v4_frame::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const CLASSIC_VISUAL_DECODER_PROFILE_V4: &str =
    "mediabunny-1.41.0-initial-canvases-half-open;static-png-jpeg-browser-image";
pub struct AuthorizedV4FrameSnapshot {
    pub snapshot: ProjectSnapshotV4,
    pub authority: TrustedJobAuthorityV1,
    pub processing_policy: LocalVisualExecutionPolicyV1,
}
pub trait V4FrameJobSubmissionStore {
    fn fixed_v4_frame_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<AuthorizedV4FrameSnapshot, EditorError>;
    fn find_accepted_v4_frame_job(
        &self,
        request: &JobRequest,
    ) -> Result<Option<JobRecord>, EditorError>;
    /// Recheck live original authority/request/snapshot/policy in one write lock.
    fn accept_v4_frame_job(
        &mut self,
        request: JobRequest,
        prepared: PreparedV4FramePng,
    ) -> Result<JobRecord, EditorError>;
}
pub trait V4FrameAssetCapture {
    /// Verify participating V4 versions and hold the CAS gate through acceptance.
    fn capture(
        &self,
        snapshot: &ProjectSnapshotV4,
        sequence_id: &str,
        asset_ids: &[String],
    ) -> Result<AssetLockManifest, EditorError>;
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredV4FramePngResultV1 {
    pub public: V4FramePngResultV1,
    pub artifact: ArtifactDescriptor,
}

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn stored_invalid() -> EditorError {
    EditorError::new(
        "INVALID_STORED_RESULT",
        "Stored V4 frame result differs from its fixed input",
    )
}
fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn validate_authority(authority: &TrustedJobAuthorityV1) -> Result<(), EditorError> {
    if let TrustedJobAuthorityV1::Credential {
        principal_id,
        credential_id,
    } = authority
        && (!valid_id(principal_id) || !valid_id(credential_id))
    {
        return Err(invalid("Invalid trusted frame authority"));
    }
    Ok(())
}
pub fn validate_submission(input: &SubmitV4FramePng) -> Result<(), EditorError> {
    if ![
        &input.idempotency_key,
        &input.project_id,
        &input.revision,
        &input.sequence_id,
    ]
    .into_iter()
    .all(|id| valid_id(id))
        || opencut_time::RationalTime::try_from(&input.time)?.ticks() < 0
    {
        return Err(invalid(
            "Frame submission requires bounded identity and nonnegative exact time",
        ));
    }
    Ok(())
}
pub fn validate_policy(policy: &LocalVisualExecutionPolicyV1) -> Result<(), EditorError> {
    if [
        policy.max_frame_pixels,
        policy.max_decoded_pixels,
        policy.max_gpu_bytes,
        policy.max_rgba_bytes,
        policy.max_encoded_bytes,
        policy.max_output_bytes,
        policy.max_rss_bytes,
    ]
    .contains(&0)
        || policy.max_gpu_bytes > 2 * 1024 * 1024 * 1024
        || policy.max_rgba_bytes > 1024 * 1024 * 1024
        || policy.max_encoded_bytes > 32 * 1024 * 1024
        || policy.max_output_bytes > 256 * 1024 * 1024
        || !(1..=300000).contains(&policy.deadline_ms)
    {
        return Err(invalid("Invalid host visual admission"));
    }
    Ok(())
}
fn prepare(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    at: &Time,
) -> Result<PreparedVisualFrameV4, EditorError> {
    let graph = compile_frame_v4(snapshot, sequence_id, at, CompositionScope::Visual)
        .map_err(|e| EditorError::new(e.code, e.message))?;
    prepare_visual_frame_v4(&graph).map_err(|e| EditorError::new(e.code, e.message))
}
fn validate_lock(
    input: &PreparedV4FramePng,
    prepared: &PreparedVisualFrameV4,
) -> Result<(), EditorError> {
    let expected: BTreeMap<_, _> = prepared
        .resources()
        .iter()
        .map(|r| {
            (
                r.asset_id.clone(),
                (r.version.content_hash.clone(), r.version.size),
            )
        })
        .collect();
    let observed: BTreeMap<_, _> = input
        .asset_lock
        .assets
        .iter()
        .map(|r| (r.asset_id.clone(), (r.content_hash.clone(), r.size)))
        .collect();
    if input.asset_lock.version != 1
        || input.asset_lock.revision != input.snapshot.revision
        || input.asset_lock.sequence_id != input.sequence_id
        || observed.len() != input.asset_lock.assets.len()
        || observed != expected
    {
        return Err(EditorError::new(
            "ASSET_LOCK_INVALID",
            "Visual lock differs from fixed participating resources",
        ));
    }
    Ok(())
}
pub fn render_input_sha256(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    at: &Time,
) -> Result<String, EditorError> {
    let bytes = serde_json::to_vec(&(
        1u32,
        snapshot,
        sequence_id,
        at,
        V4FrameProfileV1::ClassicV31GpuNormalFrame,
    ))
    .map_err(|_| invalid("Cannot hash fixed frame input"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
pub fn validate_prepared(input: &PreparedV4FramePng) -> Result<PreparedVisualFrameV4, EditorError> {
    validate_policy(&input.processing_policy)?;
    validate_authority(&input.authority)?;
    if !valid_id(&input.snapshot.project.id)
        || !valid_id(&input.snapshot.revision)
        || !valid_id(&input.sequence_id)
    {
        return Err(invalid("Invalid frame preparation scope"));
    }
    let prepared = prepare(&input.snapshot, &input.sequence_id, &input.time)?;
    crate::classic_visual::ClassicVisualSession::new(&prepared, &input.processing_policy)?;
    validate_lock(input, &prepared)?;
    Ok(prepared)
}
pub fn validate_prepared_request(
    request: &JobRequest,
    prepared: &PreparedV4FramePng,
) -> Result<(), EditorError> {
    let input: SubmitV4FramePng = serde_json::from_value(request.input.clone())
        .map_err(|_| invalid("Invalid public frame request"))?;
    validate_submission(&input)?;
    if request.kind != V4_FRAME_PNG_KIND
        || request.idempotency_key != input.idempotency_key
        || input.project_id != prepared.snapshot.project.id
        || input.revision != prepared.snapshot.revision
        || input.sequence_id != prepared.sequence_id
        || input.time != prepared.time
    {
        return Err(invalid(
            "Frame request differs from its private preparation",
        ));
    }
    validate_prepared(prepared)?;
    Ok(())
}
pub fn submit_frame_png(
    store: &mut impl V4FrameJobSubmissionStore,
    assets: &impl V4FrameAssetCapture,
    input: SubmitV4FramePng,
) -> Result<JobRecord, EditorError> {
    validate_submission(&input)?;
    let authorized = store.fixed_v4_frame_snapshot(&input.project_id, &input.revision)?;
    if authorized.snapshot.project.id != input.project_id
        || authorized.snapshot.revision != input.revision
    {
        return Err(invalid(
            "Authorized snapshot differs from fixed frame request",
        ));
    }
    opencut_editor_core::project_v4::validate_project_v4(&authorized.snapshot.project)?;
    let request = JobRequest {
        idempotency_key: input.idempotency_key.clone(),
        kind: V4_FRAME_PNG_KIND.into(),
        input: serde_json::to_value(&input)
            .map_err(|_| invalid("Cannot serialize frame request"))?,
    };
    if let Some(previous) = store.find_accepted_v4_frame_job(&request)? {
        return Ok(previous);
    }
    validate_policy(&authorized.processing_policy)?;
    validate_authority(&authorized.authority)?;
    let prepared = prepare(&authorized.snapshot, &input.sequence_id, &input.time)?;
    crate::classic_visual::ClassicVisualSession::new(&prepared, &authorized.processing_policy)?;
    let mut ids: Vec<_> = prepared
        .resources()
        .iter()
        .map(|r| r.asset_id.clone())
        .collect();
    ids.sort();
    let asset_lock = assets.capture(&authorized.snapshot, &input.sequence_id, &ids)?;
    let stored = PreparedV4FramePng {
        snapshot: authorized.snapshot,
        sequence_id: input.sequence_id,
        time: input.time,
        asset_lock,
        authority: authorized.authority,
        processing_policy: authorized.processing_policy,
    };
    validate_lock(&stored, &prepared)?;
    store.accept_v4_frame_job(request, stored)
}

pub fn public_identity(prepared: &PreparedVisualFrameV4) -> V4FrameIdentityV1 {
    let i = prepared.identity();
    V4FrameIdentityV1 {
        project_id: i.project_id.clone(),
        revision: i.revision.clone(),
        sequence_id: i.sequence_id.clone(),
        time: i.time.clone(),
        width: i.width,
        height: i.height,
        profile: V4FrameProfileV1::ClassicV31GpuNormalFrame,
    }
}
pub fn public_sources(
    prepared: &PreparedVisualFrameV4,
    decoded: &[DecodedVisualSourceV4],
) -> Result<Vec<V4FrameSourceObservationV1>, EditorError> {
    let bound = bind_visual_sources_v4(prepared, decoded)
        .map_err(|e| EditorError::new(e.code, e.message))?;
    Ok(bound
        .layers
        .iter()
        .zip(&bound.decoded)
        .map(|(layer, observation)| {
            let mut value = V4FrameSourceObservationV1 {
                element_id: layer.element_id.clone(),
                asset_id: layer.asset_id.clone(),
                requested_time: None,
                selected_video_stream: None,
                width: None,
                height: None,
                actual_timestamp_seconds: None,
                frame_duration_seconds: None,
                no_frame_reason: None,
            };
            match observation {
                DecodedVisualSourceV4::ReadyImage { width, height, .. } => {
                    value.width = Some(*width);
                    value.height = Some(*height);
                }
                DecodedVisualSourceV4::ReadyVideo {
                    width,
                    height,
                    requested_time,
                    selected_video_stream,
                    actual_timestamp_seconds,
                    frame_duration_seconds,
                    ..
                } => {
                    value.width = Some(*width);
                    value.height = Some(*height);
                    value.requested_time = Some(requested_time.clone());
                    value.selected_video_stream = *selected_video_stream;
                    value.actual_timestamp_seconds = Some(*actual_timestamp_seconds);
                    value.frame_duration_seconds = Some(*frame_duration_seconds);
                }
                DecodedVisualSourceV4::NoFrame {
                    requested_time,
                    reason,
                    ..
                } => {
                    value.requested_time = Some(requested_time.clone());
                    value.no_frame_reason = Some(match reason {
                        VisualNoFrameReasonV4::OutsideReadWindow => {
                            V4FrameNoSourceReasonV1::OutsideReadWindow
                        }
                        VisualNoFrameReasonV4::Eof => V4FrameNoSourceReasonV1::Eof,
                        VisualNoFrameReasonV4::TimestampGap => {
                            V4FrameNoSourceReasonV1::TimestampGap
                        }
                    });
                }
            }
            value
        })
        .collect())
}
fn private_sources(
    sources: &[V4FrameSourceObservationV1],
) -> Result<Vec<DecodedVisualSourceV4>, EditorError> {
    sources
        .iter()
        .map(|s| match (s.requested_time.as_ref(), s.no_frame_reason) {
            (None, None)
                if s.selected_video_stream.is_none()
                    && s.actual_timestamp_seconds.is_none()
                    && s.frame_duration_seconds.is_none() =>
            {
                Ok(DecodedVisualSourceV4::ReadyImage {
                    element_id: s.element_id.clone(),
                    width: s.width.ok_or_else(stored_invalid)?,
                    height: s.height.ok_or_else(stored_invalid)?,
                })
            }
            (Some(requested_time), None) => Ok(DecodedVisualSourceV4::ReadyVideo {
                element_id: s.element_id.clone(),
                width: s.width.ok_or_else(stored_invalid)?,
                height: s.height.ok_or_else(stored_invalid)?,
                requested_time: requested_time.clone(),
                selected_video_stream: s.selected_video_stream,
                actual_timestamp_seconds: s.actual_timestamp_seconds.ok_or_else(stored_invalid)?,
                frame_duration_seconds: s.frame_duration_seconds.ok_or_else(stored_invalid)?,
            }),
            (Some(requested_time), Some(reason))
                if s.selected_video_stream.is_none()
                    && s.width.is_none()
                    && s.height.is_none()
                    && s.actual_timestamp_seconds.is_none()
                    && s.frame_duration_seconds.is_none() =>
            {
                Ok(DecodedVisualSourceV4::NoFrame {
                    element_id: s.element_id.clone(),
                    requested_time: requested_time.clone(),
                    reason: match reason {
                        V4FrameNoSourceReasonV1::OutsideReadWindow => {
                            VisualNoFrameReasonV4::OutsideReadWindow
                        }
                        V4FrameNoSourceReasonV1::Eof => VisualNoFrameReasonV4::Eof,
                        V4FrameNoSourceReasonV1::TimestampGap => {
                            VisualNoFrameReasonV4::TimestampGap
                        }
                    },
                })
            }
            _ => Err(stored_invalid()),
        })
        .collect()
}
pub fn validate_stored_result(
    input: &PreparedV4FramePng,
    stored: &StoredV4FramePngResultV1,
) -> Result<V4FramePngResultV1, EditorError> {
    let prepared = validate_prepared(input).map_err(|_| stored_invalid())?;
    let public = &stored.public;
    let execution = &public.execution;
    let decoded = private_sources(&public.sources)?;
    crate::classic_visual::ClassicVisualSession::new(&prepared, &input.processing_policy)?
        .project_decoded_sources(&decoded)
        .map_err(|_| stored_invalid())?;
    if public.schema_version != 1
        || public.identity != public_identity(&prepared)
        || public.asset_lock != input.asset_lock
        || public.render_input_sha256
            != render_input_sha256(&input.snapshot, &input.sequence_id, &input.time)?
        || public.sources != public_sources(&prepared, &decoded)?
        || execution.playwright_version != "1.63.0"
        || execution.mediabunny_version != "1.41.0"
        || execution.gpu_backend != "webgpu"
        || !valid_id(&execution.browser_version)
        || !valid_id(&execution.gpu_device)
        || execution.texture_format != "bgra8unorm"
        || execution.decoder_profile != CLASSIC_VISUAL_DECODER_PROFILE_V4
        || [
            &execution.host_sha256,
            &execution.browser_sha256,
            &execution.module_sha256,
            &execution.shader_sha256,
        ]
        .into_iter()
        .any(|h| !valid_hash(h))
        || stored.artifact.mime_type != "image/png"
        || public.artifact.mime_type != "image/png"
        || stored.artifact.size == 0
        || stored.artifact.size > input.processing_policy.max_output_bytes
        || public.artifact.size_bytes != stored.artifact.size.to_string()
        || public.artifact.content_hash != stored.artifact.content_hash
        || !valid_hash(&stored.artifact.content_hash)
    {
        return Err(stored_invalid());
    }
    Ok(public.clone())
}

#[cfg(test)]
mod tests;
