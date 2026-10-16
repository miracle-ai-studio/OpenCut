//! Final blend composition over verified D0 source binding and shared quotas.
pub use opencut_api_contracts::visual_blend_frame_v1::ClassicBlendFramePlanV1;
use opencut_composition::project_v4::*;
use opencut_contracts::{EditorError, blend_frame_v1::*, graphic_frame_v1::BoundGraphicFrameV1};
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;

mod session;
pub use session::*;
#[cfg(test)]
pub(crate) mod tests;

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_VISUAL_PLAN", message)
}
pub fn admit_blend_frame_v1(
    prepared: &PreparedBlendFrameV1,
    policy: &LocalVisualExecutionPolicyV1,
) -> Result<(), EditorError> {
    // Blend selection adds no source surface. The one GPU execution accounts
    // its actual uniform buffers under the original full GPU budget.
    crate::classic_graphic_frame_v1::admit_graphic_frame_v1(prepared.execution(), policy)
}
pub fn plan_v1(
    prepared: &PreparedBlendFrameV1,
    policy: &LocalVisualExecutionPolicyV1,
    coverage: &[NativeTextCoverageV1],
) -> Result<ClassicBlendFramePlanV1, EditorError> {
    let execution =
        crate::classic_graphic_frame_v1::plan_v1(prepared.execution(), policy, coverage)?;
    let mut identity = execution.identity.clone();
    identity.profile = CLASSIC_BLEND_FRAME_PROFILE_V1.into();
    Ok(ClassicBlendFramePlanV1 {
        identity,
        execution,
        blends: prepared.blends().to_vec(),
    })
}

/// The caller supplies the successfully verified graphic binding from the same
/// prepared partition. This wrapper never accepts browser-authored blend facts.
pub fn bind_blend_frame_v1(
    prepared: &PreparedBlendFrameV1,
    child: &BoundGraphicFrameV1,
) -> Result<BoundBlendFrameV1, EditorError> {
    let fixed = prepared.execution().execution().execution().identity();
    if child.identity.project_id != fixed.project_id
        || child.identity.revision != fixed.revision
        || child.identity.sequence_id != fixed.sequence_id
        || child.identity.time != fixed.time
        || child.identity.width != fixed.width
        || child.identity.height != fixed.height
        || child.identity.profile != CLASSIC_GRAPHIC_FRAME_PROFILE_V1
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
    validate_graphic_receipts_v1(prepared.execution(), &child.graphics)
        .map_err(|error| invalid(&error.message))?;
    let mut identity = child.identity.clone();
    identity.profile = CLASSIC_BLEND_FRAME_PROFILE_V1.into();
    Ok(BoundBlendFrameV1 {
        identity,
        execution: child.clone(),
        blends: prepared.blends().to_vec(),
    })
}
