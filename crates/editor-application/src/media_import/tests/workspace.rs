use super::*;

fn existing() -> Fixture {
    let mut fixture = Fixture::new();
    let mut asset = fixture::asset();
    asset.id = "asset".into();
    fixture.snapshot.project.assets.push(asset);
    fixture.media.clear();
    fixture
}

fn request(fixture: &Fixture, placements: Vec<MediaPlacement>) -> MediaImportRequest {
    MediaImportRequest {
        project_id: fixture.snapshot.project.id.clone(),
        expected_revision: fixture.snapshot.revision.clone(),
        sequence_id: fixture.snapshot.project.default_sequence_id.clone(),
        idempotency_key: "import-key".into(),
        label: "Insert observed media".into(),
        bindings: vec![],
        placements,
        ripple: false,
        new_tracks: vec![],
    }
}

fn audio(fixture: &Fixture, track_id: &str) -> MediaPlacement {
    MediaPlacement {
        element_id: "detached".into(),
        name: "Audio".into(),
        track_id: track_id.into(),
        asset_id: "asset".into(),
        start: t(2, 1),
        content: MediaPlacementContent::Audio {
            timing: fixture.source_timing(),
        },
    }
}

fn declaration(id: &str, kind: SceneTrackKind) -> MediaTrack {
    MediaTrack {
        id: id.into(),
        name: "New track".into(),
        kind,
    }
}

#[test]
fn existing_asset_reinsertion_uses_head_without_rebinding_or_import_fps_ratchet() {
    let mut fixture = existing();
    let existing_audio = request(&fixture, vec![audio(&fixture, "audio")]);
    let seeded = plan(&fixture, &existing_audio).unwrap();
    fixture.snapshot.project.sequences[0].tracks[1]
        .elements
        .push((*elements(&seeded)[0]).clone());
    fixture.reads.set(0);
    let input = request(&fixture, vec![fixture.video(fixture.source_timing())]);
    let before = fixture.snapshot.clone();
    let result = plan(&fixture, &input).unwrap();
    assert_eq!(fixture.snapshot, before);
    assert_eq!(
        fixture.reads.get(),
        1,
        "no available-media probe or binding"
    );
    assert!(result.decisions.raised_fps.is_none());
    assert!(result.decisions.first_visual_fps.is_none());
    assert!(result.decisions.canvas_change.is_none());
    assert!(result.decisions.bound_asset_ids.is_empty());
    assert!(result.decisions.replaced_asset_ids.is_empty());
    assert!(
        result
            .transaction
            .operations
            .iter()
            .all(|op| matches!(op, ProjectEditOperation::ElementPut { .. }))
    );
    let candidate = crate::scene::public::plan_public(&fixture, &result.transaction).unwrap();
    assert_eq!(
        candidate.assets,
        crate::scene::public::project(&before).project.assets
    );
    assert_eq!(
        candidate.sequences[0].tracks[0].elements[0],
        *elements(&result)[0]
    );
    assert_eq!(result.transaction.expected_revision, "fixed");
}

#[test]
fn new_audio_track_and_existing_video_source_share_one_unmodified_public_transaction() {
    let fixture = existing();
    let mut input = request(&fixture, vec![audio(&fixture, "new-audio")]);
    input
        .new_tracks
        .push(declaration("new-audio", SceneTrackKind::Audio));
    let before = fixture.snapshot.clone();
    let result = plan(&fixture, &input).unwrap();
    assert_eq!(result.transaction.operations.len(), 2);
    let ProjectEditOperation::TrackPut { sequence_id, track } = &result.transaction.operations[0]
    else {
        panic!()
    };
    assert_eq!(sequence_id, "s");
    assert_eq!(track.id, "new-audio");
    assert!(track.enabled && !track.muted && !track.locked);
    assert_eq!(track.opacity, 1.0);
    assert_eq!(track.gain, Gain::Decibels { value: 0.0 });
    assert!(track.elements.is_empty() && track.channels.is_empty());
    let candidate = crate::scene::public::plan_public(&fixture, &result.transaction).unwrap();
    let created = candidate.sequences[0].tracks.last().unwrap();
    assert_eq!(created.id, "new-audio");
    assert_eq!(created.elements, vec![(*elements(&result)[0]).clone()]);
    assert!(created.elements[0].visual.is_none());
    assert_eq!(
        candidate.assets,
        crate::scene::public::project(&before).project.assets
    );
    assert_eq!(fixture.snapshot, before);
    assert_eq!(plan(&fixture, &input).unwrap(), result);
    let encoded = serde_json::to_string(&result).unwrap();
    assert!(
        !encoded.contains("/private")
            && !encoded.contains("legacy_locator")
            && !encoded.contains("\"uri\"")
    );
    let mut ripple = input;
    ripple.ripple = true;
    let repeated = plan(&fixture, &ripple).unwrap();
    assert_eq!(repeated.transaction, result.transaction);
    assert_ne!(
        repeated.original_request_digest,
        result.original_request_digest
    );
}

#[test]
fn existing_first_video_overrides_fps_after_first_placement_without_import_ratchet() {
    let mut fixture = existing();
    let SettingsProfile::ClassicGlobal { settings: initial } =
        &mut fixture.snapshot.project.settings
    else {
        panic!()
    };
    initial.fps = FrameRate {
        numerator: 60,
        denominator: 1,
    };
    fixture.snapshot.project.assets[0].metadata.frame_rate = Some(FrameRate {
        numerator: 24,
        denominator: 1,
    });
    let mut first = fixture.video(fixture.source_timing());
    first.track_id = "new-video".into();
    first.start = t(1, 16);
    let mut second = first.clone();
    second.element_id = "second-video".into();
    second.track_id = "new-video-two".into();
    let mut input = request(&fixture, vec![first, second]);
    input.new_tracks = vec![
        declaration("new-video", SceneTrackKind::Video),
        declaration("new-video-two", SceneTrackKind::Video),
    ];
    let result = plan(&fixture, &input).unwrap();
    assert!(result.decisions.raised_fps.is_none());
    assert_eq!(
        result.decisions.first_visual_fps,
        Some(FrameRate {
            numerator: 24,
            denominator: 1
        })
    );
    let starts = elements(&result)
        .iter()
        .map(|e| time(&e.timeline_range.start).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        starts[0].cmp_exact(time(&t(1, 15)).unwrap()),
        std::cmp::Ordering::Equal
    );
    assert_eq!(
        starts[1].cmp_exact(time(&t(1, 12)).unwrap()),
        std::cmp::Ordering::Equal
    );
    let SettingsProfile::ClassicGlobal { settings: updated } = settings(&result) else {
        panic!()
    };
    assert_eq!(
        updated.canvas_size,
        CanvasSize {
            width: 1920,
            height: 1080
        }
    );
    let candidate = crate::scene::public::plan_public(&fixture, &result.transaction).unwrap();
    assert_eq!(candidate.settings, settings(&result).clone());
    assert_eq!(candidate.sequences[0].tracks.len(), 4);
}

#[test]
fn workspace_rejects_unused_kinds_global_collisions_locks_missing_assets_and_empty_intents() {
    let fixture = existing();
    let base = request(&fixture, vec![audio(&fixture, "new-audio")]);
    for tracks in [
        vec![declaration("new-audio", SceneTrackKind::Caption)],
        vec![
            declaration("new-audio", SceneTrackKind::Audio),
            declaration("unused", SceneTrackKind::Audio),
        ],
        vec![
            declaration("new-audio", SceneTrackKind::Audio),
            declaration("new-audio", SceneTrackKind::Audio),
        ],
    ] {
        let input = MediaImportRequest {
            new_tracks: tracks,
            ..base.clone()
        };
        assert!(plan(&fixture, &input).is_err());
    }
    for collision in ["main", "asset", "p", "s"] {
        let mut input = base.clone();
        input.new_tracks = vec![declaration(collision, SceneTrackKind::Audio)];
        input.placements[0].track_id = collision.into();
        assert!(
            plan(&fixture, &input).is_err(),
            "global collision {collision}"
        );
    }
    let mut missing = request(&fixture, vec![audio(&fixture, "audio")]);
    missing.placements[0].asset_id = "not-bound".into();
    assert_eq!(plan(&fixture, &missing).unwrap_err().code, "NOT_FOUND");
    assert!(plan(&fixture, &request(&fixture, vec![])).is_err());
    let mut denied = existing();
    denied.deny = true;
    assert_eq!(
        plan(&denied, &request(&denied, vec![audio(&denied, "audio")]))
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    let mut locked = existing();
    locked.snapshot.project.sequences[0].tracks[0].locked = true;
    let before = locked.snapshot.clone();
    let mut input = request(
        &locked,
        vec![
            audio(&locked, "new-audio"),
            locked.video(locked.source_timing()),
        ],
    );
    input
        .new_tracks
        .push(declaration("new-audio", SceneTrackKind::Audio));
    assert_eq!(
        plan(&locked, &input).unwrap_err().code,
        "CONSTRAINT_VIOLATION"
    );
    assert_eq!(
        locked.snapshot, before,
        "failure cannot leave a created track or first element"
    );
}
