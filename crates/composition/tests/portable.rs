use opencut_composition::{Composition, CompositionScope, compile, select_project_frame_time};
use opencut_contracts::{ProjectSnapshot, Time};
use opencut_media_contracts::{FrameAlignment, MediaTime};
use serde_json::json;

fn time(ticks: i64, base: u32) -> serde_json::Value {
    json!({"ticks":ticks.to_string(), "timebase":base})
}
fn fixture() -> ProjectSnapshot {
    let clip = |id: &str, asset: &str, start, duration| {
        json!({
        "id":id,"asset_id":asset,"timeline_range":{"start":time(start,2),"duration":time(duration,2)},
        "source_range":{"start":time(2,2),"duration":time(duration*2,2)},"params":{},"channels":[]})
    };
    let track = |id: &str, kind: &str, clip| {
        json!({
        "id":id,"name":id,"kind":kind,"enabled":true,"muted":false,"locked":false,"clips":[clip],"params":{}})
    };
    let mut video = clip("video-clip", "video", 1, 2);
    video["channels"] = json!([{"parameter":"position_x","keyframes":[
        {"id":"k0","time":time(0,1),"value":0.0,"interpolation":"linear"},
        {"id":"k1","time":time(1,1),"value":40.0,"interpolation":"linear"}]}]);
    let mut graphic = clip("image-clip", "image", 0, 2);
    graphic["source_range"] = serde_json::Value::Null;
    let mut snapshot: ProjectSnapshot = serde_json::from_value(json!({
        "revision":"fixed-revision","project":{"id":"project","name":"Portable composition","schema_version":2,
        "default_sequence_id":"main","assets":[
            {"id":"video","name":"video","kind":"video","uri":"memory://not-a-native-file","duration":time(10,1)},
            {"id":"image","name":"image","kind":"image","uri":"memory://image","duration":null},
            {"id":"audio","name":"audio","kind":"audio","uri":"unavailable://audio","duration":time(10,1)}],
        "sequences":[{"id":"main","name":"Main","width":160,"height":90,"fps":{"numerator":30,"denominator":1},
            "tracks":[track("graphics","graphic",graphic),track("video-track","video",video),track("audio-track","audio",clip("audio-clip","audio",6,2))],
            "caption_cues":[{"id":"cue","timeline_range":{"start":time(0,1),"duration":time(2,1)},"text":"Portable 字幕",
                "source_anchor":null,"style":{"font_family":"Unavailable font is a backend concern","font_size":20.0,
                "color":"#ffffff","background_color":null}}]}]}})).unwrap();
    let hash = "a".repeat(64);
    snapshot.project.assets[0].version = Some(opencut_contracts::AssetVersion {
        id: format!("av_{hash}"),
        content_hash: hash,
        size: 123,
    });
    snapshot
}

#[test]
fn pure_compilation_preserves_visual_semantics_and_audio_tail_without_host_resources() {
    let snapshot = fixture();
    let full = compile(&snapshot, "main", CompositionScope::AudioVisual).unwrap();
    let visual = compile(&snapshot, "main", CompositionScope::Visual).unwrap();
    assert_eq!(full.timeline.video_clips, visual.timeline.video_clips);
    assert_eq!(full.timeline.image_clips, visual.timeline.image_clips);
    assert_eq!(full.timeline.captions, visual.timeline.captions);
    assert_eq!(full.timeline.duration.seconds(), 4.0);
    assert_eq!(full.timeline.duration, visual.timeline.duration);
    assert_eq!(full.timeline.audio_clips.len(), 2);
    assert!(full.timeline.audio_clips[0].optional);
    assert!(!full.timeline.audio_clips[1].optional);
    assert!(visual.timeline.audio_clips.is_empty());
    assert_eq!(visual.timeline.image_clips[0].z_index, 0);
    let video = &visual.timeline.video_clips[0];
    assert_eq!(video.z_index, 1);
    assert_eq!(video.timeline_start.seconds(), 0.5);
    assert_eq!(video.source_start.seconds(), 1.0);
    assert_eq!(
        video.source_duration.seconds() / video.timeline_duration.seconds(),
        2.0
    );
    assert_eq!(video.curves.position_x.segments[0].end_value, 40.0);
    assert_eq!(video.input.version, snapshot.project.assets[0].version);
    let tail = select_project_frame_time(
        &Time {
            ticks: "3".into(),
            timebase: 1,
        },
        &snapshot.project.sequences[0].fps,
        FrameAlignment::Exact,
        visual.timeline.duration,
    )
    .unwrap();
    assert_eq!(tail.ticks, "3");
    assert!(
        select_project_frame_time(
            &Time {
                ticks: "4".into(),
                timebase: 1
            },
            &snapshot.project.sequences[0].fps,
            FrameAlignment::Exact,
            visual.timeline.duration
        )
        .is_err()
    );
}

#[test]
fn description_is_uri_independent_roundtrips_and_binds_to_non_file_handles() {
    let mut snapshot = fixture();
    let description = compile(&snapshot, "main", CompositionScope::AudioVisual).unwrap();
    for asset in &mut snapshot.project.assets {
        asset.uri = "https://another-host/asset".into();
    }
    assert_eq!(
        description,
        compile(&snapshot, "main", CompositionScope::AudioVisual).unwrap()
    );
    let encoded = serde_json::to_string(&description).unwrap();
    assert!(!encoded.contains("://"));
    assert_eq!(
        description,
        serde_json::from_str::<Composition>(&encoded).unwrap()
    );
    let mut unsupported = serde_json::to_value(&description).unwrap();
    unsupported["format"] = json!("opencut.composition.v2");
    assert!(serde_json::from_value::<Composition>(unsupported).is_err());
    let bound = description
        .timeline
        .clone()
        .try_map_inputs(|source| match source.asset_id.as_str() {
            "video" => Ok(10_u64),
            "image" => Ok(20),
            "audio" => Ok(30),
            _ => Err("unexpected identity"),
        })
        .unwrap();
    assert_eq!(bound.video_clips[0].input, 10);
    assert_eq!(bound.audio_clips[0].input, 10);
    assert_eq!(bound.audio_clips[1].input, 30);
    assert_eq!(bound.image_clips[0].input, 20);
    assert_eq!(bound.duration, description.timeline.duration);
    assert_eq!(
        bound.video_clips[0].curves,
        description.timeline.video_clips[0].curves
    );
    assert_eq!(bound.captions, description.timeline.captions);
}

#[test]
fn disabled_and_muted_tracks_have_explicit_duration_and_audio_semantics() {
    let mut snapshot = fixture();
    snapshot.project.sequences[0].tracks[2].muted = true;
    snapshot.project.sequences[0].tracks[1].muted = true;
    let full = compile(&snapshot, "main", CompositionScope::AudioVisual).unwrap();
    assert_eq!(
        full.timeline.duration,
        MediaTime {
            ticks: 2,
            timebase: 1
        }
    );
    assert_eq!(full.timeline.video_clips.len(), 1);
    assert!(full.timeline.audio_clips.is_empty());
    snapshot.project.sequences[0].tracks[1].enabled = false;
    let visual = compile(&snapshot, "main", CompositionScope::Visual).unwrap();
    assert!(visual.timeline.video_clips.is_empty());
    assert_eq!(visual.timeline.duration.seconds(), 2.0);
}
