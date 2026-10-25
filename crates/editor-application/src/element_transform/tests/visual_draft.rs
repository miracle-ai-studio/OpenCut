use super::*;
use opencut_contracts::visual_edit_draft::{VisualEditDraftIntent as Intent, VisualProperty as P};
fn prepare(
    store: &Store,
    tx: &ApplyProject,
    p: P,
) -> Result<(EditorSnapshot, String), EditorError> {
    crate::visual_edit_draft::prepare(store, tx, "scene", &t(2), Intent::Property { property: p })
}
#[test]
fn every_property_changes_only_its_field_without_writing_or_rounding_others() {
    for p in [
        P::PositionXPx,
        P::PositionYPx,
        P::ScaleX,
        P::ScaleY,
        P::RotationDegrees,
        P::Opacity,
    ] {
        let mut store = Store::new();
        store
            .element_mut()
            .visual
            .as_mut()
            .unwrap()
            .transform
            .scale_y = 1.0000000000000002;
        let original = store.project.clone();
        let tx = store.tx(|e| {
            let v = e.visual.as_mut().unwrap();
            match p {
                P::PositionXPx => v.transform.position_x_px = -23.25,
                P::PositionYPx => v.transform.position_y_px = 72.,
                P::ScaleX => v.transform.scale_x = -1.25,
                P::ScaleY => v.transform.scale_y = 0.,
                P::RotationDegrees => v.transform.rotation_degrees = 370.,
                P::Opacity => v.opacity = 0.75,
            }
        });
        let (candidate, digest) = prepare(&store, &tx, p).unwrap();
        let ProjectEditOperation::ElementPut { element, .. } = &tx.operations[0] else {
            unreachable!()
        };
        assert_eq!(candidate_element(&candidate), element.as_ref());
        assert_eq!(candidate.revision, "head");
        assert!(digest.starts_with("sha256:"));
        assert_eq!(store.project, original);
    }
}
#[test]
fn properties_preserve_unrelated_channels_but_reject_own_animation_and_invalid_empty_channels() {
    let mut store = Store::new();
    let unrelated = channel("first", "x", ScalarParameter::PositionXPx, 7., 9.);
    let mut empty = channel("first", "empty", ScalarParameter::Opacity, 1., 0.5);
    let AnimationChannel::Scalar { keys, .. } = &mut empty else {
        unreachable!()
    };
    keys.clear();
    store.element_mut().channels = vec![unrelated];
    let tx = store.tx(|e| e.visual.as_mut().unwrap().opacity = 0.75);
    let (candidate, _) = prepare(&store, &tx, P::Opacity).unwrap();
    assert_eq!(
        candidate_element(&candidate).channels,
        store.element().channels
    );
    store.element_mut().channels.push(empty);
    assert_eq!(
        prepare(&store, &tx, P::Opacity).unwrap_err().code,
        "INVALID_PROJECT"
    );
    store.element_mut().channels.pop();
    store.element_mut().channels.push(channel(
        "first",
        "opacity",
        ScalarParameter::Opacity,
        1.,
        0.5,
    ));
    let tx = store.tx(|e| e.visual.as_mut().unwrap().opacity = 0.75);
    assert!(
        prepare(&store, &tx, P::Opacity)
            .unwrap_err()
            .message
            .contains("animated")
    );
}
#[test]
fn property_drafts_reject_smuggled_fields_channels_and_owners() {
    for variant in 0..6 {
        let store = Store::new();
        let mut tx = store.tx(|e| {
            e.visual.as_mut().unwrap().opacity = 0.75;
            match variant {
                0 => e.name = "unrelated edit".into(),
                1 => e.visual.as_mut().unwrap().transform.scale_x = 2.,
                2 => e.enabled = false,
                3 => e
                    .channels
                    .push(channel("first", "x", ScalarParameter::PositionXPx, 7., 9.)),
                _ => (),
            }
        });
        if variant == 4 {
            let ProjectEditOperation::ElementPut { track_id, .. } = &mut tx.operations[0] else {
                unreachable!()
            };
            *track_id = "graphics-second".into();
        }
        if variant == 5 {
            tx.operations.push(tx.operations[0].clone());
        }
        assert!(prepare(&store, &tx, P::Opacity).is_err());
    }
}
#[test]
fn property_drafts_retain_fixed_head_auth_lock_active_time_and_native_bounds() {
    for variant in 0..8 {
        let mut store = Store::new();
        match variant {
            0 => store.advance_at = Some(2),
            1 => store.deny_at = Some(2),
            2 => store.project.sequences[0].tracks[1].locked = true,
            3 => store.element_mut().enabled = false,
            4 => store.element_mut().timeline_range.start = t(3),
            5 => store.wrong_scope = true,
            _ => (),
        }
        let tx = store.tx(|e| {
            e.visual.as_mut().unwrap().opacity = if variant == 6 {
                -0.1
            } else if variant == 7 {
                1.1
            } else {
                0.75
            }
        });
        assert!(
            prepare(&store, &tx, P::Opacity).is_err(),
            "variant {variant}"
        );
    }
}
#[test]
fn explicit_gesture_still_removes_only_its_scale_axis() {
    let mut store = Store::new();
    store.element_mut().channels =
        vec![channel("first", "scale-x", ScalarParameter::ScaleX, 1., 2.)];
    let tx = store.tx(|e| {
        e.visual.as_mut().unwrap().transform.scale_x = 2.;
        strip(e, TransformGestureIntent::ScaleX);
    });
    let (expected, _) =
        prepare_draft(&store, &tx, "scene", &t(2), TransformGestureIntent::ScaleX).unwrap();
    let (actual, _) = crate::visual_edit_draft::prepare(
        &store,
        &tx,
        "scene",
        &t(2),
        Intent::Gesture {
            gesture: TransformGestureIntent::ScaleX,
        },
    )
    .unwrap();
    assert_eq!(actual, expected);
    assert!(prepare(&store, &tx, P::ScaleX).is_err());
}
