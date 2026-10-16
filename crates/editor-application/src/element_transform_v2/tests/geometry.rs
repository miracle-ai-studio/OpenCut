use super::*;

fn media_store(kind: &str) -> Store {
    let mut store = Store::new();
    store.project.assets.push(serde_json::from_value(json!({
        "id":"asset", "name":"Observed source", "kind":if kind == "video" { "video" } else { "image" },
        "version":{"id":format!("av_{}", "a".repeat(64)),"content_hash":"a".repeat(64),"size":12},
        "duration":if kind == "video" { Some(t(12)) } else { None },
        "origin":{"kind":"legacy_locator","uri":"opencut-media://media_a"},
        "metadata":{"mime_type":null,"intrinsic_size":{"width":640,"height":360},"frame_rate":null,"rotation_degrees":90,"audio":{"status":"absent"}},
        "license":null
    })).unwrap());
    let track = &mut store.project.sequences[0].tracks[1];
    track.elements.truncate(1);
    let element = &mut track.elements[0];
    element.content = match kind {
        "video" => {
            track.kind = TrackKindV4::Video;
            element.source_span = ElementSourceSpanV4::ClassicTrimmed {
                trim_start: t(1),
                trim_end: t(7),
                source_duration: Some(t(12)),
            };
            serde_json::from_value(json!({"kind":"video","asset_id":"asset","audio":{"stream":null,"gain":{"unit":"decibels","value":0},"muted":false},"retime":{"rate":{"numerator":1,"denominator":1},"maintain_pitch":false,"mapping":{"kind":"exact_rational"}}})).unwrap()
        }
        "sticker" => ElementContentV4::Sticker {
            asset_id: "asset".into(),
            // This authored legacy extent deliberately differs from the fixed
            // observation; it must never substitute for actual image metadata.
            intrinsic_size: CanvasSizeV4 {
                width: 17,
                height: 31,
            },
        },
        _ => ElementContentV4::Image {
            asset_id: "asset".into(),
        },
    };
    store
}

#[test]
fn animated_graphic_bounds_use_the_same_sample_as_transform_and_track_opacity() {
    let mut store = Store::new();
    store.element_mut().channels = vec![
        channel("first", "scale", ScalarParameterV4::ScaleX, 1., 3.),
        channel(
            "first",
            "rotate",
            ScalarParameterV4::RotationDegrees,
            10.,
            30.,
        ),
        channel("first", "opacity", ScalarParameterV4::Opacity, 0.2, 0.8),
    ];
    let mut track_opacity = channel(
        "first",
        "track-opacity",
        ScalarParameterV4::Opacity,
        0.5,
        0.5,
    );
    let AnimationChannelV4::Scalar { target, .. } = &mut track_opacity else {
        unreachable!()
    };
    *target = ChannelTargetV4::Track {
        track_id: "graphics".into(),
    };
    store.project.sequences[0].tracks[1]
        .channels
        .push(track_opacity);
    let before = store.project.clone();
    let result = sample_v2(&store, &store.sample()).unwrap();
    let entry = &result.elements[0];
    let geometry = entry.geometry.as_ref().unwrap();
    assert_eq!(
        geometry.intrinsic_size,
        CanvasSizeV4 {
            width: 100,
            height: 100
        }
    );
    assert!((geometry.quad.width - 360.).abs() < 1e-9);
    assert_eq!(geometry.quad.height, 360.);
    assert_eq!(
        (geometry.quad.center_x, geometry.quad.center_y),
        (167., 87.)
    );
    assert!((geometry.quad.rotation_radians - 20.0_f64.to_radians()).abs() < 1e-9);
    assert!((geometry.quad.opacity - 0.25).abs() < 1e-9);
    assert!(!geometry.quad.flip_x && !geometry.quad.flip_y);
    assert_eq!(store.project, before);

    // The old static operation stays strict. V2 does not call it or broaden it.
    let old = crate::scene_authoring_v1::plan(
        &store,
        &opencut_contracts::scene_authoring_v1::SceneGeometryRequestV1 {
            project_id: "project".into(),
            expected_revision: "head".into(),
            sequence_id: "scene".into(),
            element_id: "first".into(),
            time: t(2),
        },
    )
    .unwrap_err();
    assert_eq!(old.code, "UNSUPPORTED_VISUAL_FEATURE");
    assert!(old.message.contains("statically"));
}

#[test]
fn image_video_and_sticker_use_observed_upright_dimensions_and_preserve_f64() {
    for kind in ["image", "video", "sticker"] {
        let mut store = media_store(kind);
        store.element_mut().channels.push(channel(
            "first",
            "scale",
            ScalarParameterV4::ScaleX,
            -0.500000003,
            -0.500000003,
        ));
        let result = sample_v2(&store, &store.sample()).unwrap();
        let geometry = result.elements[0].geometry.as_ref().unwrap();
        assert_eq!(
            geometry.intrinsic_size,
            CanvasSizeV4 {
                width: 360,
                height: 640
            },
            "{kind}"
        );
        assert!(
            (geometry.quad.width - 101.25 * 0.500000003).abs() < 1e-9,
            "{kind}"
        );
        assert_eq!(geometry.quad.height, 360., "{kind}");
        assert!(geometry.quad.flip_x, "{kind}");
        assert_ne!(
            geometry.quad.width,
            f64::from(geometry.quad.width as f32),
            "{kind}"
        );
        let wire = serde_json::to_value(&result).unwrap();
        assert!(wire["elements"][0]["geometry"]["quad"]["centerX"].is_number());
        assert!(
            wire["elements"][0]["geometry"]["quad"]
                .get("center_x")
                .is_none()
        );
    }
}

#[test]
fn geometry_never_guesses_missing_orientation_or_dimensions_including_sticker() {
    for kind in ["image", "video", "sticker"] {
        let mut store = media_store(kind);
        store.project.assets[0].metadata.intrinsic_size = None;
        let error = sample_v2(&store, &store.sample()).unwrap_err();
        assert_eq!(error.code, "UNSUPPORTED_VISUAL_FEATURE", "{kind}");
        assert!(error.message.contains("observed media dimensions"));
        let mut store = media_store(kind);
        store.project.assets[0].metadata.rotation_degrees = Some(45.);
        let error = sample_v2(&store, &store.sample()).unwrap_err();
        assert_eq!(error.code, "UNSUPPORTED_VISUAL_FEATURE", "{kind}");
        assert!(error.message.contains("quarter-turn"));
    }
    for scale in [0., f64::MAX] {
        let mut store = Store::new();
        store
            .element_mut()
            .visual
            .as_mut()
            .unwrap()
            .transform
            .scale_x = scale;
        assert!(sample_v2(&store, &store.sample()).is_err());
    }
}

#[test]
fn text_samples_transform_without_inventing_bounds_or_reading_fonts() {
    let (snapshot, _) = crate::text_canvas_v1::tests::fixture();
    let mut store = Store::new();
    store.project = snapshot.project;
    store.project.id = "project".into();
    store.project.sequences[0].id = "scene".into();
    store.project.sequences[0].tracks[1].elements[0]
        .channels
        .push(channel(
            "e",
            "text-scale",
            ScalarParameterV4::ScaleX,
            1.,
            3.,
        ));
    let mut input = store.sample();
    input.time = t(1);
    input.element_ids = vec!["e".into()];
    let result = sample_v2(&store, &input).unwrap();
    assert!((result.elements[0].transform.scale_x - 2.).abs() < 1e-9);
    assert_eq!(result.elements[0].geometry, None);
    assert!(serde_json::to_value(result).unwrap()["elements"][0]["geometry"].is_null());
    assert_eq!((store.reads.get(), store.authorizations.get()), (2, 2));
}
