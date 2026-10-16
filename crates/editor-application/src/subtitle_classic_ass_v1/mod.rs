//! Source-compatible Classic ASS: parser losses are explicit, fixed-font
//! measurement is native-authorized, and one ordinary transaction owns all cues.
use crate::subtitle_text_v1::{self as text, SubtitleCueInputV1, SubtitleCueLayoutV1};
use opencut_composition::project_v4::PreparedTextFrameV2;
use opencut_contracts::{
    EditorError, Time, TimeRange, project_v4::*, subtitle_ass_v1::SubtitleAssFontRequirementV1,
    subtitle_classic_ass_v1::*, subtitle_text_v1::*, subtitle_v4::SubtitleFormatV4,
    text_authoring_v1::TextTrackTargetV1, text_frame_v2::TextLayerPlanV2,
};
mod parse;
#[derive(Clone, Debug)]
pub struct PreparedSubtitleClassicAssV1 {
    fixed: SubtitleClassicAssPlanRequestV1,
    inner: Option<text::PreparedSubtitleTextV1>,
    inner_request: Option<SubtitleTextPlanRequestV1>,
    requirements: Vec<SubtitleAssFontRequirementV1>,
    placements: Vec<SubtitleClassicPlacementV1>,
    warnings: Vec<String>,
    skipped: u32,
    width: u32,
    height: u32,
}
impl PreparedSubtitleClassicAssV1 {
    pub fn frames(&self) -> &[PreparedTextFrameV2] {
        self.inner.as_ref().map(|i| i.frames()).unwrap_or_default()
    }
}
fn invalid(s: &str) -> EditorError {
    EditorError::new("INVALID_SUBTITLE_CLASSIC_ASS_PLAN", s)
}
fn identity(s: &str) -> Result<(), EditorError> {
    if s.trim().is_empty() || s.len() > 128 || s.chars().any(char::is_control) {
        Err(invalid(
            "Classic ASS scope identities must contain 1..128 bounded bytes",
        ))
    } else {
        Ok(())
    }
}
fn font_weight(w: FontWeightV4) -> u16 {
    if w == FontWeightV4::Bold { 700 } else { 400 }
}
fn size(project: &ProjectDocumentV4, sequence: &str) -> Result<CanvasSizeV4, EditorError> {
    match &project.settings {
        SettingsProfileV4::ClassicGlobal { settings } => Ok(settings.canvas_size),
        SettingsProfileV4::LegacyPerScene { formats } => formats
            .iter()
            .find(|f| f.sequence_id == sequence)
            .map(|f| f.canvas_size)
            .ok_or_else(|| invalid("Classic ASS canvas is missing")),
    }
}
fn destination(
    snapshot: &ProjectSnapshotV4,
    request: &SubtitleClassicAssPlanRequestV1,
) -> Result<(), EditorError> {
    let mut check = snapshot.project.clone();
    let sequence = check
        .sequences
        .iter_mut()
        .find(|s| s.id == request.sequence_id)
        .ok_or_else(|| invalid("Classic ASS scene does not exist"))?;
    let track_id = match &request.track {
        TextTrackTargetV1::Existing { track_id } | TextTrackTargetV1::New { track_id } => track_id,
    };
    identity(track_id)?;
    if matches!(request.track, TextTrackTargetV1::New { .. }) {
        sequence.tracks.push(TrackV4 {
            id: track_id.clone(),
            name: "Subtitles".into(),
            kind: TrackKindV4::Text,
            enabled: true,
            muted: false,
            locked: false,
            opacity: 1.,
            gain: GainV4::Decibels { value: 0. },
            elements: vec![],
            channels: vec![],
        });
    }
    let track = sequence
        .tracks
        .iter()
        .find(|t| t.id == *track_id)
        .ok_or_else(|| invalid("Classic ASS destination track is missing"))?;
    if track.locked {
        return Err(EditorError::new(
            "TRACK_LOCKED",
            "Classic ASS destination track is locked",
        ));
    }
    if track.kind != TrackKindV4::Text
        || !track.enabled
        || track.opacity != 1.
        || !track.channels.is_empty()
    {
        return Err(invalid(
            "Classic ASS destination must be an enabled ordinary text track",
        ));
    }
    opencut_editor_core::project_v4::validate_project_v4(&check)?;
    Ok(())
}
pub fn prepare_with_checkpoint(
    snapshot: &ProjectSnapshotV4,
    request: &SubtitleClassicAssPlanRequestV1,
    checkpoint: &mut impl FnMut() -> Result<(), EditorError>,
) -> Result<PreparedSubtitleClassicAssV1, EditorError> {
    checkpoint()?;
    crate::require_idempotency_key(&request.idempotency_key)?;
    identity(&request.project_id)?;
    identity(&request.sequence_id)?;
    if snapshot.project.id != request.project_id || snapshot.revision != request.expected_revision {
        return Err(crate::revision_conflict(
            &request.expected_revision,
            &snapshot.revision,
        ));
    }
    opencut_editor_core::project_v4::validate_project_v4(&snapshot.project)?;
    destination(snapshot, request)?;
    let parsed = parse::parse(&request.content)?;
    let canvas = size(&snapshot.project, &request.sequence_id)?;
    let mut requirements = Vec::<SubtitleAssFontRequirementV1>::new();
    for cue in &parsed.cues {
        checkpoint()?;
        let s = &cue.style;
        if !requirements
            .iter()
            .any(|r| r.family == s.family && r.weight == s.weight && r.style == s.style)
        {
            if s.family.trim().is_empty()
                || s.family.len() > 1024
                || s.family.chars().any(char::is_control)
            {
                return Err(invalid("Classic ASS font family exceeds supported bounds"));
            }
            requirements.push(SubtitleAssFontRequirementV1 {
                family: s.family.clone(),
                weight: s.weight,
                style: s.style,
                resolved_font_id: None,
            });
        }
    }
    if requirements.len() > 256 || request.font_bindings.len() > requirements.len() {
        return Err(invalid(
            "Classic ASS font mapping exceeds its complete requirements",
        ));
    }
    let mut warnings = parsed.warnings;
    let mut used = vec![false; request.font_bindings.len()];
    for r in &mut requirements {
        let bindings = request
            .font_bindings
            .iter()
            .enumerate()
            .filter(|(_, b)| b.family == r.family && b.weight == r.weight && b.style == r.style)
            .collect::<Vec<_>>();
        if bindings.len() > 1 {
            return Err(invalid("Duplicate Classic ASS font binding"));
        }
        let candidates = snapshot
            .project
            .fonts
            .iter()
            .filter(|f| {
                f.family == r.family
                    && f.weight == font_weight(r.weight)
                    && f.style == r.style
                    && f.face_index == 0
            })
            .collect::<Vec<_>>();
        let font = if let Some((i, b)) = bindings.first() {
            used[*i] = true;
            identity(&b.font_id)?;
            Some(
                snapshot
                    .project
                    .fonts
                    .iter()
                    .find(|f| f.id == b.font_id)
                    .ok_or_else(|| {
                        invalid("Classic ASS font binding is outside the fixed project")
                    })?,
            )
        } else {
            if candidates.len() > 1 {
                return Err(invalid(
                    "Classic ASS font family has multiple fixed faces; select a binding",
                ));
            }
            candidates.first().copied()
        };
        if let Some(font) = font {
            if font.weight != font_weight(r.weight) || font.style != r.style || font.face_index != 0
            {
                return Err(invalid(
                    "Classic ASS font binding must preserve exact static weight/style",
                ));
            }
            r.resolved_font_id = Some(font.id.clone());
            if font.family != r.family {
                warnings.push(format!(
                    "Explicit font alias: {} -> {} ({}, {:?})",
                    r.family, font.family, font.weight, font.style
                ));
            }
        }
    }
    if used.iter().any(|u| !*u) {
        return Err(invalid("Unrelated Classic ASS font binding"));
    }
    let mut fixed = request.clone();
    fixed.observations = None;
    let mut prepared = PreparedSubtitleClassicAssV1 {
        fixed,
        inner: None,
        inner_request: None,
        requirements,
        placements: vec![],
        warnings,
        skipped: parsed.skipped,
        width: canvas.width,
        height: canvas.height,
    };
    if parsed.cues.is_empty()
        || prepared
            .requirements
            .iter()
            .any(|r| r.resolved_font_id.is_none())
    {
        if request.observations.is_some() {
            return Err(invalid(
                "Classic ASS observations cannot resolve absent cues or fonts",
            ));
        }
        return Ok(prepared);
    }
    let mut cues = Vec::with_capacity(parsed.cues.len());
    for cue in parsed.cues {
        checkpoint()?;
        let mut layout = cue.style.layout;
        if layout.left_margin_ratio > 0. || layout.right_margin_ratio > 0. {
            layout.max_width_px = Some(
                (f64::from(canvas.width)
                    * (1. - layout.left_margin_ratio - layout.right_margin_ratio))
                    .max(0.),
            );
        }
        // Match Classic's declared canvas defaults and PlayRes-independent raw
        // spacing. ASS external-renderer scaling is not imported here.
        let text = TextV4 {
            content: cue.text,
            font: FontBindingV4::Locked {
                font_id: prepared
                    .requirements
                    .iter()
                    .find(|r| {
                        r.family == cue.style.family
                            && r.weight == cue.style.weight
                            && r.style == cue.style.style
                    })
                    .unwrap()
                    .resolved_font_id
                    .clone()
                    .unwrap(),
            },
            font_weight: cue.style.weight,
            font_style: cue.style.style,
            decoration: cue.style.decoration,
            align: cue.style.align,
            font_size: FontSizeV4::ClassicHeight90 {
                value: cue.style.size,
            },
            color: cue.style.color,
            letter_spacing_px: cue.style.spacing,
            line_height_multiplier: 1.2,
            background: cue.style.background,
        };
        prepared.placements.push(SubtitleClassicPlacementV1 {
            left_margin_px: f64::from(canvas.width) * layout.left_margin_ratio,
            right_margin_px: f64::from(canvas.width) * layout.right_margin_ratio,
            vertical_margin_px: f64::from(canvas.height) * layout.vertical_margin_ratio,
            vertical_align: match layout.vertical_align {
                2 => SubtitleClassicVerticalAlignV1::Top,
                1 => SubtitleClassicVerticalAlignV1::Middle,
                _ => SubtitleClassicVerticalAlignV1::Bottom,
            },
        });
        cues.push(SubtitleCueInputV1 {
            range: cue.range,
            text,
            layout,
        });
    }
    let mut style = cues[0].text.clone();
    style.content.clear();
    let inner_request = SubtitleTextPlanRequestV1 {
        project_id: request.project_id.clone(),
        expected_revision: request.expected_revision.clone(),
        sequence_id: request.sequence_id.clone(),
        idempotency_key: request.idempotency_key.clone(),
        track: request.track.clone(),
        source: SubtitleTextSourceV1::File {
            format: SubtitleFormatV4::Ass,
            content: request.content.clone(),
        },
        style,
        max_width_px: None,
        observations: None,
    };
    prepared.inner = Some(text::prepare_cues_with_checkpoint(
        snapshot,
        &inner_request,
        || Ok((cues, vec![])),
        checkpoint,
    )?);
    prepared.inner_request = Some(inner_request);
    Ok(prepared)
}
pub fn finish(
    prepared: &PreparedSubtitleClassicAssV1,
    request: &SubtitleClassicAssPlanRequestV1,
    layers: &[TextLayerPlanV2],
) -> Result<SubtitleClassicAssPlanResultV1, EditorError> {
    let mut fixed = request.clone();
    fixed.observations = None;
    if fixed != prepared.fixed || layers.len() != prepared.frames().len() {
        return Err(invalid("Classic ASS fixed request or font layers changed"));
    }
    let mut result = SubtitleClassicAssPlanResultV1 {
        schema_version: 1,
        revision: request.expected_revision.clone(),
        width: prepared.width,
        height: prepared.height,
        font_requirements: prepared.requirements.clone(),
        cues: vec![],
        transaction: None,
        selection: None,
        warnings: prepared.warnings.clone(),
        skipped_cue_count: prepared.skipped,
    };
    if let Some(inner) = &prepared.inner {
        let mut inner_request = prepared.inner_request.clone().unwrap();
        inner_request.observations = request.observations.clone();
        let planned = text::finish(inner, &inner_request, layers)?;
        result.cues = planned
            .cues
            .into_iter()
            .zip(&prepared.placements)
            .map(|(cue, placement)| SubtitleClassicAssCuePlanV1 {
                timeline_range: cue.timeline_range,
                layer: cue.layer,
                directive: cue.directive,
                placement: placement.clone(),
            })
            .collect();
        result.transaction = planned.transaction;
        result.selection = planned.selection;
    } else if request.observations.is_some() {
        return Err(invalid("Classic ASS has no resolved measurement inputs"));
    }
    Ok(result)
}
#[cfg(test)]
mod tests;
