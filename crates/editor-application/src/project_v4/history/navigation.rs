use super::*;
pub fn navigate(
    store: &impl ProjectV4HistoryRead,
    request: &HistoryNavigationInputV4,
) -> Result<HistoryNavigationV4, EditorError> {
    let source = store.read_v4_history(&request.project_id, request.revision.as_deref())?;
    if source.source.document.project_id() != request.project_id {
        return Err(invalid("Navigation project scope differs"));
    }
    if !matches!(source.source.document, StoredProjectDocumentV4::V4(_)) {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "V4 navigation requires a V4 revision",
        ));
    }
    active_undos(&source.entries)?;
    let mut navigation = Navigation::default();
    for entry in source.entries {
        match entry.kind {
            HistoryEntryKindV4::Edit => navigation.push(HistoryStep::Edit(UndoTarget {
                transaction_id: entry.transaction_id,
                label: entry.label,
            })),
            HistoryEntryKindV4::Promotion => {}
            HistoryEntryKindV4::Undo {
                target_transaction_id,
            } => navigation.push(HistoryStep::Undo {
                transaction_id: entry.transaction_id,
                target: target_transaction_id,
            }),
            HistoryEntryKindV4::Redo {
                target_transaction_id,
                undo_transaction_id,
            } => navigation.push(HistoryStep::Redo {
                undo_transaction_id,
                target: target_transaction_id,
            }),
        }
    }
    let (undo, redo) = navigation.targets();
    Ok(HistoryNavigationV4 {
        project_id: request.project_id.clone(),
        revision: source.source.revision,
        undo: undo.map(|u| HistoryUndoTargetV4 {
            transaction_id: u.transaction_id,
            label: u.label,
        }),
        redo: redo.map(|r| HistoryRedoTargetV4 {
            transaction_id: r.transaction_id,
            undo_transaction_id: r.undo_transaction_id,
            label: r.label,
        }),
    })
}
