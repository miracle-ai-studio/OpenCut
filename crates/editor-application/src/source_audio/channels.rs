//! Copy only volume animation, retaining exact local-clock curve data and gain
//! units. New stable identities are deterministic and never alias source nodes.
use super::*;
use std::collections::BTreeSet;

fn reserve(value: &serde_json::Value, used: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, value) in fields {
                // Typed V4 stable identities live in id/segment_id fields.
                // Reserving CAS version ids too is conservative and harmless.
                if matches!(key.as_str(), "id" | "segment_id")
                    && let Some(value) = value.as_str()
                {
                    used.insert(value.into());
                }
                reserve(value, used);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                reserve(value, used);
            }
        }
        _ => {}
    }
}

struct Ids<'a> {
    used: BTreeSet<String>,
    revision: &'a str,
    source: &'a str,
    target: &'a str,
}
impl Ids<'_> {
    fn fresh(&mut self, kind: &str, original: &str) -> Result<String, EditorError> {
        for attempt in 0..=self.used.len() {
            let seed = crate::encode_json(
                &(
                    self.revision,
                    self.source,
                    self.target,
                    kind,
                    original,
                    attempt,
                ),
                "Source audio identity",
            )?;
            let id = crate::identity::content_id(kind, seed.as_bytes());
            if self.used.insert(id.clone()) {
                return Ok(id);
            }
        }
        Err(invalid(
            "Could not allocate source audio animation identities",
        ))
    }
}

pub(super) fn copy(
    project: &EditorDocument,
    source: &TimelineElement,
    target: &str,
    new_track: Option<&str>,
    revision: &str,
) -> Result<(Vec<AnimationChannel>, Vec<SourceAudioChannelCopy>), EditorError> {
    let mut used = BTreeSet::new();
    let document = serde_json::to_value(project).map_err(|_| {
        EditorError::new(
            "SERIALIZATION_ERROR",
            "Cannot reserve source audio identities",
        )
    })?;
    reserve(&document, &mut used);
    if !used.insert(target.into()) {
        return Err(invalid("Detached audio element ID already exists"));
    }
    if let Some(track) = new_track
        && !used.insert(track.into())
    {
        return Err(invalid("Fallback audio track ID already exists"));
    }
    let mut ids = Ids {
        used,
        revision,
        source: &source.id,
        target,
    };
    let mut channels = Vec::new();
    let mut copied = Vec::new();
    for channel in &source.channels {
        let AnimationChannel::Scalar {
            id,
            target: ChannelTarget::Element { element_id },
            binding:
                ScalarBinding::Property {
                    parameter: ScalarParameter::GainDecibels | ScalarParameter::GainLinearLegacy,
                },
            keys,
            ..
        } = channel
        else {
            continue;
        };
        if element_id != &source.id {
            continue;
        }
        let mut cloned = channel.clone();
        let mut mapping = SourceAudioChannelCopy {
            source_channel_id: id.clone(),
            channel_id: ids.fresh("sa_channel", id)?,
            key_ids: vec![],
            segment_ids: vec![],
        };
        let AnimationChannel::Scalar {
            id: new_id,
            target: new_target,
            keys: new_keys,
            ..
        } = &mut cloned
        else {
            unreachable!()
        };
        *new_id = mapping.channel_id.clone();
        *new_target = ChannelTarget::Element {
            element_id: target.into(),
        };
        for (old, key) in keys.iter().zip(new_keys) {
            key.id = ids.fresh("sa_key", &old.id)?;
            mapping.key_ids.push(SourceAudioIdentityCopy {
                source_id: old.id.clone(),
                id: key.id.clone(),
            });
            if let Some(segment) = &old.segment_id {
                let new_segment = ids.fresh("sa_segment", segment)?;
                mapping.segment_ids.push(SourceAudioIdentityCopy {
                    source_id: segment.clone(),
                    id: new_segment.clone(),
                });
                key.segment_id = Some(new_segment);
            }
        }
        channels.push(cloned);
        copied.push(mapping);
    }
    Ok((channels, copied))
}
