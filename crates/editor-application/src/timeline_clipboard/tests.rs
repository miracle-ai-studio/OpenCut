use super::*;
use crate::scene::{StoredProjectDocument, StoredProjectRevision};
use opencut_contracts::{
    AssetKind, AssetVersion, CreateProject, FrameRate, TimeRange, TransactionResult, scene_ops::*,
};
use std::cell::Cell;
mod authority;
mod cloning;
mod placement;

fn t(n: i64, d: u32) -> Time {
    Time {
        ticks: n.to_string(),
        timebase: d,
    }
}
fn version(digit: char) -> AssetVersion {
    let hash = digit.to_string().repeat(64);
    AssetVersion {
        id: format!("av_{hash}"),
        content_hash: hash,
        size: 1,
    }
}
fn color() -> Color {
    Color {
        encoding: ColorEncoding::Srgb,
        red: 0.25,
        green: 0.5,
        blue: 0.75,
        alpha: 0.8,
    }
}
fn style() -> VisualStyle {
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
        blend_mode: BlendMode::Screen,
        effects: vec![],
        masks: vec![],
    }
}
fn track(id: &str, kind: SceneTrackKind, elements: Vec<TimelineElement>) -> SceneTrack {
    SceneTrack {
        id: id.into(),
        name: id.into(),
        kind,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1.0,
        gain: Gain::Decibels { value: 0.0 },
        elements,
        channels: vec![],
    }
}
fn audio(id: &str, start: Time) -> TimelineElement {
    TimelineElement {
        id: id.into(),
        name: format!("Voice {id}"),
        timeline_range: TimeRange {
            start,
            duration: t(160000, 120000),
        },
        source_span: ElementSourceSpan::ClassicTrimmed {
            trim_start: t(1, 1),
            trim_end: t(1, 1),
            source_duration: Some(t(4, 1)),
        },
        enabled: false,
        visual: None,
        channels: vec![],
        content: ElementContent::Audio {
            asset_id: "sound-asset".into(),
            audio: AudioStyle {
                stream: Some(2),
                gain: Gain::LinearLegacy { value: 0.2 },
                muted: true,
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
    }
}
fn generated(id: &str, content: ElementContent) -> TimelineElement {
    TimelineElement {
        id: id.into(),
        name: id.into(),
        timeline_range: TimeRange {
            start: t(1, 3),
            duration: t(2, 1),
        },
        source_span: ElementSourceSpan::ClassicTrimmed {
            trim_start: t(1, 1),
            trim_end: t(1, 1),
            source_duration: Some(t(4, 1)),
        },
        enabled: true,
        content,
        visual: Some(style()),
        channels: vec![],
    }
}
fn scalar(id: &str, target: ChannelTarget, parameter: ScalarParameter) -> AnimationChannel {
    AnimationChannel::Scalar {
        id: id.into(),
        target,
        binding: ScalarBinding::Property { parameter },
        keys: vec![
            ScalarKey {
                id: format!("{id}-a"),
                segment_id: Some(format!("{id}-segment")),
                time: t(0, 1),
                value: 0.2,
                left_handle: None,
                right_handle: Some(CurveHandle {
                    dt: t(1, 3),
                    dv: 0.05,
                }),
                segment_to_next: ScalarSegment::Bezier,
                tangent_mode: TangentMode::Broken,
            },
            ScalarKey {
                id: format!("{id}-b"),
                segment_id: None,
                time: t(1, 1),
                value: 0.75,
                left_handle: Some(CurveHandle {
                    dt: t(-1, 3),
                    dv: -0.1,
                }),
                right_handle: None,
                segment_to_next: ScalarSegment::Linear,
                tangent_mode: TangentMode::Aligned,
            },
        ],
        extrapolation: Extrapolation {
            before: ExtrapolationMode::Hold,
            after: ExtrapolationMode::Linear,
        },
    }
}
fn text() -> Text {
    Text {
        content: "中文\nTitle".into(),
        font: FontBinding::Locked {
            font_id: "font".into(),
        },
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        decoration: TextDecoration::Underline,
        align: SceneTextAlign::Center,
        font_size: FontSize::ClassicHeight90 { value: 15.0 },
        color: color(),
        letter_spacing_px: 1.5,
        line_height_multiplier: 1.25,
        background: TextBackground {
            enabled: true,
            color: color(),
            corner_radius_percent: 50.0,
            padding_x_at_size15: 3.0,
            padding_y_at_size15: 2.0,
            offset_x_px: -1.0,
            offset_y_px: 1.0,
        },
    }
}
fn stroke() -> Stroke {
    Stroke {
        color: color(),
        width_px: 2.0,
        align: StrokeAlign::Inside,
    }
}

struct Fixture {
    source: EditorSnapshot,
    current: EditorSnapshot,
    authorizations: Cell<usize>,
    resource_reads: Cell<usize>,
    deny_edit_after: Option<usize>,
    deny_read_after: Option<usize>,
}
impl Fixture {
    fn new() -> Self {
        let mut project = crate::scene::new_document(&CreateProject {
            id: "project".into(),
            name: "Clipboard".into(),
            sequence_id: "source".into(),
            width: 160,
            height: 90,
            fps: FrameRate {
                numerator: 30000,
                denominator: 1001,
            },
        })
        .unwrap();
        project.sequences[0].main_track_id = "source-main".into();
        project.sequences[0].tracks[0].id = "source-main".into();
        project.assets.push(ProjectAsset {
            id: "sound-asset".into(),
            name: "Stored sound".into(),
            kind: AssetKind::Audio,
            version: version('a'),
            duration: Some(t(4, 1)),
            origin: ResourceOrigin::LegacyLocator {
                uri: "/private/offline-authorized.wav".into(),
            },
            metadata: ProjectMediaMetadata {
                mime_type: Some("audio/wav".into()),
                intrinsic_size: None,
                frame_rate: None,
                rotation_degrees: None,
                audio: AudioPresence::Present { metadata: None },
            },
            license: Some(ResourceLicense {
                identifier: Some("CC0-1.0".into()),
                source_url: None,
                notice: Some("Stored license".into()),
                attribution: None,
            }),
        });
        project.fonts.push(FontResource {
            id: "font".into(),
            family: "Stored face".into(),
            weight: 400,
            style: FontStyle::Normal,
            face_index: 0,
            content: version('b'),
            origin: ResourceOrigin::LegacyLocator {
                uri: "/private/offline-authorized.ttf".into(),
            },
            license: None,
        });
        let mut voice = audio("voice", t(1, 3));
        voice.channels.push(scalar(
            "volume",
            ChannelTarget::Element {
                element_id: "voice".into(),
            },
            ScalarParameter::GainLinearLegacy,
        ));
        let mut source_track = track("source-sound", SceneTrackKind::Audio, vec![voice]);
        source_track.muted = true;
        source_track.gain = Gain::Decibels { value: -12.0 };
        source_track.channels.push(scalar(
            "track-volume",
            ChannelTarget::Track {
                track_id: source_track.id.clone(),
            },
            ScalarParameter::GainDecibels,
        ));
        project.sequences[0].tracks.push(source_track);
        project.sequences.push(Scene {
            id: "destination".into(),
            name: "Destination".into(),
            main_track_id: "dest-main".into(),
            tracks: vec![
                track("dest-main", SceneTrackKind::Video, vec![]),
                track("dest-sound", SceneTrackKind::Audio, vec![]),
            ],
            captions: vec![],
            transitions: vec![],
            bookmarks: vec![],
        });
        opencut_editor_core::scene::validate_project_scene(&project).unwrap();
        let source = EditorSnapshot {
            project: project.clone(),
            revision: "fixed-source".into(),
        };
        Self {
            source,
            current: EditorSnapshot {
                project,
                revision: "head".into(),
            },
            authorizations: Cell::new(0),
            resource_reads: Cell::new(0),
            deny_edit_after: None,
            deny_read_after: None,
        }
    }
    fn sync(&mut self) {
        self.source.project = self.current.project.clone();
    }
    fn copied(&self, targets: &[(&str, &str)]) -> TimelineClipboard {
        copy_selection(
            self,
            &CopyTimelineSelection {
                project_id: "project".into(),
                revision: self.source.revision.clone(),
                sequence_id: "source".into(),
                targets: targets
                    .iter()
                    .map(|(track, element)| ClipboardElementTarget {
                        track_id: (*track).into(),
                        element_id: (*element).into(),
                    })
                    .collect(),
            },
        )
        .unwrap()
    }
    fn request(&self, copied: TimelineClipboard) -> PasteTimelineSelection {
        let mut groups: Vec<ClipboardTrackPlacement> = vec![];
        for item in &copied.items {
            let index = if let Some(index) = groups
                .iter()
                .position(|g| g.source_track_id == item.source_track_id)
            {
                index
            } else {
                groups.push(ClipboardTrackPlacement {
                    source_track_id: item.source_track_id.clone(),
                    target: ClipboardTrackTarget::New {
                        track_id: format!("new-{}", item.source_track_id),
                        name: "Pasted group".into(),
                        track_kind: item.source_track_kind,
                        insertion_anchor: ClipboardTrackInsertion::Highest,
                    },
                    elements: vec![],
                });
                groups.len() - 1
            };
            groups[index].elements.push(ClipboardElementIdentity {
                source_element_id: item.source_element_id.clone(),
                new_element_id: format!("clone-{}", item.source_element_id),
            });
        }
        PasteTimelineSelection {
            project_id: "project".into(),
            expected_revision: self.current.revision.clone(),
            sequence_id: "destination".into(),
            idempotency_key: "paste-key".into(),
            label: "Paste elements".into(),
            clipboard: copied,
            at: t(1, 7),
            placements: groups,
        }
    }
}
impl SceneRead for Fixture {
    fn authorize_project(&self, _: SceneCommitIntent<'_>) -> Result<(), EditorError> {
        let n = self.authorizations.get() + 1;
        self.authorizations.set(n);
        if self.deny_edit_after.is_some_and(|limit| n > limit) {
            Err(EditorError::new("NOT_AUTHORIZED", "Current edit revoked"))
        } else {
            Ok(())
        }
    }
    fn head_scene_source(&self, project: &str) -> Result<StoredProjectRevision, EditorError> {
        self.stored(project, &self.current.revision)
    }
    fn revision_scene_source(
        &self,
        project: &str,
        rev: &str,
    ) -> Result<StoredProjectRevision, EditorError> {
        self.stored(project, rev)
    }
    fn replay_scene_create(
        &self,
        _: &CreateProjectRequest,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        unreachable!("No mutation")
    }
    fn replay_scene_copy(
        &self,
        _: &CopySceneProject,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        unreachable!("No mutation")
    }
    fn replay_scene_edit(
        &self,
        _: SceneCommitIntent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        unreachable!("No receipt")
    }
}
impl Fixture {
    fn stored(&self, project: &str, rev: &str) -> Result<StoredProjectRevision, EditorError> {
        if project != "project" {
            return Err(missing("Wrong project"));
        }
        let snapshot = if rev == self.source.revision {
            &self.source
        } else if rev == self.current.revision {
            &self.current
        } else {
            return Err(missing("Unknown fixed revision"));
        };
        Ok(StoredProjectRevision {
            document: StoredProjectDocument::Editable(Box::new(snapshot.project.clone())),
            revision: snapshot.revision.clone(),
        })
    }
}
impl ProjectResourceRead for Fixture {
    fn require_resource_revision_read(&self, project: &str, rev: &str) -> Result<(), EditorError> {
        let n = self.resource_reads.get() + 1;
        self.resource_reads.set(n);
        if self.deny_read_after.is_some_and(|limit| n > limit) {
            return Err(EditorError::new("NOT_AUTHORIZED", "Source Read revoked"));
        }
        self.stored(project, rev).map(|_| ())
    }
}
impl ProjectPublicResources for Fixture {
    fn authorized_project_media_scene(
        &self,
        _: &str,
        _: &str,
        _: &AssetVersion,
    ) -> Result<ProjectAsset, EditorError> {
        unreachable!(
            "Clipboard uses stored fixed revision selectors, not available media/probe/CAS I/O"
        )
    }
}
fn proposed(fixture: &Fixture, plan: &PasteTimelinePlan) -> EditorDocument {
    let tx = &plan.transaction;
    let operations = tx
        .operations
        .iter()
        .map(|op| match op {
            ProjectEditOperation::AssetBind {
                id,
                name,
                source: AssetSelector::ProjectAsset { revision, asset_id },
            } => {
                assert_eq!(revision, &fixture.source.revision);
                let mut asset = fixture
                    .source
                    .project
                    .assets
                    .iter()
                    .find(|a| &a.id == asset_id)
                    .unwrap()
                    .clone();
                asset.id = id.clone();
                asset.name = name.clone();
                ProjectOperation::AssetPut { asset }
            }
            ProjectEditOperation::FontBind {
                id,
                source: FontSelector::ProjectFont { revision, font_id },
            } => {
                assert_eq!(revision, &fixture.source.revision);
                let mut font = fixture
                    .source
                    .project
                    .fonts
                    .iter()
                    .find(|f| &f.id == font_id)
                    .unwrap()
                    .clone();
                font.id = id.clone();
                ProjectOperation::FontPut { font }
            }
            ProjectEditOperation::SequencePut { sequence } => ProjectOperation::SequencePut {
                sequence: sequence.clone(),
            },
            _ => panic!(
                "Clipboard emits only authorized bindings and one complete destination scene"
            ),
        })
        .collect();
    let proposed = crate::scene::plan(
        &fixture.current,
        &ApplySceneProject {
            project_id: tx.project_id.clone(),
            expected_revision: tx.expected_revision.clone(),
            idempotency_key: tx.idempotency_key.clone(),
            label: tx.label.clone(),
            operations,
        },
    )
    .unwrap();
    assert_eq!(
        crate::scene::public::plan_public(fixture, tx).unwrap(),
        crate::scene::public::project(&EditorSnapshot {
            project: proposed.clone(),
            revision: fixture.current.revision.clone()
        })
        .project
    );
    proposed
}
fn element<'a>(project: &'a EditorDocument, id: &str) -> &'a TimelineElement {
    project
        .sequences
        .iter()
        .flat_map(|s| &s.tracks)
        .flat_map(|t| &t.elements)
        .find(|e| e.id == id)
        .unwrap()
}
