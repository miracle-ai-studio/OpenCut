//! V3 validates caption layout observations against immutable original text
//! before delegating the complete frame binding to the unchanged V2 kernel.
use crate::classic_text_frame_v2::ClassicTextFrameSessionV2;
use opencut_composition::project_v4::*;
use opencut_contracts::{EditorError, text_frame_v2::*, text_frame_v3::*};
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
mod session;
#[cfg(test)]
mod tests;
mod wrapping;
pub use opencut_api_contracts::visual_text_frame_v3::ClassicTextFramePlanV3;
pub use session::*;
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_VISUAL_PLAN", message)
}
fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
pub fn plan_v3(
    prepared: &PreparedTextFrameV3,
    policy: &LocalVisualExecutionPolicyV1,
    coverage: &[NativeTextCoverageV1],
) -> Result<ClassicTextFramePlanV3, EditorError> {
    let execution =
        ClassicTextFrameSessionV2::new(prepared.execution(), policy, coverage)?.typed_plan()?;
    let mut identity = execution.identity.clone();
    identity.profile = CLASSIC_TEXT_FRAME_PROFILE_V3.into();
    Ok(ClassicTextFramePlanV3 {
        identity,
        execution,
        captions: prepared.captions().to_vec(),
    })
}
/// Original native coverage is checked before accepting any browser observation.
/// The only accepted content/placement comes from the verified greedy wrap and
/// the fixed canvas facts; arbitrary client strings and transforms are rejected.
pub fn bind_caption_frame_v3(
    prepared: &PreparedTextFrameV3,
    policy: &LocalVisualExecutionPolicyV1,
    coverage: &[NativeTextCoverageV1],
    decoded: &[DecodedVisualSourceV4],
    rasters: &[TextRasterReceiptV2],
    captions: &[CaptionLayoutReceiptV3],
) -> Result<opencut_contracts::text_frame_v2::BoundTextFrameV2, EditorError> {
    ClassicTextFrameSessionV2::new(prepared.execution(), policy, coverage)?;
    let execution = prepare_caption_receipts_v3(prepared, rasters, captions)?;
    let mut covered = coverage.to_vec();
    for caption in captions {
        let proof = covered
            .iter_mut()
            .find(|proof| proof.element_id == caption.element_id)
            .ok_or_else(|| invalid("Caption native coverage missing"))?;
        proof.content = caption.wrapped_text.clone();
    }
    ClassicTextFrameSessionV2::new(&execution, policy, &covered)?
        .project_preview_sources(decoded, Some(rasters))?
        .1
        .ok_or_else(|| invalid("Caption frame binding missing"))
}

/// Persisted/public receipts are revalidated without fabricating native coverage.
/// This proves their immutable content/layout facts, not independent glyph pixels.
pub fn validate_caption_receipts_v3(
    prepared: &PreparedTextFrameV3,
    rasters: &[TextRasterReceiptV2],
    captions: &[CaptionLayoutReceiptV3],
) -> Result<(), EditorError> {
    prepare_caption_receipts_v3(prepared, rasters, captions).map(|_| ())
}
fn prepare_caption_receipts_v3(
    prepared: &PreparedTextFrameV3,
    rasters: &[TextRasterReceiptV2],
    captions: &[CaptionLayoutReceiptV3],
) -> Result<PreparedTextFrameV2, EditorError> {
    let observed: BTreeMap<_, _> = captions
        .iter()
        .map(|receipt| (&receipt.element_id, receipt))
        .collect();
    if observed.len() != captions.len() || observed.len() != prepared.captions().len() {
        return Err(invalid(
            "Caption layout observations must cover each caption once",
        ));
    }
    let mut graph = prepared.execution().graph().clone();
    let mut ids = BTreeSet::new();
    for directive in prepared.captions() {
        let receipt = observed
            .get(&directive.element_id)
            .ok_or_else(|| invalid("Caption layout observation missing"))?;
        let original = prepared
            .execution()
            .texts()
            .iter()
            .find(|plan| plan.identity.element_id == directive.element_id)
            .ok_or_else(|| invalid("Caption partition missing"))?;
        if receipt.original_text_hash != hash(&original.text.content)
            || directive.wrap_profile != CLASSIC_CAPTION_WRAP_PROFILE_V3
        {
            return Err(invalid(
                "Caption observation changed its original text/profile",
            ));
        }
        let wrapped = wrapping::verify_wrap(
            &original.text.content,
            directive.max_width_px,
            &receipt.measurements,
        )?;
        if wrapped != receipt.wrapped_text {
            return Err(invalid(
                "Caption wrapped text differs from fixed greedy observations",
            ));
        }
        let raster = rasters
            .iter()
            .find(|r| r.element_id == directive.element_id)
            .ok_or_else(|| invalid("Caption raster missing"))?;
        if !ids.insert(&raster.element_id) {
            return Err(invalid("Duplicate caption raster"));
        }
        let layout = &raster.layout;
        wrapping::verify_line_widths(&wrapped, &receipt.measurements, &layout.lines)?;
        let (left, top, width, height) = match &layout.background {
            Some(bg) => (bg.left, bg.top, bg.width, bg.height),
            None => {
                let left = match original.text.align {
                    opencut_contracts::project_v4::TextAlignV4::Left => 0.0,
                    opencut_contracts::project_v4::TextAlignV4::Right => -layout.block.max_width,
                    _ => -layout.block.max_width / 2.0,
                };
                (
                    left,
                    -layout.block.height / 2.0,
                    layout.block.max_width,
                    layout.block.height,
                )
            }
        };
        let canvas = prepared.execution().identity();
        let x = match original.text.align {
            opencut_contracts::project_v4::TextAlignV4::Left => -left,
            opencut_contracts::project_v4::TextAlignV4::Right => {
                f64::from(canvas.width) - (left + width)
            }
            _ => f64::from(canvas.width) / 2.0 - (left + width / 2.0),
        };
        let y = f64::from(canvas.height) - directive.bottom_margin_px - (top + height);
        let layer = graph
            .layers
            .iter_mut()
            .find(|layer| layer.element_id == directive.element_id)
            .ok_or_else(|| invalid("Caption sampled layer missing"))?;
        let SampledContentV4::Text { text } = &mut layer.content else {
            return Err(invalid("Caption execution partition changed"));
        };
        text.content = wrapped.clone();
        let visual = layer
            .visual
            .as_mut()
            .ok_or_else(|| invalid("Caption visual facts missing"))?;
        visual.transform.position_x_px = x - f64::from(canvas.width) / 2.0;
        visual.transform.position_y_px = y - f64::from(canvas.height) / 2.0;
    }
    let execution = prepare_text_frame_v2(&graph)
        .map_err(|error| EditorError::new(error.code, error.message))?;
    let mut observed_ids = BTreeSet::new();
    if rasters.len() != execution.texts().len() {
        return Err(invalid("V3 raster collection is incomplete"));
    }
    for raster in rasters {
        let plan = execution
            .texts()
            .iter()
            .find(|plan| plan.identity.element_id == raster.element_id)
            .ok_or_else(|| invalid("Unexpected V3 raster identity"))?;
        if !observed_ids.insert(&raster.element_id)
            || raster.width != execution.identity().width
            || raster.height != execution.identity().height
        {
            return Err(invalid("V3 raster dimensions or uniqueness changed"));
        }
        crate::classic_text_frame_v2::validate_layout(plan, raster)?;
    }
    Ok(execution)
}
