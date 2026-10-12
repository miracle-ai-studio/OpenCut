use super::*;
use crate::project_v4::{
    PreparedV4Commit, PreparedV4Create, ProjectV4Store, StoredProjectRevisionV4,
    public::ProjectV4PublicResources,
};
use opencut_contracts::{
    AssetKind, AssetVersion, CreateProject, FrameRate, Time, TransactionResult,
};
use opencut_time::RationalTime;
use std::{cell::Cell, collections::BTreeMap};

pub(super) fn t(n: i64, d: u32) -> Time {
    RationalTime::new(n, d).unwrap().into()
}
pub(super) struct Fixture {
    pub current: ProjectSnapshotV4,
    pub deny: bool,
    pub reads: Cell<usize>,
    pub commits: usize,
    receipts: BTreeMap<String, (String, TransactionResult)>,
}
pub(super) fn volume() -> AnimationChannelV4 {
    AnimationChannelV4::Scalar {
        id: "volume-channel".into(),
        target: ChannelTargetV4::Element {
            element_id: "video".into(),
        },
        binding: ScalarBindingV4::Property {
            parameter: ScalarParameterV4::GainDecibels,
        },
        keys: vec![
            ScalarKeyV4 {
                id: "volume-key-0".into(),
                segment_id: Some("volume-segment".into()),
                time: t(0, 1),
                value: -12.0,
                left_handle: None,
                right_handle: Some(CurveHandleV4 {
                    dt: t(1, 3),
                    dv: 2.0,
                }),
                segment_to_next: ScalarSegmentV4::Bezier,
                tangent_mode: TangentModeV4::Broken,
            },
            ScalarKeyV4 {
                id: "volume-key-1".into(),
                segment_id: None,
                time: t(2, 1),
                value: -3.0,
                left_handle: Some(CurveHandleV4 {
                    dt: t(-1, 3),
                    dv: -1.0,
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
impl Fixture {
    pub fn new() -> Self {
        let mut project = crate::project_v4::new_document(&CreateProject {
            id: "p".into(),
            name: "P".into(),
            sequence_id: "s".into(),
            width: 320,
            height: 180,
            fps: FrameRate {
                numerator: 25,
                denominator: 1,
            },
        })
        .unwrap();
        project.sequences[0].main_track_id = "main".into();
        project.sequences[0].tracks[0].id = "main".into();
        let mut target = project.sequences[0].tracks[0].clone();
        target.id = "audio".into();
        target.name = "Existing audio".into();
        target.kind = TrackKindV4::Audio;
        project.sequences[0].tracks.push(target);
        let hash = "1".repeat(64);
        project.assets.push(AssetV4 {
            id: "asset".into(),
            name: "Video".into(),
            kind: AssetKind::Video,
            version: AssetVersion {
                id: format!("av_{hash}"),
                content_hash: hash,
                size: 1,
            },
            duration: Some(t(10, 1)),
            origin: ResourceOriginV4::LegacyLocator {
                uri: "/private/source.mp4".into(),
            },
            metadata: MediaMetadataV4 {
                mime_type: Some("video/mp4".into()),
                intrinsic_size: Some(CanvasSizeV4 {
                    width: 320,
                    height: 180,
                }),
                frame_rate: Some(FrameRate {
                    numerator: 25,
                    denominator: 1,
                }),
                rotation_degrees: None,
                audio: AudioPresenceV4::Present { metadata: None },
            },
            license: None,
        });
        let mut position = volume();
        if let AnimationChannelV4::Scalar {
            id, binding, keys, ..
        } = &mut position
        {
            *id = "position-channel".into();
            *binding = ScalarBindingV4::Property {
                parameter: ScalarParameterV4::PositionXPx,
            };
            keys[0].id = "position-key-0".into();
            keys[0].segment_id = None;
            keys[1].id = "position-key-1".into();
        }
        project.sequences[0].tracks[0]
            .elements
            .push(TimelineElementV4 {
                id: "video".into(),
                name: "Edited video".into(),
                timeline_range: opencut_contracts::TimeRange {
                    start: t(1, 3),
                    duration: t(2, 1),
                },
                source_span: ElementSourceSpanV4::ClassicTrimmed {
                    trim_start: t(1, 1),
                    trim_end: t(6, 1),
                    source_duration: Some(t(10, 1)),
                },
                enabled: false,
                content: ElementContentV4::Video {
                    asset_id: "asset".into(),
                    video_stream: Some(1),
                    source_audio_enabled: true,
                    audio: AudioStyleV4 {
                        stream: Some(2),
                        gain: GainV4::Decibels { value: -6.0 },
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
                visual: Some(VisualStyleV4 {
                    profile: VisualProfileV4::ClassicCanvasV31,
                    transform: TransformV4 {
                        position_x_px: 20.0,
                        position_y_px: -10.0,
                        scale_x: 0.5,
                        scale_y: 0.5,
                        rotation_degrees: 12.0,
                    },
                    opacity: 0.6,
                    blend_mode: BlendModeV4::Normal,
                    effects: vec![],
                    masks: vec![],
                }),
                channels: vec![volume(), position],
            });
        opencut_editor_core::project_v4::validate_project_v4(&project).unwrap();
        Self {
            current: ProjectSnapshotV4 {
                project,
                revision: "fixed".into(),
            },
            deny: false,
            reads: Cell::new(0),
            commits: 0,
            receipts: BTreeMap::new(),
        }
    }
    pub fn request(&self) -> SourceAudioRequestV4 {
        SourceAudioRequestV4 {
            project_id: "p".into(),
            expected_revision: self.current.revision.clone(),
            sequence_id: "s".into(),
            track_id: "main".into(),
            element_id: "video".into(),
            idempotency_key: "separate".into(),
            label: "Separate source audio".into(),
            command: SourceAudioCommandV4::Separate {
                audio_element_id: "detached".into(),
                destination: SourceAudioDestinationV4::Existing {
                    track_id: "audio".into(),
                },
            },
        }
    }
    pub fn source(&self) -> &TimelineElementV4 {
        &self.current.project.sequences[0].tracks[0].elements[0]
    }
}
impl ProjectV4Read for Fixture {
    fn authorize_v4(&self, _: V4Intent<'_>) -> Result<(), EditorError> {
        if self.deny {
            Err(EditorError::new("NOT_AUTHORIZED", "Denied current caller"))
        } else {
            Ok(())
        }
    }
    fn head_v4_source(&self, _: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        self.reads.set(self.reads.get() + 1);
        Ok(StoredProjectRevisionV4 {
            document: StoredProjectDocumentV4::V4(Box::new(self.current.project.clone())),
            revision: self.current.revision.clone(),
        })
    }
    fn revision_v4_source(&self, _: &str, _: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        unreachable!("Source audio uses the fixed current head")
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
    fn replay_v4_edit(
        &self,
        intent: V4Intent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        match self.receipts.get(intent.key()) {
            Some((identity, result)) if intent.identity()?.current_digest() == identity => {
                Ok(Some(result.clone()))
            }
            Some(_) => Err(EditorError::new(
                "IDEMPOTENCY_KEY_REUSED",
                "Different submitted public transaction",
            )),
            None => Ok(None),
        }
    }
}
impl ProjectV4PublicResources for Fixture {
    fn authorized_project_media_v4(
        &self,
        _: &str,
        _: &str,
        _: &AssetVersion,
    ) -> Result<AssetV4, EditorError> {
        unreachable!("Source audio must not acquire or rebind resources")
    }
}
impl ProjectV4Store for Fixture {
    fn create_v4_atomic(
        &mut self,
        _: &CreateV4Project,
        _: PreparedV4Create,
    ) -> Result<ProjectSnapshotV4, EditorError> {
        unreachable!()
    }
    fn copy_v4_atomic(
        &mut self,
        _: &CopyV4Project,
        _: PreparedV4Create,
    ) -> Result<ProjectSnapshotV4, EditorError> {
        unreachable!()
    }
    fn commit_v4_atomic(
        &mut self,
        intent: V4Intent<'_>,
        prepared: PreparedV4Commit,
    ) -> Result<TransactionResult, EditorError> {
        self.authorize_v4(intent)?;
        assert_eq!(intent.request_json()?, prepared.request_json());
        if intent.expected_revision() != Some(self.current.revision.as_str()) {
            return Err(crate::revision_conflict(
                intent.expected_revision().unwrap(),
                &self.current.revision,
            ));
        }
        let result = prepared.result().clone();
        self.current = prepared.snapshot().clone();
        self.commits += 1;
        self.receipts.insert(
            intent.key().into(),
            (intent.identity()?.current_digest().into(), result.clone()),
        );
        Ok(result)
    }
}
