//! Fixed-revision PNG submission and completion validation. No native I/O.
use crate::project_v4_frame_jobs as media;
use crate::{JobRecord, JobRequest};
pub use media::{public_sources, validate_policy};
use media::{valid_hash, valid_id, validate_authority};
use opencut_composition::{CompositionScope, project_v4::*};
use opencut_contracts::{AssetLockManifest, EditorError, Time, project_v4::ProjectSnapshotV4};
use opencut_job_contracts::ArtifactDescriptor;
pub use opencut_job_contracts::project_v4_text_frame::*;
pub use opencut_job_execution::project_v4_audio::TrustedJobAuthorityV1;
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;
pub use opencut_job_execution::project_v4_text_frame::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub use media::CLASSIC_VISUAL_DECODER_PROFILE_V4;
pub struct AuthorizedV4TextFrameSnapshot {
    pub snapshot: ProjectSnapshotV4,
    pub authority: TrustedJobAuthorityV1,
    pub processing_policy: LocalVisualExecutionPolicyV1,
}
pub trait V4TextFrameJobSubmissionStore {
    fn fixed_v4_text_frame_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<AuthorizedV4TextFrameSnapshot, EditorError>;
    fn find_accepted_v4_text_frame_job(
        &self,
        request: &JobRequest,
    ) -> Result<Option<JobRecord>, EditorError>;
    /// Recheck live original authority/request/snapshot/policy in one write lock.
    fn accept_v4_text_frame_job(
        &mut self,
        request: JobRequest,
        prepared: PreparedV4TextFramePng,
    ) -> Result<JobRecord, EditorError>;
}
pub trait V4TextFrameAssetCapture {
    /// Verify participating V4 versions and hold the CAS gate through acceptance.
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
pub struct StoredV4TextFramePngResultV2 {
    pub public: V4TextFramePngResultV2,
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
pub fn validate_submission(input: &SubmitV4TextFramePng) -> Result<(), EditorError> {
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
pub(crate) fn prepare(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    at: &Time,
) -> Result<PreparedTextFrameV2, EditorError> {
    crate::font_synthesis_v1::reject_legacy_project(&snapshot.project)?;
    let graph = compile_frame_v4(snapshot, sequence_id, at, CompositionScope::Visual)
        .map_err(|e| EditorError::new(e.code, e.message))?;
    prepare_text_frame_v2(&graph).map_err(|e| EditorError::new(e.code, e.message))
}
pub(crate) fn validate_lock(
    input: &PreparedV4TextFramePng,
    prepared: &PreparedTextFrameV2,
) -> Result<(), EditorError> {
    let expected: BTreeMap<_, _> = prepared
        .media()
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
    let expected: BTreeMap<_, _> = prepared
        .graph()
        .fonts
        .iter()
        .map(|f| {
            (
                f.id.clone(),
                (f.content.content_hash.clone(), f.content.size),
            )
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
            "Font lock differs from fixed participating fonts",
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
        2u32,
        snapshot,
        sequence_id,
        at,
        V4TextFrameProfileV2::ClassicV31GpuNormalTextFrameV2,
    ))
    .map_err(|_| invalid("Cannot hash fixed frame input"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
pub fn validate_prepared(
    input: &PreparedV4TextFramePng,
) -> Result<PreparedTextFrameV2, EditorError> {
    validate_policy(&input.processing_policy)?;
    validate_authority(&input.authority)?;
    if !valid_id(&input.snapshot.project.id)
        || !valid_id(&input.snapshot.revision)
        || !valid_id(&input.sequence_id)
    {
        return Err(invalid("Invalid frame preparation scope"));
    }
    let prepared = prepare(&input.snapshot, &input.sequence_id, &input.time)?;
    crate::classic_text_frame_v2::admit_text_frame_v2(&prepared, &input.processing_policy)?;
    validate_lock(input, &prepared)?;
    Ok(prepared)
}
pub fn validate_prepared_request(
    request: &JobRequest,
    prepared: &PreparedV4TextFramePng,
) -> Result<(), EditorError> {
    let input: SubmitV4TextFramePng = serde_json::from_value(request.input.clone())
        .map_err(|_| invalid("Invalid public frame request"))?;
    validate_submission(&input)?;
    if request.kind != V4_TEXT_FRAME_PNG_KIND
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
pub fn submit_text_frame_png(
    store: &mut impl V4TextFrameJobSubmissionStore,
    assets: &impl V4TextFrameAssetCapture,
    input: SubmitV4TextFramePng,
) -> Result<JobRecord, EditorError> {
    validate_submission(&input)?;
    let authorized = store.fixed_v4_text_frame_snapshot(&input.project_id, &input.revision)?;
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
        kind: V4_TEXT_FRAME_PNG_KIND.into(),
        input: serde_json::to_value(&input)
            .map_err(|_| invalid("Cannot serialize frame request"))?,
    };
    if let Some(previous) = store.find_accepted_v4_text_frame_job(&request)? {
        return Ok(previous);
    }
    validate_policy(&authorized.processing_policy)?;
    validate_authority(&authorized.authority)?;
    let prepared = prepare(&authorized.snapshot, &input.sequence_id, &input.time)?;
    crate::classic_text_frame_v2::admit_text_frame_v2(&prepared, &authorized.processing_policy)?;
    let mut ids: Vec<_> = prepared
        .media()
        .resources()
        .iter()
        .map(|r| r.asset_id.clone())
        .collect();
    ids.sort();
    let mut font_ids: Vec<_> = prepared
        .graph()
        .fonts
        .iter()
        .map(|f| f.id.clone())
        .collect();
    font_ids.sort();
    let (asset_lock, font_lock) =
        assets.capture(&authorized.snapshot, &input.sequence_id, &ids, &font_ids)?;
    let stored = PreparedV4TextFramePng {
        snapshot: authorized.snapshot,
        sequence_id: input.sequence_id,
        time: input.time,
        asset_lock,
        font_lock,
        authority: authorized.authority,
        processing_policy: authorized.processing_policy,
    };
    validate_lock(&stored, &prepared)?;
    store.accept_v4_text_frame_job(request, stored)
}

pub fn public_identity(prepared: &PreparedTextFrameV2) -> V4TextFrameIdentityV2 {
    let i = prepared.identity();
    V4TextFrameIdentityV2 {
        project_id: i.project_id.clone(),
        revision: i.revision.clone(),
        sequence_id: i.sequence_id.clone(),
        time: i.time.clone(),
        width: i.width,
        height: i.height,
        profile: V4TextFrameProfileV2::ClassicV31GpuNormalTextFrameV2,
    }
}
pub fn validate_stored_result(
    input: &PreparedV4TextFramePng,
    stored: &StoredV4TextFramePngResultV2,
) -> Result<V4TextFramePngResultV2, EditorError> {
    let prepared = validate_prepared(input).map_err(|_| stored_invalid())?;
    let public = &stored.public;
    if public.texts.len() != prepared.texts().len() {
        return Err(stored_invalid());
    }
    for (plan, receipt) in prepared.texts().iter().zip(&public.texts) {
        if receipt.element_id != plan.identity.element_id
            || receipt.width != plan.identity.width
            || receipt.height != plan.identity.height
        {
            return Err(stored_invalid());
        }
        crate::classic_text_frame_v2::validate_layout(plan, receipt)
            .map_err(|_| stored_invalid())?;
    }
    let decoded = media::private_sources(&public.sources)?;
    crate::classic_visual::ClassicVisualSession::new(
        prepared.media(),
        &crate::classic_text_frame_v2::admit_text_frame_v2(&prepared, &input.processing_policy)?,
    )?
    .project_decoded_sources(&decoded)
    .map_err(|_| stored_invalid())?;
    validate_output_evidence(
        &input.processing_policy,
        &public.execution,
        &public.artifact,
        &stored.artifact,
    )?;
    if public.schema_version != 2
        || public.identity != public_identity(&prepared)
        || public.asset_lock != input.asset_lock
        || public.font_lock != input.font_lock
        || public.render_input_sha256
            != render_input_sha256(&input.snapshot, &input.sequence_id, &input.time)?
        || public.sources != public_sources(prepared.media(), &decoded)?
    {
        return Err(stored_invalid());
    }
    Ok(public.clone())
}

#[cfg(test)]
mod tests;

/// Shared encoder/runtime and PNG artifact evidence; layout remains profile-specific.
pub(crate) fn validate_output_evidence(
    policy: &LocalVisualExecutionPolicyV1,
    execution: &opencut_job_contracts::project_v4_frame::V4FrameExecutionSummaryV1,
    public: &opencut_job_contracts::project_v4_frame::V4FrameArtifactMetadataV1,
    artifact: &ArtifactDescriptor,
) -> Result<(), EditorError> {
    if execution.playwright_version != "1.63.0"
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
        || artifact.mime_type != "image/png"
        || public.mime_type != "image/png"
        || artifact.size == 0
        || artifact.size > policy.max_output_bytes
        || public.size_bytes != artifact.size.to_string()
        || public.content_hash != artifact.content_hash
        || !valid_hash(&artifact.content_hash)
    {
        return Err(stored_invalid());
    }
    Ok(())
}
