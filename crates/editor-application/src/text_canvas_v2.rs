//! Same canvas planner and resource ports, with explicit source/request identity.
use crate::{
    font_synthesis_v1::binding_for_resource,
    text_canvas_v1::{
        canvas_plan_result_v1, prepare_canvas_partition, sample_canvas_graph,
        validate_canvas_request,
    },
};
use opencut_composition::project_v4::{
    NativeTextCoverageV1, PreparedTextFrameV2, SampledContentV4,
};
use opencut_contracts::{
    EditorError, font_synthesis_v1::FontSynthesisBindingV1, project_v4::*,
    text_canvas_v1::TextCanvasPlanRequestV1, text_canvas_v2::*,
};
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;

#[derive(Clone, Debug)]
pub struct PreparedCanvasPlanV2 {
    execution: PreparedTextFrameV2,
    font_synthesis: Option<FontSynthesisBindingV1>,
}
impl PreparedCanvasPlanV2 {
    pub fn execution(&self) -> &PreparedTextFrameV2 {
        &self.execution
    }
    pub fn font_synthesis(&self) -> Option<&FontSynthesisBindingV1> {
        self.font_synthesis.as_ref()
    }
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}

pub fn prepare_canvas_plan_v2(
    snapshot: &ProjectSnapshotV4,
    request: &TextCanvasPlanRequestV1,
) -> Result<PreparedCanvasPlanV2, EditorError> {
    validate_canvas_request(snapshot, request)?;
    // A new consumer must not accept a caller-forged provider/hash, including
    // unused resources that sampling will intentionally omit.
    for font in &snapshot.project.fonts {
        binding_for_resource(font)?;
    }
    let mut sampled = sample_canvas_graph(snapshot, request)?;
    let target = sampled
        .layers
        .iter()
        .find(|layer| layer.element_id == request.element_id)
        .ok_or_else(|| invalid("Canvas text target is missing"))?;
    let SampledContentV4::Text { text } = &target.content else {
        return Err(invalid("Canvas requires free text"));
    };
    let FontBindingV4::Locked { font_id } = &text.font else {
        return Err(invalid("Canvas requires a locked font"));
    };
    let font_id = font_id.clone();
    let actual = sampled
        .fonts
        .iter()
        .find(|font| font.id == font_id)
        .ok_or_else(|| invalid("Canvas font is missing from the fixed graph"))?;
    let font_synthesis = binding_for_resource(actual)?;
    if let Some(binding) = &font_synthesis {
        let weight = match text.font_weight {
            FontWeightV4::Normal => 400,
            FontWeightV4::Bold => 700,
        };
        if weight != binding.requested.weight || text.font_style != binding.requested.style {
            return Err(invalid(
                "Canvas text request differs from the explicit synthesis binding",
            ));
        }
    }
    // Isolate only the observed target, preserving its transition facts until
    // the original pure preparer has rejected any unsupported target transition.
    // The source snapshot and sampled authored request are never persisted back.
    sampled
        .layers
        .retain(|layer| layer.element_id == request.element_id);
    sampled.fonts.retain(|font| font.id == font_id);
    if let Some(binding) = &font_synthesis {
        let SampledContentV4::Text { text } = &mut sampled.layers[0].content else {
            unreachable!()
        };
        text.font_style = binding.actual.style;
        sampled.fonts[0].origin = ResourceOriginV4::Generated {
            provider: "opencut-canvas-execution-v2".into(),
            generation_id: binding.binding_id.clone(),
        };
    }
    let execution = prepare_canvas_partition(sampled, request)?;
    Ok(PreparedCanvasPlanV2 {
        execution,
        font_synthesis,
    })
}

pub fn canvas_plan_result_v2(
    prepared: &PreparedCanvasPlanV2,
    request: &TextCanvasPlanRequestV1,
    policy: &LocalVisualExecutionPolicyV1,
    coverage: &[NativeTextCoverageV1],
) -> Result<TextCanvasPlanResultV2, EditorError> {
    let mut actual = canvas_plan_result_v1(&prepared.execution, request, policy, coverage)?;
    if let Some(binding) = &prepared.font_synthesis {
        let font = &actual.layer.font;
        if font.content_hash != binding.actual.content_hash
            || font.size != binding.actual.size_bytes
            || font.weight != binding.actual.weight
            || font.style != binding.actual.style
            || font.face_index != 0
        {
            return Err(invalid(
                "Canvas result differs from the bound actual source",
            ));
        }
    }
    actual.identity.profile = CLASSIC_TEXT_CANVAS_PROFILE_V2.into();
    Ok(TextCanvasPlanResultV2 {
        schema_version: 2,
        identity: actual.identity,
        layer: actual.layer,
        font_synthesis: prepared.font_synthesis.clone(),
    })
}

#[cfg(test)]
mod tests;
