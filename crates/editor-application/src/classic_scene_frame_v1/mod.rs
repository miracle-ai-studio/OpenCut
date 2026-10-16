//! Versioned Classic scene stacks reuse fixed-source blend sessions.
pub use opencut_api_contracts::visual_scene_frame_v1::ClassicSceneFramePlanV1;
use opencut_composition::project_v4::*;
use opencut_contracts::text_frame_v2::{TextFontResourceV2, ValidatedTextFontV2};
use opencut_contracts::{EditorError, blend_frame_v1::BoundBlendFrameV1, scene_frame_v1::*};
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
mod session;
pub use session::*;
#[cfg(test)]
mod synthesis_tests;
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_VISUAL_PLAN", message)
}
pub(crate) fn scene_execution_policy_v1(
    prepared: &PreparedSceneFrameV1,
    policy: &LocalVisualExecutionPolicyV1,
) -> Result<LocalVisualExecutionPolicyV1, EditorError> {
    crate::project_v4_frame_jobs::validate_policy(policy)?;
    let text = prepared.execution().execution().execution();
    let mut resources: BTreeMap<&str, u64> = text
        .media()
        .resources()
        .iter()
        .map(|r| (r.asset_id.as_str(), r.version.size))
        .collect();
    let mut fonts: BTreeMap<&str, u64> = text
        .texts()
        .iter()
        .map(|p| (p.font.id.as_str(), p.font.content.size))
        .collect();
    let original_fonts: BTreeSet<_> = fonts.keys().copied().collect();
    for mask in prepared.mask_texts() {
        if mask.font.content.size == 0
            || mask.font.content.size > crate::font_import_v1::MAX_FONT_BYTES_V1
        {
            return Err(invalid("Scene mask font exceeds static face admission"));
        }
        if let Some(previous) = fonts.insert(mask.font.id.as_str(), mask.font.content.size) {
            if previous != mask.font.content.size {
                return Err(invalid("Scene font identity changed"));
            }
        }
    }
    for (id, size) in &fonts {
        if resources.insert(id, size.to_owned()).is_some() {
            return Err(invalid("Scene font and media identities collide"));
        }
    }
    let encoded = resources.values().try_fold(0u64, |sum, size| {
        sum.checked_add(*size)
            .ok_or_else(|| invalid("Scene encoded accounting overflow"))
    })?;
    if resources.len() > 32 || encoded > policy.max_encoded_bytes {
        return Err(EditorError::new(
            "VISUAL_BUDGET_EXCEEDED",
            "Scene media and mask fonts exceed original encoded admission",
        ));
    }
    let extra_font_bytes = fonts
        .iter()
        .filter(|(id, _)| !original_fonts.contains(**id))
        .try_fold(0u64, |sum, (_, size)| {
            sum.checked_add(
                size.checked_mul(3)
                    .ok_or_else(|| invalid("Scene font accounting overflow"))?,
            )
            .ok_or_else(|| invalid("Scene font accounting overflow"))
        })?;
    let masks = prepared
        .stacks()
        .iter()
        .flat_map(|s| &s.masks)
        .filter(|m| m.enabled)
        .count() as u64;
    let i = prepared.execution().execution().execution().identity();
    let pixels = u64::from(i.width)
        .checked_mul(u64::from(i.height))
        .ok_or_else(|| invalid("Scene accounting overflow"))?;
    let budget = || {
        EditorError::new(
            "VISUAL_BUDGET_EXCEEDED",
            "Scene stacks exceed the original visual admission",
        )
    };
    // Shared effect/backdrop scratch plus persistent mask surfaces. The adapter
    // additionally charges every actual local Canvas, warm texture and uniform.
    let cpu = pixels
        .checked_mul(
            masks
                .checked_mul(12)
                .and_then(|n| n.checked_add(16))
                .ok_or_else(budget)?,
        )
        .ok_or_else(budget)?;
    let gpu = pixels
        .checked_mul(
            masks
                .checked_mul(48)
                .and_then(|n| n.checked_add(32))
                .ok_or_else(budget)?,
        )
        .ok_or_else(budget)?;
    let mut result = policy.clone();
    result.max_rgba_bytes = result
        .max_rgba_bytes
        .checked_sub(cpu.checked_add(extra_font_bytes).ok_or_else(budget)?)
        .ok_or_else(budget)?;
    result.max_gpu_bytes = result.max_gpu_bytes.checked_sub(gpu).ok_or_else(budget)?;
    Ok(result)
}
pub fn admit_scene_frame_v1(
    prepared: &PreparedSceneFrameV1,
    policy: &LocalVisualExecutionPolicyV1,
) -> Result<(), EditorError> {
    crate::classic_blend_frame_v1::admit_blend_frame_v1(
        prepared.blend(),
        &scene_execution_policy_v1(prepared, policy)?,
    )
}
pub fn plan_v1(
    prepared: &PreparedSceneFrameV1,
    policy: &LocalVisualExecutionPolicyV1,
    coverage: &[NativeTextCoverageV1],
) -> Result<ClassicSceneFramePlanV1, EditorError> {
    for font in &prepared.source_graph().fonts {
        crate::font_synthesis_v1::binding_for_resource(font)?;
    }
    let font_synthesis = prepared
        .font_synthesis()
        .iter()
        .map(|entry| {
            let binding = crate::font_synthesis_v1::binding_for_resource(&entry.font)?
                .ok_or_else(|| invalid("Scene synthesis owner has no approved binding"))?;
            if binding.requested.weight != entry.requested_weight
                || binding.requested.style != entry.requested_style
            {
                return Err(invalid(
                    "Scene requested typography differs from the approved binding",
                ));
            }
            Ok(SceneFontSynthesisV1 {
                element_id: entry.element_id.clone(),
                owner_id: entry.owner_id.clone(),
                owner_kind: entry.owner_kind,
                font_id: entry.font.id.clone(),
                binding,
            })
        })
        .collect::<Result<Vec<_>, EditorError>>()?;
    admit_scene_frame_v1(prepared, policy)?;
    let title_coverage = title_coverage(prepared, coverage)?;
    let execution =
        crate::classic_blend_frame_v1::plan_v1(prepared.blend(), policy, &title_coverage)?;
    let mut identity = execution.identity.clone();
    identity.profile = CLASSIC_SCENE_FRAME_PROFILE_V1.into();
    Ok(ClassicSceneFramePlanV1 {
        identity,
        execution,
        mask_fonts: mask_fonts(prepared),
        font_synthesis,
        stacks: prepared.stacks().to_vec(),
        steps: prepared.steps().to_vec(),
        background: prepared.background().clone(),
    })
}
pub fn bind_scene_frame_v1(
    prepared: &PreparedSceneFrameV1,
    child: &BoundBlendFrameV1,
) -> Result<BoundSceneFrameV1, EditorError> {
    let expected =
        crate::classic_blend_frame_v1::bind_blend_frame_v1(prepared.blend(), &child.execution)?;
    if child != &expected {
        return Err(invalid("Scene child differs from fixed blend binding"));
    }
    opencut_composition::project_v4::bind_scene_frame_v1(prepared, child)
        .map_err(|e| EditorError::new(e.code, e.message))
}

pub(crate) fn title_coverage(
    prepared: &PreparedSceneFrameV1,
    coverage: &[NativeTextCoverageV1],
) -> Result<Vec<NativeTextCoverageV1>, EditorError> {
    let titles = prepared.execution().execution().execution().texts();
    let mut seen = BTreeSet::new();
    let mut output = vec![];
    for proof in coverage {
        if !seen.insert(proof.element_id.as_str()) {
            return Err(invalid("Scene native coverage repeats an owner"));
        }
        if titles
            .iter()
            .any(|p| p.identity.element_id == proof.element_id)
        {
            output.push(proof.clone());
        } else if let Some(mask) = prepared
            .mask_texts()
            .iter()
            .find(|m| m.mask_id == proof.element_id)
        {
            if proof.font != mask.font || proof.content != mask.mask.content {
                return Err(invalid(
                    "Scene mask coverage differs from fixed font or content",
                ));
            }
        } else {
            return Err(invalid(
                "Scene native coverage contains an unknown text owner",
            ));
        }
    }
    if coverage.len() != titles.len() + prepared.mask_texts().len() {
        return Err(invalid("Scene native font coverage is incomplete"));
    }
    Ok(output)
}
fn mask_fonts(prepared: &PreparedSceneFrameV1) -> Vec<SceneTextMaskFontV1> {
    prepared
        .mask_texts()
        .iter()
        .map(|p| SceneTextMaskFontV1 {
            mask_id: p.mask_id.clone(),
            font: ValidatedTextFontV2 {
                font_id: p.font.id.clone(),
                content_hash: p.font.content.content_hash.clone(),
                size: p.font.content.size.to_string(),
                weight: p.font.weight,
                style: p.font.style,
                face_index: p.font.face_index,
                glyph_coverage_validated: true,
                validated_text_hash: format!("{:x}", Sha256::digest(p.mask.content.as_bytes())),
            },
        })
        .collect()
}
fn resource(font: &ValidatedTextFontV2) -> TextFontResourceV2 {
    TextFontResourceV2 {
        font_id: font.font_id.clone(),
        content_hash: font.content_hash.clone(),
        size: font.size.clone(),
        weight: font.weight,
        style: font.style,
        face_index: font.face_index,
    }
}
