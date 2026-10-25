//! Fixed-revision bookmark planning. A host must persist the whole annotated
//! aggregate atomically; these prepared values are in-process, not public DTOs.
use opencut_contracts::{
    EditorError,
    historical_visual::HistoricalVisualSnapshot,
    scene_bookmarks::{BookmarkOperation, SceneBookmark},
};

/// Supplied by the future annotated-project storage port, from one revision.
/// Never combine a current bookmark collection with an older project snapshot.
#[derive(Clone, Debug)]
pub struct BookmarkPlanningSnapshot {
    pub project: HistoricalVisualSnapshot,
    pub bookmarks: Vec<SceneBookmark>,
}

#[derive(Clone, Debug)]
pub struct BookmarkEditIntent {
    pub project_id: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub label: String,
    pub operations: Vec<BookmarkOperation>,
}

#[derive(Clone, Debug)]
pub struct PreparedBookmarkEdit {
    pub project: opencut_contracts::historical_visual::HistoricalVisualDocument,
    pub base_revision: String,
    pub idempotency_key: String,
    pub label: String,
    pub before: Vec<SceneBookmark>,
    pub proposed: Vec<SceneBookmark>,
}

pub fn plan(
    snapshot: &BookmarkPlanningSnapshot,
    intent: &BookmarkEditIntent,
) -> Result<PreparedBookmarkEdit, EditorError> {
    if intent.project_id != snapshot.project.project.base.id {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Bookmark project scope differs",
        ));
    }
    if intent.expected_revision != snapshot.project.revision {
        return Err(EditorError::new(
            "REVISION_CONFLICT",
            "Bookmark revision differs",
        ));
    }
    if intent.idempotency_key.trim().is_empty() || intent.label.trim().is_empty() {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Bookmark edits require a key and label",
        ));
    }
    opencut_editor_core::historical_visual::validate_project_historical_visual(
        &snapshot.project.project,
    )?;
    let proposed = opencut_editor_core::scene_bookmarks::apply(
        &snapshot.project.project.base,
        &snapshot.bookmarks,
        &intent.operations,
    )?;
    Ok(PreparedBookmarkEdit {
        project: snapshot.project.project.clone(),
        base_revision: snapshot.project.revision.clone(),
        idempotency_key: intent.idempotency_key.clone(),
        label: intent.label.clone(),
        before: snapshot.bookmarks.clone(),
        proposed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{CreateProject, FrameRate, Time};

    fn input() -> (BookmarkPlanningSnapshot, BookmarkEditIntent) {
        let project = crate::historical_visual::create(&CreateProject {
            id: "project".into(),
            name: "P".into(),
            sequence_id: "main".into(),
            width: 320,
            height: 180,
            fps: FrameRate {
                numerator: 24,
                denominator: 1,
            },
        })
        .unwrap();
        (
            BookmarkPlanningSnapshot {
                project: HistoricalVisualSnapshot {
                    project,
                    revision: "fixed".into(),
                },
                bookmarks: Vec::new(),
            },
            BookmarkEditIntent {
                project_id: "project".into(),
                expected_revision: "fixed".into(),
                idempotency_key: "stable-key".into(),
                label: "Add bookmark".into(),
                operations: vec![BookmarkOperation::Toggle {
                    sequence_id: "main".into(),
                    time: Time {
                        ticks: "1".into(),
                        timebase: 48,
                    },
                    bookmark_id: "bookmark".into(),
                }],
            },
        )
    }
    #[test]
    fn prepared_edit_keeps_complete_project_revision_key_and_original_collection() {
        let (snapshot, intent) = input();
        let prepared = plan(&snapshot, &intent).unwrap();
        assert_eq!(prepared.project, snapshot.project.project);
        assert_eq!(prepared.base_revision, "fixed");
        assert_eq!(prepared.idempotency_key, "stable-key");
        assert_eq!(prepared.label, "Add bookmark");
        assert!(prepared.before.is_empty());
        assert!(snapshot.bookmarks.is_empty());
        assert_eq!(
            prepared.proposed[0].time,
            Time {
                ticks: "1".into(),
                timebase: 24
            }
        );
        assert_eq!(
            plan(&snapshot, &intent).unwrap().proposed,
            prepared.proposed
        );
    }
    #[test]
    fn rejects_other_project_stale_revision_and_empty_replay_key() {
        let (snapshot, intent) = input();
        let mut wrong = intent.clone();
        wrong.project_id = "other".into();
        assert_eq!(
            plan(&snapshot, &wrong).unwrap_err().code,
            "INVALID_ARGUMENT"
        );
        wrong = intent.clone();
        wrong.expected_revision = "stale".into();
        assert_eq!(
            plan(&snapshot, &wrong).unwrap_err().code,
            "REVISION_CONFLICT"
        );
        wrong = intent;
        wrong.idempotency_key = " \n ".into();
        assert_eq!(
            plan(&snapshot, &wrong).unwrap_err().code,
            "INVALID_ARGUMENT"
        );
    }
}
