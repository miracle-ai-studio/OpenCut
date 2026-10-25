//! Selective three-way inversion over stable project entity IDs and properties.
use opencut_contracts::{
    ProjectDocument,
    historical_visual::{HistoricalVisualDocument, HistoricalVisualSequenceVisual},
    scene::EditorDocument,
};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
mod profile;
use profile::{MergeProfile, collection_key, same_variant};

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
pub struct HistoricalVisualMerge {
    pub project: Option<HistoricalVisualDocument>,
    pub changes: Vec<Change>,
    pub conflicts: Vec<Conflict>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneMerge {
    pub project: Option<EditorDocument>,
    pub changes: Vec<Change>,
    pub conflicts: Vec<Conflict>,
}

/// Same stable-identity merge engine, using the complete V4 aggregate and its
/// own collection identities. No legacy projection participates in inversion.
pub fn selective_merge_scene(
    from: &EditorDocument,
    to: &EditorDocument,
    head: &EditorDocument,
) -> SceneMerge {
    let (result, changes, mut conflicts) = merge_values(
        serde_json::to_value(from).expect("schema 4 project serializes"),
        serde_json::to_value(to).expect("schema 4 project serializes"),
        serde_json::to_value(head).expect("schema 4 project serializes"),
        MergeProfile::V4,
    );
    let project = result.and_then(|value| serde_json::from_value(value).ok());
    if project.is_none() && conflicts.is_empty() {
        conflicts.push(Conflict {
            path: "/".into(),
            expected: None,
            actual: None,
        });
    }
    SceneMerge {
        project,
        changes,
        conflicts,
    }
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
        MergeProfile::Legacy,
    );
    let project = result.and_then(|value| serde_json::from_value(value).ok());
    Merge {
        project,
        changes,
        conflicts,
    }
}

pub fn selective_merge_historical_visual(
    from: &HistoricalVisualDocument,
    to: &HistoricalVisualDocument,
    head: &HistoricalVisualDocument,
) -> HistoricalVisualMerge {
    let (result, changes, conflicts) = merge_values(
        serde_json::to_value(normalize_historical_visual(from))
            .expect("schema 3 project serializes"),
        serde_json::to_value(normalize_historical_visual(to)).expect("schema 3 project serializes"),
        serde_json::to_value(normalize_historical_visual(head))
            .expect("schema 3 project serializes"),
        MergeProfile::Legacy,
    );
    let mut project: Option<HistoricalVisualDocument> =
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
            let present = |document: &HistoricalVisualDocument| {
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
    HistoricalVisualMerge {
        project,
        changes,
        conflicts,
    }
}

fn normalize_historical_visual(document: &HistoricalVisualDocument) -> HistoricalVisualDocument {
    let mut result = document.clone();
    for sequence in &document.base.sequences {
        if !result
            .visual
            .iter()
            .any(|visual| visual.sequence_id == sequence.id)
        {
            result.visual.push(HistoricalVisualSequenceVisual {
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
    profile: MergeProfile,
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
        profile,
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
    profile: MergeProfile,
) -> Option<Value> {
    if from == to {
        return head.cloned();
    }
    if let (Some(Value::Object(from)), Some(Value::Object(to)), Some(Value::Object(head))) =
        (from, to, head)
        && !opaque(path)
        && same_variant(profile, from, to, head)
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
                profile,
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
        && let Some(key_field) = collection_key(profile, path)
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
                profile,
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
mod tests;
