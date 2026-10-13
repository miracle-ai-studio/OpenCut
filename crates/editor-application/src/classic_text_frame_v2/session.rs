use super::*;
use crate::classic_visual::{ClassicVisualRgbaSink, ClassicVisualSession};
use crate::font_import_v1::{MAX_FONT_BYTES_V1, MAX_FONT_TEXT_BYTES_V1};
use opencut_composition::project_v4 as composition;
use opencut_contracts::{
    EditorError,
    project_v4::{TextAlignV4, TextDecorationV4},
    visual_frame_v4::BoundVisualLayerV4,
};
use opencut_job_execution::project_v4_frame::LocalVisualExecutionPolicyV1;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

fn budget() -> EditorError {
    EditorError::new(
        "VISUAL_BUDGET_EXCEEDED",
        "Complete text/media frame exceeds admission",
    )
}
fn number(value: &str) -> Result<u64, EditorError> {
    let n = value
        .parse::<u64>()
        .map_err(|_| invalid("Invalid canonical byte count"))?;
    if n.to_string() != value {
        return Err(invalid("Noncanonical byte count"));
    }
    Ok(n)
}
fn hash(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

/// Admission precedes all native resource opening. Subtract all text/font
/// allocations before asking the unchanged media session for decode permits.
pub fn admit_text_frame_v2(
    prepared: &composition::PreparedTextFrameV2,
    policy: &LocalVisualExecutionPolicyV1,
) -> Result<LocalVisualExecutionPolicyV1, EditorError> {
    crate::project_v4_frame_jobs::validate_policy(policy)?;
    let pixels = u64::from(prepared.identity().width) * u64::from(prepared.identity().height);
    let text_pixels = pixels
        .checked_mul(prepared.texts().len() as u64)
        .ok_or_else(budget)?;
    let text_bytes = text_pixels.checked_mul(16).ok_or_else(budget)?;
    let mut fonts = BTreeMap::new();
    let mut content_bytes = 0usize;
    for plan in prepared.texts() {
        content_bytes = content_bytes
            .checked_add(plan.text.content.len())
            .ok_or_else(budget)?;
        if plan.font.content.size == 0 || plan.font.content.size > MAX_FONT_BYTES_V1 {
            return Err(budget());
        }
        fonts.insert(&plan.font.id, plan.font.content.size);
    }
    let font_bytes = fonts
        .values()
        .try_fold(0u64, |n, bytes| n.checked_add(*bytes).ok_or_else(budget))?;
    let encoded = prepared
        .media()
        .resources()
        .iter()
        .try_fold(font_bytes, |n, r| {
            n.checked_add(r.version.size).ok_or_else(budget)
        })?;
    if content_bytes > MAX_FONT_TEXT_BYTES_V1
        || prepared.layers().len() > 60
        || fonts.len() + prepared.media().resources().len() > 32
        || encoded > policy.max_encoded_bytes
        || pixels > policy.max_frame_pixels
        || text_pixels > policy.max_decoded_pixels
    {
        return Err(budget());
    }
    let mut media = policy.clone();
    media.max_rgba_bytes = policy
        .max_rgba_bytes
        .checked_sub(text_bytes)
        .and_then(|n| n.checked_sub(font_bytes.checked_mul(3)?))
        .ok_or_else(budget)?;
    media.max_gpu_bytes = policy
        .max_gpu_bytes
        .checked_sub(text_bytes)
        .ok_or_else(budget)?;
    let remaining_pixels = policy.max_decoded_pixels - text_pixels;
    if remaining_pixels == 0 && !prepared.media().layers().is_empty() {
        return Err(budget());
    }
    // An empty media subgraph allocates no decode surface; policy validation
    // still requires a positive scalar even when there are no decode requests.
    media.max_decoded_pixels = remaining_pixels.max(1);
    let admitted = ClassicVisualSession::new(prepared.media(), &media)?;
    if !prepared.media().layers().is_empty() {
        admitted.decode_admission()?;
    }
    Ok(media)
}

fn identity(prepared: &composition::PreparedTextFrameV2) -> TextFrameIdentityV2 {
    let i = prepared.identity();
    TextFrameIdentityV2 {
        project_id: i.project_id.clone(),
        revision: i.revision.clone(),
        sequence_id: i.sequence_id.clone(),
        time: i.time.clone(),
        width: i.width,
        height: i.height,
        profile: i.profile.clone(),
    }
}
fn text_input(text: &composition::ClassicTextRasterV1) -> TextRasterInputV2 {
    let bg = text.background;
    let t = text.transform;
    TextRasterInputV2 {
        content: text.content.clone(),
        font_size_px: text.font_size_px,
        line_height_px: text.line_height_px,
        letter_spacing_px: text.letter_spacing_px,
        align: match text.align {
            TextAlignV4::Left => "left",
            TextAlignV4::Center => "center",
            TextAlignV4::Right => "right",
        }
        .into(),
        decoration: match text.decoration {
            TextDecorationV4::None => "none",
            TextDecorationV4::Underline => "underline",
            TextDecorationV4::LineThrough => "lineThrough",
        }
        .into(),
        color: text.color,
        background: TextRasterBackgroundV2 {
            enabled: bg.enabled,
            color: bg.color,
            padding_x_px: bg.padding_x_px,
            padding_y_px: bg.padding_y_px,
            offset_x_px: bg.offset_x_px,
            offset_y_px: bg.offset_y_px,
            corner_radius_percent: bg.corner_radius_percent,
        },
        transform: TextRasterTransformV2 {
            center_x: t.center_x,
            center_y: t.center_y,
            scale_x: t.scale_x,
            scale_y: t.scale_y,
            rotation_radians: t.rotation_radians,
        },
    }
}
fn font_resource(plan: &composition::TextRasterPlanV1) -> TextFontResourceV2 {
    TextFontResourceV2 {
        font_id: plan.font.id.clone(),
        content_hash: plan.font.content.content_hash.clone(),
        size: plan.font.content.size.to_string(),
        weight: plan.font.weight,
        style: plan.font.style,
        face_index: plan.font.face_index,
    }
}

pub struct ClassicTextFrameSessionV2<'a> {
    prepared: &'a composition::PreparedTextFrameV2,
    policy: LocalVisualExecutionPolicyV1,
    media: ClassicVisualSession<'a>,
    coverage: BTreeMap<String, composition::NativeTextCoverageV1>,
    resolved_fonts: BTreeSet<String>,
    bound: Option<BoundTextFrameV2>,
    received_bytes: u64,
}
impl<'a> ClassicTextFrameSessionV2<'a> {
    pub fn new(
        prepared: &'a composition::PreparedTextFrameV2,
        policy: &LocalVisualExecutionPolicyV1,
        coverage: &[composition::NativeTextCoverageV1],
    ) -> Result<Self, EditorError> {
        let media_policy = admit_text_frame_v2(prepared, policy)?;
        let mut proofs = BTreeMap::new();
        for proof in coverage {
            let plan = prepared
                .texts()
                .iter()
                .find(|t| t.identity.element_id == proof.element_id)
                .ok_or_else(|| invalid("Unexpected native font coverage"))?;
            if proof.font != plan.font
                || proof.content != plan.text.content
                || proofs
                    .insert(proof.element_id.clone(), proof.clone())
                    .is_some()
            {
                return Err(invalid("Native coverage differs from fixed font or text"));
            }
        }
        if proofs.len() != prepared.texts().len() {
            return Err(invalid("Native font coverage is incomplete"));
        }
        Ok(Self {
            prepared,
            policy: policy.clone(),
            media: ClassicVisualSession::new(prepared.media(), &media_policy)?,
            coverage: proofs,
            resolved_fonts: BTreeSet::new(),
            bound: None,
            received_bytes: 0,
        })
    }
    pub fn policy(&self) -> &LocalVisualExecutionPolicyV1 {
        &self.policy
    }
    pub fn prepared(&self) -> &composition::PreparedTextFrameV2 {
        self.prepared
    }
    pub fn bound(&self) -> Option<&BoundTextFrameV2> {
        self.bound.as_ref()
    }
    pub fn typed_plan(&self) -> Result<ClassicTextFramePlanV2, EditorError> {
        let texts = self
            .prepared
            .texts()
            .iter()
            .map(|t| TextLayerPlanV2 {
                element_id: t.identity.element_id.clone(),
                text: text_input(&t.text),
                opacity: t.opacity,
                font: ValidatedTextFontV2 {
                    font_id: t.font.id.clone(),
                    content_hash: t.font.content.content_hash.clone(),
                    size: t.font.content.size.to_string(),
                    weight: t.font.weight,
                    style: t.font.style,
                    face_index: t.font.face_index,
                    glyph_coverage_validated: true,
                    validated_text_hash: hash(&t.text.content),
                },
            })
            .collect();
        Ok(ClassicTextFramePlanV2 {
            identity: identity(self.prepared),
            media: self.media.typed_plan()?,
            texts,
            order: self
                .prepared
                .layers()
                .iter()
                .map(|l| l.element_id.clone())
                .collect(),
        })
    }
    pub fn plan(&self) -> Result<Value, EditorError> {
        serde_json::to_value(self.typed_plan()?).map_err(|_| invalid("Cannot serialize text frame"))
    }
    fn bind_rasters(
        &self,
        media: &opencut_contracts::visual_frame_v4::BoundVisualFrameV4,
        rasters: &[TextRasterReceiptV2],
    ) -> Result<BoundTextFrameV2, EditorError> {
        let mut bindings = Vec::new();
        for observed in rasters {
            let plan = self
                .prepared
                .texts()
                .iter()
                .find(|t| t.identity.element_id == observed.element_id)
                .ok_or_else(|| invalid("Unexpected raster"))?;
            validate_layout(plan, observed)?;
            let l = &observed.layout;
            bindings.push(composition::TextRasterBindingV1 {
                coverage: self.coverage[&observed.element_id].clone(),
                raster: composition::TextRasterObservationV1 {
                    element_id: observed.element_id.clone(),
                    font_id: l.font_id.clone(),
                    font_content_hash: l.font_content_hash.clone(),
                    font_weight: l.font_weight,
                    font_style: l.font_style,
                    face_index: l.face_index,
                    content: l
                        .lines
                        .iter()
                        .map(|l| l.content.as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    width: observed.width,
                    height: observed.height,
                },
            });
        }
        let bound =
            composition::bind_text_frame_sources_v2(self.prepared, &media.decoded, &bindings)
                .map_err(|e| invalid(e.message))?;
        let layers = bound
            .layers
            .into_iter()
            .map(|l| match l {
                composition::BoundTextFrameLayerV2::Media(m) => m,
                composition::BoundTextFrameLayerV2::Text { plan, quad, .. } => BoundVisualLayerV4 {
                    element_id: plan.identity.element_id,
                    asset_id: plan.font.id,
                    quad: Some(quad),
                },
            })
            .collect();
        let bound = BoundTextFrameV2 {
            identity: identity(self.prepared),
            clear: bound.clear,
            background: bound.background,
            layers,
            decoded: bound.decoded,
            rasters: rasters.to_vec(),
        };
        Ok(bound)
    }
    /// Read-only browser preview projection. Native coverage still comes from
    /// authorized font bytes, and the host separately fences the owner lease.
    pub fn project_preview_sources(
        &self,
        decoded: &[opencut_contracts::visual_frame_v4::DecodedVisualSourceV4],
        rasters: Option<&[TextRasterReceiptV2]>,
    ) -> Result<
        (
            opencut_contracts::visual_frame_v4::BoundVisualFrameV4,
            Option<BoundTextFrameV2>,
        ),
        EditorError,
    > {
        let media = self.media.project_decoded_sources(decoded)?;
        let bound = rasters
            .map(|rasters| self.bind_rasters(&media, rasters))
            .transpose()?;
        Ok((media, bound))
    }
    pub fn decode_admission(
        &self,
    ) -> Result<opencut_api_contracts::visual_preview_v4::VisualDecodeAdmission, EditorError> {
        self.media.decode_admission()
    }
    pub fn callback(
        &mut self,
        callback: TextFrameCallbackV2,
        resources: &mut (impl ClassicTextFrameResources + ?Sized),
    ) -> Result<Value, EditorError> {
        resources.checkpoint()?;
        match callback {
            TextFrameCallbackV2::Media(callback) => self.media.callback(callback, resources),
            TextFrameCallbackV2::Text(TextCallbackV2::Resolve(request)) => {
                let plan = self
                    .prepared
                    .texts()
                    .iter()
                    .find(|t| t.font.id == request.font.font_id)
                    .ok_or_else(|| invalid("Font is outside fixed frame"))?;
                if self.bound.is_some()
                    || font_resource(plan) != request.font
                    || self.resolved_fonts.contains(&request.font.font_id)
                {
                    return Err(invalid("Font resolution changed or repeated"));
                }
                let (transfer, bytes) = resources.resolve_font(&request.font, &request.handle)?;
                if bytes != plan.font.content.size {
                    return Err(invalid("Resolved font size changed"));
                }
                self.resolved_fonts.insert(request.font.font_id);
                Ok(json!({"transferId":transfer,"byteLength":bytes.to_string()}))
            }
            TextFrameCallbackV2::Text(TextCallbackV2::Bind(request)) => {
                if self.bound.is_some()
                    || self.resolved_fonts.len() != self.prepared.graph().fonts.len()
                {
                    return Err(invalid(
                        "Complete text frame must bind once after all font reads",
                    ));
                }
                let media = self
                    .media
                    .bound()
                    .ok_or_else(|| invalid("Media must bind before the complete text frame"))?;
                let bound = self.bind_rasters(media, &request.rasters)?;
                let value = serde_json::to_value(&bound)
                    .map_err(|_| invalid("Cannot serialize complete text frame"))?;
                self.bound = Some(bound);
                Ok(value)
            }
        }
    }
    /// V3-only sealed entry retains this session's authorization/font-read/media
    /// fences. The application reconstructs and validates every caption fact;
    /// no caller can inject a pre-bound public DTO into the V2 session.
    pub(crate) fn bind_caption_sources_v3(
        &mut self,
        prepared: &composition::PreparedTextFrameV3,
        rasters: &[TextRasterReceiptV2],
        captions: &[opencut_contracts::text_frame_v3::CaptionLayoutReceiptV3],
    ) -> Result<Value, EditorError> {
        if self.bound.is_some()
            || self.resolved_fonts.len() != self.prepared.graph().fonts.len()
            || prepared.execution().identity() != self.prepared.identity()
        {
            return Err(invalid(
                "V3 captions must bind once after original font reads",
            ));
        }
        let media = self
            .media
            .bound()
            .ok_or_else(|| invalid("Media must bind before captions"))?;
        let coverage = self.coverage.values().cloned().collect::<Vec<_>>();
        let bound = crate::classic_text_frame_v3::bind_caption_frame_v3(
            prepared,
            &self.policy,
            &coverage,
            &media.decoded,
            rasters,
            captions,
        )?;
        let value = serde_json::to_value(&bound)
            .map_err(|_| invalid("Cannot serialize V3 bound caption frame"))?;
        self.bound = Some(bound);
        Ok(value)
    }
    pub fn rgba_chunk(
        &mut self,
        offset: u64,
        bytes: &[u8],
        sink: &mut dyn ClassicVisualRgbaSink,
    ) -> Result<(), EditorError> {
        let end = offset
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| invalid("RGBA overflow"))?;
        let i = self.prepared.identity();
        if self.bound.is_none()
            || offset != self.received_bytes
            || bytes.is_empty()
            || bytes.len() > 65536
            || !bytes.len().is_multiple_of(4)
            || end > u64::from(i.width) * u64::from(i.height) * 4
        {
            return Err(invalid(
                "RGBA must follow the complete bound frame in exact pixel order",
            ));
        }
        sink.chunk(offset, bytes)?;
        self.received_bytes = end;
        Ok(())
    }
    pub fn validate_receipt(&self, receipt: &ClassicTextFrameReceiptV2) -> Result<(), EditorError> {
        let expected = identity(self.prepared);
        let bytes = u64::from(expected.width) * u64::from(expected.height) * 4;
        if receipt.identity != expected
            || self.bound.as_ref() != Some(&receipt.bound)
            || self.received_bytes != bytes
            || number(&receipt.rgba_bytes)? != bytes
            || number(&receipt.peak_accounted_bytes)? > self.policy.max_rgba_bytes
            || receipt.engine_identity != composition::CLASSIC_TEXT_FRAME_PROFILE_V2
        {
            return Err(invalid(
                "Receipt differs from complete text/media execution",
            ));
        }
        Ok(())
    }
}

pub(crate) fn validate_layout(
    plan: &composition::TextRasterPlanV1,
    raster: &TextRasterReceiptV2,
) -> Result<(), EditorError> {
    let text = text_input(&plan.text);
    let layout = &raster.layout;
    let content = text.content.split('\n').collect::<Vec<_>>();
    if layout.font_id != plan.font.id
        || layout.font_content_hash != plan.font.content.content_hash
        || layout.validated_text_hash != hash(&text.content)
        || layout.font_weight != plan.font.weight
        || layout.font_style != plan.font.style
        || layout.face_index != plan.font.face_index
        || layout.font_size_px != text.font_size_px
        || layout.line_height_px != text.line_height_px
        || layout.letter_spacing_px != text.letter_spacing_px
        || layout.align != text.align
        || layout.baseline != "middle"
        || layout.transform != text.transform
        || layout.lines.len() != content.len()
        || layout.block.height != content.len() as f64 * text.line_height_px
        || layout.block.visual_center_offset
            != (content.len() - 1) as f64 * text.line_height_px / 2.0
    {
        return Err(invalid(
            "Raster layout differs from fixed font, text or sampled parameters",
        ));
    }
    let mut width = 0.0_f64;
    for (index, (line, content)) in layout.lines.iter().zip(content).enumerate() {
        if line.content != content
            || line.baseline_y
                != index as f64 * text.line_height_px - layout.block.visual_center_offset
            || [line.width, line.ascent, line.descent]
                .iter()
                .any(|n| !n.is_finite() || !(*n as f32).is_finite())
        {
            return Err(invalid("Raster line facts differ from the fixed text"));
        }
        width = width.max(line.width);
        let thickness = (text.font_size_px * 0.07).max(1.0);
        let decoration = match text.decoration.as_str() {
            "none" => None,
            _ => Some(TextRectV2 {
                left: aligned_left(&text.align, line.width),
                top: if text.decoration == "underline" {
                    line.baseline_y + line.descent + thickness
                } else {
                    line.baseline_y - (line.ascent - line.descent) * 0.35
                },
                width: line.width,
                height: thickness,
            }),
        };
        if line.decoration != decoration {
            return Err(invalid(
                "Raster decoration differs from sampled text and line facts",
            ));
        }
    }
    if layout.block.max_width != width || layout.background.is_some() != text.background.enabled {
        return Err(invalid("Raster block geometry differs from line facts"));
    }
    if let Some(background) = &layout.background {
        let bg = &text.background;
        let width = layout.block.max_width + 2.0 * bg.padding_x_px;
        let height = layout.block.height + 2.0 * bg.padding_y_px;
        let expected = TextBackgroundRectV2 {
            left: aligned_left(&text.align, layout.block.max_width) - bg.padding_x_px
                + bg.offset_x_px,
            top: -layout.block.height / 2.0 - bg.padding_y_px + bg.offset_y_px,
            width,
            height,
            radius: width.min(height) / 2.0 * bg.corner_radius_percent.clamp(0.0, 100.0) / 100.0,
        };
        if background != &expected {
            return Err(invalid(
                "Raster background differs from sampled text and block facts",
            ));
        }
    }
    Ok(())
}

fn aligned_left(align: &str, width: f64) -> f64 {
    match align {
        "left" => 0.0,
        "right" => -width,
        _ => -width / 2.0,
    }
}
