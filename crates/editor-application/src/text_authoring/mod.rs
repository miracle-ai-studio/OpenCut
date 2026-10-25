//! Fixed-head free-title authoring. Planning is read-only; the returned public
//! transaction uses the existing atomic apply, idempotency and recovery path.
use crate::scene::{SceneCommitIntent, StoredProjectDocument, public::ProjectPublicResources};
use opencut_contracts::{
    EditorError, FrameRate, Time, TimeRange, project_public::*, scene::*, scene_ops::*,
    text_authoring::*, timeline::TimelineTarget,
};
use opencut_time::{FrameRounding, RationalTime, align_to_frame};

#[cfg(test)]
mod tests;

fn invalid(message: impl Into<String>) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn missing(message: impl Into<String>) -> EditorError {
    EditorError::new("NOT_FOUND", message)
}
fn id(value: &str) -> Result<(), EditorError> {
    if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(invalid("Text identity must contain 1..128 bytes"));
    }
    Ok(())
}
fn time(n: i64, base: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: base,
    }
}
fn fps<'a>(project: &'a EditorDocument, sequence_id: &str) -> Result<&'a FrameRate, EditorError> {
    match &project.settings {
        SettingsProfile::ClassicGlobal { settings } => Ok(&settings.fps),
        SettingsProfile::LegacyPerScene { formats } => formats
            .iter()
            .find(|format| format.sequence_id == sequence_id)
            .map(|format| &format.fps)
            .ok_or_else(|| invalid("Text scene format is missing")),
    }
}

/// Decimal text is bounded before integer parsing. Reduce before conversion so
/// a representable value does not overflow only because it has trailing zeros.
fn decimal(value: &str) -> Result<Time, EditorError> {
    if value.len() > 64 {
        return Err(invalid("Text time exceeds 64 bytes"));
    }
    let value = value.trim();
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 9
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(invalid(
            "Text time must be nonnegative decimal seconds with at most 9 fractional digits",
        ));
    }
    let base = 10_i128.pow(fraction.len() as u32);
    let whole = whole
        .parse::<i128>()
        .map_err(|_| invalid("Text time overflows"))?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<i128>()
            .map_err(|_| invalid("Text time overflows"))?
    };
    let top = whole
        .checked_mul(base)
        .and_then(|v| v.checked_add(fraction))
        .ok_or_else(|| invalid("Text time overflows"))?;
    let (mut a, mut b) = (top, base);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    Ok(time(
        i64::try_from(top / a).map_err(|_| invalid("Text time overflows"))?,
        (base / a) as u32,
    ))
}

pub(crate) fn authored_time(
    value: &TextTimeInput,
    fps: &FrameRate,
    positive: bool,
) -> Result<Time, EditorError> {
    let result = match value {
        TextTimeInput::Exact { time } => time.clone(),
        TextTimeInput::DecimalSeconds { value } => {
            align_to_frame(&decimal(value)?, fps, FrameRounding::Nearest)?
        }
    };
    let rational = RationalTime::try_from(&result)?;
    if rational.ticks() < 0 || (positive && rational.ticks() == 0) {
        return Err(invalid(
            "Text time must be nonnegative and duration must be positive",
        ));
    }
    Ok(result)
}

fn locked_font<'a>(
    project: &'a EditorDocument,
    font_id: &str,
) -> Result<&'a FontResource, EditorError> {
    id(font_id)?;
    let font = project
        .fonts
        .iter()
        .find(|font| font.id == font_id)
        .ok_or_else(|| missing("Import the locked font before authoring text"))?;
    if !matches!(font.weight, 400 | 700) || font.face_index != 0 {
        return Err(invalid(
            "Text authoring requires a static 400 or 700 font face at index zero",
        ));
    }
    Ok(font)
}

fn values(
    project: &EditorDocument,
    sequence_id: &str,
    value: &TextValues,
) -> Result<TextInitial, EditorError> {
    if value.name.trim().is_empty()
        || value.name.len() > 4096
        || value.text.content.len() > 64 * 1024
    {
        return Err(invalid(
            "Text needs a 1..4096 byte name and at most 64 KiB of content",
        ));
    }
    let FontBinding::Locked { font_id } = &value.text.font else {
        return Err(invalid("Text authoring requires an imported locked font"));
    };
    locked_font(project, font_id)?;
    let fps = fps(project, sequence_id)?;
    Ok(TextInitial {
        name: value.name.clone(),
        text: value.text.clone(),
        timeline_range: TimeRange {
            start: authored_time(&value.start, fps, false)?,
            duration: authored_time(&value.duration, fps, true)?,
        },
    })
}

// Migrated creation defaults: Classic timeline/defaults.ts and creation.ts.
// Editor-core has no title constructor; this application constructor is the
// single owner consumed by all clients. Only the selected locked face replaces
// Classic's unresolved Arial default, with its actual weight/style together.
fn defaults(font: &FontResource, start: Time) -> TextInitial {
    let color = |white| Color {
        encoding: ColorEncoding::Srgb,
        red: white,
        green: white,
        blue: white,
        alpha: 1.0,
    };
    TextInitial {
        name: "Text".into(),
        timeline_range: TimeRange {
            start,
            duration: time(5, 1),
        },
        text: Text {
            content: "Default text".into(),
            font: FontBinding::Locked {
                font_id: font.id.clone(),
            },
            font_weight: if font.weight == 700 {
                FontWeight::Bold
            } else {
                FontWeight::Normal
            },
            font_style: font.style,
            decoration: TextDecoration::None,
            align: SceneTextAlign::Center,
            font_size: FontSize::ClassicHeight90 { value: 15.0 },
            color: color(1.0),
            letter_spacing_px: 0.0,
            line_height_multiplier: 1.2,
            background: TextBackground {
                enabled: false,
                color: color(0.0),
                corner_radius_percent: 0.0,
                padding_x_at_size15: 30.0,
                padding_y_at_size15: 42.0,
                offset_x_px: 0.0,
                offset_y_px: 0.0,
            },
        },
    }
}
fn new_element(id: String, initial: &TextInitial) -> TimelineElement {
    TimelineElement {
        id,
        name: initial.name.clone(),
        timeline_range: initial.timeline_range.clone(),
        enabled: true,
        source_span: ElementSourceSpan::ClassicTrimmed {
            trim_start: time(0, 1),
            trim_end: time(0, 1),
            source_duration: None,
        },
        content: ElementContent::Text {
            text: initial.text.clone(),
        },
        channels: vec![],
        visual: Some(VisualStyle {
            profile: VisualProfile::ClassicCanvasV31,
            transform: Transform {
                position_x_px: 0.0,
                position_y_px: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                rotation_degrees: 0.0,
            },
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            effects: vec![],
            masks: vec![],
        }),
    }
}
fn new_track(id: String) -> SceneTrack {
    SceneTrack {
        id,
        name: "Text".into(),
        kind: SceneTrackKind::Text,
        enabled: true,
        locked: false,
        muted: false,
        opacity: 1.0,
        gain: Gain::Decibels { value: 0.0 },
        elements: vec![],
        channels: vec![],
    }
}
fn text_track(track: &SceneTrack) -> Result<(), EditorError> {
    if track.locked {
        return Err(EditorError::new(
            "CONSTRAINT_VIOLATION",
            "Text track is locked",
        ));
    }
    if track.kind != SceneTrackKind::Text {
        return Err(invalid("Text must belong to an explicit text track"));
    }
    Ok(())
}

pub fn plan(
    store: &impl ProjectPublicResources,
    input: &TextPlanRequest,
) -> Result<TextPlanResult, EditorError> {
    crate::require_idempotency_key(&input.idempotency_key)?;
    id(&input.project_id)?;
    id(&input.sequence_id)?;
    if input.expected_revision.trim().is_empty() {
        return Err(invalid("Text planning requires a fixed revision"));
    }
    let mut transaction = ApplyProject {
        branch_name: None,
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: "Edit text".into(),
        operations: vec![],
    };
    store.authorize_project(SceneCommitIntent::PublicApply(&transaction))?;
    let source = store.head_scene_source(&input.project_id)?;
    if source.document.project_id() != input.project_id {
        return Err(EditorError::new("STORAGE_ERROR", "Text head scope differs"));
    }
    if source.revision != input.expected_revision {
        return Err(crate::revision_conflict(
            &input.expected_revision,
            &source.revision,
        ));
    }
    let StoredProjectDocument::Editable(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Text authoring requires a V4 head",
        ));
    };
    opencut_editor_core::scene::validate_project_scene(&project)?;
    let sequence_index = project
        .sequences
        .iter()
        .position(|s| s.id == input.sequence_id)
        .ok_or_else(|| missing("Text sequence does not exist"))?;
    let mut after = (*project).clone();
    let mut operations = vec![];
    let (initial, selection) = match &input.command {
        TextPlanCommand::Defaults { font_id, start } => {
            let initial = defaults(
                locked_font(&project, font_id)?,
                authored_time(start, fps(&project, &input.sequence_id)?, false)?,
            );
            (initial, None)
        }
        TextPlanCommand::Create {
            element_id,
            track,
            values: input_values,
        } => {
            id(element_id)?;
            let initial = values(&project, &input.sequence_id, input_values)?;
            let sequence = &mut after.sequences[sequence_index];
            let track_index = match track {
                TextTrackTarget::Existing { track_id } => {
                    id(track_id)?;
                    let index = sequence
                        .tracks
                        .iter()
                        .position(|t| t.id == *track_id)
                        .ok_or_else(|| missing("Text track does not belong to this sequence"))?;
                    text_track(&sequence.tracks[index])?;
                    index
                }
                TextTrackTarget::New { track_id } => {
                    id(track_id)?;
                    let track = new_track(track_id.clone());
                    sequence.tracks.push(track.clone());
                    operations.push(ProjectOperation::TrackPut {
                        sequence_id: input.sequence_id.clone(),
                        track,
                    });
                    sequence.tracks.len() - 1
                }
            };
            let element = new_element(element_id.clone(), &initial);
            // Append to the proposed aggregate first. Its shared validator catches
            // all global ID collisions, including IDs inside masks/channels, and
            // overlapping placements before any Put could replace an old entity.
            let track = &mut sequence.tracks[track_index];
            track.elements.push(element.clone());
            let selection = TimelineTarget {
                track_id: track.id.clone(),
                element_id: element.id.clone(),
            };
            operations.push(ProjectOperation::ElementPut {
                sequence_id: input.sequence_id.clone(),
                track_id: track.id.clone(),
                element: Box::new(element),
            });
            transaction.label = "Create text".into();
            (initial, Some(selection))
        }
        TextPlanCommand::Edit {
            target,
            values: input_values,
        } => {
            id(&target.track_id)?;
            id(&target.element_id)?;
            let mut initial = values(&project, &input.sequence_id, input_values)?;
            let track = after.sequences[sequence_index]
                .tracks
                .iter_mut()
                .find(|t| t.id == target.track_id)
                .ok_or_else(|| missing("Text track does not belong to this sequence"))?;
            text_track(track)?;
            let element = track
                .elements
                .iter_mut()
                .find(|e| e.id == target.element_id)
                .ok_or_else(|| missing("Text element does not belong to its explicit track"))?;
            if !matches!(element.content, ElementContent::Text { .. }) {
                return Err(invalid("Only free text elements can be edited here"));
            }
            let before = element.clone();
            element.name = initial.name.clone();
            element.content = ElementContent::Text {
                text: initial.text.clone(),
            };
            // Alternate rational representations of the same authored value
            // are not a title edit and must not manufacture a history entry.
            for (next, old) in [
                (
                    &mut initial.timeline_range.start,
                    &before.timeline_range.start,
                ),
                (
                    &mut initial.timeline_range.duration,
                    &before.timeline_range.duration,
                ),
            ] {
                if RationalTime::try_from(&*next)?
                    .cmp_exact(RationalTime::try_from(old)?)
                    .is_eq()
                {
                    *next = old.clone();
                }
            }
            element.timeline_range = initial.timeline_range.clone();
            if element != &before {
                operations.push(ProjectOperation::ElementPut {
                    sequence_id: input.sequence_id.clone(),
                    track_id: track.id.clone(),
                    element: Box::new(element.clone()),
                });
            }
            (initial, Some(target.clone()))
        }
    };
    opencut_editor_core::scene::validate_project_scene(&after)?;
    if !operations.is_empty() {
        let proposed = crate::scene::plan(
            &EditorSnapshot {
                project: (*project).clone(),
                revision: source.revision.clone(),
            },
            &ApplySceneProject {
                project_id: transaction.project_id.clone(),
                expected_revision: transaction.expected_revision.clone(),
                idempotency_key: transaction.idempotency_key.clone(),
                label: transaction.label.clone(),
                operations: operations.clone(),
            },
        )?;
        if proposed != after {
            return Err(EditorError::new(
                "INVALID_TEXT_PLAN",
                "Text operations differ from their validated proposal",
            ));
        }
        transaction.operations = operations
            .into_iter()
            .map(|operation| match operation {
                ProjectOperation::TrackPut { sequence_id, track } => {
                    ProjectEditOperation::TrackPut { sequence_id, track }
                }
                ProjectOperation::ElementPut {
                    sequence_id,
                    track_id,
                    element,
                } => ProjectEditOperation::ElementPut {
                    sequence_id,
                    track_id,
                    element,
                },
                _ => unreachable!("Text planning emits only explicit text track/element puts"),
            })
            .collect();
    }
    store.authorize_project(SceneCommitIntent::PublicApply(&transaction))?;
    let current = store.head_scene_source(&input.project_id)?;
    if current.document.project_id() != input.project_id {
        return Err(EditorError::new("STORAGE_ERROR", "Text head scope differs"));
    }
    if current.revision != input.expected_revision {
        return Err(crate::revision_conflict(
            &input.expected_revision,
            &current.revision,
        ));
    }
    Ok(TextPlanResult {
        revision: source.revision,
        initial: Some(initial),
        selection,
        transaction: (!transaction.operations.is_empty()).then_some(transaction),
    })
}
