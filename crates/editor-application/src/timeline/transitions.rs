//! Existing crossfades survive unrelated content/track-state changes. Only an
//! invalid actual seam, one-to-one span or source handle removes a transition.
use super::*;
use opencut_editor_core::scene::resolve_element_source_request_scene;

pub(super) fn retain(
    project: &EditorDocument,
    scene: &mut Scene,
    decision: &mut TimelineDecision,
) -> Result<(), EditorError> {
    let elements: BTreeMap<_, _> = scene
        .tracks
        .iter()
        .flat_map(|track| {
            track
                .elements
                .iter()
                .map(move |e| (e.id.as_str(), (track.id.as_str(), e)))
        })
        .collect();
    let mut remove = BTreeSet::new();
    for cut in &scene.transitions {
        let Some((out_track, out)) = elements.get(cut.outgoing_element_id.as_str()) else {
            remove.insert(cut.id.clone());
            continue;
        };
        let Some((in_track, incoming)) = elements.get(cut.incoming_element_id.as_str()) else {
            remove.insert(cut.id.clone());
            continue;
        };
        if out_track != in_track
            || end(out)?.cmp_exact(time(&incoming.timeline_range.start)?)
                != std::cmp::Ordering::Equal
        {
            remove.insert(cut.id.clone());
            continue;
        }
        let half = time(&cut.duration)?.checked_scale(R::new(1, 1)?, R::new(2, 1)?)?;
        for (element, outgoing) in [(*out, true), (*incoming, false)] {
            let ElementContent::Video { asset_id, .. } = &element.content else {
                remove.insert(cut.id.clone());
                break;
            };
            let asset = project
                .assets
                .iter()
                .find(|asset| asset.id == *asset_id)
                .ok_or_else(|| missing("Transition media is missing"))?;
            // Invalid source timing is a failed proposal, not a reason to hide
            // its error by deleting the transition.
            let source = resolve_element_source_request_scene(element, Some(asset))?;
            let duration = time(&element.timeline_range.duration)?;
            let available = if outgoing {
                let Some(extent) = &asset.duration else {
                    remove.insert(cut.id.clone());
                    break;
                };
                time(extent)?
                    .checked_sub(time(&source.start)?.checked_add(time(&source.duration)?)?)?
            } else {
                time(&source.start)?
            };
            if time(&source.duration)?.cmp_exact(duration) != std::cmp::Ordering::Equal
                || duration.cmp_exact(half).is_lt()
                || available.cmp_exact(half).is_lt()
            {
                remove.insert(cut.id.clone());
                break;
            }
        }
    }
    scene.transitions.retain(|cut| {
        if remove.contains(&cut.id) {
            decision.removed_transition_ids.push(cut.id.clone());
            false
        } else {
            true
        }
    });
    Ok(())
}
