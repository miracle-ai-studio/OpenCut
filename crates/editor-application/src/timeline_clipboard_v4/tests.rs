use super::*;
use crate::project_v4::{StoredProjectDocumentV4, StoredProjectRevisionV4};
use opencut_contracts::{
    AssetKind, AssetVersion, CreateProject, FrameRate, TimeRange, TransactionResult,
    project_v4_ops::*,
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
fn color() -> ColorV4 {
    ColorV4 {
        encoding: ColorEncodingV4::Srgb,
        red: 0.25,
        green: 0.5,
        blue: 0.75,
        alpha: 0.8,
    }
}
fn style() -> VisualStyleV4 {
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
        blend_mode: BlendModeV4::Screen,
        effects: vec![],
        masks: vec![],
    }
}
fn track(id: &str, kind: TrackKindV4, elements: Vec<TimelineElementV4>) -> TrackV4 {
    TrackV4 {
        id: id.into(),
        name: id.into(),
        kind,
        enabled: true,
        muted: false,
        locked: false,
        opacity: 1.0,
        gain: GainV4::Decibels { value: 0.0 },
        elements,
        channels: vec![],
    }
}
fn audio(id: &str, start: Time) -> TimelineElementV4 {
    TimelineElementV4 {
        id: id.into(),
        name: format!("Voice {id}"),
        timeline_range: TimeRange {
            start,
            duration: t(160000, 120000),
        },
        source_span: ElementSourceSpanV4::ClassicTrimmed {
            trim_start: t(1, 1),
            trim_end: t(1, 1),
            source_duration: Some(t(4, 1)),
        },
        enabled: false,
        visual: None,
        channels: vec![],
        content: ElementContentV4::Audio {
            asset_id: "sound-asset".into(),
            audio: AudioStyleV4 {
                stream: Some(2),
                gain: GainV4::LinearLegacy { value: 0.2 },
                muted: true,
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
    }
}
fn generated(id: &str, content: ElementContentV4) -> TimelineElementV4 {
    TimelineElementV4 {
        id: id.into(),
        name: id.into(),
        timeline_range: TimeRange {
            start: t(1, 3),
            duration: t(2, 1),
        },
        source_span: ElementSourceSpanV4::ClassicTrimmed {
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
fn scalar(id: &str, target: ChannelTargetV4, parameter: ScalarParameterV4) -> AnimationChannelV4 {
    AnimationChannelV4::Scalar {
        id: id.into(),
        target,
        binding: ScalarBindingV4::Property { parameter },
        keys: vec![
            ScalarKeyV4 {
                id: format!("{id}-a"),
                segment_id: Some(format!("{id}-segment")),
                time: t(0, 1),
                value: 0.2,
                left_handle: None,
                right_handle: Some(CurveHandleV4 {
                    dt: t(1, 3),
                    dv: 0.05,
                }),
                segment_to_next: ScalarSegmentV4::Bezier,
                tangent_mode: TangentModeV4::Broken,
            },
            ScalarKeyV4 {
                id: format!("{id}-b"),
                segment_id: None,
                time: t(1, 1),
                value: 0.75,
                left_handle: Some(CurveHandleV4 {
                    dt: t(-1, 3),
                    dv: -0.1,
                }),
                right_handle: None,
                segment_to_next: ScalarSegmentV4::Linear,
                tangent_mode: TangentModeV4::Aligned,
            },
        ],
        extrapolation: ExtrapolationV4 {
            before: ExtrapolationModeV4::Hold,
            after: ExtrapolationModeV4::Linear,
        },
    }
}
fn text() -> TextV4 {
    TextV4 {
        content: "中文\nTitle".into(),
        font: FontBindingV4::Locked {
            font_id: "font".into(),
        },
        font_weight: FontWeightV4::Normal,
        font_style: FontStyleV4::Normal,
        decoration: TextDecorationV4::Underline,
        align: TextAlignV4::Center,
        font_size: FontSizeV4::ClassicHeight90 { value: 15.0 },
        color: color(),
        letter_spacing_px: 1.5,
        line_height_multiplier: 1.25,
        background: TextBackgroundV4 {
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
fn stroke() -> StrokeV4 {
    StrokeV4 {
        color: color(),
        width_px: 2.0,
        align: StrokeAlignV4::Inside,
    }
}

struct Fixture {
    source: ProjectSnapshotV4,
    current: ProjectSnapshotV4,
    authorizations: Cell<usize>,
    resource_reads: Cell<usize>,
    deny_edit_after: Option<usize>,
    deny_read_after: Option<usize>,
}
impl Fixture {
    fn new() -> Self {
        let mut project = crate::project_v4::new_document(&CreateProject {
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
        project.assets.push(AssetV4 {
            id: "sound-asset".into(),
            name: "Stored sound".into(),
            kind: AssetKind::Audio,
            version: version('a'),
            duration: Some(t(4, 1)),
            origin: ResourceOriginV4::LegacyLocator {
                uri: "/private/offline-authorized.wav".into(),
            },
            metadata: MediaMetadataV4 {
                mime_type: Some("audio/wav".into()),
                intrinsic_size: None,
                frame_rate: None,
                rotation_degrees: None,
                audio: AudioPresenceV4::Present { metadata: None },
            },
            license: Some(ResourceLicenseV4 {
                identifier: Some("CC0-1.0".into()),
                source_url: None,
                notice: Some("Stored license".into()),
                attribution: None,
            }),
        });
        project.fonts.push(FontResourceV4 {
            id: "font".into(),
            family: "Stored face".into(),
            weight: 400,
            style: FontStyleV4::Normal,
            face_index: 0,
            content: version('b'),
            origin: ResourceOriginV4::LegacyLocator {
                uri: "/private/offline-authorized.ttf".into(),
            },
            license: None,
        });
        let mut voice = audio("voice", t(1, 3));
        voice.channels.push(scalar(
            "volume",
            ChannelTargetV4::Element {
                element_id: "voice".into(),
            },
            ScalarParameterV4::GainLinearLegacy,
        ));
        let mut source_track = track("source-sound", TrackKindV4::Audio, vec![voice]);
        source_track.muted = true;
        source_track.gain = GainV4::Decibels { value: -12.0 };
        source_track.channels.push(scalar(
            "track-volume",
            ChannelTargetV4::Track {
                track_id: source_track.id.clone(),
            },
            ScalarParameterV4::GainDecibels,
        ));
        project.sequences[0].tracks.push(source_track);
        project.sequences.push(SequenceV4 {
            id: "destination".into(),
            name: "Destination".into(),
            main_track_id: "dest-main".into(),
            tracks: vec![
                track("dest-main", TrackKindV4::Video, vec![]),
                track("dest-sound", TrackKindV4::Audio, vec![]),
            ],
            captions: vec![],
            transitions: vec![],
            bookmarks: vec![],
        });
        opencut_editor_core::project_v4::validate_project_v4(&project).unwrap();
        let source = ProjectSnapshotV4 {
            project: project.clone(),
            revision: "fixed-source".into(),
        };
        Self {
            source,
            current: ProjectSnapshotV4 {
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
    fn copied(&self, targets: &[(&str, &str)]) -> TimelineClipboardV4 {
        copy_selection(
            self,
            &CopyTimelineSelectionV4 {
                project_id: "project".into(),
                revision: self.source.revision.clone(),
                sequence_id: "source".into(),
                targets: targets
                    .iter()
                    .map(|(track, element)| ClipboardElementTargetV4 {
                        track_id: (*track).into(),
                        element_id: (*element).into(),
                    })
                    .collect(),
            },
        )
        .unwrap()
    }
    fn request(&self, copied: TimelineClipboardV4) -> PasteTimelineSelectionV4 {
        let mut groups: Vec<ClipboardTrackPlacementV4> = vec![];
        for item in &copied.items {
            let index = if let Some(index) = groups
                .iter()
                .position(|g| g.source_track_id == item.source_track_id)
            {
                index
            } else {
                groups.push(ClipboardTrackPlacementV4 {
                    source_track_id: item.source_track_id.clone(),
                    target: ClipboardTrackTargetV4::New {
                        track_id: format!("new-{}", item.source_track_id),
                        name: "Pasted group".into(),
                        track_kind: item.source_track_kind,
                        insertion_anchor: ClipboardTrackInsertionV4::Highest,
                    },
                    elements: vec![],
                });
                groups.len() - 1
            };
            groups[index].elements.push(ClipboardElementIdentityV4 {
                source_element_id: item.source_element_id.clone(),
                new_element_id: format!("clone-{}", item.source_element_id),
            });
        }
        PasteTimelineSelectionV4 {
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
impl ProjectV4Read for Fixture {
    fn authorize_v4(&self, _: V4Intent<'_>) -> Result<(), EditorError> {
        let n = self.authorizations.get() + 1;
        self.authorizations.set(n);
        if self.deny_edit_after.is_some_and(|limit| n > limit) {
            Err(EditorError::new("NOT_AUTHORIZED", "Current edit revoked"))
        } else {
            Ok(())
        }
    }
    fn head_v4_source(&self, project: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        self.stored(project, &self.current.revision)
    }
    fn revision_v4_source(
        &self,
        project: &str,
        rev: &str,
    ) -> Result<StoredProjectRevisionV4, EditorError> {
        self.stored(project, rev)
    }
    fn replay_v4_create(
        &self,
        _: &CreateV4Project,
    ) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        unreachable!("No mutation")
    }
    fn replay_v4_copy(&self, _: &CopyV4Project) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        unreachable!("No mutation")
    }
    fn replay_v4_edit(&self, _: V4Intent<'_>) -> Result<Option<TransactionResult>, EditorError> {
        unreachable!("No receipt")
    }
}
impl Fixture {
    fn stored(&self, project: &str, rev: &str) -> Result<StoredProjectRevisionV4, EditorError> {
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
        Ok(StoredProjectRevisionV4 {
            document: StoredProjectDocumentV4::V4(Box::new(snapshot.project.clone())),
            revision: snapshot.revision.clone(),
        })
    }
}
impl ProjectV4ResourceRead for Fixture {
    fn require_resource_revision_read(&self, project: &str, rev: &str) -> Result<(), EditorError> {
        let n = self.resource_reads.get() + 1;
        self.resource_reads.set(n);
        if self.deny_read_after.is_some_and(|limit| n > limit) {
            return Err(EditorError::new("NOT_AUTHORIZED", "Source Read revoked"));
        }
        self.stored(project, rev).map(|_| ())
    }
}
impl ProjectV4PublicResources for Fixture {
    fn authorized_project_media_v4(
        &self,
        _: &str,
        _: &str,
        _: &AssetVersion,
    ) -> Result<AssetV4, EditorError> {
        unreachable!(
            "Clipboard uses stored fixed revision selectors, not available media/probe/CAS I/O"
        )
    }
}
fn proposed(fixture: &Fixture, plan: &PasteTimelinePlanV4) -> ProjectDocumentV4 {
    let tx = &plan.transaction;
    let operations = tx
        .operations
        .iter()
        .map(|op| match op {
            PublicProjectOperationV4::AssetBind {
                id,
                name,
                source: PublicAssetSelectorV4::ProjectAsset { revision, asset_id },
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
                ProjectOperationV4::AssetPut { asset }
            }
            PublicProjectOperationV4::FontBind {
                id,
                source: PublicFontSelectorV4::ProjectFont { revision, font_id },
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
                ProjectOperationV4::FontPut { font }
            }
            PublicProjectOperationV4::SequencePut { sequence } => ProjectOperationV4::SequencePut {
                sequence: sequence.clone(),
            },
            _ => panic!(
                "Clipboard emits only authorized bindings and one complete destination scene"
            ),
        })
        .collect();
    let proposed = crate::project_v4::plan(
        &fixture.current,
        &ApplyV4Project {
            project_id: tx.project_id.clone(),
            expected_revision: tx.expected_revision.clone(),
            idempotency_key: tx.idempotency_key.clone(),
            label: tx.label.clone(),
            operations,
        },
    )
    .unwrap();
    assert_eq!(
        crate::project_v4::public::plan_public(fixture, tx).unwrap(),
        crate::project_v4::public::project(&ProjectSnapshotV4 {
            project: proposed.clone(),
            revision: fixture.current.revision.clone()
        })
        .project
    );
    proposed
}
fn element<'a>(project: &'a ProjectDocumentV4, id: &str) -> &'a TimelineElementV4 {
    project
        .sequences
        .iter()
        .flat_map(|s| &s.tracks)
        .flat_map(|t| &t.elements)
        .find(|e| e.id == id)
        .unwrap()
}
