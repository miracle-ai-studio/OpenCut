//! Classic masks delegate source binding to the existing blend session.
pub use opencut_api_contracts::visual_mask_frame_v1::ClassicMaskFramePlanV1;
use opencut_composition::project_v4::*;
use opencut_contracts::{EditorError, blend_frame_v1::BoundBlendFrameV1, mask_frame_v1::*};
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;
mod session;
pub use session::*;
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_VISUAL_PLAN", message)
}
/// Reserve full-frame mask surfaces before delegated media/font admission.
/// The local adapter additionally accounts exact logical canvases and all warm
/// textures before allocation; policy limits remain the caller's original ones.
pub(crate) fn mask_execution_policy_v1(
    prepared: &PreparedMaskFrameV1,
    policy: &LocalVisualExecutionPolicyV1,
) -> Result<LocalVisualExecutionPolicyV1, EditorError> {
    crate::project_v4_frame_jobs::validate_policy(policy)?;
    let n = prepared.masks().iter().filter(|m| m.mask.is_some()).count() as u64;
    let i = prepared.execution().execution().execution().identity();
    let pixels = u64::from(i.width)
        .checked_mul(u64::from(i.height))
        .and_then(|x| x.checked_mul(n))
        .ok_or_else(|| invalid("Mask raster accounting overflow"))?;
    let budget = || {
        EditorError::new(
            "VISUAL_BUDGET_EXCEEDED",
            "Aggregate mask and delegated source admission exceeded",
        )
    };
    let mut remaining = policy.clone();
    remaining.max_rgba_bytes = remaining
        .max_rgba_bytes
        .checked_sub(pixels.checked_mul(12).ok_or_else(budget)?)
        .ok_or_else(budget)?;
    remaining.max_gpu_bytes = remaining
        .max_gpu_bytes
        .checked_sub(pixels.checked_mul(48).ok_or_else(budget)?)
        .ok_or_else(budget)?;
    Ok(remaining)
}
pub fn admit_mask_frame_v1(
    prepared: &PreparedMaskFrameV1,
    policy: &LocalVisualExecutionPolicyV1,
) -> Result<(), EditorError> {
    crate::classic_blend_frame_v1::admit_blend_frame_v1(
        prepared.blend(),
        &mask_execution_policy_v1(prepared, policy)?,
    )
}
pub fn plan_v1(
    prepared: &PreparedMaskFrameV1,
    policy: &LocalVisualExecutionPolicyV1,
    coverage: &[NativeTextCoverageV1],
) -> Result<ClassicMaskFramePlanV1, EditorError> {
    admit_mask_frame_v1(prepared, policy)?;
    let execution = crate::classic_blend_frame_v1::plan_v1(prepared.blend(), policy, coverage)?;
    let mut identity = execution.identity.clone();
    identity.profile = CLASSIC_MASK_FRAME_PROFILE_V1.into();
    Ok(ClassicMaskFramePlanV1 {
        identity,
        execution,
        masks: prepared.masks().to_vec(),
    })
}
pub fn bind_mask_frame_v1(
    prepared: &PreparedMaskFrameV1,
    child: &BoundBlendFrameV1,
) -> Result<BoundMaskFrameV1, EditorError> {
    let expected =
        crate::classic_blend_frame_v1::bind_blend_frame_v1(prepared.blend(), &child.execution)?;
    if child != &expected {
        return Err(invalid("Mask child differs from fixed blend binding"));
    }
    let masks = bind_mask_layers_v1(prepared, &child.execution)
        .map_err(|e| EditorError::new(e.code, e.message))?;
    let mut identity = child.identity.clone();
    identity.profile = CLASSIC_MASK_FRAME_PROFILE_V1.into();
    Ok(BoundMaskFrameV1 {
        identity,
        execution: child.clone(),
        masks,
    })
}

#[cfg(test)]
mod tests;
