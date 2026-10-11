//! Selective three-way inversion over stable project entity IDs and properties.
use opencut_contracts::{
    ProjectDocument,
    project_v3::{ProjectDocumentV3, SequenceVisualV3},
};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub path: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    pub path: String,
    pub expected: Option<Value>,
    pub actual: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Merge {
    pub project: Option<ProjectDocument>,
    pub changes: Vec<Change>,
    pub conflicts: Vec<Conflict>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MergeV3 {
    pub project: Option<ProjectDocumentV3>,
    pub changes: Vec<Change>,
    pub conflicts: Vec<Conflict>,
}

/// `from` is the effect expected in the current head; `to` is the desired
/// state. Undo passes original after/before, redo passes original before/after.
pub fn selective_merge(
    from: &ProjectDocument,
    to: &ProjectDocument,
    head: &ProjectDocument,
) -> Merge {
    let (result, changes, conflicts) = merge_values(
        serde_json::to_value(from).expect("project document serializes"),
        serde_json::to_value(to).expect("project document serializes"),
        serde_json::to_value(head).expect("project document serializes"),
    );
    let project = result.and_then(|value| serde_json::from_value(value).ok());
    Merge {
        project,
        changes,
        conflicts,
    }
}

pub fn selective_merge_v3(
    from: &ProjectDocumentV3,
    to: &ProjectDocumentV3,
    head: &ProjectDocumentV3,
) -> MergeV3 {
    let (result, changes, conflicts) = merge_values(
        serde_json::to_value(normalize_v3(from)).expect("schema 3 project serializes"),
        serde_json::to_value(normalize_v3(to)).expect("schema 3 project serializes"),
        serde_json::to_value(normalize_v3(head)).expect("schema 3 project serializes"),
    );
    let mut project: Option<ProjectDocumentV3> =
        result.and_then(|value| serde_json::from_value(value).ok());
    if let Some(project) = &mut project {
        // Empty sequence entries are synthesized only for property-level merging.
        // Restore their actual presence so unrelated structure edits do not add nodes.
        project.visual.retain(|visual| {
            if !visual.shapes.is_empty()
                || !visual.effects.is_empty()
                || !visual.masks.is_empty()
                || !visual.transitions.is_empty()
            {
                return true;
            }
            let present = |document: &ProjectDocumentV3| {
                document
                    .visual
                    .iter()
                    .any(|v| v.sequence_id == visual.sequence_id)
            };
            let before = present(from);
            let desired = present(to);
            let current = present(head);
            if before != desired && current == before {
                desired
            } else {
                current
            }
        });
    }
    MergeV3 {
        project,
        changes,
        conflicts,
    }
}

fn normalize_v3(document: &ProjectDocumentV3) -> ProjectDocumentV3 {
    let mut result = document.clone();
    for sequence in &document.base.sequences {
        if !result
            .visual
            .iter()
            .any(|visual| visual.sequence_id == sequence.id)
        {
            result.visual.push(SequenceVisualV3 {
                sequence_id: sequence.id.clone(),
                shapes: Vec::new(),
                effects: Vec::new(),
                masks: Vec::new(),
                transitions: Vec::new(),
            });
        }
    }
    result
}

fn merge_values(
    from: Value,
    to: Value,
    head: Value,
) -> (Option<Value>, Vec<Change>, Vec<Conflict>) {
    let mut changes = Vec::new();
    let mut conflicts = Vec::new();
    let result = merge_field(
        Some(&from),
        Some(&to),
        Some(&head),
        "",
        &mut changes,
        &mut conflicts,
    );
    let result = if conflicts.is_empty() { result } else { None };
    if result.is_none() && conflicts.is_empty() {
        conflicts.push(Conflict {
            path: "/".into(),
            expected: None,
            actual: None,
        });
    }
    (result, changes, conflicts)
}

fn merge_field(
    from: Option<&Value>,
    to: Option<&Value>,
    head: Option<&Value>,
    path: &str,
    changes: &mut Vec<Change>,
    conflicts: &mut Vec<Conflict>,
) -> Option<Value> {
    if from == to {
        return head.cloned();
    }
    if let (Some(Value::Object(from)), Some(Value::Object(to)), Some(Value::Object(head))) =
        (from, to, head)
        && !opaque(path)
    {
        let mut result = Map::new();
        let keys: BTreeSet<_> = from.keys().chain(to.keys()).chain(head.keys()).collect();
        for key in keys {
            let child = join(path, key);
            if let Some(value) = merge_field(
                from.get(key),
                to.get(key),
                head.get(key),
                &child,
                changes,
                conflicts,
            ) {
                result.insert(key.clone(), value);
            }
        }
        return Some(Value::Object(result));
    }
    if let (
        Some(Value::Array(from_values)),
        Some(Value::Array(to_values)),
        Some(Value::Array(head)),
    ) = (from, to, head)
        && let Some(key_field) = collection_key(path)
        && let (Some(from), Some(to), Some(current)) = (
            keyed(from_values, key_field),
            keyed(to_values, key_field),
            keyed(head, key_field),
        )
    {
        let mut merged = BTreeMap::new();
        let keys: BTreeSet<_> = from
            .keys()
            .chain(to.keys())
            .chain(current.keys())
            .cloned()
            .collect();
        for key in keys {
            let child = join(path, &key);
            if let Some(value) = merge_field(
                from.get(&key).copied(),
                to.get(&key).copied(),
                current.get(&key).copied(),
                &child,
                changes,
                conflicts,
            ) {
                merged.insert(key, value);
            }
        }
        // Retain current order for surviving objects. New restorations use their
        // position in the desired revision relative to existing neighbors.
        let mut order: Vec<String> = head
            .iter()
            .filter_map(|value| object_key(value, key_field).map(str::to_owned))
            .filter(|key| merged.contains_key(key))
            .collect();
        for (index, value) in to_values.iter().enumerate() {
            let Some(key) = object_key(value, key_field) else {
                continue;
            };
            if !merged.contains_key(key) || order.iter().any(|existing| existing == key) {
                continue;
            }
            let predecessor = to_values
                .iter()
                .take(index)
                .filter_map(|item| object_key(item, key_field))
                .rev()
                .find_map(|candidate| order.iter().position(|existing| existing == candidate));
            let position = predecessor.map(|position| position + 1).unwrap_or(0);
            order.insert(position, key.to_owned());
        }
        // Entity identity and collection order are separate changes. A move
        // can reorder surviving clips or tracks without changing their fields.
        let shared: BTreeSet<_> = from
            .keys()
            .filter(|key| to.contains_key(*key) && merged.contains_key(*key))
            .cloned()
            .collect();
        let source_order: Vec<_> = from_values
            .iter()
            .filter_map(|value| object_key(value, key_field))
            .filter(|key| shared.contains(*key))
            .map(str::to_owned)
            .collect();
        let desired_order: Vec<_> = to_values
            .iter()
            .filter_map(|value| object_key(value, key_field))
            .filter(|key| shared.contains(*key))
            .map(str::to_owned)
            .collect();
        if source_order != desired_order {
            let actual_order: Vec<_> = order
                .iter()
                .filter(|key| shared.contains(*key))
                .cloned()
                .collect();
            let order_path = join(path, "@order");
            if actual_order != source_order {
                conflicts.push(Conflict {
                    path: order_path,
                    expected: Some(serde_json::json!(source_order)),
                    actual: Some(serde_json::json!(actual_order)),
                });
            } else {
                changes.push(Change {
                    path: order_path,
                    before: Some(serde_json::json!(source_order)),
                    after: Some(serde_json::json!(desired_order)),
                });
                let mut desired = desired_order.into_iter();
                for key in &mut order {
                    if shared.contains(key) {
                        *key = desired.next().expect("same shared keys");
                    }
                }
            }
        }
        return Some(Value::Array(
            order
                .into_iter()
                .filter_map(|key| merged.remove(&key))
                .collect(),
        ));
    }
    if head == from {
        changes.push(Change {
            path: if path.is_empty() {
                "/".into()
            } else {
                path.into()
            },
            before: head.cloned(),
            after: to.cloned(),
        });
        to.cloned()
    } else {
        conflicts.push(Conflict {
            path: if path.is_empty() {
                "/".into()
            } else {
                path.into()
            },
            expected: from.cloned(),
            actual: head.cloned(),
        });
        head.cloned()
    }
}

fn collection_key(path: &str) -> Option<&'static str> {
    match path.rsplit('/').next()? {
        "assets" | "sequences" | "tracks" | "clips" | "caption_cues" | "keyframes" | "shapes"
        | "effects" | "masks" | "transitions" | "keyframe_curves" => Some("id"),
        "visual" => Some("sequence_id"),
        "channels" => Some("parameter"),
        _ => None,
    }
}

fn keyed<'a>(values: &'a [Value], field: &str) -> Option<BTreeMap<String, &'a Value>> {
    let mut result = BTreeMap::new();
    for value in values {
        let key = object_key(value, field)?.to_owned();
        if result.insert(key, value).is_some() {
            return None;
        }
    }
    Some(result)
}

fn object_key<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.as_object()?.get(field)?.as_str()
}

fn opaque(path: &str) -> bool {
    // Parameter values and asset metadata are arbitrary JSON; only the named
    // parameter or metadata entry is a stable merge unit.
    let parts: Vec<_> = path.split('/').collect();
    if parts.last() == Some(&"value") {
        return true;
    }
    parts
        .iter()
        .rev()
        .nth(1)
        .is_some_and(|parent| matches!(*parent, "params" | "metadata" | "value"))
}

fn join(path: &str, key: &str) -> String {
    format!("{}/{}", path, key.replace('~', "~0").replace('/', "~1"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn document() -> ProjectDocument {
        serde_json::from_value(json!({
            "id":"p","name":"Project","schema_version":2,"default_sequence_id":"main",
            "assets":[],"sequences":[{"id":"main","name":"Main","width":640,"height":360,
                "fps":{"numerator":25,"denominator":1},"caption_cues":[],
                "tracks":[{"id":"voice","name":"Voice","kind":"audio","enabled":true,
                    "muted":false,"locked":false,"params":{"volume":1.0},"clips":[]}]}]
        }))
        .unwrap()
    }

    #[test]
    fn reverts_one_track_property_and_preserves_later_caption() {
        let before = document();
        let mut after = before.clone();
        after.sequences[0].tracks[0]
            .params
            .insert("volume".into(), json!(0.4));
        let mut head = after.clone();
        head.sequences[0].caption_cues.push(
            serde_json::from_value(json!({
                "id":"human-caption","timeline_range":{"start":{"ticks":"0","timebase":25},
                    "duration":{"ticks":"25","timebase":25}},"text":"Human text",
                "source_anchor":null,"style":{"font_family":"Arial","font_size":24,
                    "color":"#ffffff","background_color":null,"max_width":600,
                    "line_height":28,"text_align":"center"}
            }))
            .unwrap(),
        );
        let undone = selective_merge(&after, &before, &head);
        assert!(undone.conflicts.is_empty(), "{:?}", undone.conflicts);
        let project = undone.project.unwrap();
        assert_eq!(project.sequences[0].tracks[0].params["volume"], json!(1.0));
        assert_eq!(
            project.sequences[0].caption_cues,
            head.sequences[0].caption_cues
        );
        let redone = selective_merge(&before, &after, &project);
        assert!(redone.conflicts.is_empty());
        assert_eq!(
            redone.project.unwrap().sequences[0].tracks[0].params["volume"],
            json!(0.4)
        );
    }

    #[test]
    fn reports_same_property_and_created_object_dependencies() {
        let before = document();
        let mut after = before.clone();
        after.sequences[0].tracks[0]
            .params
            .insert("volume".into(), json!(0.4));
        let mut head = after.clone();
        head.sequences[0].tracks[0]
            .params
            .insert("volume".into(), json!(0.8));
        let conflict = selective_merge(&after, &before, &head);
        assert!(conflict.project.is_none());
        assert_eq!(
            conflict.conflicts[0].path,
            "/sequences/main/tracks/voice/params/volume"
        );

        let mut created = before.clone();
        let mut track = created.sequences[0].tracks[0].clone();
        track.id = "agent-track".into();
        created.sequences[0].tracks.push(track);
        let mut dependent = created.clone();
        dependent.sequences[0].tracks[1].name = "Human renamed".into();
        let conflict = selective_merge(&created, &before, &dependent);
        assert!(conflict.project.is_none());
        assert_eq!(
            conflict.conflicts[0].path,
            "/sequences/main/tracks/agent-track"
        );
    }

    #[test]
    fn reverses_pure_track_order_and_preserves_later_entities() {
        let mut before = document();
        for name in ["graphics", "captions"] {
            let mut track = before.sequences[0].tracks[0].clone();
            track.id = name.into();
            track.name = name.into();
            before.sequences[0].tracks.push(track);
        }
        let mut after = before.clone();
        after.sequences[0].tracks.swap(0, 1);
        let mut head = after.clone();
        let mut later = before.sequences[0].tracks[0].clone();
        later.id = "later".into();
        later.name = "Later".into();
        head.sequences[0].tracks.insert(1, later);
        head.sequences[0].tracks[0].muted = true;
        let undone = selective_merge(&after, &before, &head);
        assert!(undone.conflicts.is_empty(), "{:?}", undone.conflicts);
        assert!(
            undone
                .changes
                .iter()
                .any(|change| change.path == "/sequences/main/tracks/@order")
        );
        let tracks = &undone.project.unwrap().sequences[0].tracks;
        assert_eq!(
            tracks
                .iter()
                .map(|track| track.id.as_str())
                .collect::<Vec<_>>(),
            vec!["voice", "later", "graphics", "captions"]
        );
        assert!(
            tracks
                .iter()
                .find(|track| track.id == "graphics")
                .unwrap()
                .muted
        );

        let mut changed_order = head;
        changed_order.sequences[0].tracks.swap(0, 3);
        let conflict = selective_merge(&after, &before, &changed_order);
        assert!(conflict.project.is_none());
        assert!(
            conflict
                .conflicts
                .iter()
                .any(|item| item.path == "/sequences/main/tracks/@order")
        );
    }

    #[test]
    fn v3_undo_preserves_later_independent_shape_and_detects_same_property() {
        let base = document();
        let before = ProjectDocumentV3 {
            schema_version: 3,
            base: base.clone(),
            visual: vec![],
            keyframe_curves: vec![],
        };
        let mut after = before.clone();
        after.base.sequences[0].tracks[0]
            .params
            .insert("volume".into(), json!(0.4));
        let mut head = after.clone();
        head.visual.push(
            serde_json::from_value(json!({"sequence_id":"main","shapes":[{
            "id":"card","track_id":"voice","timeline_range":{"start":{"ticks":"0","timebase":25},
            "duration":{"ticks":"25","timebase":25}},"primitive":{"kind":"rectangle",
            "width_px":100.0,"height_px":50.0},"fill":{"red":255,"green":0,"blue":0,"alpha":255},
            "position_x":0.0,"position_y":0.0,"rotation_degrees":0.0,"opacity":1.0}],
            "effects":[],"masks":[],"transitions":[]}))
            .unwrap(),
        );
        let undone = selective_merge_v3(&after, &before, &head);
        assert!(undone.conflicts.is_empty(), "{:?}", undone.conflicts);
        let result = undone.project.unwrap();
        assert_eq!(
            result.base.sequences[0].tracks[0].params["volume"],
            json!(1.0)
        );
        assert_eq!(result.visual, head.visual);
        head.base.sequences[0].tracks[0]
            .params
            .insert("volume".into(), json!(0.8));
        let conflict = selective_merge_v3(&after, &before, &head);
        assert!(conflict.project.is_none());
        assert_eq!(
            conflict.conflicts[0].path,
            "/base/sequences/main/tracks/voice/params/volume"
        );
    }

    #[test]
    fn v3_undo_first_visual_node_preserves_later_shape_in_same_sequence() {
        let before = ProjectDocumentV3 {
            schema_version: 3,
            base: document(),
            visual: vec![],
            keyframe_curves: vec![],
        };
        let mut after = before.clone();
        after.visual.push(
            serde_json::from_value(json!({"sequence_id":"main","shapes":[{
            "id":"first","track_id":"voice","timeline_range":{"start":{"ticks":"0","timebase":25},
            "duration":{"ticks":"25","timebase":25}},"primitive":{"kind":"rectangle",
            "width_px":100.0,"height_px":50.0},"fill":{"red":255,"green":0,"blue":0,"alpha":255},
            "position_x":0.0,"position_y":0.0,"rotation_degrees":0.0,"opacity":1.0}],
            "effects":[],"masks":[],"transitions":[]}))
            .unwrap(),
        );
        let mut head = after.clone();
        let mut later = head.visual[0].shapes[0].clone();
        later.id = "later".into();
        head.visual[0].shapes.push(later);
        let undone = selective_merge_v3(&after, &before, &head);
        assert!(undone.conflicts.is_empty(), "{:?}", undone.conflicts);
        assert_eq!(
            undone.project.unwrap().visual[0]
                .shapes
                .iter()
                .map(|shape| shape.id.as_str())
                .collect::<Vec<_>>(),
            ["later"]
        );
    }

    #[test]
    fn v3_undo_of_one_keyframe_curve_preserves_an_independent_curve() {
        let base = document();
        let before = ProjectDocumentV3 {
            schema_version: 3,
            base: base.clone(),
            visual: vec![],
            keyframe_curves: vec![],
        };
        let curve = |id: &str, keyframe_id: &str| {
            serde_json::json!({
                "id":id,"schema_version":1,"sequence_id":"main","clip_id":"clip",
                "parameter":"opacity","keyframe_id":keyframe_id,
                "curve":{"x1":0.0,"y1":0.0,"x2":1.0,"y2":1.0}
            })
        };
        let first: opencut_contracts::project_v3::KeyframeCurveV1 =
            serde_json::from_value(curve("curve-a", "key-a")).unwrap();
        let second: opencut_contracts::project_v3::KeyframeCurveV1 =
            serde_json::from_value(curve("curve-b", "key-b")).unwrap();
        let mut after = before.clone();
        after.keyframe_curves = vec![first.clone()];
        let mut head = after.clone();
        head.keyframe_curves.push(second.clone());

        let undo = selective_merge_v3(&after, &before, &head);
        assert!(undo.conflicts.is_empty(), "{:?}", undo.conflicts);
        assert_eq!(undo.project.unwrap().keyframe_curves, [second]);

        let mut conflicting_head = after.clone();
        conflicting_head.keyframe_curves[0].curve.y1 = 0.25;
        let conflict = selective_merge_v3(&after, &before, &conflicting_head);
        assert_eq!(conflict.conflicts.len(), 1);
        assert!(conflict.conflicts[0].path.contains("curve-a"));
    }
}
