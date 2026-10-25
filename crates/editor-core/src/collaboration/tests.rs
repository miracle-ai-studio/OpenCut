use super::*;
use opencut_contracts::scene::*;
use serde_json::json;

fn document_scene() -> EditorDocument {
    use crate::scene::legacy::*;
    match convert_legacy_project(
        LegacyProjectInput::Base(&document()),
        &LegacyConversionInputs::default(),
    ) {
        LegacyConversionResult::Converted { project, .. } => *project,
        result => panic!("valid independent fixture must promote: {result:?}"),
    }
}
fn voice_scene(document: &mut EditorDocument) -> &mut SceneTrack {
    document.sequences[0]
        .tracks
        .iter_mut()
        .find(|t| t.id == "voice")
        .unwrap()
}
fn channel_scene(id: &str, parameter: ScalarParameter, value: f64) -> AnimationChannel {
    AnimationChannel::Scalar {
        id: id.into(),
        target: ChannelTarget::Track {
            track_id: "voice".into(),
        },
        binding: ScalarBinding::Property { parameter },
        keys: vec![ScalarKey {
            id: format!("{id}-key"),
            segment_id: None,
            time: opencut_contracts::Time {
                ticks: "0".into(),
                timebase: 1,
            },
            value,
            left_handle: None,
            right_handle: None,
            segment_to_next: ScalarSegment::Linear,
            tangent_mode: TangentMode::Aligned,
        }],
        extrapolation: Extrapolation {
            before: ExtrapolationMode::Hold,
            after: ExtrapolationMode::Hold,
        },
    }
}

#[test]
fn v4_selective_undo_never_splices_exact_time_or_rate_components() {
    use opencut_contracts::{Time, scene_bookmarks::SceneBookmark};
    let mut before = document_scene();
    before.sequences[0].bookmarks.push(SceneBookmark {
        id: "marker".into(),
        sequence_id: "main".into(),
        time: Time {
            ticks: "1".into(),
            timebase: 30,
        },
        duration: None,
        color: None,
        note: None,
    });
    let mut after = before.clone();
    after.sequences[0].bookmarks[0].time.ticks = "2".into();
    if let SettingsProfile::LegacyPerScene { formats } = &mut after.settings {
        formats[0].fps.numerator = 30;
    }
    let mut head = after.clone();
    head.sequences[0].bookmarks[0].time.timebase = 60;
    if let SettingsProfile::LegacyPerScene { formats } = &mut head.settings {
        formats[0].fps.denominator = 2;
    }
    crate::scene::validate_project_scene(&head).unwrap();
    let merged = selective_merge_scene(&after, &before, &head);
    assert!(merged.project.is_none());
    assert!(
        merged
            .conflicts
            .iter()
            .any(|c| c.path == "/sequences/main/bookmarks/marker/time")
    );
    assert!(
        merged
            .conflicts
            .iter()
            .any(|c| c.path == "/settings/formats/main/fps")
    );
}

#[test]
fn v4_gain_unit_undo_rejects_later_value_in_changed_unit() {
    let before = document_scene();
    let mut after = before.clone();
    voice_scene(&mut after).gain = Gain::Decibels { value: 0.0 };
    let mut head = after.clone();
    voice_scene(&mut head).gain = Gain::Decibels { value: -6.0 };
    let undo = selective_merge_scene(&after, &before, &head);
    assert!(undo.project.is_none());
    assert_eq!(undo.conflicts.len(), 1);
    assert_eq!(undo.conflicts[0].path, "/sequences/main/tracks/voice/gain");
}

#[test]
fn v4_channel_rebound_does_not_reinterpret_later_keys_in_old_unit() {
    let mut before = document_scene();
    voice_scene(&mut before).channels.push(channel_scene(
        "gain",
        ScalarParameter::GainLinearLegacy,
        1.0,
    ));
    let mut after = before.clone();
    voice_scene(&mut after).gain = Gain::Decibels { value: 0.0 };
    // The stored key happens to be 1 in either revision. It means amplitude 1
    // before rebinding and +1dB afterward, despite equal numeric JSON values.
    voice_scene(&mut after).channels[0] = channel_scene("gain", ScalarParameter::GainDecibels, 1.0);
    let mut head = after.clone();
    if let AnimationChannel::Scalar { keys, .. } = &mut voice_scene(&mut head).channels[0] {
        keys[0].value = 2.0;
    }
    crate::scene::validate_project_scene(&before).unwrap();
    crate::scene::validate_project_scene(&after).unwrap();
    crate::scene::validate_project_scene(&head).unwrap();
    let undo = selective_merge_scene(&after, &before, &head);
    assert!(undo.project.is_none());
    assert!(
        undo.conflicts
            .iter()
            .any(|c| c.path == "/sequences/main/tracks/voice/channels/gain")
    );
}

#[test]
fn v4_collection_merge_retains_later_channel_and_sequence_format() {
    let mut before = document_scene();
    voice_scene(&mut before).channels.push(channel_scene(
        "gain",
        ScalarParameter::GainLinearLegacy,
        1.0,
    ));
    let mut after = before.clone();
    if let AnimationChannel::Scalar { keys, .. } = &mut voice_scene(&mut after).channels[0] {
        keys[0].value = 0.5;
    }
    if let SettingsProfile::LegacyPerScene { formats } = &mut after.settings {
        formats[0].fps.numerator = 30;
    }
    let mut head = after.clone();
    let mut later = channel_scene("later-opacity", ScalarParameter::Opacity, 0.75);
    let main_id = head.sequences[0].main_track_id.clone();
    if let AnimationChannel::Scalar { target, .. } = &mut later {
        *target = ChannelTarget::Track {
            track_id: main_id.clone(),
        };
    }
    head.sequences[0]
        .tracks
        .iter_mut()
        .find(|t| t.id == main_id)
        .unwrap()
        .channels
        .push(later.clone());
    let mut scene = head.sequences[0].clone();
    scene.id = "later-scene".into();
    scene.name = "Later scene".into();
    scene.tracks.truncate(1);
    scene.tracks[0].id = "later-main".into();
    scene.tracks[0].channels.clear();
    scene.main_track_id = "later-main".into();
    head.sequences.push(scene);
    if let SettingsProfile::LegacyPerScene { formats } = &mut head.settings {
        let mut later = formats[0].clone();
        later.sequence_id = "later-scene".into();
        later.fps.numerator = 24;
        formats.insert(0, later);
    }
    crate::scene::validate_project_scene(&head).unwrap();
    let undo = selective_merge_scene(&after, &before, &head);
    assert!(undo.conflicts.is_empty(), "{:?}", undo.conflicts);
    let mut merged = undo.project.unwrap();
    crate::scene::validate_project_scene(&merged).unwrap();
    assert_eq!(
        merged.sequences[0]
            .tracks
            .iter()
            .find(|t| t.id == main_id)
            .unwrap()
            .channels[0],
        later
    );
    let AnimationChannel::Scalar { keys, .. } = &voice_scene(&mut merged).channels[0] else {
        panic!()
    };
    assert_eq!(keys[0].value, 1.0);
    let SettingsProfile::LegacyPerScene { formats } = &merged.settings else {
        panic!()
    };
    assert_eq!(formats[0].sequence_id, "later-scene");
    assert_eq!(formats[0].fps.numerator, 24);
    assert_eq!(formats[1].sequence_id, "main");
    assert_eq!(formats[1].fps.numerator, 25);
    assert!(
        undo.changes
            .iter()
            .any(|c| c.path == "/settings/formats/main/fps")
    );
    assert!(
        undo.changes
            .iter()
            .any(|c| c.path == "/sequences/main/tracks/voice/channels/gain/keys/gain-key/value")
    );
}

fn document() -> ProjectDocument {
    serde_json::from_value(json!({
        "id":"p","name":"Project","schema_version":2,"default_sequence_id":"main",
        "assets":[],"sequences":[{"id":"main","name":"Main","width":640,"height":360,
            "fps":{"numerator":25,"denominator":1},"caption_cues":[],
            "tracks":[{"id":"voice","name":"Voice","kind":"audio","enabled":true,
                "muted":false,"locked":false,"params":{"volume":1.0},"clips":[]}]}]
    }))
    .unwrap()
}

#[test]
fn reverts_one_track_property_and_preserves_later_caption() {
    let before = document();
    let mut after = before.clone();
    after.sequences[0].tracks[0]
        .params
        .insert("volume".into(), json!(0.4));
    let mut head = after.clone();
    head.sequences[0].caption_cues.push(
        serde_json::from_value(json!({
            "id":"human-caption","timeline_range":{"start":{"ticks":"0","timebase":25},
                "duration":{"ticks":"25","timebase":25}},"text":"Human text",
            "source_anchor":null,"style":{"font_family":"Arial","font_size":24,
                "color":"#ffffff","background_color":null,"max_width":600,
                "line_height":28,"text_align":"center"}
        }))
        .unwrap(),
    );
    let undone = selective_merge(&after, &before, &head);
    assert!(undone.conflicts.is_empty(), "{:?}", undone.conflicts);
    let project = undone.project.unwrap();
    assert_eq!(project.sequences[0].tracks[0].params["volume"], json!(1.0));
    assert_eq!(
        project.sequences[0].caption_cues,
        head.sequences[0].caption_cues
    );
    let redone = selective_merge(&before, &after, &project);
    assert!(redone.conflicts.is_empty());
    assert_eq!(
        redone.project.unwrap().sequences[0].tracks[0].params["volume"],
        json!(0.4)
    );
}

#[test]
fn reports_same_property_and_created_object_dependencies() {
    let before = document();
    let mut after = before.clone();
    after.sequences[0].tracks[0]
        .params
        .insert("volume".into(), json!(0.4));
    let mut head = after.clone();
    head.sequences[0].tracks[0]
        .params
        .insert("volume".into(), json!(0.8));
    let conflict = selective_merge(&after, &before, &head);
    assert!(conflict.project.is_none());
    assert_eq!(
        conflict.conflicts[0].path,
        "/sequences/main/tracks/voice/params/volume"
    );

    let mut created = before.clone();
    let mut track = created.sequences[0].tracks[0].clone();
    track.id = "agent-track".into();
    created.sequences[0].tracks.push(track);
    let mut dependent = created.clone();
    dependent.sequences[0].tracks[1].name = "Human renamed".into();
    let conflict = selective_merge(&created, &before, &dependent);
    assert!(conflict.project.is_none());
    assert_eq!(
        conflict.conflicts[0].path,
        "/sequences/main/tracks/agent-track"
    );
}

#[test]
fn reverses_pure_track_order_and_preserves_later_entities() {
    let mut before = document();
    for name in ["graphics", "captions"] {
        let mut track = before.sequences[0].tracks[0].clone();
        track.id = name.into();
        track.name = name.into();
        before.sequences[0].tracks.push(track);
    }
    let mut after = before.clone();
    after.sequences[0].tracks.swap(0, 1);
    let mut head = after.clone();
    let mut later = before.sequences[0].tracks[0].clone();
    later.id = "later".into();
    later.name = "Later".into();
    head.sequences[0].tracks.insert(1, later);
    head.sequences[0].tracks[0].muted = true;
    let undone = selective_merge(&after, &before, &head);
    assert!(undone.conflicts.is_empty(), "{:?}", undone.conflicts);
    assert!(
        undone
            .changes
            .iter()
            .any(|change| change.path == "/sequences/main/tracks/@order")
    );
    let tracks = &undone.project.unwrap().sequences[0].tracks;
    assert_eq!(
        tracks
            .iter()
            .map(|track| track.id.as_str())
            .collect::<Vec<_>>(),
        vec!["voice", "later", "graphics", "captions"]
    );
    assert!(
        tracks
            .iter()
            .find(|track| track.id == "graphics")
            .unwrap()
            .muted
    );

    let mut changed_order = head;
    changed_order.sequences[0].tracks.swap(0, 3);
    let conflict = selective_merge(&after, &before, &changed_order);
    assert!(conflict.project.is_none());
    assert!(
        conflict
            .conflicts
            .iter()
            .any(|item| item.path == "/sequences/main/tracks/@order")
    );
}

#[test]
fn v3_undo_preserves_later_independent_shape_and_detects_same_property() {
    let base = document();
    let before = HistoricalVisualDocument {
        schema_version: 3,
        base: base.clone(),
        visual: vec![],
        keyframe_curves: vec![],
    };
    let mut after = before.clone();
    after.base.sequences[0].tracks[0]
        .params
        .insert("volume".into(), json!(0.4));
    let mut head = after.clone();
    head.visual.push(
        serde_json::from_value(json!({"sequence_id":"main","shapes":[{
        "id":"card","track_id":"voice","timeline_range":{"start":{"ticks":"0","timebase":25},
        "duration":{"ticks":"25","timebase":25}},"primitive":{"kind":"rectangle",
        "width_px":100.0,"height_px":50.0},"fill":{"red":255,"green":0,"blue":0,"alpha":255},
        "position_x":0.0,"position_y":0.0,"rotation_degrees":0.0,"opacity":1.0}],
        "effects":[],"masks":[],"transitions":[]}))
        .unwrap(),
    );
    let undone = selective_merge_historical_visual(&after, &before, &head);
    assert!(undone.conflicts.is_empty(), "{:?}", undone.conflicts);
    let result = undone.project.unwrap();
    assert_eq!(
        result.base.sequences[0].tracks[0].params["volume"],
        json!(1.0)
    );
    assert_eq!(result.visual, head.visual);
    head.base.sequences[0].tracks[0]
        .params
        .insert("volume".into(), json!(0.8));
    let conflict = selective_merge_historical_visual(&after, &before, &head);
    assert!(conflict.project.is_none());
    assert_eq!(
        conflict.conflicts[0].path,
        "/base/sequences/main/tracks/voice/params/volume"
    );
}

#[test]
fn v3_undo_first_visual_node_preserves_later_shape_in_same_sequence() {
    let before = HistoricalVisualDocument {
        schema_version: 3,
        base: document(),
        visual: vec![],
        keyframe_curves: vec![],
    };
    let mut after = before.clone();
    after.visual.push(
        serde_json::from_value(json!({"sequence_id":"main","shapes":[{
        "id":"first","track_id":"voice","timeline_range":{"start":{"ticks":"0","timebase":25},
        "duration":{"ticks":"25","timebase":25}},"primitive":{"kind":"rectangle",
        "width_px":100.0,"height_px":50.0},"fill":{"red":255,"green":0,"blue":0,"alpha":255},
        "position_x":0.0,"position_y":0.0,"rotation_degrees":0.0,"opacity":1.0}],
        "effects":[],"masks":[],"transitions":[]}))
        .unwrap(),
    );
    let mut head = after.clone();
    let mut later = head.visual[0].shapes[0].clone();
    later.id = "later".into();
    head.visual[0].shapes.push(later);
    let undone = selective_merge_historical_visual(&after, &before, &head);
    assert!(undone.conflicts.is_empty(), "{:?}", undone.conflicts);
    assert_eq!(
        undone.project.unwrap().visual[0]
            .shapes
            .iter()
            .map(|shape| shape.id.as_str())
            .collect::<Vec<_>>(),
        ["later"]
    );
}

#[test]
fn v3_undo_of_one_keyframe_curve_preserves_an_independent_curve() {
    let base = document();
    let before = HistoricalVisualDocument {
        schema_version: 3,
        base: base.clone(),
        visual: vec![],
        keyframe_curves: vec![],
    };
    let curve = |id: &str, keyframe_id: &str| {
        serde_json::json!({
            "id":id,"schema_version":1,"sequence_id":"main","clip_id":"clip",
            "parameter":"opacity","keyframe_id":keyframe_id,
            "curve":{"x1":0.0,"y1":0.0,"x2":1.0,"y2":1.0}
        })
    };
    let first: opencut_contracts::historical_visual::HistoricalVisualKeyframeCurve =
        serde_json::from_value(curve("curve-a", "key-a")).unwrap();
    let second: opencut_contracts::historical_visual::HistoricalVisualKeyframeCurve =
        serde_json::from_value(curve("curve-b", "key-b")).unwrap();
    let mut after = before.clone();
    after.keyframe_curves = vec![first.clone()];
    let mut head = after.clone();
    head.keyframe_curves.push(second.clone());

    let undo = selective_merge_historical_visual(&after, &before, &head);
    assert!(undo.conflicts.is_empty(), "{:?}", undo.conflicts);
    assert_eq!(undo.project.unwrap().keyframe_curves, [second]);

    let mut conflicting_head = after.clone();
    conflicting_head.keyframe_curves[0].curve.y1 = 0.25;
    let conflict = selective_merge_historical_visual(&after, &before, &conflicting_head);
    assert_eq!(conflict.conflicts.len(), 1);
    assert!(conflict.conflicts[0].path.contains("curve-a"));
}
