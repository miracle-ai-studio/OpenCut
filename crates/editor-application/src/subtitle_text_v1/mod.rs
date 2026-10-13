//! Read-only editable subtitle planning over the complete fixed V4 aggregate.
use opencut_composition::{CompositionScope, project_v4::*};
use opencut_contracts::{
    EditorError, Time, TimeRange, project_v4::*, project_v4_ops::*, project_v4_public::*,
    subtitle_text_v1::*, subtitle_v4::SubtitleFormatV4, text_authoring_v1::TextTrackTargetV1,
    text_frame_v2::TextLayerPlanV2, text_frame_v3::CaptionRasterPlanV3,
    timeline_v4::TimelineTargetV4,
};
use opencut_job_contracts::sequence_transcribe_v4::SequenceTranscribeResultV4;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug)]
pub struct PreparedSubtitleTextV1 {
    snapshot: ProjectSnapshotV4,
    request: SubtitleTextPlanRequestV1,
    sequence: SequenceV4,
    track_id: String,
    ids: Vec<String>,
    frames: Vec<PreparedTextFrameV2>,
    warnings: Vec<String>,
}
impl PreparedSubtitleTextV1 {
    pub fn frames(&self) -> &[PreparedTextFrameV2] {
        &self.frames
    }
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_SUBTITLE_TEXT_PLAN", message)
}
fn hash(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
fn zero() -> Time {
    Time {
        ticks: "0".into(),
        timebase: 1,
    }
}
fn source(
    request: &SubtitleTextPlanRequestV1,
    result: Option<&SequenceTranscribeResultV4>,
) -> Result<(Vec<(TimeRange, String)>, Vec<String>), EditorError> {
    match &request.source {
        SubtitleTextSourceV1::File { format, content } => {
            if content.len() > 2 * 1024 * 1024 {
                return Err(invalid("Subtitle file exceeds 2 MiB"));
            }
            let fallback = opencut_contracts::CaptionStyle {
                font_family: "Explicit text style".into(),
                font_size: 24.0,
                color: "#ffffff".into(),
                background_color: None,
                font_policy: opencut_contracts::FontResolutionPolicy::Strict {},
                max_width: None,
                line_height: None,
                text_align: opencut_contracts::TextAlign::Center,
            };
            let cues = match format {
                SubtitleFormatV4::Srt => crate::subtitle_exchange::parse_srt(content, &fallback)?,
                SubtitleFormatV4::Ass => crate::subtitle_exchange::parse_ass(content, &fallback)?,
            };
            Ok((
                cues.into_iter()
                    .map(|c| (c.timeline_range, c.text))
                    .collect(),
                vec![
                    "Interchange styles are replaced by the explicitly selected TextV4 style"
                        .into(),
                ],
            ))
        }
        SubtitleTextSourceV1::Transcript {
            words_per_caption, ..
        } => {
            let result =
                result.ok_or_else(|| invalid("Fixed successful transcription is required"))?;
            if result.project_id != request.project_id
                || result.revision != request.expected_revision
                || result.sequence_id != request.sequence_id
            {
                return Err(invalid("Transcription fixed scope differs"));
            }
            let cues = crate::caption_v4::transcript_cues(*words_per_caption, result)?
                .into_iter()
                .map(|cue| (cue.timeline_range, cue.text))
                .collect();
            Ok((cues,vec!["Word timings are estimated proportionally within ASR segments; they are not measured word timestamps".into()]))
        }
    }
}

pub fn prepare(
    snapshot: &ProjectSnapshotV4,
    request: &SubtitleTextPlanRequestV1,
    result: Option<&SequenceTranscribeResultV4>,
) -> Result<PreparedSubtitleTextV1, EditorError> {
    prepare_with_checkpoint(snapshot, request, result, &mut || Ok(()))
}

pub fn prepare_with_checkpoint(
    snapshot: &ProjectSnapshotV4,
    request: &SubtitleTextPlanRequestV1,
    result: Option<&SequenceTranscribeResultV4>,
    checkpoint: &mut impl FnMut() -> Result<(), EditorError>,
) -> Result<PreparedSubtitleTextV1, EditorError> {
    checkpoint()?;
    crate::require_idempotency_key(&request.idempotency_key)?;
    if snapshot.project.id != request.project_id || snapshot.revision != request.expected_revision {
        return Err(crate::revision_conflict(
            &request.expected_revision,
            &snapshot.revision,
        ));
    }
    if !request.style.content.is_empty() {
        return Err(invalid("Subtitle style content must be empty"));
    }
    if !matches!(request.style.font, FontBindingV4::Locked { .. }) {
        return Err(invalid("Subtitle text requires an imported locked font"));
    }
    let mut sequence = snapshot
        .project
        .sequences
        .iter()
        .find(|s| s.id == request.sequence_id)
        .cloned()
        .ok_or_else(|| invalid("Subtitle scene does not exist"))?;
    let track_id = match &request.track {
        TextTrackTargetV1::Existing { track_id } | TextTrackTargetV1::New { track_id } => {
            track_id.clone()
        }
    };
    if track_id.trim().is_empty() || track_id.len() > 128 || track_id.chars().any(char::is_control)
    {
        return Err(invalid("Invalid subtitle track identity"));
    }
    let index = match &request.track {
        TextTrackTargetV1::Existing { .. } => sequence
            .tracks
            .iter()
            .position(|t| t.id == track_id)
            .ok_or_else(|| invalid("Subtitle text track is missing"))?,
        TextTrackTargetV1::New { .. } => {
            sequence.tracks.push(TrackV4 {
                id: track_id.clone(),
                name: "Subtitles".into(),
                kind: TrackKindV4::Text,
                enabled: true,
                locked: false,
                muted: false,
                opacity: 1.0,
                gain: GainV4::Decibels { value: 0.0 },
                elements: vec![],
                channels: vec![],
            });
            sequence.tracks.len() - 1
        }
    };
    let track = &mut sequence.tracks[index];
    if track.locked {
        return Err(EditorError::new("TRACK_LOCKED", "Subtitle track is locked"));
    }
    if track.kind != TrackKindV4::Text
        || !track.enabled
        || track.opacity != 1.0
        || !track.channels.is_empty()
    {
        return Err(invalid(
            "Subtitle target must be an enabled ordinary text track",
        ));
    }
    let (cues, warnings) = source(request, result)?;
    if cues.is_empty() || cues.len() > 5000 {
        return Err(invalid("Subtitle source requires 1..5000 cues"));
    }
    let mut ids = Vec::new();
    for (i, (range, content)) in cues.into_iter().enumerate() {
        let id = crate::content_id(
            "subtitle-text",
            format!(
                "{}:{}:{}:{}",
                request.project_id, request.sequence_id, request.idempotency_key, i
            )
            .as_bytes(),
        );
        let mut text = request.style.clone();
        text.content = content;
        track.elements.push(TimelineElementV4 {
            id: id.clone(),
            name: format!("Subtitle {}", i + 1),
            timeline_range: range,
            enabled: true,
            source_span: ElementSourceSpanV4::ClassicTrimmed {
                trim_start: zero(),
                trim_end: zero(),
                source_duration: None,
            },
            content: ElementContentV4::Text { text },
            channels: vec![],
            visual: Some(VisualStyleV4 {
                profile: VisualProfileV4::ClassicCanvasV31,
                transform: TransformV4 {
                    position_x_px: 0.0,
                    position_y_px: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation_degrees: 0.0,
                },
                opacity: 1.0,
                blend_mode: BlendModeV4::Normal,
                effects: vec![],
                masks: vec![],
            }),
        });
        ids.push(id);
    }
    // Validate the aggregate before Put can accidentally replace a global ID.
    let mut candidate = snapshot.project.clone();
    *candidate
        .sequences
        .iter_mut()
        .find(|s| s.id == request.sequence_id)
        .unwrap() = sequence.clone();
    opencut_editor_core::project_v4::validate_project_v4(&candidate)?;
    candidate = crate::project_v4::plan(
        snapshot,
        &ApplyV4Project {
            project_id: request.project_id.clone(),
            expected_revision: request.expected_revision.clone(),
            idempotency_key: request.idempotency_key.clone(),
            label: "Create subtitle text".into(),
            operations: vec![ProjectOperationV4::SequencePut {
                sequence: sequence.clone(),
            }],
        },
    )?;
    let candidate = ProjectSnapshotV4 {
        project: candidate,
        revision: snapshot.revision.clone(),
    };
    let mut frames = Vec::new();
    for id in &ids {
        checkpoint()?;
        let element = sequence.tracks[index]
            .elements
            .iter()
            .find(|e| &e.id == id)
            .unwrap();
        let mut sampled = compile_frame_v4(
            &candidate,
            &request.sequence_id,
            &element.timeline_range.start,
            CompositionScope::Visual,
        )
        .map_err(|e| EditorError::new(e.code, e.message))?;
        let target = prepare_text_raster_v1(&sampled, id)
            .map_err(|e| EditorError::new(e.code, e.message))?;
        sampled.layers.retain(|l| &l.element_id == id);
        sampled.fonts.retain(|f| f.id == target.font.id);
        sampled.resources.clear();
        sampled.transitions.clear();
        let frame =
            prepare_text_frame_v2(&sampled).map_err(|e| EditorError::new(e.code, e.message))?;
        let width = frame.identity().width;
        if request.max_width_px.is_some_and(|w| w == 0 || w > width) {
            return Err(invalid("Subtitle wrap width must fit canvas"));
        }
        frames.push(frame);
    }
    let mut fixed = request.clone();
    fixed.observations = None;
    Ok(PreparedSubtitleTextV1 {
        snapshot: snapshot.clone(),
        request: fixed,
        sequence,
        track_id,
        ids,
        frames,
        warnings,
    })
}

pub fn finish(
    prepared: &PreparedSubtitleTextV1,
    request: &SubtitleTextPlanRequestV1,
    layers: &[TextLayerPlanV2],
) -> Result<SubtitleTextPlanResultV1, EditorError> {
    let mut fixed = request.clone();
    fixed.observations = None;
    if fixed != prepared.request || layers.len() != prepared.frames.len() {
        return Err(invalid(
            "Subtitle fixed request or layer collection changed",
        ));
    }
    if request
        .observations
        .as_ref()
        .is_some_and(|o| o.len() != layers.len())
    {
        return Err(invalid(
            "Subtitle observations must cover every cue exactly once",
        ));
    }
    let mut sequence = prepared.sequence.clone();
    let track = sequence
        .tracks
        .iter_mut()
        .find(|t| t.id == prepared.track_id)
        .unwrap();
    let mut cues = Vec::new();
    for (i, (frame, layer)) in prepared.frames.iter().zip(layers).enumerate() {
        let original = &frame.texts()[0];
        let font = &original.font;
        if layer.element_id != prepared.ids[i]
            || layer.text != crate::classic_text_frame_v2::text_input(&original.text)
            || layer.font.font_id != font.id
            || layer.font.content_hash != font.content.content_hash
            || layer.font.size != font.content.size.to_string()
            || layer.font.weight != font.weight
            || layer.font.style != font.style
            || layer.font.face_index != font.face_index
            || !layer.font.glyph_coverage_validated
            || layer.font.validated_text_hash != hash(&original.text.content)
            || layer.opacity != original.opacity
        {
            return Err(invalid(
                "Subtitle native layer differs from its fixed text/font/transform",
            ));
        }
        let canvas = frame.identity();
        let directive = CaptionRasterPlanV3 {
            element_id: layer.element_id.clone(),
            max_width_px: request
                .max_width_px
                .map(f64::from)
                .unwrap_or(f64::from(canvas.width) * 0.8),
            bottom_margin_px: f64::from(canvas.height) * 0.05,
            wrap_profile: CLASSIC_CAPTION_WRAP_PROFILE_V3.into(),
        };
        let element = track
            .elements
            .iter_mut()
            .find(|e| e.id == layer.element_id)
            .unwrap();
        if let Some(observations) = &request.observations {
            let observation = &observations[i];
            let receipt = &observation.caption;
            let raster = &observation.raster;
            if receipt.element_id != layer.element_id
                || receipt.original_text_hash != hash(&original.text.content)
                || raster.element_id != layer.element_id
                || raster.width != canvas.width
                || raster.height != canvas.height
            {
                return Err(invalid("Subtitle observation identity/hash/canvas differs"));
            }
            let wrapped = crate::classic_text_frame_v3::verify_wrap(
                &original.text.content,
                directive.max_width_px,
                &receipt.measurements,
            )
            .map_err(|e| invalid(&e.message))?;
            if wrapped != receipt.wrapped_text {
                return Err(invalid("Subtitle wrap differs from greedy observations"));
            }
            let layout = &raster.layout;
            crate::classic_text_frame_v3::verify_line_widths(
                &wrapped,
                &receipt.measurements,
                &layout.lines,
            )
            .map_err(|e| invalid(&e.message))?;
            let (left, top, width, height) = match &layout.background {
                Some(bg) => (bg.left, bg.top, bg.width, bg.height),
                None => {
                    let left = match request.style.align {
                        TextAlignV4::Left => 0.0,
                        TextAlignV4::Right => -layout.block.max_width,
                        TextAlignV4::Center => -layout.block.max_width / 2.0,
                    };
                    (
                        left,
                        -layout.block.height / 2.0,
                        layout.block.max_width,
                        layout.block.height,
                    )
                }
            };
            let x = match request.style.align {
                TextAlignV4::Left => -left,
                TextAlignV4::Right => f64::from(canvas.width) - (left + width),
                TextAlignV4::Center => f64::from(canvas.width) / 2.0 - (left + width / 2.0),
            };
            let y = f64::from(canvas.height) - directive.bottom_margin_px - (top + height);
            // Reprepare only the sampled target after applying application-owned
            // wrapping/placement. V2 checks every font, style and layout fact.
            let mut graph = frame.graph().clone();
            let target = &mut graph.layers[0];
            let SampledContentV4::Text { text } = &mut target.content else {
                return Err(invalid("Subtitle target partition changed"));
            };
            text.content = wrapped.clone();
            let visual = target
                .visual
                .as_mut()
                .ok_or_else(|| invalid("Subtitle visual missing"))?;
            visual.transform.position_x_px = x - f64::from(canvas.width) / 2.0;
            visual.transform.position_y_px = y - f64::from(canvas.height) / 2.0;
            let next =
                prepare_text_frame_v2(&graph).map_err(|e| EditorError::new(e.code, e.message))?;
            crate::classic_text_frame_v2::validate_layout(&next.texts()[0], raster)
                .map_err(|e| invalid(&e.message))?;
            let ElementContentV4::Text { text } = &mut element.content else {
                unreachable!()
            };
            text.content = wrapped;
            let transform = &mut element.visual.as_mut().unwrap().transform;
            transform.position_x_px = x - f64::from(canvas.width) / 2.0;
            transform.position_y_px = y - f64::from(canvas.height) / 2.0;
        }
        cues.push(SubtitleTextCuePlanV1 {
            timeline_range: element.timeline_range.clone(),
            layer: layer.clone(),
            directive,
        });
    }
    let transaction = if request.observations.is_some() {
        let apply = ApplyV4Project {
            project_id: request.project_id.clone(),
            expected_revision: request.expected_revision.clone(),
            idempotency_key: request.idempotency_key.clone(),
            label: "Create subtitle text".into(),
            operations: vec![ProjectOperationV4::SequencePut {
                sequence: sequence.clone(),
            }],
        };
        crate::project_v4::plan(&prepared.snapshot, &apply)?;
        Some(ApplyPublicV4Project {
            project_id: apply.project_id,
            expected_revision: apply.expected_revision,
            idempotency_key: apply.idempotency_key,
            label: apply.label,
            operations: vec![PublicProjectOperationV4::SequencePut { sequence }],
        })
    } else {
        None
    };
    let canvas = prepared.frames[0].identity();
    Ok(SubtitleTextPlanResultV1 {
        schema_version: 1,
        revision: prepared.snapshot.revision.clone(),
        width: canvas.width,
        height: canvas.height,
        cues,
        transaction,
        selection: Some(TimelineTargetV4 {
            track_id: prepared.track_id.clone(),
            element_id: prepared.ids[0].clone(),
        }),
        warnings: prepared.warnings.clone(),
    })
}
#[cfg(test)]
mod tests;
