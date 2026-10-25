use super::*;
use opencut_contracts::scene_bookmarks::SceneBookmark;
use opencut_editor_core::scene::{
    SampledChannel, element_local_time_scene, resolve_element_source_request_scene,
    sample_animation_channel_scene,
};

pub(super) fn fps(numerator: u32, denominator: u32) -> FrameRate {
    FrameRate {
        numerator,
        denominator,
    }
}

pub(super) fn packet(rate: Option<f64>) -> opencut_media_contracts::ClassicPacketRate {
    opencut_media_contracts::ClassicPacketRate {
        schema_version: 1,
        packet_count: if rate.is_some() { 100 } else { 0 },
        average_packet_rate: rate,
    }
}

pub(super) fn set_fps(fixture: &mut Fixture, rate: FrameRate) {
    let SettingsProfile::ClassicGlobal { settings } = &mut fixture.snapshot.project.settings else {
        panic!("Classic fixture expected")
    };
    settings.fps = rate;
}

fn scene(id: &str, populated: bool) -> Scene {
    let mut value = Fixture::new().snapshot.project.sequences.remove(0);
    value.id = id.into();
    value.main_track_id = format!("{id}-main");
    value.tracks[0].id = value.main_track_id.clone();
    value.tracks[1].id = format!("{id}-audio");
    if populated {
        let element_id = format!("{id}-old-video");
        let keys = [(0, 0.2), (1, 0.8)]
            .into_iter()
            .map(|(n, value)| ScalarKey {
                id: format!("{id}-key-{n}"),
                segment_id: None,
                time: t(n, 1),
                value,
                left_handle: None,
                right_handle: None,
                segment_to_next: ScalarSegment::Linear,
                tangent_mode: TangentMode::Flat,
            })
            .collect();
        value.tracks[0].elements.push(TimelineElement {
            id: element_id.clone(),
            name: "Existing retimed video".into(),
            timeline_range: TimeRange {
                start: t(1, 3),
                duration: t(2, 1),
            },
            // 10 - 1/7 - 48/7 = 3 source seconds at 3/2 speed.
            source_span: ElementSourceSpan::ClassicTrimmed {
                trim_start: t(1, 7),
                trim_end: t(48, 7),
                source_duration: Some(t(10, 1)),
            },
            enabled: true,
            content: ElementContent::Video {
                asset_id: "old".into(),
                video_stream: None,
                source_audio_enabled: false,
                audio: AudioStyle {
                    stream: None,
                    gain: Gain::Decibels { value: 0.0 },
                    muted: false,
                },
                retime: Retime {
                    rate: RationalRate {
                        numerator: 3,
                        denominator: 2,
                    },
                    maintain_pitch: true,
                    mapping: RetimeMapping::ClassicRoundedTicks { timebase: 120_000 },
                },
            },
            visual: Some(VisualStyle {
                profile: VisualProfile::ClassicCanvasV31,
                transform: Transform {
                    position_x_px: 0.0,
                    position_y_px: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation_degrees: 0.0,
                },
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                effects: vec![],
                masks: vec![],
            }),
            channels: vec![AnimationChannel::Scalar {
                id: format!("{id}-opacity"),
                target: ChannelTarget::Element { element_id },
                binding: ScalarBinding::Property {
                    parameter: ScalarParameter::Opacity,
                },
                keys,
                extrapolation: Extrapolation {
                    before: ExtrapolationMode::Hold,
                    after: ExtrapolationMode::Hold,
                },
            }],
        });
        value.bookmarks.push(SceneBookmark {
            id: format!("{id}-bookmark"),
            sequence_id: id.into(),
            time: t(2, 3),
            duration: Some(t(1, 7)),
            color: None,
            note: Some("Exact historical annotation".into()),
        });
    }
    value
}

pub(super) fn fixture(second_populated: bool) -> Fixture {
    let mut fixture = Fixture::new();
    let mut old = fixture::asset();
    old.id = "old".into();
    fixture.snapshot.project.assets.push(old);
    fixture.snapshot.project.sequences = vec![scene("s", true), scene("other", second_populated)];
    fixture
}

pub(super) fn assert_existing_clock(sequence: &Scene, asset: &ProjectAsset) {
    let element = &sequence.tracks[0].elements[0];
    assert_eq!(
        element.timeline_range,
        TimeRange {
            start: t(1, 3),
            duration: t(2, 1)
        }
    );
    let source = resolve_element_source_request_scene(element, Some(asset)).unwrap();
    assert_eq!(
        time(&source.start)
            .unwrap()
            .cmp_exact(time(&t(1, 7)).unwrap()),
        std::cmp::Ordering::Equal
    );
    assert_eq!(
        time(&source.duration)
            .unwrap()
            .cmp_exact(time(&t(3, 1)).unwrap()),
        std::cmp::Ordering::Equal
    );
    // 5/6 sequence seconds - 1/3 start = 1/2 local seconds.
    // Linear opacity from .2 to .8 over one local second must be .5,
    // independently of the 3/2 source speed and either project frame grid.
    let local = element_local_time_scene(&t(5, 6), &element.timeline_range.start).unwrap();
    assert_eq!(local, t(1, 2));
    let SampledChannel::Scalar { raw, physical, .. } =
        sample_animation_channel_scene(&element.channels[0], &local).unwrap()
    else {
        panic!()
    };
    assert!((raw - 0.5).abs() < 1e-12);
    assert!((physical - 0.5).abs() < 1e-12);
    assert_eq!(sequence.bookmarks[0].time, t(2, 3));
    assert_eq!(sequence.bookmarks[0].duration, Some(t(1, 7)));
}

#[test]
fn binding_in_nonempty_other_scene_updates_global_fps_and_preserves_both_complete_scenes() {
    let mut fixture = fixture(true);
    fixture
        .media
        .get_mut("available")
        .unwrap()
        .metadata
        .frame_rate = Some(fps(5994, 100));
    let mut input = fixture.request();
    input.sequence_id = "other".into();
    // Mixed observations: only videos compete; unknown video FPS is reported.
    for (id, kind, rate) in [
        ("low", AssetKind::Video, Some(fps(25, 1))),
        ("audio", AssetKind::Audio, None),
        ("image", AssetKind::Image, None),
        ("unknown", AssetKind::Video, Some(fps(120, 1))),
    ] {
        let mut asset = fixture::asset();
        asset.kind = kind.clone();
        asset.metadata.frame_rate = rate;
        if kind == AssetKind::Image {
            asset.metadata.audio = AudioPresence::Absent {};
        }
        fixture.media.insert(id.into(), asset.clone());
        input.bindings.push(MediaBinding {
            media_id: id.into(),
            version: asset.version,
            asset_id: format!("import-{id}"),
            name: id.into(),
            mode: MediaBindingMode::Bind,
        });
    }
    fixture.media_packets.insert("unknown".into(), packet(None));
    let before = fixture.snapshot.clone();
    let result = plan(&fixture, &input).unwrap();
    assert_eq!(result.decisions.raised_fps, Some(fps(60000, 1001)));
    assert!(result.decisions.first_visual_fps.is_none());
    assert_eq!(
        result.decisions.video_asset_ids_without_fps,
        vec!["import-unknown"]
    );
    let candidate = crate::scene::public::plan_public(&fixture, &result.transaction).unwrap();
    let SettingsProfile::ClassicGlobal { settings } = &candidate.settings else {
        panic!()
    };
    assert_eq!(settings.fps, fps(60000, 1001));
    assert_eq!(candidate.sequences, before.project.sequences);
    for scene in &candidate.sequences {
        assert_existing_clock(scene, &before.project.assets[0]);
    }
    assert_eq!(fixture.snapshot, before);
}

#[test]
fn empty_other_scene_first_video_lowers_global_fps_but_keeps_nonempty_scene_exact() {
    let mut fixture = fixture(false);
    set_fps(&mut fixture, fps(60, 1));
    fixture.snapshot.project.assets[0].metadata.frame_rate = Some(fps(60, 1));
    fixture
        .asset_packets
        .insert("old".into(), packet(Some(24.0)));
    let mut overlay = fixture.snapshot.project.sequences[1].tracks[0].clone();
    overlay.id = "other-overlay".into();
    fixture.snapshot.project.sequences[1].tracks.push(overlay);
    let mut input = fixture.request();
    input.sequence_id = "other".into();
    input.bindings.clear();
    let mut video = fixture.video(MediaTiming::PinnedTimeline {
        source_start: t(1, 7),
        duration: t(2, 1),
        rate: RationalRate {
            numerator: 3,
            denominator: 2,
        },
        maintain_pitch: true,
    });
    video.track_id = "other-overlay".into();
    video.asset_id = "old".into();
    video.start = t(1, 16);
    input.placements.push(video);
    let before = fixture.snapshot.clone();
    let result = plan(
        &fixture,
        &MediaImportRequest {
            project_id: input.project_id,
            expected_revision: input.expected_revision,
            sequence_id: input.sequence_id,
            idempotency_key: input.idempotency_key,
            label: input.label,
            bindings: input.bindings,
            placements: input.placements,
            ripple: input.ripple,
            new_tracks: vec![],
        },
    )
    .unwrap();
    assert!(result.decisions.raised_fps.is_none());
    assert_eq!(result.decisions.first_visual_fps, Some(fps(24, 1)));
    // First placement uses the old 60 FPS grid: round(60/16)=4 -> 1/15.
    assert_eq!(
        time(&elements(&result)[0].timeline_range.start)
            .unwrap()
            .cmp_exact(time(&t(1, 15)).unwrap()),
        std::cmp::Ordering::Equal
    );
    let candidate = crate::scene::public::plan_public(&fixture, &result.transaction).unwrap();
    assert_eq!(candidate.sequences[0], before.project.sequences[0]);
    assert_existing_clock(&candidate.sequences[0], &before.project.assets[0]);
    assert_eq!(
        candidate.sequences[1].tracks[2].elements[0]
            .timeline_range
            .duration,
        t(2, 1)
    );
    let SettingsProfile::ClassicGlobal { settings } = &candidate.settings else {
        panic!()
    };
    assert_eq!(settings.fps, fps(24, 1));
    assert_eq!(fixture.snapshot, before);
}

#[test]
fn existing_unknown_video_warns_once_for_actual_placements_and_ignores_unreferenced_assets() {
    let mut fixture = fixture(true);
    fixture.snapshot.project.assets[0].metadata.frame_rate = Some(fps(120, 1));
    fixture.asset_packets.insert("old".into(), packet(None));
    let mut unused = fixture.snapshot.project.assets[0].clone();
    unused.id = "unused-unknown".into();
    unused.metadata.frame_rate = None;
    fixture.snapshot.project.assets.push(unused);
    let mut first = fixture.video(fixture.source_timing());
    first.element_id = "unknown-first".into();
    first.asset_id = "old".into();
    first.track_id = "other-main".into();
    first.start = t(3, 1);
    let mut second = first.clone();
    second.element_id = "unknown-second".into();
    second.start = t(4, 1);
    let input = MediaImportRequest {
        project_id: "p".into(),
        expected_revision: "fixed".into(),
        sequence_id: "other".into(),
        idempotency_key: "unknown-placement".into(),
        label: "Place existing unknown video twice".into(),
        bindings: vec![],
        placements: vec![first, second],
        ripple: false,
        new_tracks: vec![],
    };
    let result = plan(&fixture, &input).unwrap();
    assert_eq!(result.decisions.video_asset_ids_without_fps, vec!["old"]);
    assert!(result.decisions.raised_fps.is_none());
    assert!(result.decisions.first_visual_fps.is_none());
    let candidate = crate::scene::public::plan_public(&fixture, &result.transaction).unwrap();
    assert_eq!(candidate.settings, fixture.snapshot.project.settings);
    assert_eq!(
        candidate.sequences[0],
        fixture.snapshot.project.sequences[0]
    );
    for scene in &candidate.sequences {
        assert_existing_clock(scene, &fixture.snapshot.project.assets[0]);
    }

    // A binding-only request does not reference either existing unknown asset.
    let unplaced = plan(&fixture, &fixture.request()).unwrap();
    assert!(unplaced.decisions.video_asset_ids_without_fps.is_empty());
}
