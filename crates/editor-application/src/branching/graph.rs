//! Original merge-DAG ancestor algorithm;callers supply newest-first nodes and scoped parent reads.
use opencut_contracts::EditorError;
use std::collections::{HashMap, VecDeque};

fn distances(
    head: &str,
    parents: &mut impl FnMut(&str) -> Result<Vec<String>, EditorError>,
) -> Result<HashMap<String, usize>, EditorError> {
    let mut seen = HashMap::new();
    let mut queue = VecDeque::from([(head.to_owned(), 0usize)]);
    while let Some((revision, distance)) = queue.pop_front() {
        if seen.contains_key(&revision) {
            continue;
        }
        if seen.len() >= 100_000 {
            return Err(EditorError::new(
                "STORAGE_ERROR",
                "Branch ancestry exceeds traversal limit",
            ));
        }
        seen.insert(revision.clone(), distance);
        for parent in parents(&revision)? {
            queue.push_back((parent, distance + 1));
        }
    }
    Ok(seen)
}

pub fn common_ancestor(
    source: &str,
    target: &str,
    newest_first: impl IntoIterator<Item = Result<String, EditorError>>,
    mut parents: impl FnMut(&str) -> Result<Vec<String>, EditorError>,
) -> Result<String, EditorError> {
    let source_ancestors = distances(source, &mut parents)?;
    let target_ancestors = distances(target, &mut parents)?;
    let common: std::collections::HashSet<_> = source_ancestors
        .keys()
        .filter(|revision| target_ancestors.contains_key(*revision))
        .cloned()
        .collect();
    if common.is_empty() {
        return Err(EditorError::new(
            "CAUSAL_CONFLICT",
            "Branches have no common project ancestor",
        ));
    }
    // A latest common ancestor is not necessarily the one with the shortest
    // summed path in a merge DAG. Walk newest revisions first and discard all
    // older common ancestors reachable from each candidate.
    let mut newest = Vec::new();
    let mut excluded = std::collections::HashSet::new();
    for row in newest_first {
        let revision = row?;
        if !common.contains(&revision) || excluded.contains(&revision) {
            continue;
        }
        newest.push(revision.clone());
        let mut pending = VecDeque::from([revision]);
        let mut visited = std::collections::HashSet::new();
        while let Some(node) = pending.pop_front() {
            if !visited.insert(node.clone()) {
                continue;
            }
            if visited.len() > 100_000 {
                return Err(EditorError::new(
                    "STORAGE_ERROR",
                    "Merge ancestry exceeds traversal limit",
                ));
            }
            for parent in parents(&node)? {
                if common.contains(&parent) {
                    excluded.insert(parent.clone());
                }
                pending.push_back(parent);
            }
        }
    }
    if newest.len() != 1 {
        return Err(EditorError::new(
            "CAUSAL_CONFLICT",
            "Branches have multiple incomparable merge bases",
        ));
    }
    Ok(newest.remove(0))
}
