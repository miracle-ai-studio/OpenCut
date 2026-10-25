use super::*;
use opencut_contracts::{Asset, AssetKind, AssetVersion, FrameRate, scene_bookmarks::*};
use opencut_editor_application::access::{Caller, PrincipalPolicy, ProjectGrant};
use std::collections::BTreeMap;

mod authority;
mod persistence;
mod resources;

fn create_request(id: &str) -> CreateProjectRequest {
    CreateProjectRequest {
        project: CreateProject {
            id: id.into(),
            name: id.into(),
            sequence_id: "main".into(),
            width: 64,
            height: 48,
            fps: FrameRate {
                numerator: 25,
                denominator: 1,
            },
        },
        idempotency_key: format!("create-{id}"),
    }
}
fn inspect(app: &EditorApplication, id: &str) -> EditorSnapshot {
    app.inspect_scene_project(&InspectProject {
        project_id: id.into(),
        revision: None,
    })
    .unwrap()
}
fn edit(
    snapshot: &EditorSnapshot,
    key: &str,
    operations: Vec<ProjectOperation>,
) -> ApplySceneProject {
    ApplySceneProject {
        project_id: snapshot.project.id.clone(),
        expected_revision: snapshot.revision.clone(),
        idempotency_key: key.into(),
        label: key.into(),
        operations,
    }
}
fn bookmark(id: &str) -> ProjectOperation {
    ProjectOperation::Bookmark {
        operation: BookmarkOperation::Put {
            bookmark: SceneBookmark {
                id: id.into(),
                sequence_id: "main".into(),
                time: opencut_contracts::Time {
                    ticks: "3003".into(),
                    timebase: 3000,
                },
                duration: None,
                color: Some("#12AB34".into()),
                note: Some("Stored with the complete revision".into()),
            },
        },
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
        "project_media_bindings",
    ]
    .into_iter()
    .map(|table| {
        app.connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    })
    .collect()
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
fn capture_asset(
    app: &EditorApplication,
    directory: &std::path::Path,
    id: &str,
    bytes: &[u8],
) -> ProjectAsset {
    let path = directory.join(format!("{id}.bin"));
    std::fs::write(&path, bytes).unwrap();
    let legacy = Asset {
        id: id.into(),
        name: id.into(),
        kind: AssetKind::Image,
        uri: path.to_string_lossy().into(),
        duration: None,
        version: None,
        metadata: BTreeMap::new(),
    };
    let pinned = AssetStore::open(&app.asset_store_root)
        .unwrap()
        .pin_asset(&legacy)
        .unwrap();
    ProjectAsset {
        id: id.into(),
        name: id.into(),
        kind: AssetKind::Image,
        version: pinned.version.unwrap(),
        duration: None,
        origin: ResourceOrigin::LocalImport {
            original_name: format!("{id}.bin"),
        },
        metadata: ProjectMediaMetadata {
            mime_type: Some("application/octet-stream".into()),
            intrinsic_size: None,
            frame_rate: None,
            rotation_degrees: None,
            audio: AudioPresence::Absent {},
        },
        license: None,
    }
}
fn credential(app: &mut EditorApplication, project: &str) -> (Caller, String) {
    app.set_principal_policy(PrincipalPolicy {
        principal_id: "agent-v4".into(),
        expected_revision: "0".into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: project.into(),
            actions: vec![
                ProjectAction::Read,
                ProjectAction::Edit,
                ProjectAction::Restore,
            ],
        }],
    })
    .unwrap();
    let issued = app.issue_credential("agent-v4").unwrap();
    (
        app.authenticate(&issued.secret).unwrap(),
        issued.credential_id,
    )
}

/// Capture a real application-prepared payload to exercise a race after its
/// initial authorization, without adding a public constructor or test-only port.
struct Capture<'a> {
    app: &'a mut EditorApplication,
    prepared: Option<PreparedSceneCommit>,
}
impl SceneRead for Capture<'_> {
    fn authorize_project(&self, intent: SceneCommitIntent<'_>) -> Result<(), EditorError> {
        self.app.authorize_project(intent)
    }
    fn head_scene_source(&self, id: &str) -> Result<StoredProjectRevision, EditorError> {
        self.app.head_scene_source(id)
    }
    fn revision_scene_source(
        &self,
        id: &str,
        revision: &str,
    ) -> Result<StoredProjectRevision, EditorError> {
        self.app.revision_scene_source(id, revision)
    }
    fn replay_scene_create(
        &self,
        input: &CreateProjectRequest,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        self.app.replay_scene_create(input)
    }
    fn replay_scene_copy(
        &self,
        input: &CopySceneProject,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        self.app.replay_scene_copy(input)
    }
    fn replay_scene_edit(
        &self,
        intent: SceneCommitIntent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        self.app.replay_scene_edit(intent)
    }
}
impl SceneStore for Capture<'_> {
    fn create_scene_atomic(
        &mut self,
        input: &CreateProjectRequest,
        prepared: PreparedCreate,
    ) -> Result<EditorSnapshot, EditorError> {
        self.app.create_scene_atomic(input, prepared)
    }
    fn copy_scene_atomic(
        &mut self,
        input: &CopySceneProject,
        prepared: PreparedCreate,
    ) -> Result<EditorSnapshot, EditorError> {
        self.app.copy_scene_atomic(input, prepared)
    }
    fn commit_scene_atomic(
        &mut self,
        _: SceneCommitIntent<'_>,
        prepared: PreparedSceneCommit,
    ) -> Result<TransactionResult, EditorError> {
        self.prepared = Some(prepared);
        Err(EditorError::new(
            "CAPTURED",
            "Application payload captured before native publication",
        ))
    }
}
fn prepared(app: &mut EditorApplication, request: &ApplySceneProject) -> PreparedSceneCommit {
    let mut capture = Capture {
        app,
        prepared: None,
    };
    assert_eq!(
        scene::apply(&mut capture, request).unwrap_err().code,
        "CAPTURED"
    );
    capture.prepared.unwrap()
}
