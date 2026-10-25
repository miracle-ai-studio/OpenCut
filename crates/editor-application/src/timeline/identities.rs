//! Stable right-hand identities. Reservation includes every existing ID, not
//! just visible elements, and is shared by all splits in one planned command.
use super::*;
pub(super) struct Ids<'a> {
    used: BTreeSet<String>,
    revision: &'a str,
    key: &'a str,
}
fn reserve_channels(channels: &[AnimationChannel], used: &mut BTreeSet<String>) {
    for channel in channels {
        match channel {
            AnimationChannel::Scalar { id, keys, .. } => {
                used.insert(id.clone());
                for key in keys {
                    used.insert(key.id.clone());
                    if let Some(segment) = &key.segment_id {
                        used.insert(segment.clone());
                    }
                }
            }
            AnimationChannel::Discrete { id, keys, .. } => {
                used.insert(id.clone());
                used.extend(keys.iter().map(|key| key.id.clone()));
            }
        }
    }
}
fn reserve(project: &EditorDocument, used: &mut BTreeSet<String>) {
    used.insert(project.id.clone());
    used.extend(project.assets.iter().map(|a| a.id.clone()));
    used.extend(project.fonts.iter().map(|f| f.id.clone()));
    for sequence in &project.sequences {
        used.insert(sequence.id.clone());
        used.extend(sequence.captions.iter().map(|c| c.id.clone()));
        used.extend(sequence.bookmarks.iter().map(|b| b.id.clone()));
        used.extend(sequence.transitions.iter().map(|t| t.id.clone()));
        for track in &sequence.tracks {
            used.insert(track.id.clone());
            reserve_channels(&track.channels, used);
            for element in &track.elements {
                used.insert(element.id.clone());
                reserve_channels(&element.channels, used);
                if let ElementContent::EffectLayer { effect } = &element.content {
                    used.insert(effect.id.clone());
                }
                if let Some(visual) = &element.visual {
                    used.extend(visual.effects.iter().map(|e| e.id.clone()));
                    for mask in &visual.masks {
                        used.insert(mask.id.clone());
                        if let LayerMaskShape::Freeform { path } = &mask.shape {
                            used.extend(path.points.iter().map(|p| p.id.clone()));
                        }
                    }
                }
            }
        }
    }
}
impl<'a> Ids<'a> {
    pub(super) fn new(
        project: &EditorDocument,
        revision: &'a str,
        key: &'a str,
    ) -> Result<Self, EditorError> {
        let mut used = BTreeSet::new();
        reserve(project, &mut used);
        Ok(Self {
            used,
            revision,
            key,
        })
    }
    pub(super) fn claim(&mut self, value: &str) -> Result<(), EditorError> {
        id(value)?;
        if !self.used.insert(value.into()) {
            return Err(invalid("New timeline identity collides with the project"));
        }
        Ok(())
    }
    pub(super) fn fresh(
        &mut self,
        right: &str,
        kind: &str,
        old: &str,
    ) -> Result<String, EditorError> {
        for attempt in 0..=self.used.len() {
            let seed = crate::encode_json(
                &(self.revision, self.key, right, kind, old, attempt),
                "Timeline identity",
            )?;
            let id = crate::identity::content_id(kind, seed.as_bytes());
            if self.used.insert(id.clone()) {
                return Ok(id);
            }
        }
        Err(invalid("Could not allocate timeline identity"))
    }
    pub(super) fn rebind(
        &mut self,
        e: &mut TimelineElement,
        right: &str,
    ) -> Result<(), EditorError> {
        let mut map = BTreeMap::from([(e.id.clone(), right.to_string())]);
        if let ElementContent::EffectLayer { effect } = &mut e.content {
            let next = self.fresh(right, "tl_effect", &effect.id)?;
            map.insert(effect.id.clone(), next.clone());
            effect.id = next;
        }
        if let Some(v) = &mut e.visual {
            for effect in &mut v.effects {
                let next = self.fresh(right, "tl_effect", &effect.id)?;
                map.insert(effect.id.clone(), next.clone());
                effect.id = next;
            }
            for mask in &mut v.masks {
                let next = self.fresh(right, "tl_mask", &mask.id)?;
                map.insert(mask.id.clone(), next.clone());
                mask.id = next;
                if let LayerMaskShape::Freeform { path } = &mut mask.shape {
                    for point in &mut path.points {
                        point.id = self.fresh(right, "tl_point", &point.id)?;
                    }
                }
            }
        }
        for channel in &mut e.channels {
            let target = match channel {
                AnimationChannel::Scalar {
                    id, target, keys, ..
                } => {
                    *id = self.fresh(right, "tl_channel", id)?;
                    for key in keys {
                        key.id = self.fresh(right, "tl_key", &key.id)?;
                        if let Some(id) = &mut key.segment_id {
                            *id = self.fresh(right, "tl_segment", id)?;
                        }
                    }
                    target
                }
                AnimationChannel::Discrete {
                    id, target, keys, ..
                } => {
                    *id = self.fresh(right, "tl_channel", id)?;
                    for key in keys {
                        key.id = self.fresh(right, "tl_key", &key.id)?;
                    }
                    target
                }
            };
            let remap = |id: &mut String| -> Result<(), EditorError> {
                *id = map
                    .get(id)
                    .cloned()
                    .ok_or_else(|| invalid("Split animation target escapes its element"))?;
                Ok(())
            };
            match target {
                ChannelTarget::Element { element_id } => remap(element_id)?,
                ChannelTarget::Effect {
                    element_id,
                    effect_id,
                } => {
                    remap(element_id)?;
                    remap(effect_id)?;
                }
                ChannelTarget::Mask {
                    element_id,
                    mask_id,
                } => {
                    remap(element_id)?;
                    remap(mask_id)?;
                }
                ChannelTarget::Track { .. } => {
                    return Err(invalid("Element channel cannot target a track"));
                }
            }
        }
        e.id = right.into();
        Ok(())
    }
}
