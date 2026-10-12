use super::*;

#[test]
fn bookmark_edits_are_revision_owned_and_fixed_history_is_immutable() {
    let mut store = MemoryStore::default();
    let initial = create(&mut store, &input()).unwrap();
    let put = edit(
        &initial,
        "put",
        vec![book(BookmarkOperationV1::Put {
            bookmark: marker("b"),
        })],
    );
    let result = apply(&mut store, &put).unwrap();
    let after = inspect(
        &store,
        &InspectV4Project {
            project_id: "p".into(),
            revision: Some(result.revision.clone()),
        },
    )
    .unwrap();
    let b = &after.project.sequences[0].bookmarks[0];
    assert_eq!(
        opencut_time::RationalTime::try_from(&b.time)
            .unwrap()
            .cmp_exact(opencut_time::RationalTime::new(1, 24).unwrap()),
        std::cmp::Ordering::Equal
    );
    assert!(
        inspect(
            &store,
            &InspectV4Project {
                project_id: "p".into(),
                revision: Some(initial.revision.clone())
            }
        )
        .unwrap()
        .project
        .sequences[0]
            .bookmarks
            .is_empty()
    );
    let moved = apply(
        &mut store,
        &edit(
            &after,
            "move",
            vec![book(BookmarkOperationV1::Move {
                sequence_id: "s".into(),
                bookmark_id: "b".into(),
                time: time(1, 2),
            })],
        ),
    )
    .unwrap();
    assert_eq!(apply(&mut store, &put).unwrap(), result);
    let moved_snapshot = inspect(
        &store,
        &InspectV4Project {
            project_id: "p".into(),
            revision: Some(moved.revision),
        },
    )
    .unwrap();
    let deleted = apply(
        &mut store,
        &edit(
            &moved_snapshot,
            "delete",
            vec![book(BookmarkOperationV1::Delete {
                sequence_id: "s".into(),
                bookmark_id: "b".into(),
            })],
        ),
    )
    .unwrap();
    assert!(deleted.deleted_ids.contains(&"b".into()));
    let current = inspect(
        &store,
        &InspectV4Project {
            project_id: "p".into(),
            revision: None,
        },
    )
    .unwrap();
    let restored = restore(
        &mut store,
        &RestoreV4Revision {
            project_id: "p".into(),
            expected_revision: current.revision,
            target_revision: after.revision.clone(),
            idempotency_key: "restore".into(),
            asset_versions: BTreeMap::new(),
        },
    )
    .unwrap();
    assert_ne!(restored.revision, after.revision);
    assert_eq!(
        inspect(
            &store,
            &InspectV4Project {
                project_id: "p".into(),
                revision: None
            }
        )
        .unwrap()
        .project,
        after.project
    );
}

#[test]
fn inserting_tracks_preserves_locked_content_but_reordering_or_replacing_it_does_not() {
    let mut initial = ProjectSnapshotV4 {
        project: new_document(&input().project).unwrap(),
        revision: "fixed".into(),
    };
    let main = initial.project.sequences[0].tracks[0].clone();
    let mut protected = main.clone();
    protected.id = "protected-overlay".into();
    protected.name = "Protected".into();
    protected.locked = true;
    initial.project.sequences[0].tracks.push(protected.clone());
    let mut added = main.clone();
    added.id = "new-overlay".into();
    added.name = "New".into();
    let mut proposed = initial.project.sequences[0].clone();
    proposed.tracks.insert(1, added);
    let result = plan(
        &initial,
        &edit(
            &initial,
            "insert",
            vec![ProjectOperationV4::SequencePut {
                sequence: proposed.clone(),
            }],
        ),
    )
    .unwrap();
    assert_eq!(result.sequences[0].tracks[2], protected);
    let inserted = ProjectSnapshotV4 {
        project: result,
        revision: "inserted".into(),
    };
    let restored = plan(
        &inserted,
        &edit(
            &inserted,
            "remove-inserted-track",
            vec![ProjectOperationV4::SequencePut {
                sequence: initial.project.sequences[0].clone(),
            }],
        ),
    )
    .unwrap();
    assert_eq!(restored, initial.project);
    let mut deleted_locked = initial.project.sequences[0].clone();
    deleted_locked.tracks.pop();
    assert_eq!(
        plan(
            &initial,
            &edit(
                &initial,
                "delete-locked",
                vec![ProjectOperationV4::SequencePut {
                    sequence: deleted_locked,
                }],
            ),
        )
        .unwrap_err()
        .code,
        "CONSTRAINT_VIOLATION"
    );
    proposed.tracks[2].name = "Changed".into();
    assert_eq!(
        plan(
            &initial,
            &edit(
                &initial,
                "replace",
                vec![ProjectOperationV4::SequencePut { sequence: proposed },]
            )
        )
        .unwrap_err()
        .code,
        "CONSTRAINT_VIOLATION"
    );
    let mut reordered = initial.project.sequences[0].clone();
    reordered.tracks.swap(0, 1);
    assert_eq!(
        plan(
            &initial,
            &edit(
                &initial,
                "reorder",
                vec![ProjectOperationV4::SequencePut {
                    sequence: reordered
                },]
            )
        )
        .unwrap_err()
        .code,
        "CONSTRAINT_VIOLATION"
    );
}

#[test]
fn sequence_replacement_preserves_unrelated_unlocked_reorder_semantics() {
    let mut initial = ProjectSnapshotV4 {
        project: new_document(&input().project).unwrap(),
        revision: "fixed".into(),
    };
    let mut protected = initial.project.sequences[0].tracks[0].clone();
    protected.id = "protected".into();
    protected.locked = true;
    let mut other = protected.clone();
    other.id = "other".into();
    other.locked = false;
    let mut last = other.clone();
    last.id = "last".into();
    initial.project.sequences[0]
        .tracks
        .extend([protected.clone(), other, last]);
    let mut proposed = initial.project.sequences[0].clone();
    proposed.tracks.swap(2, 3);
    let result = plan(
        &initial,
        &edit(
            &initial,
            "reorder-unlocked",
            vec![ProjectOperationV4::SequencePut {
                sequence: proposed.clone(),
            }],
        ),
    )
    .unwrap();
    assert_eq!(result.sequences[0], proposed);
    assert_eq!(result.sequences[0].tracks[1], protected);
}

#[test]
fn sequence_replacement_cannot_unlock_or_bypass_annotation_clocks() {
    let initial = ProjectSnapshotV4 {
        project: new_document(&input().project).unwrap(),
        revision: "fixed".into(),
    };
    let mut locked = initial.clone();
    locked.project.sequences[0].tracks[0].locked = true;
    let mut proposed = locked.project.sequences[0].clone();
    proposed.tracks[0].locked = false;
    assert_eq!(
        plan(
            &locked,
            &edit(
                &locked,
                "bypass",
                vec![ProjectOperationV4::SequencePut { sequence: proposed }]
            )
        )
        .unwrap_err()
        .code,
        "CONSTRAINT_VIOLATION"
    );
    let mut proposed = initial.project.sequences[0].clone();
    proposed.bookmarks.push(marker("raw"));
    assert!(
        plan(
            &initial,
            &edit(
                &initial,
                "raw",
                vec![ProjectOperationV4::SequencePut { sequence: proposed }]
            )
        )
        .is_err()
    );
    let main_id = locked.project.sequences[0].main_track_id.clone();
    let after = plan(
        &locked,
        &edit(
            &locked,
            "unlock",
            vec![
                ProjectOperationV4::TrackLockSet {
                    sequence_id: "s".into(),
                    track_id: main_id,
                    locked: false,
                },
                book(BookmarkOperationV1::Toggle {
                    sequence_id: "s".into(),
                    time: time(1, 48),
                    bookmark_id: "b".into(),
                }),
            ],
        ),
    )
    .unwrap();
    assert!(!after.sequences[0].tracks[0].locked);
    assert_eq!(after.sequences[0].bookmarks.len(), 1);
    assert!(locked.project.sequences[0].tracks[0].locked);
}

#[test]
fn bookmarks_tolerate_temporarily_incomplete_aggregate_in_atomic_batch() {
    let initial = ProjectSnapshotV4 {
        project: new_document(&input().project).unwrap(),
        revision: "fixed".into(),
    };
    let main = initial.project.sequences[0].tracks[0].clone();
    let after = plan(
        &initial,
        &edit(
            &initial,
            "batch",
            vec![
                ProjectOperationV4::TrackDelete {
                    sequence_id: "s".into(),
                    track_id: main.id.clone(),
                },
                book(BookmarkOperationV1::Toggle {
                    sequence_id: "s".into(),
                    time: time(1, 48),
                    bookmark_id: "b".into(),
                }),
                ProjectOperationV4::TrackPut {
                    sequence_id: "s".into(),
                    track: main,
                },
            ],
        ),
    )
    .unwrap();
    assert_eq!(after.sequences[0].bookmarks[0].time, time(1, 24));
    assert_eq!(
        after.sequences[0].tracks,
        initial.project.sequences[0].tracks
    );
}
