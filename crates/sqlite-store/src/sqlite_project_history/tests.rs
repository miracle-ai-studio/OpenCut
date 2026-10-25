use super::*;
use opencut_contracts::{Asset, AssetKind, FrameRate, Time, scene::*, scene_bookmarks::*};
use opencut_editor_application::access::{Caller, PrincipalPolicy, ProjectGrant};
use std::collections::BTreeMap;
mod authority;
mod selective;
fn create(app: &mut EditorApplication) -> EditorSnapshot {
    app.create_scene_project(&CreateProjectRequest {
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
fn inspect(app: &EditorApplication) -> EditorSnapshot {
    app.inspect_scene_project(&InspectProject {
        project_id: "p".into(),
        revision: None,
    })
    .unwrap()
}
fn edit(
    app: &mut EditorApplication,
    key: &str,
    operations: Vec<ProjectOperation>,
) -> TransactionResult {
    app.apply_scene_project(&ApplySceneProject {
        project_id: "p".into(),
        expected_revision: inspect(app).revision,
        idempotency_key: key.into(),
        label: key.into(),
        operations,
    })
    .unwrap()
}
fn marker(id: &str) -> ProjectOperation {
    ProjectOperation::Bookmark {
        operation: BookmarkOperation::Put {
            bookmark: SceneBookmark {
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
fn undo(app: &EditorApplication, target: &str, key: &str) -> UndoRequest {
    UndoRequest {
        project_id: "p".into(),
        expected_revision: inspect(app).revision,
        target_transaction_id: target.into(),
        idempotency_key: key.into(),
    }
}
fn redo(app: &EditorApplication, target: &str, key: &str) -> RedoRequest {
    RedoRequest {
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
fn navigate(app: &EditorApplication) -> HistoryNavigation {
    app.editor_history_navigation(&HistoryNavigationInput {
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
