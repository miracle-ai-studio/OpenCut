//! Fixed-head parametric graphics authoring. Planning is read-only; the returned public
//! transaction uses the existing atomic apply, idempotency and recovery path.
use crate::project_v4::{StoredProjectDocumentV4, V4Intent, public::ProjectV4PublicResources};
use crate::text_authoring_v1::authored_time;
use opencut_contracts::{
    EditorError, FrameRate, Time, TimeRange, graphic_edit_v1::*, project_v4::*, project_v4_ops::*,
    project_v4_public::*, timeline_v4::TimelineTargetV4,
};
use opencut_time::RationalTime;

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
        return Err(invalid("Graphic identity must contain 1..128 bytes"));
    }
    Ok(())
}
fn time(n: i64, base: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: base,
    }
}
fn fps<'a>(
    project: &'a ProjectDocumentV4,
    sequence_id: &str,
) -> Result<&'a FrameRate, EditorError> {
    match &project.settings {
        SettingsProfileV4::ClassicGlobal { settings } => Ok(&settings.fps),
        SettingsProfileV4::LegacyPerScene { formats } => formats
            .iter()
            .find(|format| format.sequence_id == sequence_id)
            .map(|format| &format.fps)
            .ok_or_else(|| invalid("Graphic scene format is missing")),
    }
}

fn values(
    project: &ProjectDocumentV4,
    sequence_id: &str,
    value: &GraphicValuesV1,
) -> Result<GraphicInitialV1, EditorError> {
    if value.name.trim().is_empty() || value.name.len() > 4096 {
        return Err(invalid("Graphic needs a 1..4096 byte name"));
    }
    parametric(&value.graphic)?;
    let fps = fps(project, sequence_id)?;
    Ok(GraphicInitialV1 {
        name: value.name.clone(),
        graphic: value.graphic.clone(),
        transform: value.transform.clone(),
        opacity: value.opacity,
        timeline_range: TimeRange {
            start: authored_time(&value.start, fps, false)?,
            duration: authored_time(&value.duration, fps, true)?,
        },
    })
}

fn parametric(graphic: &GraphicV4) -> Result<(), EditorError> {
    if matches!(
        graphic.geometry,
        GraphicGeometryV4::LegacyRectangle { .. } | GraphicGeometryV4::LegacyEllipse { .. }
    ) {
        return Err(invalid(
            "Legacy graphics are not editable by the parametric graphic authoring use case",
        ));
    }
    Ok(())
}
// Classic graphics definitions and DEFAULT_GRAPHIC_SOURCE_SIZE/creation defaults.
fn defaults(kind: GraphicKindV1, start: Time) -> GraphicInitialV1 {
    let color = |white| ColorV4 {
        encoding: ColorEncodingV4::Srgb,
        red: white,
        green: white,
        blue: white,
        alpha: 1.0,
    };
    let (name, geometry) = match kind {
        GraphicKindV1::Rectangle => (
            "Rectangle",
            GraphicGeometryV4::Rectangle {
                corner_radius_percent: 0.0,
            },
        ),
        GraphicKindV1::Ellipse => ("Ellipse", GraphicGeometryV4::Ellipse),
        GraphicKindV1::Polygon => (
            "Polygon",
            GraphicGeometryV4::Polygon {
                sides: 5.0,
                corner_radius_percent: 0.0,
            },
        ),
        GraphicKindV1::Star => (
            "Star",
            GraphicGeometryV4::Star {
                points: 5.0,
                depth_percent: 45.0,
            },
        ),
    };
    GraphicInitialV1 {
        name: name.into(),
        graphic: GraphicV4 {
            geometry,
            source_size: CanvasSizeV4 {
                width: 512,
                height: 512,
            },
            fill: color(1.0),
            stroke: StrokeV4 {
                color: color(0.0),
                width_px: 0.0,
                align: StrokeAlignV4::Center,
            },
        },
        transform: TransformV4 {
            position_x_px: 0.0,
            position_y_px: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
        },
        opacity: 1.0,
        timeline_range: TimeRange {
            start,
            duration: time(5, 1),
        },
    }
}
fn new_element(id: String, initial: &GraphicInitialV1) -> TimelineElementV4 {
    TimelineElementV4 {
        id,
        name: initial.name.clone(),
        timeline_range: initial.timeline_range.clone(),
        enabled: true,
        source_span: ElementSourceSpanV4::ClassicTrimmed {
            trim_start: time(0, 1),
            trim_end: time(0, 1),
            source_duration: None,
        },
        content: ElementContentV4::Graphic {
            graphic: initial.graphic.clone(),
        },
        channels: vec![],
        visual: Some(VisualStyleV4 {
            profile: VisualProfileV4::ClassicCanvasV31,
            transform: initial.transform.clone(),
            opacity: initial.opacity,
            blend_mode: BlendModeV4::Normal,
            effects: vec![],
            masks: vec![],
        }),
    }
}
fn new_track(id: String) -> TrackV4 {
    TrackV4 {
        id,
        name: "Graphic".into(),
        kind: TrackKindV4::Graphic,
        enabled: true,
        locked: false,
        muted: false,
        opacity: 1.0,
        gain: GainV4::Decibels { value: 0.0 },
        elements: vec![],
        channels: vec![],
    }
}
fn graphic_track(track: &TrackV4) -> Result<(), EditorError> {
    if track.locked {
        return Err(EditorError::new(
            "CONSTRAINT_VIOLATION",
            "Graphic track is locked",
        ));
    }
    if track.kind != TrackKindV4::Graphic {
        return Err(invalid("Graphic must belong to an explicit graphic track"));
    }
    Ok(())
}

pub fn plan(
    store: &impl ProjectV4PublicResources,
    input: &GraphicPlanRequestV1,
) -> Result<GraphicPlanResultV1, EditorError> {
    crate::require_idempotency_key(&input.idempotency_key)?;
    id(&input.project_id)?;
    id(&input.sequence_id)?;
    if input.expected_revision.trim().is_empty() {
        return Err(invalid("Graphic planning requires a fixed revision"));
    }
    let mut transaction = ApplyPublicV4Project {
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: input.idempotency_key.clone(),
        label: "Edit graphic".into(),
        operations: vec![],
    };
    store.authorize_v4(V4Intent::PublicApply(&transaction))?;
    let source = store.head_v4_source(&input.project_id)?;
    if source.document.project_id() != input.project_id {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Graphic head scope differs",
        ));
    }
    if source.revision != input.expected_revision {
        return Err(crate::revision_conflict(
            &input.expected_revision,
            &source.revision,
        ));
    }
    let StoredProjectDocumentV4::V4(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Graphic authoring requires a V4 head",
        ));
    };
    opencut_editor_core::project_v4::validate_project_v4(&project)?;
    let sequence_index = project
        .sequences
        .iter()
        .position(|s| s.id == input.sequence_id)
        .ok_or_else(|| missing("Graphic sequence does not exist"))?;
    let mut after = (*project).clone();
    let mut operations = vec![];
    let (initial, selection) = match &input.command {
        GraphicPlanCommandV1::Defaults { geometry, start } => {
            let initial = defaults(
                *geometry,
                authored_time(start, fps(&project, &input.sequence_id)?, false)?,
            );
            (initial, None)
        }
        GraphicPlanCommandV1::Create {
            element_id,
            track,
            values: input_values,
        } => {
            id(element_id)?;
            let initial = values(&project, &input.sequence_id, input_values)?;
            let sequence = &mut after.sequences[sequence_index];
            let track_index = match track {
                GraphicTrackTargetV1::Existing { track_id } => {
                    id(track_id)?;
                    let index = sequence
                        .tracks
                        .iter()
                        .position(|t| t.id == *track_id)
                        .ok_or_else(|| missing("Graphic track does not belong to this sequence"))?;
                    graphic_track(&sequence.tracks[index])?;
                    index
                }
                GraphicTrackTargetV1::New { track_id } => {
                    id(track_id)?;
                    let track = new_track(track_id.clone());
                    sequence.tracks.push(track.clone());
                    operations.push(ProjectOperationV4::TrackPut {
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
            let selection = TimelineTargetV4 {
                track_id: track.id.clone(),
                element_id: element.id.clone(),
            };
            operations.push(ProjectOperationV4::ElementPut {
                sequence_id: input.sequence_id.clone(),
                track_id: track.id.clone(),
                element: Box::new(element),
            });
            transaction.label = "Create graphic".into();
            (initial, Some(selection))
        }
        GraphicPlanCommandV1::Edit {
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
                .ok_or_else(|| missing("Graphic track does not belong to this sequence"))?;
            graphic_track(track)?;
            let element = track
                .elements
                .iter_mut()
                .find(|e| e.id == target.element_id)
                .ok_or_else(|| missing("Graphic element does not belong to its explicit track"))?;
            if !matches!(element.content, ElementContentV4::Graphic { .. }) {
                return Err(invalid("Only graphic elements can be edited here"));
            }
            let ElementContentV4::Graphic { graphic } = &element.content else {
                unreachable!()
            };
            parametric(graphic)?;
            let before = element.clone();
            element.name = initial.name.clone();
            element.content = ElementContentV4::Graphic {
                graphic: initial.graphic.clone(),
            };
            let visual = element
                .visual
                .as_mut()
                .ok_or_else(|| invalid("Graphic visual style is missing"))?;
            visual.transform = initial.transform.clone();
            visual.opacity = initial.opacity;
            // Alternate rational representations of the same authored value
            // are not a graphic edit and must not manufacture a history entry.
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
                operations.push(ProjectOperationV4::ElementPut {
                    sequence_id: input.sequence_id.clone(),
                    track_id: track.id.clone(),
                    element: Box::new(element.clone()),
                });
            }
            (initial, Some(target.clone()))
        }
    };
    opencut_editor_core::project_v4::validate_project_v4(&after)?;
    if !operations.is_empty() {
        let proposed = crate::project_v4::plan(
            &ProjectSnapshotV4 {
                project: (*project).clone(),
                revision: source.revision.clone(),
            },
            &ApplyV4Project {
                project_id: transaction.project_id.clone(),
                expected_revision: transaction.expected_revision.clone(),
                idempotency_key: transaction.idempotency_key.clone(),
                label: transaction.label.clone(),
                operations: operations.clone(),
            },
        )?;
        if proposed != after {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Graphic operations differ from their validated proposal",
            ));
        }
        transaction.operations = operations
            .into_iter()
            .map(|operation| match operation {
                ProjectOperationV4::TrackPut { sequence_id, track } => {
                    PublicProjectOperationV4::TrackPut { sequence_id, track }
                }
                ProjectOperationV4::ElementPut {
                    sequence_id,
                    track_id,
                    element,
                } => PublicProjectOperationV4::ElementPut {
                    sequence_id,
                    track_id,
                    element,
                },
                _ => {
                    unreachable!("Graphic planning emits only explicit graphic track/element puts")
                }
            })
            .collect();
    }
    store.authorize_v4(V4Intent::PublicApply(&transaction))?;
    let current = store.head_v4_source(&input.project_id)?;
    if current.document.project_id() != input.project_id {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Graphic head scope differs",
        ));
    }
    if current.revision != input.expected_revision {
        return Err(crate::revision_conflict(
            &input.expected_revision,
            &current.revision,
        ));
    }
    Ok(GraphicPlanResultV1 {
        schema_version: 1,
        revision: source.revision,
        initial: Some(initial),
        selection,
        transaction: (!transaction.operations.is_empty()).then_some(transaction),
    })
}
