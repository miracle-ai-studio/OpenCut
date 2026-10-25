use super::*;

trait Entity {
    fn id(&self) -> &str;
}
macro_rules! entities { ($($t:ty),+) => { $(impl Entity for $t { fn id(&self) -> &str { &self.id } })+ }; }
entities!(
    ProjectAsset,
    FontResource,
    Scene,
    SceneTrack,
    TimelineElement,
    Caption,
    SceneTransition
);
fn put<T: Entity>(items: &mut Vec<T>, value: T) {
    if let Some(at) = items.iter().position(|item| item.id() == value.id()) {
        items[at] = value;
    } else {
        items.push(value);
    }
}
fn delete<T: Entity>(items: &mut Vec<T>, id: &str) -> Result<(), EditorError> {
    let index = items
        .iter()
        .position(|item| item.id() == id)
        .ok_or_else(|| missing("entity"))?;
    items.remove(index);
    Ok(())
}
fn sequence_mut<'a>(
    project: &'a mut EditorDocument,
    id: &str,
) -> Result<&'a mut Scene, EditorError> {
    project
        .sequences
        .iter_mut()
        .find(|s| s.id == id)
        .ok_or_else(|| missing("sequence"))
}

/// Pure proposal from one fixed snapshot. Intermediate edits may temporarily
/// change references, but only a complete, valid final aggregate can commit.
pub fn plan(
    snapshot: &EditorSnapshot,
    request: &ApplySceneProject,
) -> Result<EditorDocument, EditorError> {
    if request.project_id != snapshot.project.id {
        return Err(invalid("Edit project scope differs"));
    }
    if request.expected_revision != snapshot.revision {
        return Err(revision_conflict(
            &request.expected_revision,
            &snapshot.revision,
        ));
    }
    require_idempotency_key(&request.idempotency_key)?;
    if request.label.trim().is_empty() || request.label.len() > 4096 {
        return Err(invalid("Edit label must contain 1..4096 bytes"));
    }
    if request.operations.is_empty() || request.operations.len() > 128 {
        return Err(invalid("Choose 1 to 128 aggregate operations"));
    }
    validate_project_scene(&snapshot.project)?;
    let mut project = snapshot.project.clone();
    for operation in &request.operations {
        locks::check(&project, operation)?;
        apply_operation(&mut project, operation)?;
    }
    validate_project_scene(&project)?;
    Ok(project)
}
fn apply_operation(
    project: &mut EditorDocument,
    operation: &ProjectOperation,
) -> Result<(), EditorError> {
    use ProjectOperation::*;
    match operation {
        Rename { name } => project.name = name.clone(),
        SettingsSet { settings } => project.settings = settings.clone(),
        DefaultSequenceSet { sequence_id } => project.default_sequence_id = sequence_id.clone(),
        AssetPut { asset } => put(&mut project.assets, asset.clone()),
        AssetDelete { asset_id } => delete(&mut project.assets, asset_id)?,
        FontPut { font } => put(&mut project.fonts, font.clone()),
        FontDelete { font_id } => delete(&mut project.fonts, font_id)?,
        SequencePut { sequence } => {
            // Bookmarks must use the shared frame-clock edit rules. A parent
            // replacement cannot bypass them or silently transplant annotations.
            let old_bookmarks = project
                .sequences
                .iter()
                .find(|s| s.id == sequence.id)
                .map(|s| s.bookmarks.as_slice())
                .unwrap_or(&[]);
            if sequence.bookmarks != old_bookmarks {
                return Err(invalid(
                    "Use bookmark operations to change sequence annotations",
                ));
            }
            put(&mut project.sequences, sequence.clone());
        }
        SequenceDelete { sequence_id } => delete(&mut project.sequences, sequence_id)?,
        TrackPut { sequence_id, track } => put(
            &mut sequence_mut(project, sequence_id)?.tracks,
            track.clone(),
        ),
        TrackDelete {
            sequence_id,
            track_id,
        } => delete(&mut sequence_mut(project, sequence_id)?.tracks, track_id)?,
        TrackLockSet {
            sequence_id,
            track_id,
            locked,
        } => {
            sequence_mut(project, sequence_id)?
                .tracks
                .iter_mut()
                .find(|t| t.id == *track_id)
                .ok_or_else(|| missing("track"))?
                .locked = *locked;
        }
        ElementPut {
            sequence_id,
            track_id,
            element,
        } => {
            let s = sequence_mut(project, sequence_id)?;
            let destination = s
                .tracks
                .iter()
                .position(|t| t.id == *track_id)
                .ok_or_else(|| missing("track"))?;
            if let Some(origin) = s
                .tracks
                .iter()
                .position(|t| t.elements.iter().any(|e| e.id == element.id))
                && origin != destination
            {
                delete(&mut s.tracks[origin].elements, &element.id)?;
            }
            put(&mut s.tracks[destination].elements, (**element).clone());
        }
        ElementDelete {
            sequence_id,
            element_id,
        } => {
            let s = sequence_mut(project, sequence_id)?;
            let track = s
                .tracks
                .iter_mut()
                .find(|t| t.elements.iter().any(|e| e.id == *element_id))
                .ok_or_else(|| missing("element"))?;
            delete(&mut track.elements, element_id)?;
        }
        CaptionPut {
            sequence_id,
            caption,
        } => put(
            &mut sequence_mut(project, sequence_id)?.captions,
            caption.clone(),
        ),
        CaptionDelete {
            sequence_id,
            caption_id,
        } => delete(
            &mut sequence_mut(project, sequence_id)?.captions,
            caption_id,
        )?,
        TransitionPut {
            sequence_id,
            transition,
        } => put(
            &mut sequence_mut(project, sequence_id)?.transitions,
            transition.clone(),
        ),
        TransitionDelete {
            sequence_id,
            transition_id,
        } => delete(
            &mut sequence_mut(project, sequence_id)?.transitions,
            transition_id,
        )?,
        Bookmark { operation } => *project = bookmarks::apply(project, operation)?,
    }
    Ok(())
}
