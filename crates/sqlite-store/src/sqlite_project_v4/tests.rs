use super::*;
use opencut_contracts::{Asset, AssetKind, AssetVersion, FrameRate, scene_bookmarks::*};
use opencut_editor_application::access::{Caller, PrincipalPolicy, ProjectGrant};
use std::collections::BTreeMap;

mod authority;
mod persistence;
mod resources;

fn create_request(id: &str) -> CreateV4Project {
    CreateV4Project {
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
fn inspect(app: &EditorApplication, id: &str) -> ProjectSnapshotV4 {
    app.inspect_v4_project(&InspectV4Project {
        project_id: id.into(),
        revision: None,
    })
    .unwrap()
}
fn edit(
    snapshot: &ProjectSnapshotV4,
    key: &str,
    operations: Vec<ProjectOperationV4>,
) -> ApplyV4Project {
    ApplyV4Project {
        project_id: snapshot.project.id.clone(),
        expected_revision: snapshot.revision.clone(),
        idempotency_key: key.into(),
        label: key.into(),
        operations,
    }
}
fn bookmark(id: &str) -> ProjectOperationV4 {
    ProjectOperationV4::Bookmark {
        operation: BookmarkOperationV1::Put {
            bookmark: SceneBookmarkV1 {
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
) -> AssetV4 {
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
    AssetV4 {
        id: id.into(),
        name: id.into(),
        kind: AssetKind::Image,
        version: pinned.version.unwrap(),
        duration: None,
        origin: ResourceOriginV4::LocalImport {
            original_name: format!("{id}.bin"),
        },
        metadata: MediaMetadataV4 {
            mime_type: Some("application/octet-stream".into()),
            intrinsic_size: None,
            frame_rate: None,
            rotation_degrees: None,
            audio: AudioPresenceV4::Absent {},
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
    prepared: Option<PreparedV4Commit>,
}
impl ProjectV4Read for Capture<'_> {
    fn authorize_v4(&self, intent: V4Intent<'_>) -> Result<(), EditorError> {
        self.app.authorize_v4(intent)
    }
    fn head_v4_source(&self, id: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        self.app.head_v4_source(id)
    }
    fn revision_v4_source(
        &self,
        id: &str,
        revision: &str,
    ) -> Result<StoredProjectRevisionV4, EditorError> {
        self.app.revision_v4_source(id, revision)
    }
    fn replay_v4_create(
        &self,
        input: &CreateV4Project,
    ) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        self.app.replay_v4_create(input)
    }
    fn replay_v4_copy(
        &self,
        input: &CopyV4Project,
    ) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        self.app.replay_v4_copy(input)
    }
    fn replay_v4_edit(
        &self,
        intent: V4Intent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        self.app.replay_v4_edit(intent)
    }
}
impl ProjectV4Store for Capture<'_> {
    fn create_v4_atomic(
        &mut self,
        input: &CreateV4Project,
        prepared: PreparedV4Create,
    ) -> Result<ProjectSnapshotV4, EditorError> {
        self.app.create_v4_atomic(input, prepared)
    }
    fn copy_v4_atomic(
        &mut self,
        input: &CopyV4Project,
        prepared: PreparedV4Create,
    ) -> Result<ProjectSnapshotV4, EditorError> {
        self.app.copy_v4_atomic(input, prepared)
    }
    fn commit_v4_atomic(
        &mut self,
        _: V4Intent<'_>,
        prepared: PreparedV4Commit,
    ) -> Result<TransactionResult, EditorError> {
        self.prepared = Some(prepared);
        Err(EditorError::new(
            "CAPTURED",
            "Application payload captured before native publication",
        ))
    }
}
fn prepared(app: &mut EditorApplication, request: &ApplyV4Project) -> PreparedV4Commit {
    let mut capture = Capture {
        app,
        prepared: None,
    };
    assert_eq!(
        project_v4::apply(&mut capture, request).unwrap_err().code,
        "CAPTURED"
    );
    capture.prepared.unwrap()
}
