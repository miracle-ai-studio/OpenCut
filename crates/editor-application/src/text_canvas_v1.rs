//! Canvas editing observes the same sampled IR and native font coverage as the
//! actual renderer. It creates neither a second composition nor a durable job.
use crate::classic_text_frame_v2::ClassicTextFrameSessionV2;
use opencut_composition::{
    CompositionScope,
    project_v4::{
        NativeTextCoverageV1, PreparedTextFrameV2, compile_frame_v4, prepare_text_frame_v2,
        prepare_text_raster_v1,
    },
};
use opencut_contracts::{EditorError, project_v4::ProjectSnapshotV4, text_canvas_v1::*};
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn bounded_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}

/// Host supplies an authorized immutable snapshot; admission precedes its font
/// resource inspection. No native paths or caller identity enter public facts.
pub fn prepare_canvas_plan_v1(
    snapshot: &ProjectSnapshotV4,
    request: &TextCanvasPlanRequestV1,
) -> Result<PreparedTextFrameV2, EditorError> {
    if ![
        &request.project_id,
        &request.revision,
        &request.sequence_id,
        &request.element_id,
    ]
    .into_iter()
    .all(|id| bounded_id(id))
        || opencut_time::RationalTime::try_from(&request.time)?.ticks() < 0
    {
        return Err(invalid(
            "Canvas text requires bounded fixed identities and nonnegative exact time",
        ));
    }
    if snapshot.project.id != request.project_id || snapshot.revision != request.revision {
        return Err(invalid(
            "Canvas text snapshot differs from the requested fixed revision",
        ));
    }
    let sequence = snapshot
        .project
        .sequences
        .iter()
        .find(|s| s.id == request.sequence_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Canvas text scene is missing"))?;
    let (track, element) = sequence
        .tracks
        .iter()
        .find_map(|track| {
            track
                .elements
                .iter()
                .find(|element| element.id == request.element_id)
                .map(|element| (track, element))
        })
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Canvas text element is missing"))?;
    if track.locked {
        return Err(EditorError::new(
            "TRACK_LOCKED",
            "Canvas text track is locked",
        ));
    }
    if !matches!(
        element.content,
        opencut_contracts::project_v4::ElementContentV4::Text { .. }
    ) {
        return Err(invalid("Canvas editing requires a free-text element"));
    }
    // Sample the authorized complete source exactly once. Canvas input observes
    // one target, so unrelated captions/graphics need not be executable by the
    // free-text frame backend. This is an execution partition of sampled IR,
    // never a rewritten/projected source document or another scene clock.
    let sampled = compile_frame_v4(
        snapshot,
        &request.sequence_id,
        &request.time,
        CompositionScope::Visual,
    )
    .map_err(|error| EditorError::new(error.code, error.message))?;
    if !sampled
        .layers
        .iter()
        .any(|layer| layer.element_id == request.element_id)
    {
        return Err(EditorError::new(
            "NOT_FOUND",
            "Canvas text is inactive at the requested exact time",
        ));
    }
    // Validate against the full sampled graph first: target effects, masks and
    // transitions must not disappear merely because we then select its layer.
    let target = prepare_text_raster_v1(&sampled, &request.element_id)
        .map_err(|error| EditorError::new(error.code, error.message))?;
    let mut partition = sampled;
    partition
        .layers
        .retain(|layer| layer.element_id == request.element_id);
    partition.fonts.retain(|font| font.id == target.font.id);
    partition.resources.clear();
    partition.transitions.clear();
    let prepared = prepare_text_frame_v2(&partition)
        .map_err(|error| EditorError::new(error.code, error.message))?;
    Ok(prepared)
}

/// Native font capture/inspection supplies coverage after normal frame budget
/// admission. Browser metrics remain the existing Classic font measurement;
/// this result deliberately does not invent glyph widths in Rust.
pub fn canvas_plan_result_v1(
    prepared: &PreparedTextFrameV2,
    request: &TextCanvasPlanRequestV1,
    policy: &LocalVisualExecutionPolicyV1,
    coverage: &[NativeTextCoverageV1],
) -> Result<TextCanvasPlanResultV1, EditorError> {
    let plan = ClassicTextFrameSessionV2::new(prepared, policy, coverage)?.typed_plan()?;
    if plan.identity.project_id != request.project_id
        || plan.identity.revision != request.revision
        || plan.identity.sequence_id != request.sequence_id
        || plan.identity.time != request.time
    {
        return Err(invalid(
            "Canvas plan differs from the requested fixed scope",
        ));
    }
    let layer = plan
        .texts
        .into_iter()
        .find(|layer| layer.element_id == request.element_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Canvas text is inactive"))?;
    Ok(TextCanvasPlanResultV1 {
        schema_version: 1,
        identity: plan.identity,
        layer,
    })
}

#[cfg(test)]
mod tests;
