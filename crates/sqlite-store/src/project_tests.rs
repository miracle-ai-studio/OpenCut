use crate::EditorApplication;
use opencut_api_contracts::RestoreRevisionRequest;
use opencut_contracts::*;
use opencut_editor_application::projects::*;
use serde_json::json;
use std::cell::RefCell;

// Real SQLite adapter, with one other connection committing precisely between
// the optimistic receipt read and the next head read. No timing sleeps.
struct InterleavedStore {
    db: crate::SqliteConnection,
    before_head: RefCell<Option<Box<dyn FnOnce()>>>,
}
impl ProjectRead for InterleavedStore {
    fn authorize_edit(&self, project_id: &str, intent: EditIntent<'_>) -> Result<(), EditorError> {
        self.db.authorize_edit(project_id, intent)
    }
    fn head(&self, id: &str) -> Result<ProjectSnapshot, EditorError> {
        if let Some(action) = self.before_head.borrow_mut().take() {
            action();
        }
        self.db.head(id)
    }
    fn revision(&self, id: &str, rev: &str) -> Result<ProjectSnapshot, EditorError> {
        self.db.revision(id, rev)
    }
    fn replay_create(
        &self,
        input: &CreateProject,
        key: &str,
    ) -> Result<Option<ProjectSnapshot>, EditorError> {
        self.db.replay_create(input, key)
    }
    fn replay_edit(
        &self,
        id: &str,
        intent: EditIntent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        self.db.replay_edit(id, intent)
    }
}
impl ProjectStore for InterleavedStore {
    fn create_atomic(
        &mut self,
        input: &CreateProject,
        key: &str,
        snapshot: ProjectSnapshot,
    ) -> Result<ProjectSnapshot, EditorError> {
        self.db.create_atomic(input, key, snapshot)
    }
    fn commit_atomic(
        &mut self,
        intent: EditIntent<'_>,
        prepared: PreparedCommit,
    ) -> Result<TransactionResult, EditorError> {
        self.db.commit_atomic(intent, prepared)
    }
}
struct NoAssets;
impl AssetPinning for NoAssets {
    fn pin(&mut self, _: &Asset) -> Result<Asset, EditorError> {
        panic!("no asset preparation allowed on replay")
    }
}
fn input() -> CreateProject {
    CreateProject {
        id: "p".into(),
        name: "Project".into(),
        sequence_id: "main".into(),
        width: 320,
        height: 180,
        fps: opencut_contracts::FrameRate {
            numerator: 24,
            denominator: 1,
        },
    }
}
fn rename(rev: &str, key: &str) -> TransactionRequest {
    TransactionRequest {
        expected_revision: rev.into(),
        idempotency_key: key.into(),
        label: key.into(),
        operations: vec![Operation::UpdateSequenceSettings {
            sequence_id: "main".into(),
            name: Some(key.into()),
            width: None,
            height: None,
            fps: None,
        }],
    }
}
#[test]
fn create_rechecks_receipt_when_the_project_appears_after_the_initial_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite");
    let reader = EditorApplication::open(&path).unwrap();
    let mut writer = EditorApplication::open(&path).unwrap();
    let mut store = InterleavedStore {
        db: reader.connection,
        before_head: RefCell::new(Some(Box::new(move || {
            let snapshot = writer.create_project(input(), "create").unwrap();
            writer
                .apply("p", &rename(&snapshot.revision, "later"))
                .unwrap();
        }))),
    };
    let original = create(&mut store, input(), "create").unwrap();
    assert_ne!(original.revision, store.head("p").unwrap().revision);
    assert_eq!(original.project.sequences[0].name, "Main");
    assert_eq!(
        store.db.replay_create(&input(), "create").unwrap().unwrap(),
        original
    );
}
#[test]
fn apply_rechecks_original_receipt_before_reporting_a_concurrent_revision_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite");
    let mut writer = EditorApplication::open(&path).unwrap();
    let first = writer.create_project(input(), "create").unwrap();
    let request = rename(&first.revision, "wanted");
    let other = request.clone();
    let reader = EditorApplication::open(&path).unwrap();
    let mut store = InterleavedStore {
        db: reader.connection,
        before_head: RefCell::new(Some(Box::new(move || {
            let committed = writer.apply("p", &other).unwrap();
            writer
                .apply("p", &rename(&committed.revision, "later"))
                .unwrap();
        }))),
    };
    let result = apply(&mut store, &mut NoAssets, "p", &request).unwrap();
    assert_ne!(result.revision, store.head("p").unwrap().revision);
    assert_eq!(
        store
            .db
            .replay_edit("p", EditIntent::Apply(&request))
            .unwrap()
            .unwrap(),
        result
    );
}
#[test]
fn restore_rechecks_original_receipt_before_reporting_a_concurrent_revision_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite");
    let mut writer = EditorApplication::open(&path).unwrap();
    let first = writer.create_project(input(), "create").unwrap();
    let edit = writer.apply("p", &rename(&first.revision, "edit")).unwrap();
    let request = RestoreRevisionRequest {
        expected_revision: edit.revision,
        target_revision: first.revision,
        idempotency_key: "restore".into(),
        label: "restore".into(),
    };
    let other = request.clone();
    let reader = EditorApplication::open(&path).unwrap();
    let mut store = InterleavedStore {
        db: reader.connection,
        before_head: RefCell::new(Some(Box::new(move || {
            let committed = writer.restore_revision("p", &other).unwrap();
            writer
                .apply("p", &rename(&committed.revision, "later"))
                .unwrap();
        }))),
    };
    let result = restore(&mut store, "p", &request).unwrap();
    assert_ne!(result.revision, store.head("p").unwrap().revision);
    assert_eq!(
        store
            .db
            .replay_edit("p", EditIntent::Restore(&request))
            .unwrap()
            .unwrap(),
        result
    );
}
#[test]
fn commit_rejects_a_head_changed_during_asset_capture_without_publishing_the_plan() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite");
    let mut app = EditorApplication::open(&path).unwrap();
    let first = app.create_project(input(), "create").unwrap();
    struct RacingPin {
        writer: EditorApplication,
        expected: String,
    }
    impl AssetPinning for RacingPin {
        fn pin(&mut self, asset: &Asset) -> Result<Asset, EditorError> {
            self.writer
                .apply("p", &rename(&self.expected, "competing-edit"))?;
            Ok(asset.clone())
        }
    }
    let mut pin = RacingPin {
        writer: EditorApplication::open(&path).unwrap(),
        expected: first.revision.clone(),
    };
    let request:TransactionRequest=serde_json::from_value(json!({
        "expected_revision":first.revision,"idempotency_key":"bind","label":"bind",
        "operations":[{"type":"asset.bind","asset":{"id":"a","name":"a","kind":"video","uri":"test-source","metadata":{}}}]
    })).unwrap();
    assert_eq!(
        apply(&mut app.connection, &mut pin, "p", &request)
            .unwrap_err()
            .code,
        "REVISION_CONFLICT"
    );
    let observed = EditorApplication::open(path).unwrap();
    assert!(observed.snapshot("p").unwrap().project.assets.is_empty());
    assert_eq!(
        observed.snapshot("p").unwrap().project.sequences[0].name,
        "competing-edit"
    );
    assert!(
        observed
            .connection
            .replay_edit("p", EditIntent::Apply(&request))
            .unwrap()
            .is_none()
    );
    assert_eq!(observed.events_after(0, 100).unwrap().len(), 2);
}
