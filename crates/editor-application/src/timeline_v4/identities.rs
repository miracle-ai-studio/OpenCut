//! Stable right-hand identities. Reservation includes every existing ID, not
//! just visible elements, and is shared by all splits in one planned command.
use super::*;
pub(super) struct Ids<'a> {
    used: BTreeSet<String>,
    revision: &'a str,
    key: &'a str,
}
fn reserve_channels(channels: &[AnimationChannelV4], used: &mut BTreeSet<String>) {
    for channel in channels {
        match channel {
            AnimationChannelV4::Scalar { id, keys, .. } => {
                used.insert(id.clone());
                for key in keys {
                    used.insert(key.id.clone());
                    if let Some(segment) = &key.segment_id {
                        used.insert(segment.clone());
                    }
                }
            }
            AnimationChannelV4::Discrete { id, keys, .. } => {
                used.insert(id.clone());
                used.extend(keys.iter().map(|key| key.id.clone()));
            }
        }
    }
}
fn reserve(project: &ProjectDocumentV4, used: &mut BTreeSet<String>) {
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
                if let ElementContentV4::EffectLayer { effect } = &element.content {
                    used.insert(effect.id.clone());
                }
                if let Some(visual) = &element.visual {
                    used.extend(visual.effects.iter().map(|e| e.id.clone()));
                    for mask in &visual.masks {
                        used.insert(mask.id.clone());
                        if let MaskShapeV4::Freeform { path } = &mask.shape {
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
        project: &ProjectDocumentV4,
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
    fn fresh(&mut self, right: &str, kind: &str, old: &str) -> Result<String, EditorError> {
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
        e: &mut TimelineElementV4,
        right: &str,
    ) -> Result<(), EditorError> {
        no_animation(e)?;
        if let Some(v) = &mut e.visual {
            for effect in &mut v.effects {
                let next = self.fresh(right, "tl_effect", &effect.id)?;
                effect.id = next;
            }
            for mask in &mut v.masks {
                let next = self.fresh(right, "tl_mask", &mask.id)?;
                mask.id = next;
                if let MaskShapeV4::Freeform { path } = &mut mask.shape {
                    for point in &mut path.points {
                        point.id = self.fresh(right, "tl_point", &point.id)?;
                    }
                }
            }
        }
        e.id = right.into();
        Ok(())
    }
}
