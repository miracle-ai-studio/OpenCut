//! Application behavior only. SQLite tests separately establish durability,
//! resource authority, publication rollback and cross-process serialization.
use super::*;
use opencut_contracts::{
    FrameRate, Time,
    scene_bookmarks::{BookmarkOperation, SceneBookmark},
};
mod bookmarks;
mod brands;
mod store;
use store::MemoryStore;

#[test]
fn receipt_identities_exclude_arbitrary_metadata_and_cas_version_ids() {
    let mut legacy = opencut_editor_core::create_project(&input().project).unwrap();
    legacy.assets.push(opencut_contracts::Asset {
        id: "a".into(),
        name: "A".into(),
        kind: opencut_contracts::AssetKind::Image,
        uri: "asset://a".into(),
        duration: None,
        version: Some(opencut_contracts::AssetVersion {
            id: "immutable-version".into(),
            content_hash: "0".repeat(64),
            size: 1,
        }),
        metadata: BTreeMap::from([(
            "custom".into(),
            serde_json::json!({"id":"untrusted-metadata-id"}),
        )]),
    });
    let nodes = entities::collect(&StoredProjectDocument::Legacy(Box::new(legacy))).unwrap();
    assert!(nodes.contains_key("a"));
    assert!(!nodes.contains_key("immutable-version"));
    assert!(!nodes.contains_key("untrusted-metadata-id"));
}

fn input() -> CreateProjectRequest {
    CreateProjectRequest {
        project: CreateProject {
            id: "p".into(),
            name: "P".into(),
            sequence_id: "s".into(),
            width: 320,
            height: 180,
            fps: FrameRate {
                numerator: 24,
                denominator: 1,
            },
        },
        idempotency_key: "create".into(),
    }
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
        label: "Edit".into(),
        operations,
    }
}
fn time(ticks: i64, timebase: u32) -> Time {
    Time {
        ticks: ticks.to_string(),
        timebase,
    }
}
fn marker(id: &str) -> SceneBookmark {
    SceneBookmark {
        id: id.into(),
        sequence_id: "s".into(),
        time: time(1, 48),
        duration: Some(time(1, 1)),
        color: Some("#aabbcc".into()),
        note: Some("note".into()),
    }
}
fn book(operation: BookmarkOperation) -> ProjectOperation {
    ProjectOperation::Bookmark { operation }
}

#[test]
fn create_replay_retains_initial_snapshot_after_head_advances_and_denial_precedes_reads() {
    let mut store = MemoryStore::default();
    let request = input();
    let initial = create(&mut store, &request).unwrap();
    assert!(matches!(
        initial.project.settings,
        SettingsProfile::ClassicGlobal { .. }
    ));
    assert_eq!(initial.project.sequences[0].tracks[0].name, "Main Track");
    let result = apply(
        &mut store,
        &edit(
            &initial,
            "rename",
            vec![ProjectOperation::Rename {
                name: "Changed".into(),
            }],
        ),
    )
    .unwrap();
    assert_ne!(result.revision, initial.revision);
    assert_eq!(create(&mut store, &request).unwrap(), initial);
    let reads = store.reads.get();
    store.deny = true;
    assert_eq!(
        create(&mut store, &request).unwrap_err().code,
        "ACCESS_DENIED"
    );
    assert_eq!(store.reads.get(), reads);
}

#[test]
fn failed_later_edit_or_commit_reauthorization_publishes_nothing() {
    let mut store = MemoryStore::default();
    let initial = create(&mut store, &input()).unwrap();
    let request = edit(
        &initial,
        "bad",
        vec![
            book(BookmarkOperation::Put {
                bookmark: marker("b"),
            }),
            ProjectOperation::AssetDelete {
                asset_id: "missing".into(),
            },
        ],
    );
    let publications = store.publications;
    assert!(apply(&mut store, &request).is_err());
    assert_eq!(store.publications, publications);
    assert_eq!(
        inspect(
            &store,
            &InspectProject {
                project_id: "p".into(),
                revision: None
            }
        )
        .unwrap(),
        initial
    );
    store.deny_commit = true;
    assert_eq!(
        apply(
            &mut store,
            &edit(
                &initial,
                "revoked",
                vec![ProjectOperation::Rename { name: "new".into() }]
            )
        )
        .unwrap_err()
        .code,
        "ACCESS_DENIED"
    );
    assert_eq!(store.publications, publications);
}

#[test]
fn legacy_promotion_and_restore_always_append_v4_without_mutating_old_history() {
    let legacy = crate::historical_visual::create(&input().project).unwrap();
    let old = StoredProjectRevision {
        document: StoredProjectDocument::HistoricalVisual(Box::new(legacy.clone())),
        revision: "old".into(),
    };
    let mut store = MemoryStore::with_source(old.clone());
    let promoted = promote(
        &mut store,
        &PromoteProject {
            branch_name: None,
            project_id: "p".into(),
            expected_revision: "old".into(),
            idempotency_key: "promote".into(),
            asset_versions: BTreeMap::new(),
        },
    )
    .unwrap();
    let head = inspect(
        &store,
        &InspectProject {
            project_id: "p".into(),
            revision: None,
        },
    )
    .unwrap();
    assert_eq!(head.project.schema_version, 4);
    assert!(matches!(
        head.project.settings,
        SettingsProfile::LegacyPerScene { .. }
    ));
    let result = restore(
        &mut store,
        &RestoreProject {
            branch_name: None,
            label: None,
            project_id: "p".into(),
            expected_revision: promoted.revision,
            target_revision: "old".into(),
            idempotency_key: "restore-old".into(),
            asset_versions: BTreeMap::new(),
        },
    )
    .unwrap();
    assert_ne!(result.revision, "old");
    assert_eq!(
        inspect(
            &store,
            &InspectProject {
                project_id: "p".into(),
                revision: None
            }
        )
        .unwrap()
        .project,
        head.project
    );
    assert_eq!(store.revisions.get("old"), Some(&old));
}

#[test]
fn copy_uses_fixed_v4_content_and_shared_workspace_receipt_namespace() {
    let mut store = MemoryStore::default();
    let source = create(&mut store, &input()).unwrap();
    apply(
        &mut store,
        &edit(
            &source,
            "rename",
            vec![ProjectOperation::Rename {
                name: "new head".into(),
            }],
        ),
    )
    .unwrap();
    let request = CopySceneProject {
        source_project_id: "p".into(),
        source_revision: source.revision.clone(),
        project_id: "copy".into(),
        name: "Copied".into(),
        idempotency_key: "copy".into(),
    };
    let copied = copy(&mut store, &request).unwrap();
    assert_eq!(copied.project.sequences, source.project.sequences);
    assert_eq!(copied.project.name, "Copied");
    assert_eq!(copy(&mut store, &request).unwrap(), copied);
    let mut collision = request.clone();
    collision.idempotency_key = "create".into();
    assert_eq!(
        copy(&mut store, &collision).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    let mut invalid_id = request;
    invalid_id.project_id = "s".into();
    invalid_id.idempotency_key = "bad-copy".into();
    assert!(copy(&mut store, &invalid_id).is_err());
}
