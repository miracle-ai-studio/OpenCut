use super::*;
mod fixture;
use crate::scene::public::{apply_public, plan_public};
use fixture::{Fixture, t};

fn put(plan: &SourceAudioPlan, id: &str) -> TimelineElement {
    plan.transaction
        .operations
        .iter()
        .find_map(|op| match op {
            ProjectEditOperation::ElementPut { element, .. } if element.id == id => {
                Some((**element).clone())
            }
            _ => None,
        })
        .unwrap()
}

#[test]
fn separation_copies_exact_source_clock_gain_and_only_volume_animation_with_new_ids() {
    let mut fixture = Fixture::new();
    let input = fixture.request();
    let original = fixture.source().clone();
    let result = plan(&fixture, &input).unwrap();
    assert_eq!(plan(&fixture, &input).unwrap(), result);
    assert_eq!(fixture.commits, 0);
    let audio = put(&result, "detached");
    assert_eq!(audio.timeline_range, original.timeline_range);
    assert_eq!(audio.source_span, original.source_span);
    assert_eq!(audio.name, original.name);
    assert!(!original.enabled && audio.enabled);
    assert!(audio.visual.is_none());
    let ElementContent::Video {
        audio: original_audio,
        retime: original_retime,
        ..
    } = &original.content
    else {
        panic!()
    };
    let ElementContent::Audio {
        asset_id,
        audio: copied_audio,
        retime,
        ..
    } = &audio.content
    else {
        panic!()
    };
    assert_eq!(asset_id, "asset");
    assert_eq!(copied_audio, original_audio);
    assert_eq!(retime, original_retime);
    assert_eq!(audio.channels.len(), 1);
    let AnimationChannel::Scalar {
        id,
        target,
        keys,
        binding,
        extrapolation,
    } = &audio.channels[0]
    else {
        panic!()
    };
    let AnimationChannel::Scalar {
        id: old_id,
        keys: old_keys,
        binding: old_binding,
        extrapolation: old_extrapolation,
        ..
    } = &original.channels[0]
    else {
        panic!()
    };
    assert_ne!(id, old_id);
    assert_eq!(
        target,
        &ChannelTarget::Element {
            element_id: "detached".into()
        }
    );
    assert_eq!(binding, old_binding);
    assert_eq!(extrapolation, old_extrapolation);
    for (key, old) in keys.iter().zip(old_keys) {
        assert_ne!(key.id, old.id);
        let mut normalized = key.clone();
        normalized.id = old.id.clone();
        normalized.segment_id = old.segment_id.clone();
        assert_eq!(&normalized, old);
        if old.segment_id.is_some() {
            assert_ne!(key.segment_id, old.segment_id);
        }
    }
    assert_eq!(
        result.decision.source_request,
        opencut_contracts::TimeRange {
            start: t(1, 1),
            duration: t(360000, 120000)
        }
    );
    assert!(!result.decision.source_audio_enabled);
    let json = serde_json::to_string(&result).unwrap();
    assert!(
        !json.contains("/private/source")
            && !json.contains("\"uri\"")
            && !json.contains("legacy_locator")
    );
    let restored: SourceAudioPlan = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, result);
    let public = plan_public(&fixture, &result.transaction).unwrap();
    assert_eq!(public.sequences[0].tracks[1].elements[0], audio);
    let submitted = apply_public(&mut fixture, &result.transaction).unwrap();
    assert_eq!(
        fixture.current.project.sequences[0].tracks[1].elements[0],
        audio
    );
    assert_eq!(
        apply_public(&mut fixture, &result.transaction).unwrap(),
        submitted
    );
    assert_eq!(fixture.commits, 1);
}

#[test]
fn restoring_reenables_source_and_keeps_independently_edited_detached_audio() {
    let mut fixture = Fixture::new();
    let separate = plan(&fixture, &fixture.request()).unwrap();
    apply_public(&mut fixture, &separate.transaction).unwrap();
    let mut audio = fixture.current.project.sequences[0].tracks[1].elements[0].clone();
    audio.timeline_range.start = t(8, 1);
    if let ElementContent::Audio { audio, .. } = &mut audio.content {
        audio.muted = false;
    }
    let edit = ApplyProject {
        branch_name: None,
        project_id: "p".into(),
        expected_revision: fixture.current.revision.clone(),
        idempotency_key: "independent".into(),
        label: "Move detached audio".into(),
        operations: vec![ProjectEditOperation::ElementPut {
            sequence_id: "s".into(),
            track_id: "audio".into(),
            element: Box::new(audio.clone()),
        }],
    };
    apply_public(&mut fixture, &edit).unwrap();
    let source = fixture.source().clone();
    let mut input = fixture.request();
    input.idempotency_key = "restore".into();
    input.command = SourceAudioCommand::Restore;
    let result = plan(&fixture, &input).unwrap();
    assert!(result.decision.detached.is_none() && result.decision.source_audio_enabled);
    assert_eq!(result.transaction.operations.len(), 1);
    let mut expected_source = source;
    if let ElementContent::Video {
        source_audio_enabled,
        ..
    } = &mut expected_source.content
    {
        *source_audio_enabled = true;
    }
    assert_eq!(put(&result, "video"), expected_source);
    apply_public(&mut fixture, &result.transaction).unwrap();
    assert_eq!(
        fixture.current.project.sequences[0].tracks[1].elements[0],
        audio
    );
    assert_eq!(fixture.source(), &expected_source);
}

#[test]
fn first_available_uses_stable_order_half_open_intervals_or_creates_default_track() {
    let mut fixture = Fixture::new();
    fixture.current.project.sequences[0].tracks[1].locked = true;
    let mut second = fixture.current.project.sequences[0].tracks[1].clone();
    second.id = "second-audio".into();
    second.locked = false;
    let mut adjacent = fixture.source().clone();
    adjacent.id = "adjacent".into();
    adjacent.name = "Adjacent".into();
    adjacent.visual = None;
    adjacent.channels.clear();
    adjacent.timeline_range.start = t(7, 3);
    let ElementContent::Video {
        asset_id,
        audio,
        retime,
        ..
    } = adjacent.content
    else {
        panic!()
    };
    adjacent.content = ElementContent::Audio {
        asset_id,
        audio,
        retime,
    };
    second.elements.push(adjacent);
    fixture.current.project.sequences[0].tracks.push(second);
    let mut input = fixture.request();
    input.command = SourceAudioCommand::Separate {
        audio_element_id: "detached".into(),
        destination: SourceAudioDestination::FirstAvailable {
            fallback_track_id: "new-audio".into(),
        },
    };
    let result = plan(&fixture, &input).unwrap();
    assert_eq!(
        result.decision.detached.as_ref().unwrap().track_id,
        "second-audio"
    );
    assert!(!result.decision.detached.as_ref().unwrap().track_created);
    fixture.current.project.sequences[0].tracks[2].locked = true;
    let fallback = plan(&fixture, &input).unwrap();
    assert_eq!(
        fallback.decision.detached.as_ref().unwrap().track_id,
        "new-audio"
    );
    assert!(fallback.decision.detached.as_ref().unwrap().track_created);
    let public = plan_public(&fixture, &fallback.transaction).unwrap();
    let track = public.sequences[0].tracks.last().unwrap();
    assert_eq!(track.name, "Audio track");
    assert!(track.enabled && !track.muted && !track.locked);
    assert_eq!(track.gain, Gain::Decibels { value: 0.0 });
}

#[test]
fn capability_scope_locks_state_and_identity_collisions_fail_without_mutation() {
    let mut fixture = Fixture::new();
    let input = fixture.request();
    fixture.deny = true;
    assert_eq!(plan(&fixture, &input).unwrap_err().code, "NOT_AUTHORIZED");
    assert_eq!(fixture.reads.get(), 0);
    fixture.deny = false;
    let mut changed = input.clone();
    changed.expected_revision = "stale".into();
    assert_eq!(
        plan(&fixture, &changed).unwrap_err().code,
        "REVISION_CONFLICT"
    );
    changed = input.clone();
    changed.track_id = "audio".into();
    assert_eq!(plan(&fixture, &changed).unwrap_err().code, "NOT_FOUND");
    fixture.current.project.assets[0].metadata.audio = AudioPresence::Unknown {};
    assert_eq!(
        plan(&fixture, &input).unwrap_err().code,
        "MEDIA_METADATA_REQUIRED"
    );
    fixture.current.project.assets[0].metadata.audio = AudioPresence::Absent {};
    assert_eq!(plan(&fixture, &input).unwrap_err().code, "NO_AUDIO_TRACK");
    fixture.current.project.assets[0].metadata.audio = AudioPresence::Present { metadata: None };
    fixture.current.project.sequences[0].tracks[0].locked = true;
    assert_eq!(
        plan(&fixture, &input).unwrap_err().code,
        "CONSTRAINT_VIOLATION"
    );
    fixture.current.project.sequences[0].tracks[0].locked = false;
    fixture.current.project.sequences[0].tracks[1].locked = true;
    assert_eq!(
        plan(&fixture, &input).unwrap_err().code,
        "CONSTRAINT_VIOLATION"
    );
    changed = input.clone();
    changed.command = SourceAudioCommand::Separate {
        audio_element_id: "volume-key-0".into(),
        destination: SourceAudioDestination::FirstAvailable {
            fallback_track_id: "new-audio".into(),
        },
    };
    assert_eq!(
        plan(&fixture, &changed).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    changed.command = SourceAudioCommand::Separate {
        audio_element_id: "detached".into(),
        destination: SourceAudioDestination::FirstAvailable {
            fallback_track_id: "asset".into(),
        },
    };
    assert_eq!(
        plan(&fixture, &changed).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    changed = input.clone();
    changed.command = SourceAudioCommand::Restore;
    assert_eq!(
        plan(&fixture, &changed).unwrap_err().code,
        "CONSTRAINT_VIOLATION"
    );
    assert_eq!(fixture.commits, 0);
}

#[test]
fn legacy_linear_gain_zero_and_exact_source_profile_survive_public_separation() {
    let mut fixture = Fixture::new();
    let source = &mut fixture.current.project.sequences[0].tracks[0].elements[0];
    source.source_span = ElementSourceSpan::LegacySourceRange {
        range: opencut_contracts::TimeRange {
            start: t(1, 1),
            duration: t(3, 1),
        },
    };
    let ElementContent::Video { audio, retime, .. } = &mut source.content else {
        panic!()
    };
    audio.gain = Gain::LinearLegacy { value: 0.0 };
    retime.mapping = RetimeMapping::ExactRational;
    if let AnimationChannel::Scalar { binding, keys, .. } = &mut source.channels[0] {
        *binding = ScalarBinding::Property {
            parameter: ScalarParameter::GainLinearLegacy,
        };
        keys[0].value = 0.0;
        keys[0].right_handle = None;
        keys[1].value = 1.0;
        keys[1].left_handle = None;
    }
    let result = plan(&fixture, &fixture.request()).unwrap();
    let detached = put(&result, "detached");
    assert!(matches!(
        detached.source_span,
        ElementSourceSpan::LegacySourceRange { .. }
    ));
    let ElementContent::Audio { audio, retime, .. } = &detached.content else {
        panic!()
    };
    assert_eq!(audio.gain, Gain::LinearLegacy { value: 0.0 });
    assert_eq!(retime.mapping, RetimeMapping::ExactRational);
    let public = plan_public(&fixture, &result.transaction).unwrap();
    assert_eq!(public.sequences[0].tracks[1].elements[0], detached);
}
