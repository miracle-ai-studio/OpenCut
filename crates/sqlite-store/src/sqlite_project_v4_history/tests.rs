use super::*;
use opencut_contracts::{Asset, AssetKind, FrameRate, Time, project_v4::*, scene_bookmarks::*};
use opencut_editor_application::access::{Caller, PrincipalPolicy, ProjectGrant};
use std::collections::BTreeMap;
mod authority;
mod selective;
fn create(app: &mut EditorApplication) -> ProjectSnapshotV4 {
    app.create_v4_project(&CreateV4Project {
        project: CreateProject {
            id: "p".into(),
            name: "Original".into(),
            sequence_id: "main".into(),
            width: 64,
            height: 48,
            fps: FrameRate {
                numerator: 25,
                denominator: 1,
            },
        },
        idempotency_key: "create".into(),
    })
    .unwrap()
}
fn inspect(app: &EditorApplication) -> ProjectSnapshotV4 {
    app.inspect_v4_project(&InspectV4Project {
        project_id: "p".into(),
        revision: None,
    })
    .unwrap()
}
fn edit(
    app: &mut EditorApplication,
    key: &str,
    operations: Vec<ProjectOperationV4>,
) -> TransactionResult {
    app.apply_v4_project(&ApplyV4Project {
        project_id: "p".into(),
        expected_revision: inspect(app).revision,
        idempotency_key: key.into(),
        label: key.into(),
        operations,
    })
    .unwrap()
}
fn marker(id: &str) -> ProjectOperationV4 {
    ProjectOperationV4::Bookmark {
        operation: BookmarkOperationV1::Put {
            bookmark: SceneBookmarkV1 {
                id: id.into(),
                sequence_id: "main".into(),
                time: Time {
                    ticks: "1".into(),
                    timebase: 1,
                },
                duration: None,
                color: None,
                note: None,
            },
        },
    }
}
fn undo(app: &EditorApplication, target: &str, key: &str) -> UndoV4Project {
    UndoV4Project {
        project_id: "p".into(),
        expected_revision: inspect(app).revision,
        target_transaction_id: target.into(),
        idempotency_key: key.into(),
    }
}
fn redo(app: &EditorApplication, target: &str, key: &str) -> RedoV4Project {
    RedoV4Project {
        project_id: "p".into(),
        expected_revision: inspect(app).revision,
        undo_transaction_id: target.into(),
        idempotency_key: key.into(),
    }
}
fn counts(app: &EditorApplication) -> Vec<i64> {
    [
        "projects",
        "revisions",
        "editor_transactions",
        "idempotency",
        "outbox",
        "audit_records",
    ]
    .iter()
    .map(|table| {
        app.connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    })
    .collect()
}
fn navigate(app: &EditorApplication) -> HistoryNavigationV4 {
    app.v4_history_navigation(&HistoryNavigationInputV4 {
        project_id: "p".into(),
        revision: None,
    })
    .unwrap()
}
fn raw(app: &EditorApplication, revision: &str) -> String {
    app.connection
        .query_row(
            "SELECT document_json FROM revisions WHERE revision=?1",
            [revision],
            |row| row.get(0),
        )
        .unwrap()
}
