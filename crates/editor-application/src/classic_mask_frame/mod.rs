//! Classic masks delegate source binding to the existing blend session.
pub use opencut_api_contracts::mask_frame_plan::MaskFramePlan;
use opencut_composition::scene::*;
use opencut_contracts::{EditorError, blend_frame::BoundBlendFrame, mask_frame::*};
use opencut_job_execution::media_frame::LocalVisualExecutionPolicy;
mod session;
pub use session::*;
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_VISUAL_PLAN", message)
}
/// Reserve full-frame mask surfaces before delegated media/font admission.
/// The local adapter additionally accounts exact logical canvases and all warm
/// textures before allocation; policy limits remain the caller's original ones.
pub(crate) fn mask_execution_policy(
    prepared: &PreparedMaskFrame,
    policy: &LocalVisualExecutionPolicy,
) -> Result<LocalVisualExecutionPolicy, EditorError> {
    crate::media_frame_jobs::validate_policy(policy)?;
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
pub fn admit_mask_frame(
    prepared: &PreparedMaskFrame,
    policy: &LocalVisualExecutionPolicy,
) -> Result<(), EditorError> {
    crate::classic_blend_frame::admit_blend_frame(
        prepared.blend(),
        &mask_execution_policy(prepared, policy)?,
    )
}
pub fn plan_frame(
    prepared: &PreparedMaskFrame,
    policy: &LocalVisualExecutionPolicy,
    coverage: &[NativeTextCoverage],
) -> Result<MaskFramePlan, EditorError> {
    admit_mask_frame(prepared, policy)?;
    let execution = crate::classic_blend_frame::plan_frame(prepared.blend(), policy, coverage)?;
    let mut identity = execution.identity.clone();
    identity.profile = CLASSIC_MASK_FRAME_PROFILE.into();
    Ok(MaskFramePlan {
        identity,
        execution,
        masks: prepared.masks().to_vec(),
    })
}
pub fn bind_mask_frame(
    prepared: &PreparedMaskFrame,
    child: &BoundBlendFrame,
) -> Result<BoundMaskFrame, EditorError> {
    let expected =
        crate::classic_blend_frame::bind_blend_frame(prepared.blend(), &child.execution)?;
    if child != &expected {
        return Err(invalid("Mask child differs from fixed blend binding"));
    }
    let masks = bind_mask_layers(prepared, &child.execution)
        .map_err(|e| EditorError::new(e.code, e.message))?;
    let mut identity = child.identity.clone();
    identity.profile = CLASSIC_MASK_FRAME_PROFILE.into();
    Ok(BoundMaskFrame {
        identity,
        execution: child.clone(),
        masks,
    })
}

#[cfg(test)]
mod tests;
