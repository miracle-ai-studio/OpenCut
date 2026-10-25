//! Recipes reuse immutable evidence, source clocks, complete timeline plans and
//! public proposal validation. No intermediate edit, request or model is created.
use crate::{
    queries::JobRead,
    scene::{self, SceneCommitIntent, public::ProjectPublicResources},
};
use opencut_api_contracts::analysis_evidence::{AnalysisEvidenceInput, AnalysisEvidenceItem};
use opencut_contracts::{
    EditorError, Time, TimeRange, agent_edit::*, agent_recipe::*,
    project_inspection::InspectProject, project_public::*, scene::*, timeline::*,
};
use opencut_time::{RationalTime as R, range_end, validate_range};
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn extent(range: &TimeRange) -> Result<(R, R), EditorError> {
    validate_range(range, true)?;
    let start = R::try_from(&range.start)?;
    if start.ticks() < 0 {
        return Err(invalid("Recipe ranges must be nonnegative"));
    }
    Ok((start, R::try_from(&range_end(range)?)?))
}
fn merge(ranges: &[TimeRange]) -> Result<Vec<(R, R)>, EditorError> {
    let mut values = ranges.iter().map(extent).collect::<Result<Vec<_>, _>>()?;
    values.sort_by(|a, b| a.0.cmp_exact(b.0));
    let mut result: Vec<(R, R)> = vec![];
    for (start, end) in values {
        if let Some(last) = result.last_mut() {
            if !start.cmp_exact(last.1).is_gt() {
                if end.cmp_exact(last.1).is_gt() {
                    last.1 = end;
                }
                continue;
            }
        }
        result.push((start, end));
    }
    Ok(result)
}
fn element<'a>(
    snapshot: &'a EditorSnapshot,
    sequence: &str,
    target: &TimelineTarget,
) -> Result<&'a TimelineElement, EditorError> {
    snapshot
        .project
        .sequences
        .iter()
        .find(|s| s.id == sequence)
        .and_then(|s| s.tracks.iter().find(|t| t.id == target.track_id))
        .and_then(|t| t.elements.iter().find(|e| e.id == target.element_id))
        .ok_or_else(|| invalid("Recipe placement does not exist"))
}
fn id(request: &RecipeRequest, label: &str) -> String {
    crate::content_id(
        "recipe",
        format!("{}:{}:{label}", request.project_id, request.idempotency_key).as_bytes(),
    )
}
fn step(request: &RecipeRequest, command: TimelineCommand) -> AgentEditStep {
    AgentEditStep::Timeline {
        sequence_id: request.sequence_id.clone(),
        command,
    }
}
fn cuts(
    snapshot: &EditorSnapshot,
    request: &RecipeRequest,
    targets: &[TimelineTarget],
    ranges: &[TimeRange],
    ripple: bool,
) -> Result<Vec<AgentEditStep>, EditorError> {
    if targets.is_empty() || targets.len() > 16 || ranges.len() > 16 {
        return Err(invalid(
            "Cut recipe accepts 1..16 placements and at most16 ranges",
        ));
    }
    let intervals = merge(ranges)?;
    let mut steps = vec![];
    let mut seen = std::collections::BTreeSet::new();
    let mut targets = targets.to_vec();
    targets.sort_by(|a, b| {
        a.track_id.cmp(&b.track_id).then_with(|| {
            let a = element(snapshot, &request.sequence_id, a)
                .and_then(|e| R::try_from(&e.timeline_range.start));
            let b = element(snapshot, &request.sequence_id, b)
                .and_then(|e| R::try_from(&e.timeline_range.start));
            match (a, b) {
                (Ok(a), Ok(b)) => b.cmp_exact(a),
                _ => std::cmp::Ordering::Equal,
            }
        })
    });
    if ripple {
        for pair in targets.windows(2) {
            if pair[0].track_id == pair[1].track_id {
                let later = element(snapshot, &request.sequence_id, &pair[0])?;
                let earlier = element(snapshot, &request.sequence_id, &pair[1])?;
                if R::try_from(&opencut_time::range_end(&earlier.timeline_range)?)?
                    .cmp_exact(R::try_from(&later.timeline_range.start)?)
                    .is_gt()
                {
                    return Err(invalid(
                        "Ripple cut targets on the same track must not overlap",
                    ));
                }
            }
        }
    }
    for target in &targets {
        if !seen.insert(&target.element_id) {
            return Err(invalid("Recipe placements must be unique"));
        }
        let original = element(snapshot, &request.sequence_id, target)?;
        let (first, mut last) = extent(&original.timeline_range)?;
        for (index, (start, end)) in intervals.iter().enumerate().rev() {
            let start = if start.cmp_exact(first).is_lt() {
                first
            } else {
                *start
            };
            let end = if end.cmp_exact(last).is_gt() {
                last
            } else {
                *end
            };
            if !start.cmp_exact(end).is_lt() {
                continue;
            }
            if end.cmp_exact(last).is_lt() {
                steps.push(step(
                    request,
                    TimelineCommand::Split {
                        targets: vec![TimelineSplit {
                            target: target.clone(),
                            right_element_id: id(
                                request,
                                &format!("{}:{index}:tail", target.element_id),
                            ),
                        }],
                        at: end.into(),
                        retain: TimelineSplitRetain::Both,
                        ripple: false,
                    },
                ));
            }
            if start.cmp_exact(first).is_eq() {
                steps.push(step(
                    request,
                    TimelineCommand::Delete {
                        targets: vec![target.clone()],
                        ripple,
                    },
                ));
                break;
            }
            steps.push(step(
                request,
                TimelineCommand::Split {
                    targets: vec![TimelineSplit {
                        target: target.clone(),
                        right_element_id: id(
                            request,
                            &format!("{}:{index}:cut", target.element_id),
                        ),
                    }],
                    at: start.into(),
                    retain: TimelineSplitRetain::Left,
                    ripple,
                },
            ));
            last = start;
        }
    }
    Ok(steps)
}
fn scalar(
    request: &RecipeRequest,
    owner: &str,
    parameter: ScalarParameter,
    values: Vec<(Time, f64)>,
) -> AnimationChannel {
    let label = format!("{owner}:{parameter:?}");
    AnimationChannel::Scalar {
        id: id(request, &label),
        target: ChannelTarget::Element {
            element_id: owner.into(),
        },
        binding: ScalarBinding::Property { parameter },
        keys: values
            .into_iter()
            .enumerate()
            .map(|(i, (time, value))| ScalarKey {
                id: id(request, &format!("{label}:{i}")),
                segment_id: None,
                time,
                value,
                left_handle: None,
                right_handle: None,
                segment_to_next: ScalarSegment::Linear,
                tangent_mode: TangentMode::Auto,
            })
            .collect(),
        extrapolation: Extrapolation {
            before: ExtrapolationMode::Hold,
            after: ExtrapolationMode::Hold,
        },
    }
}
fn seconds(t: R) -> f64 {
    t.ticks() as f64 / f64::from(t.timebase())
}
pub fn plan(
    store: &(impl ProjectPublicResources + JobRead),
    request: &RecipeRequest,
) -> Result<RecipePlan, EditorError> {
    crate::require_idempotency_key(&request.idempotency_key)?;
    let header = ApplyProject {
        branch_name: None,
        project_id: request.project_id.clone(),
        expected_revision: request.expected_revision.clone(),
        idempotency_key: request.idempotency_key.clone(),
        label: request.label.clone(),
        operations: vec![],
    };
    store.authorize_project(SceneCommitIntent::PublicApply(&header))?;
    let snapshot = scene::inspect(
        store,
        &InspectProject {
            project_id: request.project_id.clone(),
            revision: None,
        },
    )?;
    if snapshot.revision != request.expected_revision {
        return Err(crate::revision_conflict(
            &request.expected_revision,
            &snapshot.revision,
        ));
    }
    if !snapshot
        .project
        .sequences
        .iter()
        .any(|s| s.id == request.sequence_id)
    {
        return Err(invalid("Recipe scene does not exist"));
    }
    let mut operations = vec![];
    let mut affected = vec![];
    let mut evidence = vec![];
    let mut warnings = vec![];
    let mut steps = vec![];
    match &request.recipe {
        Recipe::JumpCut {
            targets,
            ranges,
            ripple,
        } => {
            steps = cuts(&snapshot, request, targets, ranges, *ripple)?;
            affected.extend(targets.iter().map(|t| t.element_id.clone()));
            warnings.push("Cuts use the existing frame alignment and per-track ripple semantics; include every synchronized placement.".into());
        }
        Recipe::RemoveEvidence {
            target,
            analysis_job_id,
            evidence_ids,
            ripple,
        } => {
            if evidence_ids.is_empty()
                || evidence_ids.len() > 16
                || evidence_ids
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != evidence_ids.len()
            {
                return Err(invalid("Select 1..16 silence or transcript evidence IDs"));
            }
            let placement = element(&snapshot, &request.sequence_id, target)?;
            let (asset_id, retime) = match &placement.content {
                ElementContent::Video {
                    asset_id, retime, ..
                }
                | ElementContent::Audio {
                    asset_id, retime, ..
                } => (asset_id, retime),
                _ => return Err(invalid("Evidence cuts require timed media")),
            };
            let asset = snapshot
                .project
                .assets
                .iter()
                .find(|a| &a.id == asset_id)
                .ok_or_else(|| invalid("Recipe asset missing"))?;
            let source = opencut_editor_core::scene::resolve_element_source_request_scene(
                placement,
                Some(asset),
            )?;
            let (source_start, source_end) = extent(&source)?;
            let (sequence_start, _) = extent(&placement.timeline_range)?;
            let mut cursor = None;
            let mut found = std::collections::BTreeMap::new();
            loop {
                let page = crate::analysis_evidence::query(
                    store,
                    AnalysisEvidenceInput {
                        job_id: analysis_job_id.clone(),
                        text: None,
                        source_range: Some(source.clone()),
                        cursor,
                        limit: 100,
                    },
                )?;
                if page.project_id != request.project_id
                    || page.asset_id != *asset_id
                    || page.asset_version.content_hash != asset.version.content_hash
                    || page.asset_version.size != asset.version.size
                {
                    return Err(EditorError::new(
                        "EVIDENCE_VERSION_MISMATCH",
                        "Recipe evidence asset differs",
                    ));
                }
                for item in page.evidence {
                    match item {
                        AnalysisEvidenceItem::TranscriptSegment {
                            evidence_id,
                            source_range,
                            ..
                        }
                        | AnalysisEvidenceItem::SilenceInterval {
                            evidence_id,
                            source_range,
                            ..
                        } => {
                            if evidence_ids.contains(&evidence_id) {
                                found.insert(evidence_id, source_range);
                            }
                        }
                        _ => {}
                    }
                }
                if found.len() == evidence_ids.len() || page.next_cursor.is_none() {
                    break;
                }
                cursor = page.next_cursor;
            }
            let mut ranges = vec![];
            for selected in evidence_ids {
                let range=found.get(selected).ok_or_else(||invalid("Selected evidence is absent, duplicated, or is not a transcript/silence interval"))?;
                let (mut start, mut end) = extent(range)?;
                if start.cmp_exact(source_start).is_lt() {
                    start = source_start;
                }
                if end.cmp_exact(source_end).is_gt() {
                    end = source_end;
                }
                let map = |at: R| -> Result<R, EditorError> {
                    sequence_start.checked_add(
                        opencut_editor_core::scene::map_retime_duration_scene(
                            at.checked_sub(source_start)?,
                            retime,
                            true,
                        )?,
                    )
                };
                let start = map(start)?;
                let end = map(end)?;
                ranges.push(TimeRange {
                    start: start.into(),
                    duration: end.checked_sub(start)?.into(),
                });
            }
            steps = cuts(
                &snapshot,
                request,
                std::slice::from_ref(target),
                &ranges,
                *ripple,
            )?;
            affected.push(target.element_id.clone());
            evidence = evidence_ids.clone();
            warnings.push("Transcript segments are segment-level editorial evidence; word accuracy and speaker identity are not inferred.".into());
        }
        Recipe::PlaceMedia {
            bindings,
            placements,
            new_tracks,
        } => {
            let result = crate::media_import::plan(
                store,
                &opencut_contracts::media_import::MediaImportRequest {
                    project_id: request.project_id.clone(),
                    expected_revision: request.expected_revision.clone(),
                    sequence_id: request.sequence_id.clone(),
                    idempotency_key: request.idempotency_key.clone(),
                    label: request.label.clone(),
                    bindings: bindings.clone(),
                    placements: placements.clone(),
                    ripple: false,
                    new_tracks: new_tracks.clone(),
                },
            )?;
            operations = result.transaction.operations;
            affected.extend(placements.iter().map(|p| p.element_id.clone()));
        }
        Recipe::DuckMusic {
            targets,
            speech_ranges,
            duck_decibels,
            ramp,
            replace_gain_automation,
        } => {
            let ramp = R::try_from(ramp)?;
            if ramp.ticks() <= 0
                || !duck_decibels.is_finite()
                || !(-60.0..=20.0).contains(duck_decibels)
                || targets.is_empty()
                || targets.len() > 16
                || speech_ranges.len() > 64
            {
                return Err(invalid(
                    "Duck recipe needs 1..16 placements, <=64 intervals, a positive ramp and -60..20dB",
                ));
            }
            if targets
                .iter()
                .map(|t| &t.element_id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != targets.len()
            {
                return Err(invalid("Music placements must be unique"));
            }
            let ranges = merge(speech_ranges)?;
            for target in targets {
                let mut e = element(&snapshot, &request.sequence_id, target)?.clone();
                let (start, end) = extent(&e.timeline_range)?;
                let audio = match &e.content {
                    ElementContent::Audio { audio, .. }
                    | ElementContent::Video {
                        audio,
                        source_audio_enabled: true,
                        ..
                    } => audio,
                    _ => return Err(invalid("Ducking requires a placement with source audio")),
                };
                let baseline = match audio.gain {
                    Gain::Decibels { value } => value,
                    Gain::LinearLegacy { value } if value > 0.0 => 20.0 * value.log10(),
                    _ => return Err(invalid("Ducking requires nonzero finite baseline gain")),
                };
                if !baseline.is_finite() || !(-60.0..=20.0).contains(&baseline) {
                    return Err(invalid(
                        "Normalize a legacy gain outside -60..20dB explicitly before ducking; the recipe never clamps its baseline",
                    ));
                }
                if *duck_decibels > baseline {
                    return Err(invalid("Duck gain must not amplify the original baseline"));
                }
                let is_gain = |c: &AnimationChannel| {
                    matches!(
                        c,
                        AnimationChannel::Scalar {
                            binding: ScalarBinding::Property {
                                parameter: ScalarParameter::GainDecibels
                                    | ScalarParameter::GainLinearLegacy
                            },
                            ..
                        }
                    )
                };
                if e.channels.iter().any(is_gain) && !*replace_gain_automation {
                    return Err(invalid(
                        "Existing gain automation requires explicit replacement",
                    ));
                }
                let mut points = vec![start, end];
                for (a, b) in &ranges {
                    for time in [a.checked_sub(ramp)?, *a, *b, b.checked_add(ramp)?] {
                        if !time.cmp_exact(start).is_lt() && !time.cmp_exact(end).is_gt() {
                            points.push(time);
                        }
                    }
                }
                for pair in ranges.windows(2) {
                    let mid = pair[0]
                        .1
                        .checked_add(pair[1].0)?
                        .checked_scale(R::new(1, 1)?, R::new(2, 1)?)?;
                    if !mid.cmp_exact(start).is_lt() && !mid.cmp_exact(end).is_gt() {
                        points.push(mid);
                    }
                }
                points.sort_by(|a, b| a.cmp_exact(*b));
                points.dedup_by(|a, b| a.cmp_exact(*b).is_eq());
                let values = points
                    .into_iter()
                    .map(|at| {
                        let factor = ranges
                            .iter()
                            .map(|(a, b)| {
                                if !at.cmp_exact(*a).is_lt() && !at.cmp_exact(*b).is_gt() {
                                    1.0
                                } else {
                                    let distance = if at.cmp_exact(*a).is_lt() {
                                        seconds(a.checked_sub(at).expect("ordered"))
                                    } else {
                                        seconds(at.checked_sub(*b).expect("ordered"))
                                    };
                                    (1.0 - distance / seconds(ramp)).clamp(0.0, 1.0)
                                }
                            })
                            .fold(0.0_f64, f64::max);
                        Ok((
                            at.checked_sub(start)?.into(),
                            baseline + (*duck_decibels - baseline) * factor,
                        ))
                    })
                    .collect::<Result<Vec<_>, EditorError>>()?;
                match &mut e.content {
                    ElementContent::Audio { audio, .. } | ElementContent::Video { audio, .. } => {
                        audio.gain = Gain::Decibels { value: baseline }
                    }
                    _ => {}
                }
                e.channels.retain(|c| !is_gain(c));
                e.channels.push(scalar(
                    request,
                    &e.id,
                    ScalarParameter::GainDecibels,
                    values,
                ));
                affected.push(e.id.clone());
                operations.push(ProjectEditOperation::ElementPut {
                    sequence_id: request.sequence_id.clone(),
                    track_id: target.track_id.clone(),
                    element: Box::new(e),
                });
            }
        }
        Recipe::Reframe { canvas, placements } => {
            if placements.is_empty() || placements.len() > 16 {
                return Err(invalid("Framing requires 1..16 placements"));
            }
            if placements
                .iter()
                .map(|p| &p.target.element_id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != placements.len()
            {
                return Err(invalid("Framing placements must be unique"));
            }
            let mut settings = snapshot.project.settings.clone();
            let SettingsProfile::ClassicGlobal { settings: s } = &mut settings else {
                return Err(invalid("Recipe requires Classic global canvas settings"));
            };
            s.canvas_size = canvas.clone();
            s.canvas_size_mode = CanvasSizeMode::Custom;
            s.last_custom_canvas_size = Some(canvas.clone());
            operations.push(ProjectEditOperation::SettingsSet { settings });
            for placement in placements {
                let mut e = element(&snapshot, &request.sequence_id, &placement.target)?.clone();
                if e.visual.is_none()
                    || placement.samples.is_empty()
                    || placement.samples.len() > 64
                {
                    return Err(invalid(
                        "Framing needs visual content and 1..64 explicit local samples",
                    ));
                }
                let duration = R::try_from(&e.timeline_range.duration)?;
                let mut previous = None;
                for sample in &placement.samples {
                    let at = R::try_from(&sample.local_time)?;
                    if at.ticks() < 0
                        || at.cmp_exact(duration).is_gt()
                        || previous.is_some_and(|p: R| !at.cmp_exact(p).is_gt())
                    {
                        return Err(invalid(
                            "Framing samples must be strictly ordered within the placement",
                        ));
                    }
                    previous = Some(at);
                }
                let parameters = [
                    ScalarParameter::PositionXPx,
                    ScalarParameter::PositionYPx,
                    ScalarParameter::ScaleX,
                    ScalarParameter::ScaleY,
                    ScalarParameter::RotationDegrees,
                ];
                if e.channels.iter().any(|c|matches!(c,AnimationChannel::Scalar{binding:ScalarBinding::Property{parameter},..} if parameters.contains(parameter))){return Err(invalid("Existing transform automation must be explicitly edited before a framing recipe"));}
                for (index, parameter) in parameters.into_iter().enumerate() {
                    let values = placement
                        .samples
                        .iter()
                        .map(|sample| {
                            let t = &sample.transform;
                            (
                                sample.local_time.clone(),
                                [
                                    t.position_x_px,
                                    t.position_y_px,
                                    t.scale_x,
                                    t.scale_y,
                                    t.rotation_degrees,
                                ][index],
                            )
                        })
                        .collect();
                    e.channels.push(scalar(request, &e.id, parameter, values));
                }
                affected.push(e.id.clone());
                operations.push(ProjectEditOperation::ElementPut {
                    sequence_id: request.sequence_id.clone(),
                    track_id: placement.target.track_id.clone(),
                    element: Box::new(e),
                });
            }
            warnings.push("Framing samples are explicit editorial choices, not automatic face or subject tracking.".into());
        }
        Recipe::Captions {
            analysis_job_id,
            track_id,
            words_per_caption,
            style,
        } => {
            // Internal recipes read the authorized original task once. The
            // native stored decoder proves the captured mix before captioning;
            // no public-view round-trip or reconstructed receipt is involved.
            let record = store.read_job(analysis_job_id)?;
            if record.state != crate::JobState::Succeeded
                || record.kind
                    != opencut_job_contracts::scene_transcription::SCENE_TRANSCRIPTION_KIND
            {
                return Err(invalid(
                    "Captions require a successful fixed-scene transcription",
                ));
            }
            let prepared: crate::scene_transcription::PreparedSceneTranscription =
                serde_json::from_value(record.input)
                    .map_err(|_| invalid("Transcription input differs"))?;
            let result = crate::scene_transcription::validate_stored_result(
                &prepared,
                record
                    .result
                    .ok_or_else(|| invalid("Transcription result missing"))?,
            )?;
            let input = opencut_contracts::subtitles::GenerateCaptions {
                project_id: request.project_id.clone(),
                expected_revision: request.expected_revision.clone(),
                sequence_id: request.sequence_id.clone(),
                track_id: track_id.clone(),
                analysis_job_id: analysis_job_id.clone(),
                idempotency_key: request.idempotency_key.clone(),
                words_per_caption: *words_per_caption,
                style: style.clone(),
            };
            let tx = crate::captions::generate(&snapshot, &input, &result)?;
            for op in tx.operations {
                if let opencut_contracts::scene_ops::ProjectOperation::SequencePut { sequence } = op
                {
                    let original = snapshot
                        .project
                        .sequences
                        .iter()
                        .find(|s| s.id == request.sequence_id)
                        .expect("checked scene");
                    for caption in sequence.captions {
                        if !original.captions.iter().any(|c| c.id == caption.id) {
                            operations.push(ProjectEditOperation::CaptionPut {
                                sequence_id: request.sequence_id.clone(),
                                caption: Box::new(caption),
                            });
                        }
                    }
                }
            }
            evidence.push(analysis_job_id.clone());
            warnings.push("Caption timing preserves the transcription's declared segment/word timing accuracy; recognition is not human verification.".into());
        }
    }
    if !operations.is_empty() {
        steps.push(AgentEditStep::Operations { operations });
    }
    let plan = if steps.is_empty() {
        None
    } else {
        Some(crate::agent_edit::plan(
            store,
            &AgentEditRequest {
                project_id: request.project_id.clone(),
                expected_revision: request.expected_revision.clone(),
                idempotency_key: request.idempotency_key.clone(),
                label: request.label.clone(),
                steps,
            },
        )?)
    };
    Ok(RecipePlan {
        original_request_digest: crate::identity::request_identity(
            "agent_recipe",
            &request.project_id,
            request,
        )?
        .current_digest()
        .into(),
        affected_elements: affected,
        evidence_ids: evidence,
        warnings,
        plan,
    })
}
