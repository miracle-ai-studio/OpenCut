use super::*;

#[test]
fn source_digest_fixed_head_live_read_edit_and_cross_project_rejections_are_atomic() {
    let fixture = Fixture::new();
    let original = fixture.current.clone();
    let request = fixture.request(fixture.copied(&[("source-sound", "voice")]));
    for field in ["digest", "revision", "kind", "target-head", "project"] {
        let mut forged = request.clone();
        match field {
            "digest" => forged.clipboard.selection_digest = "0".repeat(64),
            "revision" => forged.clipboard.source_revision = "head".into(),
            "kind" => forged.clipboard.items[0].source_track_kind = SceneTrackKind::Video,
            "target-head" => forged.expected_revision = "stale-head".into(),
            "project" => forged.project_id = "other-project".into(),
            _ => unreachable!(),
        }
        assert_eq!(
            plan_paste(&fixture, &forged).unwrap_err().code,
            match field {
                "target-head" => "REVISION_CONFLICT",
                "project" => "UNSUPPORTED_CLIPBOARD_TRANSFER",
                _ => "CLIPBOARD_SOURCE_MISMATCH",
            }
        );
        assert_eq!(fixture.current, original);
    }
    for allowed in [0, 2] {
        let mut revoked = Fixture::new();
        let request = revoked.request(revoked.copied(&[("source-sound", "voice")]));
        revoked.resource_reads.set(0);
        revoked.deny_read_after = Some(allowed);
        assert_eq!(
            plan_paste(&revoked, &request).unwrap_err().code,
            "NOT_AUTHORIZED"
        );
        assert_eq!(revoked.current.project, revoked.source.project);
    }
    for allowed in [0, 1, 2] {
        let mut revoked = Fixture::new();
        let request = revoked.request(revoked.copied(&[("source-sound", "voice")]));
        revoked.deny_edit_after = Some(allowed);
        assert_eq!(
            plan_paste(&revoked, &request).unwrap_err().code,
            "NOT_AUTHORIZED"
        );
    }
}

#[test]
fn stable_namespace_includes_nested_and_historical_nodes_and_clipboard_has_no_authored_payload() {
    let mut fixture = Fixture::new();
    let copied = fixture.copied(&[("source-sound", "voice")]);
    fixture.current.project.sequences[0].tracks[1]
        .elements
        .clear();
    fixture.current.project.sequences[0].tracks[1]
        .channels
        .clear();
    for collision in [
        "project",
        "source",
        "font",
        "volume-a",
        "volume-segment",
        "voice",
    ] {
        let mut request = fixture.request(copied.clone());
        request.placements[0].elements[0].new_element_id = collision.into();
        assert_eq!(
            plan_paste(&fixture, &request).unwrap_err().code,
            "INVALID_ARGUMENT"
        );
    }
    let mut json = serde_json::to_value(copied).unwrap();
    json["items"][0]["element"] = serde_json::json!({"enabled":true});
    assert!(serde_json::from_value::<TimelineClipboard>(json).is_err());
    let mut request = fixture.request(fixture.copied(&[("source-sound", "voice")]));
    request.placements[0].target = ClipboardTrackTarget::ClassicAboveSource {
        new_track_id: "volume".into(),
        name: "Reserved even if reused".into(),
    };
    assert_eq!(
        plan_paste(&fixture, &request).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
}
