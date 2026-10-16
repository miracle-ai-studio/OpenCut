//! One aggregate GPU frame; child receipts describe delegated binding/quotas only.
pub use opencut_api_contracts::visual_graphic_frame_v1::ClassicGraphicFramePlanV1;
use opencut_composition::project_v4::*;
use opencut_contracts::text_frame_v2::{BoundTextFrameV2, TextFrameIdentityV2};
use opencut_contracts::{EditorError, graphic_frame_v1::*};
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;
use std::collections::BTreeMap;
mod session;
pub use session::*;
#[cfg(test)]
mod tests;
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_VISUAL_PLAN", message)
}
fn error(e: opencut_composition::CompositionError) -> EditorError {
    EditorError::new(e.code, e.message)
}
pub fn admit_graphic_frame_v1(
    prepared: &PreparedGraphicFrameV1,
    policy: &LocalVisualExecutionPolicyV1,
) -> Result<(), EditorError> {
    graphic_media_policy_v1(prepared, policy).map(|_| ())
}
pub(crate) fn graphic_media_policy_v1(
    prepared: &PreparedGraphicFrameV1,
    policy: &LocalVisualExecutionPolicyV1,
) -> Result<LocalVisualExecutionPolicyV1, EditorError> {
    let execution = graphic_execution_policy_v1(prepared, policy)?;
    crate::classic_text_frame_v2::admit_text_frame_v2(prepared.execution().execution(), &execution)
}
/// Reserve only graphics; the delegated caption/text session applies its own
/// existing text/font admission exactly once before media decode admission.
pub(crate) fn graphic_execution_policy_v1(
    prepared: &PreparedGraphicFrameV1,
    policy: &LocalVisualExecutionPolicyV1,
) -> Result<LocalVisualExecutionPolicyV1, EditorError> {
    crate::project_v4_frame_jobs::validate_policy(policy)?;
    let budget = || {
        EditorError::new(
            "VISUAL_BUDGET_EXCEEDED",
            "Aggregate graphic/media/text frame exceeds raster admission",
        )
    };
    let receipts = graphic_raster_receipts_v1(prepared).map_err(error)?;
    let pixels = receipts.iter().try_fold(0u64, |total, r| {
        total
            .checked_add(
                u64::from(r.width)
                    .checked_mul(u64::from(r.height))
                    .ok_or_else(budget)?,
            )
            .ok_or_else(budget)
    })?;
    let bytes = pixels.checked_mul(16).ok_or_else(budget)?;
    let mut remaining = policy.clone();
    remaining.max_rgba_bytes = remaining
        .max_rgba_bytes
        .checked_sub(bytes)
        .ok_or_else(budget)?;
    remaining.max_gpu_bytes = remaining
        .max_gpu_bytes
        .checked_sub(bytes)
        .ok_or_else(budget)?;
    remaining.max_decoded_pixels = remaining
        .max_decoded_pixels
        .checked_sub(pixels)
        .filter(|remaining| *remaining > 0)
        .ok_or_else(budget)?;
    Ok(remaining)
}
pub fn plan_v1(
    prepared: &PreparedGraphicFrameV1,
    policy: &LocalVisualExecutionPolicyV1,
    coverage: &[NativeTextCoverageV1],
) -> Result<ClassicGraphicFramePlanV1, EditorError> {
    admit_graphic_frame_v1(prepared, policy)?;
    let execution = crate::classic_text_frame_v3::plan_v3(prepared.execution(), policy, coverage)?;
    let mut identity = execution.identity.clone();
    identity.profile = CLASSIC_GRAPHIC_FRAME_PROFILE_V1.into();
    Ok(ClassicGraphicFramePlanV1 {
        identity,
        execution,
        graphics: prepared.graphics().to_vec(),
        order: prepared.order().to_vec(),
    })
}
pub fn bind_graphic_frame_v1(
    prepared: &PreparedGraphicFrameV1,
    child: &BoundTextFrameV2,
    captions: &[opencut_contracts::text_frame_v3::CaptionLayoutReceiptV3],
    receipts: &[GraphicRasterReceiptV1],
) -> Result<BoundGraphicFrameV1, EditorError> {
    validate_graphic_receipts_v1(prepared, receipts).map_err(error)?;
    let fixed = prepared.execution().execution().identity();
    if child.identity.project_id != fixed.project_id
        || child.identity.revision != fixed.revision
        || child.identity.sequence_id != fixed.sequence_id
        || child.identity.time != fixed.time
        || child.identity.width != fixed.width
        || child.identity.height != fixed.height
        || child.identity.profile != CLASSIC_TEXT_FRAME_PROFILE_V2
    {
        return Err(invalid(
            "Delegated graphic frame identity differs from fixed scope",
        ));
    }

    let mut layers: BTreeMap<String, opencut_contracts::visual_frame_v4::BoundVisualLayerV4> =
        BTreeMap::new();
    for layer in &child.layers {
        if layers
            .insert(layer.element_id.clone(), layer.clone())
            .is_some()
        {
            return Err(invalid("Delegated layer duplicated"));
        }
    }
    for graphic in prepared.graphics() {
        if layers
            .insert(
                graphic.element_id.clone(),
                opencut_contracts::visual_frame_v4::BoundVisualLayerV4 {
                    element_id: graphic.element_id.clone(),
                    asset_id: format!("graphic:{}", graphic.element_id),
                    quad: graphic.quad.clone(),
                },
            )
            .is_some()
        {
            return Err(invalid("Graphic and delegated layer identities overlap"));
        }
    }
    let ordered = prepared
        .order()
        .iter()
        .map(|id| {
            layers
                .remove(id)
                .ok_or_else(|| invalid("Original graphic frame order is incomplete"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !layers.is_empty() {
        return Err(invalid("Graphic binding contains unexpected layers"));
    }
    let mut identity = child.identity.clone();
    identity.profile = CLASSIC_GRAPHIC_FRAME_PROFILE_V1.into();
    Ok(BoundGraphicFrameV1 {
        identity,
        clear: child.clear,
        background: child.background,
        layers: ordered,
        decoded: child.decoded.clone(),
        rasters: child.rasters.clone(),
        captions: captions.to_vec(),
        graphics: receipts.to_vec(),
    })
}
