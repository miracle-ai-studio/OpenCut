//! Typed identity traversal. JSON field names and user strings are not IDs.
use super::*;

fn channels(values: &[AnimationChannelV4], ids: &mut BTreeSet<String>) {
    for value in values {
        match value {
            AnimationChannelV4::Scalar { id, keys, .. } => {
                ids.insert(id.clone());
                for key in keys {
                    ids.insert(key.id.clone());
                    if let Some(segment) = &key.segment_id {
                        ids.insert(segment.clone());
                    }
                }
            }
            AnimationChannelV4::Discrete { id, keys, .. } => {
                ids.insert(id.clone());
                ids.extend(keys.iter().map(|key| key.id.clone()));
            }
        }
    }
}
pub(super) fn collect(project: &ProjectDocumentV4) -> BTreeSet<String> {
    let mut ids = BTreeSet::from([project.id.clone()]);
    ids.extend(project.assets.iter().map(|a| a.id.clone()));
    ids.extend(project.fonts.iter().map(|f| f.id.clone()));
    for sequence in &project.sequences {
        ids.insert(sequence.id.clone());
        ids.extend(sequence.captions.iter().map(|c| c.id.clone()));
        ids.extend(sequence.transitions.iter().map(|c| c.id.clone()));
        ids.extend(sequence.bookmarks.iter().map(|c| c.id.clone()));
        for track in &sequence.tracks {
            ids.insert(track.id.clone());
            channels(&track.channels, &mut ids);
            for element in &track.elements {
                ids.insert(element.id.clone());
                channels(&element.channels, &mut ids);
                if let ElementContentV4::EffectLayer { effect } = &element.content {
                    ids.insert(effect.id.clone());
                }
                if let Some(visual) = &element.visual {
                    ids.extend(visual.effects.iter().map(|e| e.id.clone()));
                    for mask in &visual.masks {
                        ids.insert(mask.id.clone());
                        if let MaskShapeV4::Freeform { path } = &mask.shape {
                            ids.extend(path.points.iter().map(|p| p.id.clone()));
                        }
                    }
                }
            }
        }
    }
    ids
}
pub(super) struct Ids<'a> {
    used: BTreeSet<String>,
    revision: &'a str,
    key: &'a str,
}
impl<'a> Ids<'a> {
    pub(super) fn new(
        target: &ProjectDocumentV4,
        source: &ProjectDocumentV4,
        revision: &'a str,
        key: &'a str,
    ) -> Self {
        let mut used = collect(target);
        // Even deleted source nodes get new clone identities; an old stable
        // identity is never reused just because it disappeared from the head.
        used.extend(collect(source));
        Self {
            used,
            revision,
            key,
        }
    }
    pub(super) fn claim(&mut self, value: &str) -> Result<(), EditorError> {
        id(value)?;
        if !self.used.insert(value.into()) {
            return Err(invalid(
                "New clipboard identity collides with a source or target project node",
            ));
        }
        Ok(())
    }
    pub(super) fn fresh(
        &mut self,
        owner: &str,
        kind: &str,
        old: &str,
    ) -> Result<String, EditorError> {
        for attempt in 0..=self.used.len() {
            let seed = crate::encode_json(
                &(self.revision, self.key, owner, kind, old, attempt),
                "Clipboard identity",
            )?;
            let next = crate::identity::content_id(kind, seed.as_bytes());
            if self.used.insert(next.clone()) {
                return Ok(next);
            }
        }
        Err(invalid("Cannot allocate a fresh clipboard identity"))
    }
    fn replace(
        &mut self,
        owner: &str,
        kind: &str,
        value: &mut String,
        map: &mut BTreeMap<String, String>,
    ) -> Result<(), EditorError> {
        let old = value.clone();
        let new = self.fresh(owner, kind, &old)?;
        map.insert(old, new.clone());
        *value = new;
        Ok(())
    }
    pub(super) fn rebind(
        &mut self,
        element: &mut TimelineElementV4,
        new_id: &str,
    ) -> Result<(), EditorError> {
        let mut map = BTreeMap::from([(element.id.clone(), new_id.to_string())]);
        if let ElementContentV4::EffectLayer { effect } = &mut element.content {
            self.replace(new_id, "cb_effect", &mut effect.id, &mut map)?;
        }
        if let Some(visual) = &mut element.visual {
            for effect in &mut visual.effects {
                self.replace(new_id, "cb_effect", &mut effect.id, &mut map)?;
            }
            for mask in &mut visual.masks {
                self.replace(new_id, "cb_mask", &mut mask.id, &mut map)?;
                if let MaskShapeV4::Freeform { path } = &mut mask.shape {
                    for point in &mut path.points {
                        self.replace(new_id, "cb_point", &mut point.id, &mut map)?;
                    }
                }
            }
        }
        for channel in &mut element.channels {
            let target = match channel {
                AnimationChannelV4::Scalar {
                    id, target, keys, ..
                } => {
                    self.replace(new_id, "cb_channel", id, &mut map)?;
                    for key in keys {
                        self.replace(new_id, "cb_key", &mut key.id, &mut map)?;
                        if let Some(segment) = &mut key.segment_id {
                            self.replace(new_id, "cb_segment", segment, &mut map)?;
                        }
                    }
                    target
                }
                AnimationChannelV4::Discrete {
                    id, target, keys, ..
                } => {
                    self.replace(new_id, "cb_channel", id, &mut map)?;
                    for key in keys {
                        self.replace(new_id, "cb_key", &mut key.id, &mut map)?;
                    }
                    target
                }
            };
            let mapped = |value: &mut String| -> Result<(), EditorError> {
                *value = map.get(value).cloned().ok_or_else(|| {
                    invalid("Clipboard channel points outside its copied element")
                })?;
                Ok(())
            };
            match target {
                ChannelTargetV4::Element { element_id } => mapped(element_id)?,
                ChannelTargetV4::Effect {
                    element_id,
                    effect_id,
                } => {
                    mapped(element_id)?;
                    mapped(effect_id)?;
                }
                ChannelTargetV4::Mask {
                    element_id,
                    mask_id,
                } => {
                    mapped(element_id)?;
                    mapped(mask_id)?;
                }
                ChannelTargetV4::Track { .. } => {
                    return Err(invalid(
                        "Element clipboard cannot copy a scene-clock track channel",
                    ));
                }
            }
        }
        element.id = new_id.into();
        Ok(())
    }
}
