use super::*;
use crate::scene::{SceneRead, StoredProjectRevision, public::plan_public};
use opencut_contracts::{AssetKind, AssetVersion, CreateProject, TimeRange, TransactionResult};
use std::cell::Cell;
mod behavior;
mod complete;
mod elements;
mod timing;
fn t(n: i64, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
fn eqtime(actual: &Time, n: i64, d: u32) {
    assert_eq!(
        time(actual).unwrap().cmp_exact(time(&t(n, d)).unwrap()),
        std::cmp::Ordering::Equal
    );
}
fn selected() -> TimelineTarget {
    TimelineTarget {
        track_id: "sound".into(),
        element_id: "voice".into(),
    }
}
struct Fixture {
    current: EditorSnapshot,
    calls: Cell<usize>,
    reads: Cell<usize>,
    deny_after: Option<usize>,
}
impl Fixture {
    fn new() -> Self {
        let mut project = crate::scene::new_document(&CreateProject {
            id: "project".into(),
            name: "Timeline".into(),
            sequence_id: "scene".into(),
            width: 160,
            height: 90,
            fps: FrameRate {
                numerator: 30,
                denominator: 1,
            },
        })
        .unwrap();
        project.sequences[0].main_track_id = "main".into();
        project.sequences[0].tracks[0].id = "main".into();
        project.assets.push(ProjectAsset {
            id: "asset".into(),
            name: "Offline audio".into(),
            kind: AssetKind::Audio,
            version: AssetVersion {
                id: format!("av_{}", "a".repeat(64)),
                content_hash: "a".repeat(64),
                size: 1,
            },
            duration: Some(t(10, 1)),
            origin: ResourceOrigin::LegacyLocator {
                uri: "/private/no-cas-read.wav".into(),
            },
            metadata: ProjectMediaMetadata {
                mime_type: None,
                intrinsic_size: None,
                frame_rate: None,
                rotation_degrees: None,
                audio: AudioPresence::Present { metadata: None },
            },
            license: None,
        });
        let mut sound = project.sequences[0].tracks[0].clone();
        sound.id = "sound".into();
        sound.kind = SceneTrackKind::Audio;
        sound.elements.push(TimelineElement {
            id: "voice".into(),
            name: "Authored".into(),
            timeline_range: TimeRange {
                start: t(1, 3),
                duration: t(2, 1),
            },
            source_span: ElementSourceSpan::ClassicTrimmed {
                trim_start: t(1, 1),
                trim_end: t(6, 1),
                source_duration: Some(t(10, 1)),
            },
            enabled: true,
            content: ElementContent::Audio {
                asset_id: "asset".into(),
                audio: AudioStyle {
                    stream: Some(2),
                    gain: Gain::Decibels { value: -6.05 },
                    muted: false,
                },
                retime: Retime {
                    rate: RationalRate {
                        numerator: 3,
                        denominator: 2,
                    },
                    maintain_pitch: true,
                    mapping: RetimeMapping::ClassicRoundedTicks { timebase: 120000 },
                },
            },
            visual: None,
            channels: vec![],
        });
        project.sequences[0].tracks.push(sound);
        opencut_editor_core::scene::validate_project_scene(&project).unwrap();
        Self {
            current: EditorSnapshot {
                project,
                revision: "fixed-head".into(),
            },
            calls: Cell::new(0),
            reads: Cell::new(0),
            deny_after: None,
        }
    }
    fn request(&self, command: TimelineCommand) -> TimelineEditRequest {
        TimelineEditRequest {
            project_id: "project".into(),
            expected_revision: self.current.revision.clone(),
            sequence_id: "scene".into(),
            idempotency_key: "timeline-key".into(),
            label: "Media edit".into(),
            command,
        }
    }
    fn e(&self) -> &TimelineElement {
        &self.current.project.sequences[0].tracks[1].elements[0]
    }
    fn em(&mut self) -> &mut TimelineElement {
        &mut self.current.project.sequences[0].tracks[1].elements[0]
    }
    fn add_tail(&mut self) {
        let mut tail = self.e().clone();
        tail.id = "tail".into();
        tail.timeline_range = TimeRange {
            start: t(3, 1),
            duration: t(1, 1),
        };
        tail.source_span = ElementSourceSpan::ClassicTrimmed {
            trim_start: t(1, 1),
            trim_end: t(15, 2),
            source_duration: Some(t(10, 1)),
        };
        self.current.project.sequences[0].tracks[1]
            .elements
            .push(tail);
    }
    fn second_track(&mut self) {
        let mut track = self.current.project.sequences[0].tracks[1].clone();
        track.id = "other".into();
        track.elements.clear();
        self.current.project.sequences[0].tracks.push(track);
    }
    fn as_video(&mut self) {
        self.current.project.assets[0].kind = AssetKind::Video;
        self.current.project.sequences[0].tracks[1].kind = SceneTrackKind::Video;
        let e = self.em();
        let ElementContent::Audio {
            asset_id,
            audio,
            retime,
        } = &e.content
        else {
            panic!()
        };
        e.content = ElementContent::Video {
            asset_id: asset_id.clone(),
            audio: audio.clone(),
            retime: retime.clone(),
            video_stream: Some(1),
            source_audio_enabled: false,
        };
        e.visual = Some(visual());
    }
}
fn visual() -> VisualStyle {
    VisualStyle {
        profile: VisualProfile::ClassicCanvasV31,
        transform: Transform {
            position_x_px: 8.0,
            position_y_px: -3.0,
            scale_x: -0.5,
            scale_y: 2.0,
            rotation_degrees: 23.0,
        },
        opacity: 0.75,
        blend_mode: BlendMode::Normal,
        effects: vec![],
        masks: vec![],
    }
}
impl SceneRead for Fixture {
    fn authorize_project(&self, _: SceneCommitIntent<'_>) -> Result<(), EditorError> {
        let next = self.calls.get() + 1;
        self.calls.set(next);
        if self.deny_after.is_some_and(|n| next > n) {
            Err(EditorError::new("NOT_AUTHORIZED", "Revoked"))
        } else {
            Ok(())
        }
    }
    fn head_scene_source(&self, _: &str) -> Result<StoredProjectRevision, EditorError> {
        self.reads.set(self.reads.get() + 1);
        Ok(StoredProjectRevision {
            revision: self.current.revision.clone(),
            document: StoredProjectDocument::Editable(Box::new(self.current.project.clone())),
        })
    }
    fn revision_scene_source(
        &self,
        _: &str,
        _: &str,
    ) -> Result<StoredProjectRevision, EditorError> {
        unreachable!("No historical head or resource rebinding")
    }
    fn replay_scene_create(
        &self,
        _: &CreateProjectRequest,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        unreachable!()
    }
    fn replay_scene_copy(
        &self,
        _: &CopySceneProject,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        unreachable!()
    }
    fn replay_scene_edit(
        &self,
        _: SceneCommitIntent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        unreachable!("Planner writes no receipts")
    }
}
impl ProjectPublicResources for Fixture {
    fn authorized_project_media_scene(
        &self,
        _: &str,
        _: &str,
        _: &AssetVersion,
    ) -> Result<ProjectAsset, EditorError> {
        unreachable!("Media already bound to fixed head")
    }
}
fn proposed(f: &Fixture, plan: &TimelineEditPlan) -> EditorDocument {
    let tx = plan.transaction.as_ref().unwrap();
    // The planner only emits the shared, locator-free operation variants; test
    // their exact serialized objects through both internal and public planners.
    let ops: Vec<ProjectOperation> = tx
        .operations
        .iter()
        .map(|op| serde_json::from_value(serde_json::to_value(op).unwrap()).unwrap())
        .collect();
    let proposed = crate::scene::plan(
        &f.current,
        &ApplySceneProject {
            project_id: tx.project_id.clone(),
            expected_revision: tx.expected_revision.clone(),
            idempotency_key: tx.idempotency_key.clone(),
            label: tx.label.clone(),
            operations: ops,
        },
    )
    .unwrap();
    assert_eq!(
        plan_public(f, tx).unwrap(),
        crate::scene::public::project(&EditorSnapshot {
            project: proposed.clone(),
            revision: f.current.revision.clone()
        })
        .project
    );
    let wire = serde_json::to_string(plan).unwrap();
    assert!(!wire.contains("/private/"));
    assert!(!wire.contains("\"uri\""));
    proposed
}
fn split(at: Time, retain: TimelineSplitRetain, ripple: bool) -> TimelineCommand {
    TimelineCommand::Split {
        targets: vec![TimelineSplit {
            target: selected(),
            right_element_id: "right".into(),
        }],
        at,
        retain,
        ripple,
    }
}
fn move_to(start: Time, track: &str, ripple: bool) -> TimelineCommand {
    TimelineCommand::MoveSelection {
        moves: vec![TimelineMove {
            target: selected(),
            target_track_id: track.into(),
            new_start: start,
        }],
        ripple,
    }
}
