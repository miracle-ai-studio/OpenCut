use super::*;
use crate::Time;
use serde_json::json;

#[test]
fn global_settings_cannot_accept_a_hidden_sequence_override() {
    let settings = json!({"profile":"classic_global","settings":{
        "fps":{"numerator":30000,"denominator":1001},
        "canvas_size":{"width":1920,"height":1080},"canvas_size_mode":"preset",
        "last_custom_canvas_size":null,"original_canvas_size":null,
        "background":{"kind":"main_track_blur","intensity":200.0}
    }});
    let profile: SettingsProfileV4 = serde_json::from_value(settings.clone()).unwrap();
    assert_eq!(serde_json::to_value(profile).unwrap(), settings);
    let mut mixed = settings;
    mixed["formats"] = json!([]);
    assert!(serde_json::from_value::<SettingsProfileV4>(mixed).is_err());
}

#[test]
fn gain_units_and_equal_endpoint_curve_handles_survive_roundtrip() {
    for gain in [
        GainV4::LinearLegacy { value: 0.0 },
        GainV4::Decibels { value: 20.0 },
    ] {
        let bytes = serde_json::to_vec(&gain).unwrap();
        assert_eq!(serde_json::from_slice::<GainV4>(&bytes).unwrap(), gain);
    }
    let key = ScalarKeyV4 {
        id: "k".into(),
        segment_id: Some("curve".into()),
        time: Time {
            ticks: "1001".into(),
            timebase: 30000,
        },
        value: 1.0,
        left_handle: Some(CurveHandleV4 {
            dt: Time {
                ticks: "-1001".into(),
                timebase: 90000,
            },
            dv: 0.3,
        }),
        right_handle: Some(CurveHandleV4 {
            dt: Time {
                ticks: "1001".into(),
                timebase: 90000,
            },
            dv: 0.3,
        }),
        segment_to_next: ScalarSegmentV4::Bezier,
        tangent_mode: TangentModeV4::Broken,
    };
    assert_eq!(
        serde_json::from_slice::<ScalarKeyV4>(&serde_json::to_vec(&key).unwrap()).unwrap(),
        key
    );
}

#[test]
fn masks_fonts_and_effects_reject_untyped_or_unknown_payloads() {
    assert!(
        serde_json::from_value::<EffectKindV4>(
            json!({"kind":"classic_blur","intensity":15,"radius_px":3})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<FontBindingV4>(
            json!({"kind":"locked","font_id":"f","path":"/tmp/font.ttf"})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<MaskShapeV4>(json!({"kind":"freeform","path":"M 0 0"})).is_err()
    );
    assert!(
        serde_json::from_value::<DiscreteValueV4>(json!({"kind":"stroke_align","value":3}))
            .is_err()
    );
}

#[test]
fn audio_probe_knowledge_cannot_be_conflated_with_null_or_missing_metadata() {
    for audio in [
        json!({"status":"unknown"}),
        json!({"status":"absent"}),
        json!({"status":"present","metadata":null}),
        json!({"status":"present","metadata":{
            "sample_rate":48000,"channels":2,"channel_layout":"stereo"
        }}),
    ] {
        let decoded: AudioPresenceV4 = serde_json::from_value(audio.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), audio);
    }
    for invalid in [
        json!(null),
        json!({"status":"unknown","metadata":null}),
        json!({"status":"absent","metadata":{"sample_rate":48000,"channels":2}}),
    ] {
        assert!(serde_json::from_value::<AudioPresenceV4>(invalid).is_err());
    }
    assert!(
        serde_json::from_value::<MediaMetadataV4>(json!({
            "intrinsic_size":null,"frame_rate":null,"rotation_degrees":null
        }))
        .is_err()
    );
}
