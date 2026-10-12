pub mod collaboration;
pub mod dependencies;
pub mod project_v3;
pub mod project_v4;
pub mod scene_bookmarks;
use std::cmp::Ordering;

use opencut_contracts::*;
use opencut_time::{RationalTime, range_end, validate_range};

pub fn create_project(input: &CreateProject) -> Result<ProjectDocument, EditorError> {
    if input.id.trim().is_empty()
        || input.sequence_id.trim().is_empty()
        || input.name.trim().is_empty()
    {
        return Err(invalid(
            "project id, sequence id, and name must not be empty",
        ));
    }
    if input.width == 0
        || input.height == 0
        || input.fps.numerator == 0
        || input.fps.denominator == 0
    {
        return Err(invalid(
            "dimensions and frame rate components must be greater than zero",
        ));
    }
    let project = ProjectDocument {
        id: input.id.clone(),
        name: input.name.clone(),
        schema_version: 2,
        default_sequence_id: input.sequence_id.clone(),
        assets: Vec::new(),
        sequences: vec![Sequence {
            id: input.sequence_id.clone(),
            name: "Main".into(),
            width: input.width,
            height: input.height,
            fps: input.fps.clone(),
            tracks: Vec::new(),
            caption_cues: Vec::new(),
        }],
    };
    validate_project(&project)?;
    Ok(project)
}

pub fn validate_project(project: &ProjectDocument) -> Result<(), EditorError> {
    if project.id.trim().is_empty()
        || project.name.trim().is_empty()
        || !matches!(project.schema_version, 1 | 2)
    {
        return Err(invalid(
            "invalid project identity or unsupported schema version",
        ));
    }
    let mut ids = std::collections::HashSet::new();
    ids.insert(project.id.as_str());
    for asset in &project.assets {
        unique(&mut ids, &asset.id)?;
        if asset.name.trim().is_empty() || asset.uri.trim().is_empty() {
            return Err(invalid("asset name and uri must not be empty"));
        }
        if let Some(duration) = &asset.duration {
            validate_duration(duration)?;
        }
        if let Some(version) = &asset.version {
            if project.schema_version < 2 {
                return Err(invalid("versioned assets require project schema version 2"));
            }
            validate_asset_version(version)?;
        }
    }
    let mut has_default = false;
    for sequence in &project.sequences {
        unique(&mut ids, &sequence.id)?;
        has_default |= sequence.id == project.default_sequence_id;
        if sequence.width == 0
            || sequence.height == 0
            || sequence.fps.numerator == 0
            || sequence.fps.denominator == 0
        {
            return Err(invalid(
                "sequence dimensions and frame rate must be positive",
            ));
        }
        for track in &sequence.tracks {
            unique(&mut ids, &track.id)?;
            validate_params("track", &track.params)?;
            for clip in &track.clips {
                unique(&mut ids, &clip.id)?;
                validate_range(&clip.timeline_range, true)?;
                if let Some(source) = &clip.source_range {
                    validate_range(source, true)?;
                }
                if let Some(asset_id) = &clip.asset_id {
                    let asset = project
                        .assets
                        .iter()
                        .find(|asset| asset.id == *asset_id)
                        .ok_or_else(|| not_found("clip asset", asset_id))?;
                    if let (Some(source), Some(duration)) = (&clip.source_range, &asset.duration)
                        && cmp(&range_end(source)?, duration)? == Ordering::Greater
                    {
                        return Err(invalid("clip source range exceeds asset duration"));
                    }
                } else if clip.source_range.is_some() {
                    return Err(invalid(
                        "a clip without an asset cannot have a source range",
                    ));
                }
                validate_params("clip", &clip.params)?;
                for channel in &clip.channels {
                    validate_channel(channel)?;
                    validate_channel_bounds(channel, &clip.timeline_range.duration)?;
                }
                validate_track_asset_kind(project, track, clip)?;
            }
            validate_no_overlaps(&track.clips)?;
        }
        for cue in &sequence.caption_cues {
            unique(&mut ids, &cue.id)?;
            validate_caption(project, cue)?;
        }
    }
    if !has_default {
        return Err(not_found("default sequence", &project.default_sequence_id));
    }
    Ok(())
}

pub fn operation_descriptors() -> Vec<OperationDescriptor> {
    [
        ("sequence.update_settings", "updates_sequence_settings"),
        ("track.create", "creates_track"),
        ("track.delete", "deletes_empty_track"),
        ("asset.bind", "binds_asset"),
        ("asset.replace", "replaces_asset_binding"),
        ("asset.unbind", "unbinds_unreferenced_asset"),
        ("clip.insert", "creates_clip"),
        ("clip.split", "splits_constant_rate_clip"),
        ("clip.move", "moves_clip"),
        ("clip.trim", "trims_clip"),
        ("clip.delete", "deletes_clip"),
        ("parameter.set_clip", "updates_clip_parameters"),
        ("parameter.set_track", "updates_track_parameters"),
        ("track.update", "updates_track_state"),
        ("clip.retime", "retimes_clip"),
        ("keyframe.channel_set", "sets_keyframe_channel"),
        ("keyframe.channel_delete", "deletes_keyframe_channel"),
        ("caption.insert", "creates_caption"),
        ("caption.update", "updates_caption"),
        ("caption.delete", "deletes_caption"),
    ]
    .into_iter()
    .map(|(name, effect)| OperationDescriptor {
        name: name.into(),
        version: 1,
        kind: "command".into(),
        required_capabilities: Vec::new(),
        declared_effects: vec![effect.into()],
        deterministic: true,
        idempotency_policy: "required".into(),
        conflict_policy: "expected_revision".into(),
    })
    .collect()
}

pub fn parameter_descriptors() -> Vec<ParameterDescriptor> {
    [
        ("opacity", "clip", Some(0.0), Some(1.0)),
        ("volume", "clip", Some(0.0), Some(4.0)),
        ("position_x", "clip", None, None),
        ("position_y", "clip", None, None),
        ("rotation_degrees", "clip", None, None),
        ("scale_x", "clip", Some(0.0), None),
        ("scale_y", "clip", Some(0.0), None),
        ("opacity", "track", Some(0.0), Some(1.0)),
        ("volume", "track", Some(0.0), Some(4.0)),
    ]
    .into_iter()
    .map(|(name, target, minimum, maximum)| ParameterDescriptor {
        name: name.into(),
        version: 1,
        target: target.into(),
        value_type: "number".into(),
        unit: match name {
            "position_x" | "position_y" => "canvas_pixels_from_center",
            "rotation_degrees" => "clockwise_degrees",
            "volume" => "linear_gain",
            "scale_x" | "scale_y" => "contain_scale",
            _ => "ratio",
        }
        .into(),
        default_value: serde_json::json!(if matches!(
            name,
            "position_x" | "position_y" | "rotation_degrees"
        ) {
            0.0
        } else {
            1.0
        }),
        animatable: target == "clip",
        interpolation: if target == "clip" {
            vec!["hold".into(), "linear".into()]
        } else {
            Vec::new()
        },
        minimum,
        maximum,
    })
    .collect()
}

pub fn plan_transaction(
    snapshot: &ProjectSnapshot,
    request: &TransactionRequest,
) -> Result<EditPlan, EditorError> {
    if request.expected_revision != snapshot.revision {
        return Err(EditorError::new("REVISION_CONFLICT", "snapshot revision does not match expected revision")
            .with_details(serde_json::json!({"expected_revision": request.expected_revision, "actual_revision": snapshot.revision})));
    }
    if request.idempotency_key.trim().is_empty() {
        return Err(invalid("idempotency key must not be empty"));
    }
    if request.operations.is_empty() {
        return Err(invalid("transaction must contain at least one operation"));
    }
    if request.operations.len() > 500 {
        return Err(invalid("transaction exceeds the 500 operation limit"));
    }
    validate_project(&snapshot.project)?;
    let mut project = snapshot.project.clone();
    let mut changes = Changes::default();
    for operation in &request.operations {
        apply_operation(&mut project, operation, &mut changes)?;
    }
    validate_project(&project)?;
    let unresolved_asset_ids = project
        .assets
        .iter()
        .filter(|asset| asset.version.is_none())
        .map(|asset| asset.id.clone())
        .collect();
    Ok(EditPlan {
        base_revision: snapshot.revision.clone(),
        proposed_project: project,
        created_ids: changes.created,
        changed_ids: changes.changed,
        deleted_ids: changes.deleted,
        unresolved_asset_ids,
    })
}

#[derive(Default)]
struct Changes {
    created: Vec<String>,
    changed: Vec<String>,
    deleted: Vec<String>,
}
impl Changes {
    fn create(&mut self, id: &str) {
        push_unique(&mut self.created, id);
    }
    fn change(&mut self, id: &str) {
        if !self.created.iter().any(|x| x == id) {
            push_unique(&mut self.changed, id);
        }
    }
    fn delete(&mut self, id: &str) {
        self.created.retain(|x| x != id);
        self.changed.retain(|x| x != id);
        push_unique(&mut self.deleted, id);
    }
}

fn apply_operation(
    project: &mut ProjectDocument,
    op: &Operation,
    changes: &mut Changes,
) -> Result<(), EditorError> {
    match op {
        Operation::UpdateSequenceSettings {
            sequence_id,
            name,
            width,
            height,
            fps,
        } => {
            let sequence = sequence_mut(project, sequence_id)?;
            if let Some(name) = name {
                if name.trim().is_empty() {
                    return Err(invalid("sequence name must not be empty"));
                }
                sequence.name = name.clone();
            }
            if let Some(width) = width {
                if *width == 0 {
                    return Err(invalid("sequence width must be positive"));
                }
                sequence.width = *width;
            }
            if let Some(height) = height {
                if *height == 0 {
                    return Err(invalid("sequence height must be positive"));
                }
                sequence.height = *height;
            }
            if let Some(fps) = fps {
                if fps.numerator == 0 || fps.denominator == 0 {
                    return Err(invalid("frame rate must be positive"));
                }
                sequence.fps = fps.clone();
            }
            changes.change(sequence_id);
        }
        Operation::AddTrack { sequence_id, track } => {
            ensure_id_available(project, &track.id)?;
            if !track.clips.is_empty() {
                return Err(invalid("new tracks must be empty; insert clips explicitly"));
            }
            sequence_mut(project, sequence_id)?
                .tracks
                .push(track.clone());
            changes.create(&track.id);
            changes.change(sequence_id);
        }
        Operation::RemoveTrack {
            sequence_id,
            track_id,
        } => {
            let sequence = sequence_mut(project, sequence_id)?;
            let index = sequence
                .tracks
                .iter()
                .position(|x| x.id == *track_id)
                .ok_or_else(|| not_found("track", track_id))?;
            if sequence.tracks[index].locked {
                return Err(locked(track_id));
            }
            if !sequence.tracks[index].clips.is_empty() {
                return Err(constraint(
                    "a non-empty track cannot be deleted without an explicit cascade operation",
                ));
            }
            sequence.tracks.remove(index);
            changes.delete(track_id);
            changes.change(sequence_id);
        }
        Operation::BindAsset { asset } => {
            ensure_id_available(project, &asset.id)?;
            project.assets.push(asset.clone());
            if asset.version.is_some() {
                project.schema_version = 2;
            }
            changes.create(&asset.id);
        }
        Operation::ReplaceAsset { asset } => {
            if asset.id.trim().is_empty() {
                return Err(invalid("asset id must not be empty"));
            }
            let existing = project
                .assets
                .iter_mut()
                .find(|existing| existing.id == asset.id)
                .ok_or_else(|| not_found("asset", &asset.id))?;
            *existing = asset.clone();
            if asset.version.is_some() {
                project.schema_version = 2;
            }
            changes.change(&asset.id);
        }
        Operation::RemoveAsset { asset_id } => {
            if project
                .sequences
                .iter()
                .flat_map(|s| &s.tracks)
                .flat_map(|t| &t.clips)
                .any(|c| c.asset_id.as_deref() == Some(asset_id))
                || project
                    .sequences
                    .iter()
                    .flat_map(|sequence| &sequence.caption_cues)
                    .any(|cue| {
                        cue.source_anchor
                            .as_ref()
                            .is_some_and(|anchor| anchor.asset_id == *asset_id)
                    })
            {
                return Err(constraint("asset is still referenced by a clip"));
            }
            let index = project
                .assets
                .iter()
                .position(|x| x.id == *asset_id)
                .ok_or_else(|| not_found("asset", asset_id))?;
            project.assets.remove(index);
            changes.delete(asset_id);
        }
        Operation::InsertClip {
            sequence_id,
            track_id,
            clip,
        } => {
            ensure_id_available(project, &clip.id)?;
            validate_clip_asset(project, clip)?;
            let track = track_mut(project, sequence_id, track_id)?;
            ensure_unlocked(track)?;
            track.clips.push(clip.clone());
            sort_clips(&mut track.clips)?;
            changes.create(&clip.id);
            changes.change(track_id);
        }
        Operation::DeleteClip {
            sequence_id,
            track_id,
            clip_id,
        } => {
            let track = track_mut(project, sequence_id, track_id)?;
            ensure_unlocked(track)?;
            let index = clip_index(track, clip_id)?;
            track.clips.remove(index);
            changes.delete(clip_id);
            changes.change(track_id);
        }
        Operation::UpdateClipParams {
            sequence_id,
            track_id,
            clip_id,
            params,
        } => {
            validate_params("clip", params)?;
            let track = track_mut(project, sequence_id, track_id)?;
            ensure_unlocked(track)?;
            let index = clip_index(track, clip_id)?;
            let clip = &mut track.clips[index];
            for (key, value) in params {
                clip.params.insert(key.clone(), value.clone());
            }
            changes.change(clip_id);
        }
        Operation::UpdateTrackParams {
            sequence_id,
            track_id,
            params,
        } => {
            validate_params("track", params)?;
            let track = track_mut(project, sequence_id, track_id)?;
            ensure_unlocked(track)?;
            for (key, value) in params {
                track.params.insert(key.clone(), value.clone());
            }
            changes.change(track_id);
        }
        Operation::UpdateTrack {
            sequence_id,
            track_id,
            name,
            enabled,
            muted,
            locked,
        } => {
            let track = track_mut(project, sequence_id, track_id)?;
            if let Some(name) = name {
                if name.trim().is_empty() {
                    return Err(invalid("track name must not be empty"));
                }
                track.name = name.clone();
            }
            if let Some(value) = enabled {
                track.enabled = *value;
            }
            if let Some(value) = muted {
                track.muted = *value;
            }
            if let Some(value) = locked {
                track.locked = *value;
            }
            changes.change(track_id);
        }
        Operation::RetimeClip {
            sequence_id,
            track_id,
            clip_id,
            timeline_duration,
        } => {
            validate_duration(timeline_duration)?;
            let track = track_mut(project, sequence_id, track_id)?;
            ensure_unlocked(track)?;
            let index = clip_index(track, clip_id)?;
            track.clips[index].timeline_range.duration = timeline_duration.clone();
            sort_clips(&mut track.clips)?;
            changes.change(clip_id);
            changes.change(track_id);
        }
        Operation::SetKeyframeChannel {
            sequence_id,
            track_id,
            clip_id,
            channel,
        } => {
            validate_channel(channel)?;
            let track = track_mut(project, sequence_id, track_id)?;
            ensure_unlocked(track)?;
            let index = clip_index(track, clip_id)?;
            let clip = &mut track.clips[index];
            validate_channel_bounds(channel, &clip.timeline_range.duration)?;
            if let Some(existing) = clip
                .channels
                .iter_mut()
                .find(|item| item.parameter == channel.parameter)
            {
                *existing = channel.clone();
            } else {
                clip.channels.push(channel.clone());
            }
            changes.change(clip_id);
        }
        Operation::DeleteKeyframeChannel {
            sequence_id,
            track_id,
            clip_id,
            parameter,
        } => {
            let track = track_mut(project, sequence_id, track_id)?;
            ensure_unlocked(track)?;
            let index = clip_index(track, clip_id)?;
            let clip = &mut track.clips[index];
            let before = clip.channels.len();
            clip.channels.retain(|item| item.parameter != *parameter);
            if before == clip.channels.len() {
                return Err(not_found("keyframe channel", parameter));
            }
            changes.change(clip_id);
        }
        Operation::InsertCaption { sequence_id, cue } => {
            ensure_id_available(project, &cue.id)?;
            validate_caption(project, cue)?;
            sequence_mut(project, sequence_id)?
                .caption_cues
                .push(cue.clone());
            changes.create(&cue.id);
            changes.change(sequence_id);
        }
        Operation::UpdateCaption { sequence_id, cue } => {
            validate_caption(project, cue)?;
            let sequence = sequence_mut(project, sequence_id)?;
            let index = sequence
                .caption_cues
                .iter()
                .position(|item| item.id == cue.id)
                .ok_or_else(|| not_found("caption cue", &cue.id))?;
            sequence.caption_cues[index] = cue.clone();
            changes.change(&cue.id);
        }
        Operation::DeleteCaption {
            sequence_id,
            cue_id,
        } => {
            let sequence = sequence_mut(project, sequence_id)?;
            let index = sequence
                .caption_cues
                .iter()
                .position(|item| item.id == *cue_id)
                .ok_or_else(|| not_found("caption cue", cue_id))?;
            sequence.caption_cues.remove(index);
            changes.delete(cue_id);
            changes.change(sequence_id);
        }
        Operation::TrimClip {
            sequence_id,
            track_id,
            clip_id,
            timeline_range,
            source_range,
        } => {
            validate_range(timeline_range, true)?;
            if let Some(range) = source_range {
                validate_range(range, true)?;
            }
            let replacement = {
                let track = track_mut(project, sequence_id, track_id)?;
                ensure_unlocked(track)?;
                let idx = clip_index(track, clip_id)?;
                let mut clip = track.clips[idx].clone();
                clip.timeline_range = timeline_range.clone();
                clip.source_range = source_range.clone();
                clip
            };
            validate_clip_asset(project, &replacement)?;
            let track = track_mut(project, sequence_id, track_id)?;
            let index = clip_index(track, clip_id)?;
            track.clips[index] = replacement;
            sort_clips(&mut track.clips)?;
            changes.change(clip_id);
            changes.change(track_id);
        }
        Operation::MoveClip {
            sequence_id,
            track_id,
            clip_id,
            destination_track_id,
            start,
        } => {
            RationalTime::try_from(start)?;
            let clip = {
                let source = track_mut(project, sequence_id, track_id)?;
                ensure_unlocked(source)?;
                let idx = clip_index(source, clip_id)?;
                source.clips.remove(idx)
            };
            let mut moved = clip;
            moved.timeline_range.start = start.clone();
            let destination = track_mut(project, sequence_id, destination_track_id)?;
            ensure_unlocked(destination)?;
            destination.clips.push(moved);
            sort_clips(&mut destination.clips)?;
            changes.change(clip_id);
            changes.change(track_id);
            changes.change(destination_track_id);
        }
        Operation::SplitClip {
            sequence_id,
            track_id,
            clip_id,
            at,
            right_clip_id,
        } => {
            ensure_id_available(project, right_clip_id)?;
            let track = track_mut(project, sequence_id, track_id)?;
            ensure_unlocked(track)?;
            let idx = clip_index(track, clip_id)?;
            let original = track.clips[idx].clone();
            let start = RationalTime::try_from(&original.timeline_range.start)?;
            let end = RationalTime::try_from(&range_end(&original.timeline_range)?)?;
            let point = RationalTime::try_from(at)?;
            if point.cmp_exact(start) != Ordering::Greater || point.cmp_exact(end) != Ordering::Less
            {
                return Err(invalid("split point must be strictly inside the clip"));
            }
            let left_duration = point.checked_sub(start)?;
            let right_duration = end.checked_sub(point)?;
            track.clips[idx].timeline_range.duration = left_duration.into();
            let mut right = original.clone();
            right.id = right_clip_id.clone();
            right.timeline_range.start = at.clone();
            right.timeline_range.duration = right_duration.into();
            if let Some(source) = &original.source_range {
                let source_start = RationalTime::try_from(&source.start)?;
                let source_duration = RationalTime::try_from(&source.duration)?;
                let timeline_duration = RationalTime::try_from(&original.timeline_range.duration)?;
                let left_source =
                    source_duration.checked_scale(left_duration, timeline_duration)?;
                let right_source = source_duration.checked_sub(left_source)?;
                track.clips[idx].source_range.as_mut().unwrap().duration = left_source.into();
                right.source_range = Some(TimeRange {
                    start: source_start.checked_add(left_source)?.into(),
                    duration: right_source.into(),
                });
            }
            let mut left_channels = Vec::new();
            let mut right_channels = Vec::new();
            for channel in &original.channels {
                let (left, right_channel) = split_channel(channel, left_duration, right_clip_id)?;
                left_channels.push(left);
                right_channels.push(right_channel);
            }
            track.clips[idx].channels = left_channels;
            right.channels = right_channels;
            track.clips.insert(idx + 1, right);
            changes.change(clip_id);
            changes.create(right_clip_id);
            changes.change(track_id);
        }
    }
    Ok(())
}

fn validate_clip_asset(project: &ProjectDocument, clip: &Clip) -> Result<(), EditorError> {
    validate_range(&clip.timeline_range, true)?;
    match (&clip.asset_id, &clip.source_range) {
        (Some(id), source) => {
            let asset = project
                .assets
                .iter()
                .find(|a| a.id == *id)
                .ok_or_else(|| not_found("asset", id))?;
            if let Some(source) = source {
                validate_range(source, true)?;
                if let Some(duration) = &asset.duration
                    && cmp(&range_end(source)?, duration)? == Ordering::Greater
                {
                    return Err(invalid("clip source range exceeds asset duration"));
                }
            }
        }
        (None, Some(_)) => {
            return Err(invalid(
                "a clip without an asset cannot have a source range",
            ));
        }
        _ => {}
    }
    Ok(())
}

fn validate_caption(project: &ProjectDocument, cue: &CaptionCue) -> Result<(), EditorError> {
    if cue.text.trim().is_empty()
        || cue.style.font_family.trim().is_empty()
        || !cue.style.font_size.is_finite()
        || cue.style.font_size <= 0.0
        || cue.style.color.trim().is_empty()
    {
        return Err(invalid("caption text and valid style are required"));
    }
    if cue.style.max_width == Some(0)
        || cue
            .style
            .line_height
            .is_some_and(|height| !height.is_finite() || height <= 0.0)
    {
        return Err(invalid(
            "caption layout dimensions must be finite and positive",
        ));
    }
    if let FontResolutionPolicy::ExplicitFallback { families } = &cue.style.font_policy {
        let mut seen = std::collections::HashSet::new();
        seen.insert(cue.style.font_family.trim().to_lowercase());
        if families.is_empty()
            || families.iter().any(|family| {
                family.trim().is_empty() || !seen.insert(family.trim().to_lowercase())
            })
        {
            return Err(invalid(
                "caption fallback families must be nonempty, distinct and ordered",
            ));
        }
    }
    validate_range(&cue.timeline_range, true)?;
    if let Some(anchor) = &cue.source_anchor {
        validate_range(&anchor.source_range, true)?;
        let asset = project
            .assets
            .iter()
            .find(|item| item.id == anchor.asset_id)
            .ok_or_else(|| not_found("caption source asset", &anchor.asset_id))?;
        if let Some(duration) = &asset.duration
            && cmp(&range_end(&anchor.source_range)?, duration)? == Ordering::Greater
        {
            return Err(constraint("caption source anchor exceeds asset duration"));
        }
    }
    Ok(())
}

fn validate_channel(channel: &KeyframeChannel) -> Result<(), EditorError> {
    let params =
        std::collections::BTreeMap::from([(channel.parameter.clone(), serde_json::json!(0))]);
    validate_params("clip", &params)?;
    if channel.keyframes.is_empty() {
        return Err(invalid(
            "keyframe channel must contain at least one keyframe",
        ));
    }
    let mut previous: Option<RationalTime> = None;
    let mut ids = std::collections::HashSet::new();
    for keyframe in &channel.keyframes {
        if !ids.insert(&keyframe.id) {
            return Err(constraint("duplicate keyframe id"));
        }
        validate_params(
            "clip",
            &std::collections::BTreeMap::from([(
                channel.parameter.clone(),
                keyframe.value.clone(),
            )]),
        )?;
        let time = RationalTime::try_from(&keyframe.time)?;
        if time.ticks() < 0
            || previous.is_some_and(|value| time.cmp_exact(value) != Ordering::Greater)
        {
            return Err(constraint(
                "keyframes must be strictly ordered at non-negative clip-local times",
            ));
        }
        previous = Some(time);
    }
    Ok(())
}

/// Keyframe times are clip-local. Each key's interpolation describes the span
/// to the next key; outside the key range, the nearest endpoint is held.
pub fn sample_keyframe_channel(channel: &KeyframeChannel, time: &Time) -> Result<f64, EditorError> {
    validate_channel(channel)?;
    let point = RationalTime::try_from(time)?;
    let first = &channel.keyframes[0];
    if point.cmp_exact(RationalTime::try_from(&first.time)?) != Ordering::Greater {
        return Ok(first.value.as_f64().expect("validated number"));
    }
    for pair in channel.keyframes.windows(2) {
        let start = RationalTime::try_from(&pair[0].time)?;
        let end = RationalTime::try_from(&pair[1].time)?;
        let last = pair[1].value.as_f64().expect("validated number");
        if point.cmp_exact(end) == Ordering::Equal {
            return Ok(last);
        }
        if point.cmp_exact(end) == Ordering::Less {
            let first = pair[0].value.as_f64().expect("validated number");
            if pair[0].interpolation == KeyframeInterpolation::Hold {
                return Ok(first);
            }
            let offset = point.checked_sub(start)?;
            let duration = end.checked_sub(start)?;
            let fraction = (offset.ticks() as f64 * duration.timebase() as f64)
                / (offset.timebase() as f64 * duration.ticks() as f64);
            // A convex combination avoids overflowing last-first for opposite
            // large finite numeric parameter values.
            return Ok(first * (1.0 - fraction) + last * fraction);
        }
    }
    Ok(channel
        .keyframes
        .last()
        .expect("nonempty channel")
        .value
        .as_f64()
        .expect("validated number"))
}

fn split_channel(
    channel: &KeyframeChannel,
    point: RationalTime,
    right_clip_id: &str,
) -> Result<(KeyframeChannel, KeyframeChannel), EditorError> {
    let sampled = sample_keyframe_channel(channel, &point.into())?;
    let interpolation = channel
        .keyframes
        .iter()
        .rev()
        .find(|key| {
            RationalTime::try_from(&key.time).is_ok_and(|t| t.cmp_exact(point) != Ordering::Greater)
        })
        .map(|k| k.interpolation.clone())
        .unwrap_or(KeyframeInterpolation::Hold);
    let mut left = KeyframeChannel {
        parameter: channel.parameter.clone(),
        keyframes: Vec::new(),
    };
    let mut right = left.clone();
    for key in &channel.keyframes {
        let time = RationalTime::try_from(&key.time)?;
        if time.cmp_exact(point) != Ordering::Greater {
            left.keyframes.push(key.clone());
        }
        if time.cmp_exact(point) != Ordering::Less {
            let mut shifted = key.clone();
            shifted.time = time.checked_sub(point)?.into();
            right.keyframes.push(shifted);
        }
    }
    let make_key = |suffix: &str, time: Time, keys: &[Keyframe]| {
        let base = format!("{right_clip_id}:{}:{suffix}", channel.parameter);
        let mut id = base.clone();
        let mut attempt = 1;
        while keys.iter().any(|key| key.id == id) {
            id = format!("{base}:{attempt}");
            attempt += 1;
        }
        Keyframe {
            id,
            time,
            value: serde_json::json!(sampled),
            interpolation: interpolation.clone(),
        }
    };
    if left.keyframes.last().is_none_or(|key| {
        RationalTime::try_from(&key.time).is_ok_and(|time| time.cmp_exact(point) != Ordering::Equal)
    }) {
        left.keyframes
            .push(make_key("left-boundary", point.into(), &left.keyframes));
    }
    if right
        .keyframes
        .first()
        .is_none_or(|key| key.time.ticks != "0")
    {
        right.keyframes.insert(
            0,
            make_key(
                "right-boundary",
                RationalTime::new(0, 1)?.into(),
                &right.keyframes,
            ),
        );
    }
    Ok((left, right))
}

fn validate_channel_bounds(channel: &KeyframeChannel, duration: &Time) -> Result<(), EditorError> {
    let duration = RationalTime::try_from(duration)?;
    if channel.keyframes.iter().any(|keyframe| {
        RationalTime::try_from(&keyframe.time)
            .is_ok_and(|time| time.cmp_exact(duration) == Ordering::Greater)
    }) {
        Err(constraint("keyframe time exceeds clip duration"))
    } else {
        Ok(())
    }
}

fn validate_track_asset_kind(
    project: &ProjectDocument,
    track: &Track,
    clip: &Clip,
) -> Result<(), EditorError> {
    let Some(asset_id) = &clip.asset_id else {
        return Ok(());
    };
    let asset = project
        .assets
        .iter()
        .find(|asset| asset.id == *asset_id)
        .ok_or_else(|| not_found("asset", asset_id))?;
    let compatible = matches!(
        (&track.kind, &asset.kind),
        (TrackKind::Video, AssetKind::Video | AssetKind::Image)
            | (TrackKind::Audio, AssetKind::Audio)
            | (TrackKind::Caption, AssetKind::Subtitle | AssetKind::Text)
            | (
                TrackKind::Graphic,
                AssetKind::Graphic | AssetKind::Image | AssetKind::Text
            )
    );
    if compatible {
        Ok(())
    } else {
        Err(constraint(
            "asset kind is incompatible with the destination track kind",
        ))
    }
}

fn validate_params(
    target: &str,
    params: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<(), EditorError> {
    for (name, value) in params {
        let descriptor = parameter_descriptors()
            .into_iter()
            .find(|item| item.target == target && item.name == *name)
            .ok_or_else(|| {
                EditorError::new(
                    "UNSUPPORTED_OPERATION",
                    format!("unsupported {target} parameter: {name}"),
                )
            })?;
        let number = value
            .as_f64()
            .ok_or_else(|| invalid("render parameters must be JSON numbers"))?;
        if descriptor.minimum.is_some_and(|minimum| number < minimum)
            || descriptor.maximum.is_some_and(|maximum| number > maximum)
        {
            return Err(constraint(&format!(
                "parameter {name} is outside its supported range"
            )));
        }
    }
    Ok(())
}

fn validate_no_overlaps(clips: &[Clip]) -> Result<(), EditorError> {
    let mut refs: Vec<_> = clips.iter().collect();
    refs.sort_by(|a, b| {
        cmp(&a.timeline_range.start, &b.timeline_range.start).unwrap_or(Ordering::Equal)
    });
    for pair in refs.windows(2) {
        if cmp(
            &range_end(&pair[0].timeline_range)?,
            &pair[1].timeline_range.start,
        )? == Ordering::Greater
        {
            return Err(constraint("clips on one track must not overlap"));
        }
    }
    Ok(())
}
fn sort_clips(clips: &mut [Clip]) -> Result<(), EditorError> {
    validate_no_overlaps(clips)?;
    clips.sort_by(|a, b| {
        cmp(&a.timeline_range.start, &b.timeline_range.start).unwrap_or(Ordering::Equal)
    });
    Ok(())
}
fn validate_duration(time: &Time) -> Result<(), EditorError> {
    if RationalTime::try_from(time)?.ticks() <= 0 {
        Err(invalid("duration must be positive"))
    } else {
        Ok(())
    }
}
fn validate_asset_version(version: &AssetVersion) -> Result<(), EditorError> {
    let valid_hash = version.content_hash.len() == 64
        && version
            .content_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'));
    if !valid_hash {
        return Err(invalid(
            "asset version content_hash must be a 64-character lowercase SHA-256 hex digest",
        ));
    }
    if version.id != format!("av_{}", version.content_hash) {
        return Err(invalid("asset version id must be av_<content_hash>"));
    }
    Ok(())
}
fn cmp(a: &Time, b: &Time) -> Result<Ordering, EditorError> {
    Ok(RationalTime::try_from(a)?.cmp_exact(RationalTime::try_from(b)?))
}
fn sequence_mut<'a>(
    project: &'a mut ProjectDocument,
    id: &str,
) -> Result<&'a mut Sequence, EditorError> {
    project
        .sequences
        .iter_mut()
        .find(|x| x.id == id)
        .ok_or_else(|| not_found("sequence", id))
}
fn track_mut<'a>(
    project: &'a mut ProjectDocument,
    sequence_id: &str,
    track_id: &str,
) -> Result<&'a mut Track, EditorError> {
    sequence_mut(project, sequence_id)?
        .tracks
        .iter_mut()
        .find(|x| x.id == track_id)
        .ok_or_else(|| not_found("track", track_id))
}
fn clip_index(track: &Track, id: &str) -> Result<usize, EditorError> {
    track
        .clips
        .iter()
        .position(|x| x.id == id)
        .ok_or_else(|| not_found("clip", id))
}
fn ensure_unlocked(track: &Track) -> Result<(), EditorError> {
    if track.locked {
        Err(locked(&track.id))
    } else {
        Ok(())
    }
}
fn locked(id: &str) -> EditorError {
    constraint(&format!("track {id} is locked"))
}
fn unique<'a>(
    ids: &mut std::collections::HashSet<&'a str>,
    id: &'a str,
) -> Result<(), EditorError> {
    if id.trim().is_empty() {
        return Err(invalid("ids must not be empty"));
    }
    if !ids.insert(id) {
        return Err(constraint(&format!("duplicate id: {id}")));
    }
    Ok(())
}
fn ensure_id_available(project: &ProjectDocument, id: &str) -> Result<(), EditorError> {
    let mut ids = std::collections::HashSet::new();
    ids.insert(project.id.as_str());
    for a in &project.assets {
        ids.insert(&a.id);
    }
    for s in &project.sequences {
        ids.insert(&s.id);
        for t in &s.tracks {
            ids.insert(&t.id);
            for c in &t.clips {
                ids.insert(&c.id);
            }
        }
    }
    if id.trim().is_empty() {
        Err(invalid("id must not be empty"))
    } else if ids.contains(id) {
        Err(constraint(&format!("duplicate id: {id}")))
    } else {
        Ok(())
    }
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn constraint(message: &str) -> EditorError {
    EditorError::new("CONSTRAINT_VIOLATION", message)
}
fn not_found(kind: &str, id: &str) -> EditorError {
    EditorError::new("NOT_FOUND", format!("{kind} not found: {id}"))
}
fn push_unique(values: &mut Vec<String>, id: &str) {
    if !values.iter().any(|x| x == id) {
        values.push(id.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn invalid_caption_layout_rejects_the_entire_edit_plan() {
        let snapshot = fixture();
        let cue = CaptionCue {
            id: "caption-layout".into(),
            timeline_range: range(0, 24),
            text: "Hello 中文".into(),
            source_anchor: None,
            style: CaptionStyle {
                font_family: "Arial".into(),
                font_size: 24.0,
                color: "white".into(),
                background_color: None,
                font_policy: FontResolutionPolicy::ExplicitFallback {
                    families: vec!["PingFang SC".into()],
                },
                max_width: Some(320),
                line_height: Some(32.0),
                text_align: TextAlign::Left,
            },
        };
        let insert = |cue| Operation::InsertCaption {
            sequence_id: "s".into(),
            cue,
        };
        assert!(plan_transaction(&snapshot, &request(vec![insert(cue.clone())])).is_ok());
        let mut cases = vec![cue.clone(); 5];
        cases[0].style.font_size = f64::NAN;
        cases[1].style.max_width = Some(0);
        cases[2].style.line_height = Some(f32::INFINITY);
        cases[3].style.font_policy = FontResolutionPolicy::ExplicitFallback { families: vec![] };
        cases[4].style.font_policy = FontResolutionPolicy::ExplicitFallback {
            families: vec![" arial ".into()],
        };
        for invalid_cue in cases {
            let error = plan_transaction(
                &snapshot,
                &request(vec![
                    Operation::DeleteClip {
                        sequence_id: "s".into(),
                        track_id: "t".into(),
                        clip_id: "c".into(),
                    },
                    insert(invalid_cue),
                ]),
            )
            .unwrap_err();
            assert_eq!(error.code, "INVALID_ARGUMENT");
            assert_eq!(snapshot, fixture());
        }
    }

    fn time(ticks: i64) -> Time {
        Time {
            ticks: ticks.to_string(),
            timebase: 24,
        }
    }
    fn range(start: i64, duration: i64) -> TimeRange {
        TimeRange {
            start: time(start),
            duration: time(duration),
        }
    }
    fn fixture() -> ProjectSnapshot {
        let mut project = create_project(&CreateProject {
            id: "p".into(),
            name: "Project".into(),
            sequence_id: "s".into(),
            width: 1920,
            height: 1080,
            fps: FrameRate {
                numerator: 24,
                denominator: 1,
            },
        })
        .unwrap();
        project.assets.push(Asset {
            id: "a".into(),
            name: "Asset".into(),
            kind: AssetKind::Video,
            uri: "file:///a.mp4".into(),
            duration: Some(time(240)),
            version: None,
            metadata: BTreeMap::new(),
        });
        project.sequences[0].tracks.push(Track {
            id: "t".into(),
            name: "Video".into(),
            kind: TrackKind::Video,
            enabled: true,
            muted: false,
            locked: false,
            clips: vec![Clip {
                id: "c".into(),
                asset_id: Some("a".into()),
                timeline_range: range(0, 120),
                source_range: Some(range(0, 120)),
                params: BTreeMap::new(),
                channels: Vec::new(),
            }],
            params: BTreeMap::new(),
        });
        ProjectSnapshot {
            project,
            revision: "rev-1".into(),
        }
    }
    fn request(operations: Vec<Operation>) -> TransactionRequest {
        TransactionRequest {
            expected_revision: "rev-1".into(),
            idempotency_key: "key".into(),
            label: "edit".into(),
            operations,
        }
    }

    #[test]
    fn split_preserves_exact_timeline_and_source_ranges() {
        let plan = plan_transaction(
            &fixture(),
            &request(vec![Operation::SplitClip {
                sequence_id: "s".into(),
                track_id: "t".into(),
                clip_id: "c".into(),
                at: time(48),
                right_clip_id: "right".into(),
            }]),
        )
        .unwrap();
        let clips = &plan.proposed_project.sequences[0].tracks[0].clips;
        assert_eq!(clips[0].timeline_range, range(0, 48));
        assert_eq!(clips[1].timeline_range, range(48, 72));
        assert_eq!(clips[1].source_range, Some(range(48, 72)));
        assert_eq!(plan.created_ids, ["right"]);
    }

    #[test]
    fn split_preserves_animation_at_and_between_keys() {
        for interpolation in [KeyframeInterpolation::Linear, KeyframeInterpolation::Hold] {
            for cut in [12, 24, 60, 96, 108] {
                let mut snapshot = fixture();
                let channel = KeyframeChannel {
                    parameter: "opacity".into(),
                    keyframes: vec![
                        Keyframe {
                            id: "first".into(),
                            time: time(24),
                            value: serde_json::json!(0.2),
                            interpolation: interpolation.clone(),
                        },
                        Keyframe {
                            id: "right:opacity:right-boundary".into(),
                            time: time(96),
                            value: serde_json::json!(0.8),
                            interpolation: KeyframeInterpolation::Linear,
                        },
                    ],
                };
                snapshot.project.sequences[0].tracks[0].clips[0]
                    .channels
                    .push(channel.clone());
                let plan = plan_transaction(
                    &snapshot,
                    &request(vec![Operation::SplitClip {
                        sequence_id: "s".into(),
                        track_id: "t".into(),
                        clip_id: "c".into(),
                        at: time(cut),
                        right_clip_id: "right".into(),
                    }]),
                )
                .unwrap();
                let clips = &plan.proposed_project.sequences[0].tracks[0].clips;
                assert_eq!(clips[0].channels.len(), 1);
                assert_eq!(clips[1].channels.len(), 1);
                for tick in 0..=120 {
                    let expected = if tick < 24 {
                        0.2
                    } else if tick >= 96 {
                        0.8
                    } else if interpolation == KeyframeInterpolation::Hold {
                        0.2
                    } else {
                        0.2 + 0.6 * (tick - 24) as f64 / 72.0
                    };
                    let (clip, local) = if tick < cut {
                        (&clips[0], tick)
                    } else {
                        (&clips[1], tick - cut)
                    };
                    let actual = sample_keyframe_channel(&clip.channels[0], &time(local)).unwrap();
                    assert!(
                        (actual - expected).abs() < 1e-12,
                        "cut={cut} tick={tick} actual={actual} expected={expected}"
                    );
                }
                assert_eq!(
                    snapshot.project.sequences[0].tracks[0].clips[0].channels[0],
                    channel
                );
            }
        }
    }

    #[test]
    fn failed_batch_does_not_mutate_the_snapshot() {
        let snapshot = fixture();
        let before = snapshot.clone();
        let result = plan_transaction(
            &snapshot,
            &request(vec![
                Operation::DeleteClip {
                    sequence_id: "s".into(),
                    track_id: "t".into(),
                    clip_id: "c".into(),
                },
                Operation::DeleteClip {
                    sequence_id: "s".into(),
                    track_id: "t".into(),
                    clip_id: "missing".into(),
                },
            ]),
        );
        assert_eq!(result.unwrap_err().code, "NOT_FOUND");
        assert_eq!(snapshot, before);
    }

    #[test]
    fn rejects_overlap_and_out_of_asset_bounds() {
        let overlap = Clip {
            id: "other".into(),
            asset_id: Some("a".into()),
            timeline_range: range(100, 20),
            source_range: Some(range(0, 20)),
            params: BTreeMap::new(),
            channels: Vec::new(),
        };
        assert_eq!(
            plan_transaction(
                &fixture(),
                &request(vec![Operation::InsertClip {
                    sequence_id: "s".into(),
                    track_id: "t".into(),
                    clip: overlap
                }])
            )
            .unwrap_err()
            .code,
            "CONSTRAINT_VIOLATION"
        );
        let bad = Clip {
            id: "other".into(),
            asset_id: Some("a".into()),
            timeline_range: range(120, 40),
            source_range: Some(range(220, 40)),
            params: BTreeMap::new(),
            channels: Vec::new(),
        };
        assert_eq!(
            plan_transaction(
                &fixture(),
                &request(vec![Operation::InsertClip {
                    sequence_id: "s".into(),
                    track_id: "t".into(),
                    clip: bad
                }])
            )
            .unwrap_err()
            .code,
            "INVALID_ARGUMENT"
        );
    }

    #[test]
    fn stale_revision_is_rejected_before_editing() {
        let mut req = request(vec![Operation::DeleteClip {
            sequence_id: "s".into(),
            track_id: "t".into(),
            clip_id: "c".into(),
        }]);
        req.expected_revision = "old".into();
        assert_eq!(
            plan_transaction(&fixture(), &req).unwrap_err().code,
            "REVISION_CONFLICT"
        );
    }

    #[test]
    fn rejects_incompatible_assets_and_non_empty_track_deletion() {
        let mut snapshot = fixture();
        snapshot.project.sequences[0].tracks.push(Track {
            id: "audio".into(),
            name: "Audio".into(),
            kind: TrackKind::Audio,
            enabled: true,
            muted: false,
            locked: false,
            clips: Vec::new(),
            params: BTreeMap::new(),
        });
        let clip = Clip {
            id: "bad-kind".into(),
            asset_id: Some("a".into()),
            timeline_range: range(120, 24),
            source_range: Some(range(0, 24)),
            params: BTreeMap::new(),
            channels: Vec::new(),
        };
        assert_eq!(
            plan_transaction(
                &snapshot,
                &request(vec![Operation::InsertClip {
                    sequence_id: "s".into(),
                    track_id: "audio".into(),
                    clip
                }])
            )
            .unwrap_err()
            .code,
            "CONSTRAINT_VIOLATION"
        );
        assert_eq!(
            plan_transaction(
                &snapshot,
                &request(vec![Operation::RemoveTrack {
                    sequence_id: "s".into(),
                    track_id: "t".into()
                }])
            )
            .unwrap_err()
            .code,
            "CONSTRAINT_VIOLATION"
        );
    }

    #[test]
    fn validates_parameter_registry_type_range_and_unknown_names() {
        let params = BTreeMap::from([("opacity".into(), serde_json::json!(1.5))]);
        assert_eq!(
            plan_transaction(
                &fixture(),
                &request(vec![Operation::UpdateClipParams {
                    sequence_id: "s".into(),
                    track_id: "t".into(),
                    clip_id: "c".into(),
                    params
                }])
            )
            .unwrap_err()
            .code,
            "CONSTRAINT_VIOLATION"
        );
        let params = BTreeMap::from([("blur".into(), serde_json::json!(5))]);
        assert_eq!(
            plan_transaction(
                &fixture(),
                &request(vec![Operation::UpdateClipParams {
                    sequence_id: "s".into(),
                    track_id: "t".into(),
                    clip_id: "c".into(),
                    params
                }])
            )
            .unwrap_err()
            .code,
            "UNSUPPORTED_OPERATION"
        );
        assert_eq!(operation_descriptors().len(), 20);
        assert!(
            parameter_descriptors()
                .iter()
                .any(|item| item.name == "opacity" && item.target == "clip")
        );
    }

    fn asset_version(hex: char, size: u64) -> AssetVersion {
        let content_hash: String = std::iter::repeat_n(hex, 64).collect();
        AssetVersion {
            id: format!("av_{content_hash}"),
            content_hash,
            size,
        }
    }

    #[test]
    fn legacy_projects_remain_valid_and_plans_report_unresolved_assets() {
        let mut snapshot = fixture();
        snapshot.project.schema_version = 1;
        validate_project(&snapshot.project).unwrap();

        let plan = plan_transaction(
            &snapshot,
            &request(vec![Operation::UpdateSequenceSettings {
                sequence_id: "s".into(),
                name: Some("Legacy edit".into()),
                width: None,
                height: None,
                fps: None,
            }]),
        )
        .unwrap();
        assert_eq!(plan.proposed_project.schema_version, 1);
        assert_eq!(plan.unresolved_asset_ids, ["a"]);
    }

    #[test]
    fn replacing_an_asset_versions_it_without_rewriting_clip_references() {
        let mut snapshot = fixture();
        snapshot.project.schema_version = 1;
        let replacement = Asset {
            id: "a".into(),
            name: "Replacement".into(),
            kind: AssetKind::Video,
            uri: "asset://immutable/a".into(),
            duration: Some(time(240)),
            version: Some(asset_version('a', 0)),
            metadata: BTreeMap::new(),
        };
        let plan = plan_transaction(
            &snapshot,
            &request(vec![Operation::ReplaceAsset { asset: replacement }]),
        )
        .unwrap();
        assert_eq!(plan.proposed_project.schema_version, 2);
        assert_eq!(plan.proposed_project.assets[0].name, "Replacement");
        assert_eq!(
            plan.proposed_project.sequences[0].tracks[0].clips[0]
                .asset_id
                .as_deref(),
            Some("a")
        );
        assert_eq!(plan.changed_ids, ["a"]);
        assert!(plan.unresolved_asset_ids.is_empty());
    }

    #[test]
    fn binding_a_versioned_asset_upgrades_a_legacy_document() {
        let mut snapshot = fixture();
        snapshot.project.schema_version = 1;
        let asset = Asset {
            id: "b".into(),
            name: "Still".into(),
            kind: AssetKind::Image,
            uri: "asset://immutable/b".into(),
            duration: None,
            version: Some(asset_version('b', 42)),
            metadata: BTreeMap::new(),
        };
        let plan =
            plan_transaction(&snapshot, &request(vec![Operation::BindAsset { asset }])).unwrap();
        assert_eq!(plan.proposed_project.schema_version, 2);
        assert_eq!(plan.created_ids, ["b"]);
        assert_eq!(plan.unresolved_asset_ids, ["a"]);
    }

    #[test]
    fn asset_replacement_is_atomic_and_validates_versions_and_references() {
        let snapshot = fixture();
        let mut replacement = snapshot.project.assets[0].clone();
        replacement.duration = Some(time(24));
        assert_eq!(
            plan_transaction(
                &snapshot,
                &request(vec![Operation::ReplaceAsset { asset: replacement }])
            )
            .unwrap_err()
            .code,
            "INVALID_ARGUMENT"
        );
        assert_eq!(snapshot, fixture());

        let mut invalid_version = snapshot.project.assets[0].clone();
        invalid_version.version = Some(AssetVersion {
            id: "av_bad".into(),
            content_hash: "A".repeat(64),
            size: 1,
        });
        assert_eq!(
            plan_transaction(
                &snapshot,
                &request(vec![Operation::ReplaceAsset {
                    asset: invalid_version
                }])
            )
            .unwrap_err()
            .code,
            "INVALID_ARGUMENT"
        );
        assert_eq!(snapshot, fixture());

        let mut mismatched_id = snapshot.project.assets[0].clone();
        let mut version = asset_version('c', 1);
        version.id = format!("av_{}", "d".repeat(64));
        mismatched_id.version = Some(version);
        assert_eq!(
            plan_transaction(
                &snapshot,
                &request(vec![Operation::ReplaceAsset {
                    asset: mismatched_id
                }])
            )
            .unwrap_err()
            .code,
            "INVALID_ARGUMENT"
        );
        assert_eq!(snapshot, fixture());
    }

    #[test]
    fn replacing_a_missing_or_empty_asset_id_is_rejected() {
        let snapshot = fixture();
        for id in ["missing", ""] {
            let mut asset = snapshot.project.assets[0].clone();
            asset.id = id.into();
            assert!(
                plan_transaction(&snapshot, &request(vec![Operation::ReplaceAsset { asset }]))
                    .is_err()
            );
        }
    }

    #[test]
    fn split_retimed_clip_scales_source_ranges_exactly() {
        let mut snapshot = fixture();
        snapshot.project.sequences[0].tracks[0].clips[0].source_range = Some(range(0, 60));
        let plan = plan_transaction(
            &snapshot,
            &request(vec![Operation::SplitClip {
                sequence_id: "s".into(),
                track_id: "t".into(),
                clip_id: "c".into(),
                at: time(48),
                right_clip_id: "right".into(),
            }]),
        )
        .unwrap();
        let clips = &plan.proposed_project.sequences[0].tracks[0].clips;
        assert_eq!(
            cmp(&clips[0].source_range.as_ref().unwrap().duration, &time(24)).unwrap(),
            Ordering::Equal
        );
        assert_eq!(
            cmp(&clips[1].source_range.as_ref().unwrap().start, &time(24)).unwrap(),
            Ordering::Equal
        );
        assert_eq!(
            cmp(&clips[1].source_range.as_ref().unwrap().duration, &time(36)).unwrap(),
            Ordering::Equal
        );
    }

    #[test]
    fn retime_keyframes_and_caption_edits_are_validated_and_planned() {
        let channel = KeyframeChannel {
            parameter: "opacity".into(),
            keyframes: vec![
                Keyframe {
                    id: "k1".into(),
                    time: time(0),
                    value: serde_json::json!(0.0),
                    interpolation: KeyframeInterpolation::Linear,
                },
                Keyframe {
                    id: "k2".into(),
                    time: time(48),
                    value: serde_json::json!(1.0),
                    interpolation: KeyframeInterpolation::Linear,
                },
            ],
        };
        let cue = CaptionCue {
            id: "caption".into(),
            timeline_range: range(0, 48),
            text: "Hello".into(),
            source_anchor: Some(CaptionSourceAnchor {
                asset_id: "a".into(),
                source_range: range(0, 48),
            }),
            style: CaptionStyle {
                font_family: "Inter".into(),
                font_size: 48.0,
                color: "#ffffff".into(),
                background_color: None,
                font_policy: FontResolutionPolicy::Strict {},
                max_width: None,
                line_height: None,
                text_align: TextAlign::Center,
            },
        };
        let plan = plan_transaction(
            &fixture(),
            &request(vec![
                Operation::RetimeClip {
                    sequence_id: "s".into(),
                    track_id: "t".into(),
                    clip_id: "c".into(),
                    timeline_duration: time(144),
                },
                Operation::SetKeyframeChannel {
                    sequence_id: "s".into(),
                    track_id: "t".into(),
                    clip_id: "c".into(),
                    channel,
                },
                Operation::InsertCaption {
                    sequence_id: "s".into(),
                    cue,
                },
                Operation::UpdateTrack {
                    sequence_id: "s".into(),
                    track_id: "t".into(),
                    name: Some("Picture".into()),
                    enabled: None,
                    muted: Some(true),
                    locked: None,
                },
            ]),
        )
        .unwrap();
        let sequence = &plan.proposed_project.sequences[0];
        assert_eq!(sequence.caption_cues[0].text, "Hello");
        assert_eq!(sequence.tracks[0].clips[0].channels[0].keyframes.len(), 2);
        assert!(sequence.tracks[0].muted);
    }
}
