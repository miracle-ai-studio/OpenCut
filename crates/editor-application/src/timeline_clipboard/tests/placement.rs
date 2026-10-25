use super::*;

#[test]
fn duplicate_retains_authored_time_and_requires_new_highest_current_source() {
    let mut fixture = Fixture::new();
    fixture.current.revision = fixture.source.revision.clone();
    let mut request = fixture.request(fixture.copied(&[("source-sound", "voice")]));
    request.sequence_id = "source".into();
    request.at = t(1, 3);
    let before = fixture.current.clone();
    let output = plan_duplicate(&fixture, &request).unwrap();
    assert_eq!(plan_duplicate(&fixture, &request).unwrap(), output);
    let after = proposed(&fixture, &output);
    let cloned = element(&after, "clone-voice");
    let original = element(&before.project, "voice");
    assert_eq!(cloned.timeline_range, original.timeline_range);
    assert_eq!(cloned.source_span, original.source_span);
    assert_eq!(cloned.name, format!("{} (copy)", original.name));
    match (&cloned.channels[0], &original.channels[0]) {
        (AnimationChannel::Scalar { id: a, .. }, AnimationChannel::Scalar { id: b, .. }) => {
            assert_ne!(a, b);
        }
        _ => panic!("Fixture has a scalar gain channel"),
    }
    assert_eq!(after.sequences[0].tracks[1].id, "new-source-sound");
    assert_eq!(
        after.sequences[0].tracks[2],
        before.project.sequences[0].tracks[1]
    );
    assert_eq!(fixture.current, before);
    // The old public paste remains a paste, with the source name unchanged.
    let pasted = proposed(&fixture, &plan_paste(&fixture, &request).unwrap());
    assert_eq!(element(&pasted, "clone-voice").name, original.name);
    request.at = t(0, 1);
    assert_eq!(
        plan_duplicate(&fixture, &request).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    request.at = t(1, 3);
    request.placements[0].target = ClipboardTrackTarget::ClassicAboveSource {
        new_track_id: "fallback".into(),
        name: "Auto".into(),
    };
    assert_eq!(
        plan_duplicate(&fixture, &request).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    fixture.current.revision = "later-head".into();
    request.expected_revision = fixture.current.revision.clone();
    assert_eq!(
        plan_duplicate(&fixture, &request).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
}

#[test]
fn new_track_insertion_keeps_locked_tracks_and_existing_layer_order_unchanged() {
    let mut fixture = Fixture::new();
    fixture.current.project.sequences[1].tracks[1].locked = true;
    let old = fixture.current.project.sequences[1].clone();
    let request = fixture.request(fixture.copied(&[("source-sound", "voice")]));
    let output = plan_paste(&fixture, &request).unwrap();
    let proposed = proposed(&fixture, &output);
    let scene = &proposed.sequences[1];
    assert_eq!(
        scene
            .tracks
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        vec!["dest-main", "new-source-sound", "dest-sound"]
    );
    assert_eq!(scene.tracks[0], old.tracks[0]);
    assert_eq!(scene.tracks[2], old.tracks[1]);
    let created = &scene.tracks[1];
    assert!(created.enabled && !created.muted && !created.locked && created.channels.is_empty());
    assert_eq!(created.gain, Gain::Decibels { value: 0.0 });
    assert_eq!(created.opacity, 1.0);
    // Source track mute/gain/automation are not element properties and are not
    // cloned into a new destination track. The element's own mute is retained.
    assert!(matches!(&created.elements[0].content,ElementContent::Audio{audio,..} if audio.muted));
    let mut locked = request.clone();
    locked.placements[0].target = ClipboardTrackTarget::Existing {
        track_id: "dest-sound".into(),
    };
    assert_eq!(
        plan_paste(&fixture, &locked).unwrap_err().code,
        "CONSTRAINT_VIOLATION"
    );
}

#[test]
fn classic_automatic_above_then_first_available_half_open_then_highest() {
    let mut fixture = Fixture::new();
    let scene = &mut fixture.current.project.sequences[0];
    let occupied = audio("prior", t(1, 3));
    scene
        .tracks
        .insert(1, track("above", SceneTrackKind::Audio, vec![occupied]));
    fixture.sync();
    let copied = fixture.copied(&[("source-sound", "voice")]);
    let mut request = fixture.request(copied);
    request.sequence_id = "source".into();
    request.at = t(5, 3);
    request.placements[0].target = ClipboardTrackTarget::ClassicAboveSource {
        new_track_id: "fallback".into(),
        name: "Auto".into(),
    };
    let output = plan_paste(&fixture, &request).unwrap();
    // The disabled source and disabled prior element still occupy time, but
    // exact touching at 5/3 is legal. Immediate-above wins over other tracks.
    assert_eq!(output.decision.selection[0].track_id, "above");
    assert!(output.decision.new_track_ids.is_empty());
    fixture.current.project.sequences[0].tracks[1].locked = true;
    let output = plan_paste(&fixture, &request).unwrap();
    assert_eq!(output.decision.selection[0].track_id, "source-sound");
    fixture.current.project.sequences[0].tracks[2].locked = true;
    let output = plan_paste(&fixture, &request).unwrap();
    assert_eq!(output.decision.new_track_ids, vec!["fallback"]);
    let result = proposed(&fixture, &output);
    let scene = &result.sequences[0];
    assert_eq!(scene.tracks[1].id, "fallback");
    assert!(scene.tracks[2].locked && scene.tracks[3].locked);
    // Cross-scene source track is absent: Classic's first available rule is
    // resolved from this destination, without fabricating a source index.
    request.sequence_id = "destination".into();
    let output = plan_paste(&fixture, &request).unwrap();
    assert_eq!(output.decision.selection[0].track_id, "dest-sound");
}

#[test]
fn main_track_pin_moves_entire_group_and_explicit_placement_never_falls_back() {
    let mut fixture = Fixture::new();
    fixture.current.project.assets[0].kind = AssetKind::Image;
    fixture.current.project.assets[0].duration = None;
    fixture.current.project.assets[0].metadata.audio = AudioPresence::Absent {};
    fixture.current.project.sequences[0].tracks[1]
        .elements
        .clear();
    fixture.current.project.sequences[0].tracks[1]
        .channels
        .clear();
    let mut first = generated(
        "first",
        ElementContent::Image {
            asset_id: "sound-asset".into(),
        },
    );
    first.timeline_range.start = t(1, 1);
    let mut second = first.clone();
    second.id = "second".into();
    second.name = "Second".into();
    second.timeline_range.start = t(4, 1);
    fixture.current.project.sequences[0].tracks[0].elements = vec![first, second];
    fixture.sync();
    let mut request =
        fixture.request(fixture.copied(&[("source-main", "second"), ("source-main", "first")]));
    request.at = t(7, 1);
    request.placements[0].target = ClipboardTrackTarget::Existing {
        track_id: "dest-main".into(),
    };
    let output = plan_paste(&fixture, &request).unwrap();
    let result = proposed(&fixture, &output);
    assert_eq!(
        element(&result, "clone-first").timeline_range.start,
        t(0, 1)
    );
    assert_eq!(
        element(&result, "clone-second").timeline_range.start,
        t(3, 1)
    );
    for id in ["first", "second"] {
        super::cloning::assert_full_clone(
            element(&fixture.source.project, id),
            element(&result, &format!("clone-{id}")),
        );
    }
    // Requested group starts at zero after pinning, so this already occupied
    // main must reject rather than pick a different visual track.
    let mut occupied = generated(
        "occupied",
        ElementContent::Image {
            asset_id: "sound-asset".into(),
        },
    );
    occupied.timeline_range.start = t(0, 1);
    fixture.current.project.sequences[1].tracks[0]
        .elements
        .push(occupied);
    request.at = t(0, 1);
    assert_eq!(
        plan_paste(&fixture, &request).unwrap_err().code,
        "CONSTRAINT_VIOLATION"
    );
    assert_eq!(
        fixture.current.project.sequences[1].tracks[0]
            .elements
            .len(),
        1
    );
}

#[test]
fn insertion_anchor_visual_direction_and_kind_and_complete_group_are_explicit() {
    let fixture = Fixture::new();
    let mut request = fixture.request(fixture.copied(&[("source-sound", "voice")]));
    request.placements[0].target = ClipboardTrackTarget::New {
        track_id: "new-sound".into(),
        name: "Below".into(),
        track_kind: SceneTrackKind::Audio,
        insertion_anchor: ClipboardTrackInsertion::Below {
            track_id: "dest-sound".into(),
        },
    };
    let output = plan_paste(&fixture, &request).unwrap();
    assert_eq!(
        proposed(&fixture, &output).sequences[1].tracks[2].id,
        "new-sound"
    );
    if let ClipboardTrackTarget::New {
        insertion_anchor, ..
    } = &mut request.placements[0].target
    {
        *insertion_anchor = ClipboardTrackInsertion::Above {
            track_id: "dest-main".into(),
        };
    }
    assert_eq!(
        plan_paste(&fixture, &request).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    if let ClipboardTrackTarget::New {
        track_kind,
        insertion_anchor,
        ..
    } = &mut request.placements[0].target
    {
        *track_kind = SceneTrackKind::Video;
        *insertion_anchor = ClipboardTrackInsertion::Highest;
    }
    assert_eq!(
        plan_paste(&fixture, &request).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    request.placements[0].elements.clear();
    assert_eq!(
        plan_paste(&fixture, &request).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
}
