//! Final blend composition over verified D0 source binding and shared quotas.
pub use opencut_api_contracts::blend_frame_plan::BlendFramePlan;
use opencut_composition::scene::*;
use opencut_contracts::{EditorError, blend_frame::*, graphic_frame::BoundGraphicFrame};
use opencut_job_execution::media_frame::LocalVisualExecutionPolicy;

mod session;
pub use session::*;
#[cfg(test)]
pub(crate) mod tests;

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_VISUAL_PLAN", message)
}
pub fn admit_blend_frame(
    prepared: &PreparedBlendFrame,
    policy: &LocalVisualExecutionPolicy,
) -> Result<(), EditorError> {
    // Blend selection adds no source surface. The one GPU execution accounts
    // its actual uniform buffers under the original full GPU budget.
    crate::classic_graphic_frame::admit_graphic_frame(prepared.execution(), policy)
}
pub fn plan_frame(
    prepared: &PreparedBlendFrame,
    policy: &LocalVisualExecutionPolicy,
    coverage: &[NativeTextCoverage],
) -> Result<BlendFramePlan, EditorError> {
    let execution =
        crate::classic_graphic_frame::plan_frame(prepared.execution(), policy, coverage)?;
    let mut identity = execution.identity.clone();
    identity.profile = CLASSIC_BLEND_FRAME_PROFILE.into();
    Ok(BlendFramePlan {
        identity,
        execution,
        blends: prepared.blends().to_vec(),
    })
}

/// The caller supplies the successfully verified graphic binding from the same
/// prepared partition. This wrapper never accepts browser-authored blend facts.
pub fn bind_blend_frame(
    prepared: &PreparedBlendFrame,
    child: &BoundGraphicFrame,
) -> Result<BoundBlendFrame, EditorError> {
    let fixed = prepared.execution().execution().execution().identity();
    if child.identity.project_id != fixed.project_id
        || child.identity.revision != fixed.revision
        || child.identity.sequence_id != fixed.sequence_id
        || child.identity.time != fixed.time
        || child.identity.width != fixed.width
        || child.identity.height != fixed.height
        || child.identity.profile != CLASSIC_GRAPHIC_FRAME_PROFILE
        || !child
            .layers
            .iter()
            .map(|layer| &layer.element_id)
            .eq(prepared.blends().iter().map(|blend| &blend.element_id))
    {
        return Err(invalid(
            "Blend binding differs from fixed scope or complete layer order",
        ));
    }
    validate_graphic_receipts(prepared.execution(), &child.graphics)
        .map_err(|error| invalid(&error.message))?;
    let mut identity = child.identity.clone();
    identity.profile = CLASSIC_BLEND_FRAME_PROFILE.into();
    Ok(BoundBlendFrame {
        identity,
        execution: child.clone(),
        blends: prepared.blends().to_vec(),
    })
}
