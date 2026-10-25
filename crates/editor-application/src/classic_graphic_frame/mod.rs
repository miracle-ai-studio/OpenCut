//! One aggregate GPU frame; child receipts describe delegated binding/quotas only.
pub use opencut_api_contracts::graphic_frame_plan::GraphicFramePlan;
use opencut_composition::scene::*;
use opencut_contracts::text_frame::{BoundTextFrame, TextFrameIdentity};
use opencut_contracts::{EditorError, graphic_frame::*};
use opencut_job_execution::media_frame::LocalVisualExecutionPolicy;
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
pub fn admit_graphic_frame(
    prepared: &PreparedGraphicFrame,
    policy: &LocalVisualExecutionPolicy,
) -> Result<(), EditorError> {
    graphic_media_policy(prepared, policy).map(|_| ())
}
pub(crate) fn graphic_media_policy(
    prepared: &PreparedGraphicFrame,
    policy: &LocalVisualExecutionPolicy,
) -> Result<LocalVisualExecutionPolicy, EditorError> {
    let execution = graphic_execution_policy(prepared, policy)?;
    crate::classic_text_frame::admit_text_frame(prepared.execution().execution(), &execution)
}
/// Reserve only graphics; the delegated caption/text session applies its own
/// existing text/font admission exactly once before media decode admission.
pub(crate) fn graphic_execution_policy(
    prepared: &PreparedGraphicFrame,
    policy: &LocalVisualExecutionPolicy,
) -> Result<LocalVisualExecutionPolicy, EditorError> {
    crate::media_frame_jobs::validate_policy(policy)?;
    let budget = || {
        EditorError::new(
            "VISUAL_BUDGET_EXCEEDED",
            "Aggregate graphic/media/text frame exceeds raster admission",
        )
    };
    let receipts = graphic_raster_receipts(prepared).map_err(error)?;
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
pub fn plan_frame(
    prepared: &PreparedGraphicFrame,
    policy: &LocalVisualExecutionPolicy,
    coverage: &[NativeTextCoverage],
) -> Result<GraphicFramePlan, EditorError> {
    admit_graphic_frame(prepared, policy)?;
    let execution =
        crate::classic_caption_frame::plan_caption_frame(prepared.execution(), policy, coverage)?;
    let mut identity = execution.identity.clone();
    identity.profile = CLASSIC_GRAPHIC_FRAME_PROFILE.into();
    Ok(GraphicFramePlan {
        identity,
        execution,
        graphics: prepared.graphics().to_vec(),
        order: prepared.order().to_vec(),
    })
}
pub fn bind_graphic_frame(
    prepared: &PreparedGraphicFrame,
    child: &BoundTextFrame,
    captions: &[opencut_contracts::caption_frame::CaptionLayoutReceipt],
    receipts: &[GraphicRasterReceipt],
) -> Result<BoundGraphicFrame, EditorError> {
    validate_graphic_receipts(prepared, receipts).map_err(error)?;
    let fixed = prepared.execution().execution().identity();
    if child.identity.project_id != fixed.project_id
        || child.identity.revision != fixed.revision
        || child.identity.sequence_id != fixed.sequence_id
        || child.identity.time != fixed.time
        || child.identity.width != fixed.width
        || child.identity.height != fixed.height
        || child.identity.profile != CLASSIC_TEXT_FRAME_PROFILE
    {
        return Err(invalid(
            "Delegated graphic frame identity differs from fixed scope",
        ));
    }

    let mut layers: BTreeMap<String, opencut_contracts::visual_frame::BoundVisualLayer> =
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
                opencut_contracts::visual_frame::BoundVisualLayer {
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
    identity.profile = CLASSIC_GRAPHIC_FRAME_PROFILE.into();
    Ok(BoundGraphicFrame {
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
