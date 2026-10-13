use super::*;
use crate::project_v4::{ProjectV4Read, StoredProjectRevisionV4, public::plan_public};
use opencut_contracts::{AssetKind, AssetVersion, CreateProject, TimeRange, TransactionResult};
use std::cell::Cell;
mod behavior;
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
fn selected() -> TimelineTargetV4 {
    TimelineTargetV4 {
        track_id: "sound".into(),
        element_id: "voice".into(),
    }
}
struct Fixture {
    current: ProjectSnapshotV4,
    calls: Cell<usize>,
    reads: Cell<usize>,
    deny_after: Option<usize>,
}
impl Fixture {
    fn new() -> Self {
        let mut project = crate::project_v4::new_document(&CreateProject {
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
        project.assets.push(AssetV4 {
            id: "asset".into(),
            name: "Offline audio".into(),
            kind: AssetKind::Audio,
            version: AssetVersion {
                id: format!("av_{}", "a".repeat(64)),
                content_hash: "a".repeat(64),
                size: 1,
            },
            duration: Some(t(10, 1)),
            origin: ResourceOriginV4::LegacyLocator {
                uri: "/private/no-cas-read.wav".into(),
            },
            metadata: MediaMetadataV4 {
                mime_type: None,
                intrinsic_size: None,
                frame_rate: None,
                rotation_degrees: None,
                audio: AudioPresenceV4::Present { metadata: None },
            },
            license: None,
        });
        let mut sound = project.sequences[0].tracks[0].clone();
        sound.id = "sound".into();
        sound.kind = TrackKindV4::Audio;
        sound.elements.push(TimelineElementV4 {
            id: "voice".into(),
            name: "Authored".into(),
            timeline_range: TimeRange {
                start: t(1, 3),
                duration: t(2, 1),
            },
            source_span: ElementSourceSpanV4::ClassicTrimmed {
                trim_start: t(1, 1),
                trim_end: t(6, 1),
                source_duration: Some(t(10, 1)),
            },
            enabled: true,
            content: ElementContentV4::Audio {
                asset_id: "asset".into(),
                audio: AudioStyleV4 {
                    stream: Some(2),
                    gain: GainV4::Decibels { value: -6.05 },
                    muted: false,
                },
                retime: RetimeV4 {
                    rate: RationalRateV4 {
                        numerator: 3,
                        denominator: 2,
                    },
                    maintain_pitch: true,
                    mapping: RetimeMappingV4::ClassicRoundedTicks { timebase: 120000 },
                },
            },
            visual: None,
            channels: vec![],
        });
        project.sequences[0].tracks.push(sound);
        opencut_editor_core::project_v4::validate_project_v4(&project).unwrap();
        Self {
            current: ProjectSnapshotV4 {
                project,
                revision: "fixed-head".into(),
            },
            calls: Cell::new(0),
            reads: Cell::new(0),
            deny_after: None,
        }
    }
    fn request(&self, command: TimelineCommandV4) -> TimelineEditRequestV4 {
        TimelineEditRequestV4 {
            project_id: "project".into(),
            expected_revision: self.current.revision.clone(),
            sequence_id: "scene".into(),
            idempotency_key: "timeline-key".into(),
            label: "Media edit".into(),
            command,
        }
    }
    fn e(&self) -> &TimelineElementV4 {
        &self.current.project.sequences[0].tracks[1].elements[0]
    }
    fn em(&mut self) -> &mut TimelineElementV4 {
        &mut self.current.project.sequences[0].tracks[1].elements[0]
    }
    fn add_tail(&mut self) {
        let mut tail = self.e().clone();
        tail.id = "tail".into();
        tail.timeline_range = TimeRange {
            start: t(3, 1),
            duration: t(1, 1),
        };
        tail.source_span = ElementSourceSpanV4::ClassicTrimmed {
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
        self.current.project.sequences[0].tracks[1].kind = TrackKindV4::Video;
        let e = self.em();
        let ElementContentV4::Audio {
            asset_id,
            audio,
            retime,
        } = &e.content
        else {
            panic!()
        };
        e.content = ElementContentV4::Video {
            asset_id: asset_id.clone(),
            audio: audio.clone(),
            retime: retime.clone(),
            video_stream: Some(1),
            source_audio_enabled: false,
        };
        e.visual = Some(visual());
    }
}
fn visual() -> VisualStyleV4 {
    VisualStyleV4 {
        profile: VisualProfileV4::ClassicCanvasV31,
        transform: TransformV4 {
            position_x_px: 8.0,
            position_y_px: -3.0,
            scale_x: -0.5,
            scale_y: 2.0,
            rotation_degrees: 23.0,
        },
        opacity: 0.75,
        blend_mode: BlendModeV4::Normal,
        effects: vec![],
        masks: vec![],
    }
}
impl ProjectV4Read for Fixture {
    fn authorize_v4(&self, _: V4Intent<'_>) -> Result<(), EditorError> {
        let next = self.calls.get() + 1;
        self.calls.set(next);
        if self.deny_after.is_some_and(|n| next > n) {
            Err(EditorError::new("NOT_AUTHORIZED", "Revoked"))
        } else {
            Ok(())
        }
    }
    fn head_v4_source(&self, _: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        self.reads.set(self.reads.get() + 1);
        Ok(StoredProjectRevisionV4 {
            revision: self.current.revision.clone(),
            document: StoredProjectDocumentV4::V4(Box::new(self.current.project.clone())),
        })
    }
    fn revision_v4_source(&self, _: &str, _: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        unreachable!("No historical head or resource rebinding")
    }
    fn replay_v4_create(
        &self,
        _: &CreateV4Project,
    ) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        unreachable!()
    }
    fn replay_v4_copy(&self, _: &CopyV4Project) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        unreachable!()
    }
    fn replay_v4_edit(&self, _: V4Intent<'_>) -> Result<Option<TransactionResult>, EditorError> {
        unreachable!("Planner writes no receipts")
    }
}
impl ProjectV4PublicResources for Fixture {
    fn authorized_project_media_v4(
        &self,
        _: &str,
        _: &str,
        _: &AssetVersion,
    ) -> Result<AssetV4, EditorError> {
        unreachable!("Media already bound to fixed head")
    }
}
fn proposed(f: &Fixture, plan: &TimelineEditPlanV4) -> ProjectDocumentV4 {
    let tx = plan.transaction.as_ref().unwrap();
    // The planner only emits the shared, locator-free operation variants; test
    // their exact serialized objects through both internal and public planners.
    let ops: Vec<ProjectOperationV4> = tx
        .operations
        .iter()
        .map(|op| serde_json::from_value(serde_json::to_value(op).unwrap()).unwrap())
        .collect();
    let proposed = crate::project_v4::plan(
        &f.current,
        &ApplyV4Project {
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
        crate::project_v4::public::project(&ProjectSnapshotV4 {
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
fn split(at: Time, retain: TimelineSplitRetainV4, ripple: bool) -> TimelineCommandV4 {
    TimelineCommandV4::Split {
        targets: vec![TimelineSplitV4 {
            target: selected(),
            right_element_id: "right".into(),
        }],
        at,
        retain,
        ripple,
    }
}
fn move_to(start: Time, track: &str, ripple: bool) -> TimelineCommandV4 {
    TimelineCommandV4::MoveSelection {
        moves: vec![TimelineMoveV4 {
            target: selected(),
            target_track_id: track.into(),
            new_start: start,
        }],
        ripple,
    }
}
