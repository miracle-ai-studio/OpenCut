use super::*;
mod fixture;
mod fps;
mod pinned_boundary;
mod workspace;
use fixture::{Fixture, t, version};

fn elements(plan: &MediaImportPlanV4) -> Vec<&TimelineElementV4> {
    plan.transaction
        .operations
        .iter()
        .filter_map(|op| match op {
            PublicProjectOperationV4::ElementPut { element, .. } => Some(element.as_ref()),
            _ => None,
        })
        .collect()
}
fn settings(plan: &MediaImportPlanV4) -> &SettingsProfileV4 {
    plan.transaction
        .operations
        .iter()
        .find_map(|op| match op {
            PublicProjectOperationV4::SettingsSet { settings } => Some(settings),
            _ => None,
        })
        .unwrap()
}

#[test]
fn global_fps_native_canvas_and_exact_source_rounding_produce_safe_public_apply() {
    let fixture = Fixture::new();
    let mut input = fixture.request();
    input
        .placements
        .push(fixture.video(fixture.source_timing()));
    let before = fixture.snapshot.clone();
    let result = plan(&fixture, &input).unwrap();
    assert_eq!(fixture.snapshot, before);
    assert_eq!(result.transaction.idempotency_key, input.idempotency_key);
    assert_eq!(
        result.transaction.expected_revision,
        input.expected_revision
    );
    assert_eq!(
        result.decisions.raised_fps,
        Some(FrameRate {
            numerator: 30000,
            denominator: 1001
        })
    );
    let SettingsProfileV4::ClassicGlobal { settings } = settings(&result) else {
        panic!()
    };
    assert_eq!(
        settings.canvas_size,
        CanvasSizeV4 {
            width: 1920,
            height: 1080
        }
    );
    assert_eq!(settings.original_canvas_size, Some(settings.canvas_size));
    let element = elements(&result)[0];
    assert_eq!(element.timeline_range.start, t(0, 1));
    assert_eq!(element.timeline_range.duration, t(11429, 120000));
    assert_eq!(
        result.decisions.placements[0]
            .source_range
            .as_ref()
            .unwrap()
            .duration,
        t(17144, 120000)
    );
    assert!(result.decisions.placements[0].main_track_start_adjusted);
    let json = serde_json::to_string(&result).unwrap();
    assert!(
        !json.contains("/private/source")
            && !json.contains("legacy_locator")
            && !json.contains("\"uri\"")
    );
    let decoded: MediaImportPlanV4 = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, result);
    let differently_requested = MediaImportRequestV4 {
        ripple: true,
        ..input
    };
    let second = plan(&fixture, &differently_requested).unwrap();
    assert_eq!(second.transaction, result.transaction);
    assert_ne!(
        second.original_request_digest,
        result.original_request_digest
    );
}

#[test]
fn another_scene_raises_global_fps_without_changing_independent_scene_content() {
    let mut fixture = Fixture::new();
    let mut scene = fixture.snapshot.project.sequences[0].clone();
    scene.id = "other".into();
    scene.main_track_id = "other-main".into();
    scene.tracks.truncate(1);
    scene.tracks[0].id = "other-main".into();
    fixture.snapshot.project.sequences.push(scene);
    fixture
        .media
        .get_mut("available")
        .unwrap()
        .metadata
        .frame_rate = Some(FrameRate {
        numerator: 60000,
        denominator: 1001,
    });
    let mut input = fixture.request();
    input.sequence_id = "other".into();
    let result = plan(&fixture, &input).unwrap();
    assert_eq!(
        result.decisions.settings_scope,
        MediaSettingsScopeV4::ClassicGlobal
    );
    let SettingsProfileV4::ClassicGlobal { settings } = settings(&result) else {
        panic!()
    };
    assert_eq!(
        settings.fps,
        FrameRate {
            numerator: 60000,
            denominator: 1001
        }
    );
    assert_eq!(
        settings.canvas_size,
        CanvasSizeV4 {
            width: 320,
            height: 180
        }
    );
    assert!(result.decisions.canvas_change.is_none());
    assert!(
        result
            .transaction
            .operations
            .iter()
            .all(|op| !matches!(op, PublicProjectOperationV4::SequencePut { .. }))
    );
}

#[test]
fn explicit_audio_then_video_preserves_pinned_duration_and_does_not_reset_canvas() {
    let fixture = Fixture::new();
    let mut input = fixture.request();
    let timing = MediaTimingV4::PinnedTimeline {
        source_start: t(1, 1),
        duration: t(1, 7),
        rate: RationalRateV4 {
            numerator: 3,
            denominator: 2,
        },
        maintain_pitch: true,
    };
    input.placements = vec![
        MediaPlacementV4 {
            element_id: "sound".into(),
            name: "Sound".into(),
            track_id: "audio".into(),
            asset_id: "asset".into(),
            start: t(0, 1),
            content: MediaPlacementContentV4::Audio {
                timing: timing.clone(),
            },
        },
        fixture.video(timing),
    ];
    let result = plan(&fixture, &input).unwrap();
    assert!(result.decisions.canvas_change.is_none());
    for element in elements(&result) {
        assert_eq!(element.timeline_range.duration, t(1, 7));
    }
    assert_eq!(
        result.decisions.placements[0]
            .source_range
            .as_ref()
            .unwrap()
            .duration,
        t(25714, 120000)
    );
    assert!(elements(&result)[0].visual.is_none());
}

#[test]
fn legacy_scope_and_exact_source_profile_are_preserved_without_classic_conversion() {
    let mut fixture = Fixture::new();
    let SettingsProfileV4::ClassicGlobal { settings } = fixture.snapshot.project.settings.clone()
    else {
        panic!()
    };
    fixture.snapshot.project.settings = SettingsProfileV4::LegacyPerScene {
        formats: vec![LegacySequenceFormatV4 {
            sequence_id: "s".into(),
            fps: settings.fps,
            canvas_size: settings.canvas_size,
            background: settings.background,
        }],
    };
    let mut input = fixture.request();
    input
        .placements
        .push(fixture.video(fixture.source_timing()));
    let result = plan(&fixture, &input).unwrap();
    assert_eq!(
        result.decisions.settings_scope,
        MediaSettingsScopeV4::LegacyPerScene {
            sequence_id: "s".into()
        }
    );
    assert!(result.decisions.canvas_change.is_none());
    assert_eq!(elements(&result)[0].timeline_range.duration, t(2, 21));
    assert!(matches!(
        elements(&result)[0].source_span,
        ElementSourceSpanV4::LegacySourceRange { .. }
    ));
    assert_eq!(
        elements(&result)[0].visual.as_ref().unwrap().profile,
        VisualProfileV4::LegacyCompositionV3
    );
    assert!(!result.decisions.placements[0].main_track_start_adjusted);
}

#[test]
fn replacement_keeps_dependents_and_refuses_shorter_source_or_locked_owner() {
    let mut fixture = Fixture::new();
    let mut input = fixture.request();
    input
        .placements
        .push(fixture.video(MediaTimingV4::SourceRange {
            range: TimeRange {
                start: t(8, 1),
                duration: t(2, 1),
            },
            rate: RationalRateV4 {
                numerator: 1,
                denominator: 1,
            },
            maintain_pitch: false,
        }));
    let initial = plan(&fixture, &input).unwrap();
    let mut old = fixture.media["available"].clone();
    old.id = "asset".into();
    old.name = "Video".into();
    fixture.snapshot.project.assets.push(old);
    fixture.snapshot.project.sequences[0].tracks[0]
        .elements
        .push((*elements(&initial)[0]).clone());
    let original = fixture.snapshot.project.sequences[0].tracks[0].elements[0].clone();
    let mut replacement = fixture.request();
    replacement.bindings[0].mode = MediaBindingModeV4::Replace;
    fixture.media.get_mut("available").unwrap().version = version('2');
    replacement.bindings[0].version = version('2');
    let result = plan(&fixture, &replacement).unwrap();
    assert_eq!(result.decisions.replaced_asset_ids, vec!["asset"]);
    assert!(elements(&result).is_empty());
    assert_eq!(
        fixture.snapshot.project.sequences[0].tracks[0].elements[0],
        original
    );
    fixture.media.get_mut("available").unwrap().duration = Some(t(9, 1));
    assert_eq!(
        plan(&fixture, &replacement).unwrap_err().code,
        "INVALID_PROJECT"
    );
    fixture.media.get_mut("available").unwrap().duration = Some(t(10, 1));
    fixture.snapshot.project.sequences[0].tracks[0].locked = true;
    assert_eq!(
        plan(&fixture, &replacement).unwrap_err().code,
        "CONSTRAINT_VIOLATION"
    );
}

#[test]
fn policy_fixed_revision_missing_observation_audio_knowledge_and_overlap_fail_closed() {
    let mut fixture = Fixture::new();
    let input = fixture.request();
    fixture.deny = true;
    assert_eq!(plan(&fixture, &input).unwrap_err().code, "NOT_AUTHORIZED");
    assert_eq!(fixture.reads.get(), 0);
    fixture.deny = false;
    let mut stale = input.clone();
    stale.expected_revision = "stale".into();
    assert_eq!(
        plan(&fixture, &stale).unwrap_err().code,
        "REVISION_CONFLICT"
    );
    let mut absent = input.clone();
    absent.bindings[0].media_id = "missing".into();
    assert_eq!(plan(&fixture, &absent).unwrap_err().code, "NOT_FOUND");
    let mut wrong = input.clone();
    wrong.bindings[0].version = version('2');
    assert_eq!(plan(&fixture, &wrong).unwrap_err().code, "NOT_AUTHORIZED");
    let mut place = input.clone();
    place
        .placements
        .push(fixture.video(fixture.source_timing()));
    fixture.media.get_mut("available").unwrap().metadata.audio = AudioPresenceV4::Unknown {};
    assert_eq!(
        plan(&fixture, &place).unwrap_err().code,
        "MEDIA_METADATA_REQUIRED"
    );
    fixture.media.get_mut("available").unwrap().metadata.audio = AudioPresenceV4::Absent {};
    assert_eq!(plan(&fixture, &place).unwrap_err().code, "NO_AUDIO_TRACK");
    if let MediaPlacementContentV4::Video {
        source_audio_enabled,
        ..
    } = &mut place.placements[0].content
    {
        *source_audio_enabled = false;
    }
    fixture.media.get_mut("available").unwrap().duration = None;
    assert_eq!(
        plan(&fixture, &place).unwrap_err().code,
        "MEDIA_METADATA_REQUIRED"
    );
    fixture.media.get_mut("available").unwrap().duration = Some(t(10, 1));
    fixture
        .media
        .get_mut("available")
        .unwrap()
        .metadata
        .frame_rate = None;
    let result = plan(&fixture, &place).unwrap();
    assert_eq!(result.decisions.video_asset_ids_without_fps, vec!["asset"]);
    let mut second = place.placements[0].clone();
    second.element_id = "overlap".into();
    second.start = t(0, 1);
    place.placements.push(second);
    place.ripple = true;
    assert_eq!(plan(&fixture, &place).unwrap_err().code, "INVALID_PROJECT");
    fixture.snapshot.project.sequences[0].tracks[0].locked = true;
    assert_eq!(
        plan(&fixture, &input).unwrap().decisions.bound_asset_ids,
        vec!["asset"]
    );
    assert_eq!(
        plan(&fixture, &place).unwrap_err().code,
        "CONSTRAINT_VIOLATION"
    );
}

#[test]
fn native_image_size_preserves_custom_mode_original_canvas_and_explicit_duration() {
    let mut fixture = Fixture::new();
    let SettingsProfileV4::ClassicGlobal {
        settings: initial_settings,
    } = &mut fixture.snapshot.project.settings
    else {
        panic!()
    };
    initial_settings.canvas_size_mode = CanvasSizeModeV4::Custom;
    initial_settings.last_custom_canvas_size = Some(initial_settings.canvas_size);
    initial_settings.original_canvas_size = Some(CanvasSizeV4 {
        width: 640,
        height: 360,
    });
    let asset = fixture.media.get_mut("available").unwrap();
    asset.kind = AssetKind::Image;
    asset.duration = None;
    asset.metadata.frame_rate = None;
    asset.metadata.audio = AudioPresenceV4::Absent {};
    let mut input = fixture.request();
    input.placements.push(MediaPlacementV4 {
        element_id: "image".into(),
        name: "Image".into(),
        track_id: "main".into(),
        asset_id: "asset".into(),
        start: t(99, 1),
        content: MediaPlacementContentV4::Image {
            duration: t(123, 100),
        },
    });
    let result = plan(&fixture, &input).unwrap();
    let SettingsProfileV4::ClassicGlobal { settings } = settings(&result) else {
        panic!()
    };
    assert_eq!(settings.canvas_size_mode, CanvasSizeModeV4::Custom);
    assert_eq!(
        settings.last_custom_canvas_size,
        Some(CanvasSizeV4 {
            width: 320,
            height: 180
        })
    );
    assert_eq!(
        settings.original_canvas_size,
        Some(CanvasSizeV4 {
            width: 640,
            height: 360
        })
    );
    assert!(result.decisions.raised_fps.is_none());
    assert_eq!(elements(&result)[0].timeline_range.duration, t(123, 100));
    assert!(result.decisions.placements[0].source_range.is_none());
}

#[test]
fn first_video_overwrites_global_fps_even_when_lower_and_without_dimensions() {
    let mut fixture = Fixture::new();
    let SettingsProfileV4::ClassicGlobal {
        settings: initial_settings,
    } = &mut fixture.snapshot.project.settings
    else {
        panic!()
    };
    initial_settings.fps = FrameRate {
        numerator: 60,
        denominator: 1,
    };
    let asset = fixture.media.get_mut("available").unwrap();
    asset.metadata.frame_rate = Some(FrameRate {
        numerator: 24,
        denominator: 1,
    });
    asset.metadata.intrinsic_size = None;
    let mut request = fixture.request();
    request
        .placements
        .push(fixture.video(fixture.source_timing()));
    let result = plan(&fixture, &request).unwrap();
    assert_eq!(
        result.decisions.previous_fps,
        FrameRate {
            numerator: 60,
            denominator: 1
        }
    );
    assert!(result.decisions.raised_fps.is_none());
    assert_eq!(
        result.decisions.first_visual_fps,
        Some(FrameRate {
            numerator: 24,
            denominator: 1
        })
    );
    let SettingsProfileV4::ClassicGlobal { settings } = settings(&result) else {
        panic!()
    };
    assert_eq!(
        settings.fps,
        FrameRate {
            numerator: 24,
            denominator: 1
        }
    );
    assert!(result.decisions.canvas_change.is_none());
}

#[test]
fn first_placement_uses_binding_fps_then_later_placement_uses_first_video_fps() {
    let mut fixture = Fixture::new();
    fixture
        .media
        .get_mut("available")
        .unwrap()
        .metadata
        .frame_rate = Some(FrameRate {
        numerator: 24,
        denominator: 1,
    });
    let mut fast = fixture.media["available"].clone();
    fast.id = "fast-observation".into();
    fast.version = version('2');
    fast.metadata.frame_rate = Some(FrameRate {
        numerator: 60,
        denominator: 1,
    });
    fixture.media.insert("fast-media".into(), fast);
    let main = fixture.snapshot.project.sequences[0].tracks[0].clone();
    for id in ["first-overlay", "second-overlay"] {
        let mut track = main.clone();
        track.id = id.into();
        fixture.snapshot.project.sequences[0].tracks.push(track);
    }
    let mut request = fixture.request();
    request.bindings.push(MediaBindingV4 {
        media_id: "fast-media".into(),
        version: version('2'),
        asset_id: "fast-asset".into(),
        name: "Fast".into(),
        mode: MediaBindingModeV4::Bind,
    });
    let mut first = fixture.video(fixture.source_timing());
    first.track_id = "first-overlay".into();
    first.start = t(1, 16);
    let mut second = first.clone();
    second.element_id = "second-video".into();
    second.track_id = "second-overlay".into();
    second.asset_id = "fast-asset".into();
    request.placements = vec![first, second];
    let result = plan(&fixture, &request).unwrap();
    assert_eq!(
        result.decisions.raised_fps,
        Some(FrameRate {
            numerator: 60,
            denominator: 1
        })
    );
    assert_eq!(
        result.decisions.first_visual_fps,
        Some(FrameRate {
            numerator: 24,
            denominator: 1
        })
    );
    // 1/16s: at60FPS round(3.75)=4 -> 1/15; after first video's
    // override to24FPS round(1.5)=2 -> 1/12. No main-track zero masks this.
    assert_eq!(
        time(&elements(&result)[0].timeline_range.start)
            .unwrap()
            .cmp_exact(time(&t(1, 15)).unwrap()),
        std::cmp::Ordering::Equal
    );
    assert_eq!(
        time(&elements(&result)[1].timeline_range.start)
            .unwrap()
            .cmp_exact(time(&t(1, 12)).unwrap()),
        std::cmp::Ordering::Equal
    );
    let SettingsProfileV4::ClassicGlobal { settings } = settings(&result) else {
        panic!()
    };
    assert_eq!(
        settings.fps,
        FrameRate {
            numerator: 24,
            denominator: 1
        }
    );
    let candidate = crate::project_v4::public::plan_public(&fixture, &result.transaction).unwrap();
    assert_eq!(candidate.settings, self::settings(&result).clone());
    for planned in elements(&result) {
        assert_eq!(
            candidate.sequences[0]
                .tracks
                .iter()
                .flat_map(|t| &t.elements)
                .find(|e| e.id == planned.id),
            Some(planned)
        );
    }
}

#[test]
fn classic_binding_normalization_applies_after_raw_comparison_even_when_slightly_lower() {
    for (raw, expected) in [
        (30.005, None),
        (30.0, None),
        (30.006, Some(fps::fps(30, 1))),
    ] {
        let mut fixture = fps::fixture(true);
        fps::set_fps(&mut fixture, fps::fps(30005, 1000));
        fixture
            .media
            .get_mut("available")
            .unwrap()
            .metadata
            .frame_rate = Some(fps::fps(30, 1));
        fixture
            .media_packets
            .insert("available".into(), fps::packet(Some(raw)));
        let mut input = fixture.request();
        input.sequence_id = "other".into();
        let result = plan(&fixture, &input).unwrap();
        assert_eq!(result.decisions.raised_fps, expected);
        assert!(result.decisions.first_visual_fps.is_none());
        let candidate =
            crate::project_v4::public::plan_public(&fixture, &result.transaction).unwrap();
        assert_eq!(candidate.sequences, fixture.snapshot.project.sequences);
        let SettingsProfileV4::ClassicGlobal { settings } = &candidate.settings else {
            panic!()
        };
        assert_eq!(settings.fps, expected.unwrap_or(fps::fps(30005, 1000)));
        for scene in &candidate.sequences {
            fps::assert_existing_clock(scene, &fixture.snapshot.project.assets[0]);
        }
    }
}
