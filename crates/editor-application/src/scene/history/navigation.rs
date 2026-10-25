use super::*;
pub fn navigate(
    store: &impl ProjectHistoryRead,
    request: &HistoryNavigationInput,
) -> Result<HistoryNavigation, EditorError> {
    let source = store.read_scene_history(&request.project_id, request.revision.as_deref())?;
    if source.source.document.project_id() != request.project_id {
        return Err(invalid("Navigation project scope differs"));
    }
    if !matches!(source.source.document, StoredProjectDocument::Editable(_)) {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "V4 navigation requires a V4 revision",
        ));
    }
    active_undos(store, &source)?;
    let mut navigation = Navigation::default();
    for entry in source.entries {
        match entry.kind {
            HistoryEntryKind::Edit => navigation.push(HistoryStep::Edit(UndoTarget {
                transaction_id: entry.transaction_id,
                label: entry.label,
            })),
            HistoryEntryKind::Promotion => {}
            HistoryEntryKind::Undo {
                target_transaction_id,
            } => navigation.push(HistoryStep::Undo {
                transaction_id: entry.transaction_id,
                target: target_transaction_id,
            }),
            HistoryEntryKind::Redo {
                target_transaction_id,
                undo_transaction_id,
            } => navigation.push(HistoryStep::Redo {
                undo_transaction_id,
                target: target_transaction_id,
            }),
        }
    }
    let (undo, redo) = navigation.targets();
    Ok(HistoryNavigation {
        project_id: request.project_id.clone(),
        revision: source.source.revision,
        undo: undo.map(|u| UndoTarget {
            transaction_id: u.transaction_id,
            label: u.label,
        }),
        redo: redo.map(|r| RedoTarget {
            transaction_id: r.transaction_id,
            undo_transaction_id: r.undo_transaction_id,
            label: r.label,
        }),
    })
}
