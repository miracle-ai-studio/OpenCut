use super::*;
use crate::scene::{SceneRead, StoredProjectRevision};
use opencut_contracts::{
    AssetKind, AssetVersion, CreateProject, FrameRate, Time, TimeRange, TransactionResult,
    scene::*,
    scene_ops::{CopySceneProject, CreateProjectRequest},
};
use std::cell::Cell;

fn time(ticks: i64) -> Time {
    Time {
        ticks: ticks.to_string(),
        timebase: 1,
    }
}
struct Store {
    project: EditorDocument,
    reads: Cell<usize>,
    authorizations: Cell<usize>,
    deny_after: Option<usize>,
    advance_on_second_read: bool,
}
impl Store {
    fn new() -> Self {
        let mut project = crate::scene::new_document(&CreateProject {
            id: "project".into(),
            name: "Track removal".into(),
            sequence_id: "scene".into(),
            width: 160,
            height: 90,
            fps: FrameRate {
                numerator: 30,
                denominator: 1,
            },
        })
        .unwrap();
        project.assets.push(ProjectAsset {
            id: "media".into(),
            name: "Retained source".into(),
            kind: AssetKind::Video,
            version: AssetVersion {
                id: format!("av_{}", "a".repeat(64)),
                content_hash: "a".repeat(64),
                size: 1,
            },
            duration: Some(time(10)),
            origin: ResourceOrigin::LegacyLocator {
                uri: "/never/read/or/capture.mp4".into(),
            },
            license: None,
            metadata: ProjectMediaMetadata {
                mime_type: Some("video/mp4".into()),
                intrinsic_size: Some(CanvasSize {
                    width: 160,
                    height: 90,
                }),
                frame_rate: None,
                rotation_degrees: None,
                audio: AudioPresence::Absent {},
            },
        });
        project.fonts.push(FontResource {
            id: "font".into(),
            family: "Retained family".into(),
            weight: 400,
            style: FontStyle::Normal,
            face_index: 0,
            content: AssetVersion {
                id: format!("av_{}", "b".repeat(64)),
                content_hash: "b".repeat(64),
                size: 1,
            },
            origin: ResourceOrigin::LegacyLocator {
                uri: "/never/read/or/capture.ttf".into(),
            },
            license: None,
        });
        let element = TimelineElement { id: "outgoing".into(), name: "Clip".into(), enabled: true,
            timeline_range: TimeRange { start: time(0), duration: time(2) },
            source_span: ElementSourceSpan::ClassicTrimmed { trim_start: time(1), trim_end: time(7), source_duration: Some(time(10)) },
            content: ElementContent::Video { asset_id: "media".into(), video_stream: None, source_audio_enabled: false,
                audio: AudioStyle { stream: None, gain: Gain::Decibels { value: 0. }, muted: false },
                retime: Retime { rate: RationalRate { numerator: 1, denominator: 1 }, maintain_pitch: true, mapping: RetimeMapping::ClassicRoundedTicks { timebase: 120000 } } },
            visual: Some(serde_json::from_value(serde_json::json!({"profile":"classic_canvas_v31","transform":{"position_x_px":0,"position_y_px":0,"scale_x":1,"scale_y":1,"rotation_degrees":0},"opacity":1,"blend_mode":"normal","effects":[],"masks":[]})).unwrap()), channels: vec![] };
        let scene = &mut project.sequences[0];
        let mut track = scene.tracks[0].clone();
        track.id = "remove".into();
        track.name = "Remove me".into();
        let mut incoming = element.clone();
        incoming.id = "incoming".into();
        incoming.timeline_range.start = time(2);
        track.elements = vec![element, incoming];
        scene.tracks.push(track);
        scene.transitions.push(SceneTransition {
            id: "seam".into(),
            outgoing_element_id: "outgoing".into(),
            incoming_element_id: "incoming".into(),
            duration: time(1),
            transition: TransitionKind::LegacyCrossfadeV3,
        });
        let mut captions = scene.tracks[0].clone();
        captions.id = "captions".into();
        captions.kind = SceneTrackKind::Caption;
        scene.tracks.push(captions);
        scene.captions.push(serde_json::from_value(serde_json::json!({"id":"cue","track_id":"captions","timeline_range":{"start":time(0),"duration":time(2)},"text":"Retained caption","source_anchor":null,"style":{"font":{"kind":"unresolved_family","family":"Arial","policy":{"mode":"strict"}},"font_size":{"unit":"classic_height90","value":15},"color":{"encoding":"srgb","red":1,"green":1,"blue":1,"alpha":1},"background_color":null,"max_width_px":null,"line_height_px":null,"text_align":"center","layout":"legacy_caption_v2"}})).unwrap());
        let mut other = scene.tracks[0].clone();
        other.id = "locked-other".into();
        other.locked = true;
        scene.tracks.push(other);
        opencut_editor_core::scene::validate_project_scene(&project).unwrap();
        Self {
            project,
            reads: Cell::new(0),
            authorizations: Cell::new(0),
            deny_after: None,
            advance_on_second_read: false,
        }
    }
    fn request(&self, track: &str) -> PlanTrackRemoval {
        PlanTrackRemoval {
            project_id: "project".into(),
            expected_revision: "fixed".into(),
            idempotency_key: "remove-original".into(),
            label: "Remove track".into(),
            sequence_id: "scene".into(),
            track_id: track.into(),
        }
    }
}
impl SceneRead for Store {
    fn authorize_project(&self, _: SceneCommitIntent<'_>) -> Result<(), EditorError> {
        let count = self.authorizations.get() + 1;
        self.authorizations.set(count);
        if self.deny_after.is_some_and(|after| count > after) {
            Err(EditorError::new("NOT_AUTHORIZED", "Revoked"))
        } else {
            Ok(())
        }
    }
    fn head_scene_source(&self, _: &str) -> Result<StoredProjectRevision, EditorError> {
        let count = self.reads.get() + 1;
        self.reads.set(count);
        Ok(StoredProjectRevision {
            revision: if self.advance_on_second_read && count >= 2 {
                "advanced"
            } else {
                "fixed"
            }
            .into(),
            document: StoredProjectDocument::Editable(Box::new(self.project.clone())),
        })
    }
    fn revision_scene_source(
        &self,
        _: &str,
        _: &str,
    ) -> Result<StoredProjectRevision, EditorError> {
        panic!("No historical resource lookup")
    }
    fn replay_scene_create(
        &self,
        _: &CreateProjectRequest,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        panic!("Read-only planner")
    }
    fn replay_scene_copy(
        &self,
        _: &CopySceneProject,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        panic!("Read-only planner")
    }
    fn replay_scene_edit(
        &self,
        _: SceneCommitIntent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        panic!("No idempotency write or replay during planning")
    }
}
impl ProjectPublicResources for Store {
    fn authorized_project_media_scene(
        &self,
        _: &str,
        _: &str,
        _: &AssetVersion,
    ) -> Result<ProjectAsset, EditorError> {
        panic!("Track removal never rebinds resources")
    }
}

#[test]
fn nonempty_track_removal_cleans_transitions_first_and_preserves_every_unrelated_field() {
    let store = Store::new();
    let before = store.project.clone();
    let request = store.request("remove");
    let plan = plan(&store, &request).unwrap();
    assert_eq!(
        plan.operations,
        vec![
            Op::TransitionDelete {
                sequence_id: "scene".into(),
                transition_id: "seam".into()
            },
            Op::TrackDelete {
                sequence_id: "scene".into(),
                track_id: "remove".into()
            }
        ]
    );
    assert_eq!(plan.idempotency_key, request.idempotency_key);
    assert_eq!(store.project, before);
    let result = plan_public(&store, &plan).unwrap();
    let mut expected = before.clone();
    expected.sequences[0].tracks.remove(1);
    expected.sequences[0].transitions.clear();
    assert_eq!(
        result,
        crate::scene::public::project(&EditorSnapshot {
            project: expected,
            revision: "fixed".into()
        })
        .project
    );
    assert!(store.authorizations.get() >= 3);
}
#[test]
fn caption_track_removal_cleans_cues_without_touching_video_transitions() {
    let store = Store::new();
    let plan = plan(&store, &store.request("captions")).unwrap();
    assert_eq!(
        plan.operations,
        vec![
            Op::CaptionDelete {
                sequence_id: "scene".into(),
                caption_id: "cue".into()
            },
            Op::TrackDelete {
                sequence_id: "scene".into(),
                track_id: "captions".into()
            }
        ]
    );
    let result = plan_public(&store, &plan).unwrap();
    assert!(result.sequences[0].captions.is_empty());
    assert_eq!(
        result.sequences[0].transitions,
        store.project.sequences[0].transitions
    );
    assert_eq!(result.assets.len(), 1);
}
#[test]
fn every_track_kind_uses_the_same_removal_contract_and_main_locked_or_stale_targets_are_rejected() {
    for kind in [
        SceneTrackKind::Video,
        SceneTrackKind::Audio,
        SceneTrackKind::Text,
        SceneTrackKind::Graphic,
        SceneTrackKind::Effect,
        SceneTrackKind::Caption,
    ] {
        let mut store = Store::new();
        let mut target = store.project.sequences[0].tracks[0].clone();
        target.id = "extra".into();
        target.kind = kind;
        store.project.sequences[0].tracks.push(target);
        assert!(plan(&store, &store.request("extra")).is_ok());
    }
    let store = Store::new();
    for track in [
        store.project.sequences[0].main_track_id.as_str(),
        "locked-other",
    ] {
        assert_eq!(
            plan(&store, &store.request(track)).unwrap_err().code,
            "CONSTRAINT_VIOLATION"
        );
    }
    assert_eq!(
        plan(&store, &store.request("missing")).unwrap_err().code,
        "NOT_FOUND"
    );
    let mut request = store.request("remove");
    request.expected_revision = "old".into();
    assert_eq!(
        plan(&store, &request).unwrap_err().code,
        "REVISION_CONFLICT"
    );
}
#[test]
fn read_and_full_public_validation_remain_authorized_and_fixed_before_returning_a_proposal() {
    let mut store = Store::new();
    store.deny_after = Some(0);
    assert_eq!(
        plan(&store, &store.request("remove")).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(store.reads.get(), 0);
    let mut store = Store::new();
    store.deny_after = Some(2);
    assert_eq!(
        plan(&store, &store.request("remove")).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    let mut store = Store::new();
    store.advance_on_second_read = true;
    assert_eq!(
        plan(&store, &store.request("remove")).unwrap_err().code,
        "REVISION_CONFLICT"
    );
}
