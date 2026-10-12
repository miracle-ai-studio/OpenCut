//! Receipt identities are declared aggregate nodes, never arbitrary metadata IDs
//! or CAS version IDs. Parent values include their children, as in legacy edits.
use super::*;
use serde::Serialize;
use serde_json::Value;

type Entities = BTreeMap<String, Value>;
fn put(result: &mut Entities, id: &str, value: &impl Serialize) -> Result<(), EditorError> {
    let value = serde_json::to_value(value)
        .map_err(|e| EditorError::new("SERIALIZATION_ERROR", e.to_string()))?;
    result.insert(id.into(), value);
    Ok(())
}
fn base(result: &mut Entities, project: &ProjectDocument) -> Result<(), EditorError> {
    put(result, &project.id, project)?;
    for asset in &project.assets {
        put(result, &asset.id, asset)?;
    }
    for sequence in &project.sequences {
        put(result, &sequence.id, sequence)?;
        for cue in &sequence.caption_cues {
            put(result, &cue.id, cue)?;
        }
        for track in &sequence.tracks {
            put(result, &track.id, track)?;
            for clip in &track.clips {
                put(result, &clip.id, clip)?;
                for channel in &clip.channels {
                    for key in &channel.keyframes {
                        put(result, &key.id, key)?;
                    }
                }
            }
        }
    }
    Ok(())
}
fn channels(result: &mut Entities, values: &[AnimationChannelV4]) -> Result<(), EditorError> {
    for channel in values {
        match channel {
            AnimationChannelV4::Scalar { id, keys, .. } => {
                put(result, id, channel)?;
                for key in keys {
                    put(result, &key.id, key)?;
                    if let Some(id) = &key.segment_id {
                        put(result, id, key)?;
                    }
                }
            }
            AnimationChannelV4::Discrete { id, keys, .. } => {
                put(result, id, channel)?;
                for key in keys {
                    put(result, &key.id, key)?;
                }
            }
        }
    }
    Ok(())
}
pub(super) fn collect(source: &StoredProjectDocumentV4) -> Result<Entities, EditorError> {
    let mut result = BTreeMap::new();
    match source {
        StoredProjectDocumentV4::Legacy(project) => base(&mut result, project)?,
        StoredProjectDocumentV4::V3(project) => {
            base(&mut result, &project.base)?;
            put(&mut result, &project.base.id, project)?;
            for visual in &project.visual {
                for shape in &visual.shapes {
                    put(&mut result, &shape.id, shape)?;
                }
                for effect in &visual.effects {
                    put(&mut result, &effect.id, effect)?;
                }
                for mask in &visual.masks {
                    put(&mut result, &mask.id, mask)?;
                }
                for cut in &visual.transitions {
                    put(&mut result, &cut.id, cut)?;
                }
            }
            for curve in &project.keyframe_curves {
                put(&mut result, &curve.id, curve)?;
            }
        }
        StoredProjectDocumentV4::V4(project) => {
            put(&mut result, &project.id, project)?;
            for asset in &project.assets {
                put(&mut result, &asset.id, asset)?;
            }
            for font in &project.fonts {
                put(&mut result, &font.id, font)?;
            }
            for sequence in &project.sequences {
                put(&mut result, &sequence.id, sequence)?;
                for caption in &sequence.captions {
                    put(&mut result, &caption.id, caption)?;
                }
                for cut in &sequence.transitions {
                    put(&mut result, &cut.id, cut)?;
                }
                for bookmark in &sequence.bookmarks {
                    put(&mut result, &bookmark.id, bookmark)?;
                }
                for track in &sequence.tracks {
                    put(&mut result, &track.id, track)?;
                    channels(&mut result, &track.channels)?;
                    for element in &track.elements {
                        put(&mut result, &element.id, element)?;
                        channels(&mut result, &element.channels)?;
                        if let ElementContentV4::EffectLayer { effect } = &element.content {
                            put(&mut result, &effect.id, effect)?;
                        }
                        if let Some(visual) = &element.visual {
                            for effect in &visual.effects {
                                put(&mut result, &effect.id, effect)?;
                            }
                            for mask in &visual.masks {
                                put(&mut result, &mask.id, mask)?;
                                if let MaskShapeV4::Freeform { path } = &mask.shape {
                                    for point in &path.points {
                                        put(&mut result, &point.id, point)?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(result)
}
