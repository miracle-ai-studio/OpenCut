use super::*;
use crate::scene::{
    PreparedCreate, PreparedSceneCommit, SceneStore, StoredProjectRevision,
    public::ProjectPublicResources,
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
    pub current: EditorSnapshot,
    pub deny: bool,
    pub reads: Cell<usize>,
    pub commits: usize,
    receipts: BTreeMap<String, (String, TransactionResult)>,
}
pub(super) fn volume() -> AnimationChannel {
    AnimationChannel::Scalar {
        id: "volume-channel".into(),
        target: ChannelTarget::Element {
            element_id: "video".into(),
        },
        binding: ScalarBinding::Property {
            parameter: ScalarParameter::GainDecibels,
        },
        keys: vec![
            ScalarKey {
                id: "volume-key-0".into(),
                segment_id: Some("volume-segment".into()),
                time: t(0, 1),
                value: -12.0,
                left_handle: None,
                right_handle: Some(CurveHandle {
                    dt: t(1, 3),
                    dv: 2.0,
                }),
                segment_to_next: ScalarSegment::Bezier,
                tangent_mode: TangentMode::Broken,
            },
            ScalarKey {
                id: "volume-key-1".into(),
                segment_id: None,
                time: t(2, 1),
                value: -3.0,
                left_handle: Some(CurveHandle {
                    dt: t(-1, 3),
                    dv: -1.0,
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
impl Fixture {
    pub fn new() -> Self {
        let mut project = crate::scene::new_document(&CreateProject {
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
        target.kind = SceneTrackKind::Audio;
        project.sequences[0].tracks.push(target);
        let hash = "1".repeat(64);
        project.assets.push(ProjectAsset {
            id: "asset".into(),
            name: "Video".into(),
            kind: AssetKind::Video,
            version: AssetVersion {
                id: format!("av_{hash}"),
                content_hash: hash,
                size: 1,
            },
            duration: Some(t(10, 1)),
            origin: ResourceOrigin::LegacyLocator {
                uri: "/private/source.mp4".into(),
            },
            metadata: ProjectMediaMetadata {
                mime_type: Some("video/mp4".into()),
                intrinsic_size: Some(CanvasSize {
                    width: 320,
                    height: 180,
                }),
                frame_rate: Some(FrameRate {
                    numerator: 25,
                    denominator: 1,
                }),
                rotation_degrees: None,
                audio: AudioPresence::Present { metadata: None },
            },
            license: None,
        });
        let mut position = volume();
        if let AnimationChannel::Scalar {
            id, binding, keys, ..
        } = &mut position
        {
            *id = "position-channel".into();
            *binding = ScalarBinding::Property {
                parameter: ScalarParameter::PositionXPx,
            };
            keys[0].id = "position-key-0".into();
            keys[0].segment_id = None;
            keys[1].id = "position-key-1".into();
        }
        project.sequences[0].tracks[0]
            .elements
            .push(TimelineElement {
                id: "video".into(),
                name: "Edited video".into(),
                timeline_range: opencut_contracts::TimeRange {
                    start: t(1, 3),
                    duration: t(2, 1),
                },
                source_span: ElementSourceSpan::ClassicTrimmed {
                    trim_start: t(1, 1),
                    trim_end: t(6, 1),
                    source_duration: Some(t(10, 1)),
                },
                enabled: false,
                content: ElementContent::Video {
                    asset_id: "asset".into(),
                    video_stream: Some(1),
                    source_audio_enabled: true,
                    audio: AudioStyle {
                        stream: Some(2),
                        gain: Gain::Decibels { value: -6.0 },
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
                visual: Some(VisualStyle {
                    profile: VisualProfile::ClassicCanvasV31,
                    transform: Transform {
                        position_x_px: 20.0,
                        position_y_px: -10.0,
                        scale_x: 0.5,
                        scale_y: 0.5,
                        rotation_degrees: 12.0,
                    },
                    opacity: 0.6,
                    blend_mode: BlendMode::Normal,
                    effects: vec![],
                    masks: vec![],
                }),
                channels: vec![volume(), position],
            });
        opencut_editor_core::scene::validate_project_scene(&project).unwrap();
        Self {
            current: EditorSnapshot {
                project,
                revision: "fixed".into(),
            },
            deny: false,
            reads: Cell::new(0),
            commits: 0,
            receipts: BTreeMap::new(),
        }
    }
    pub fn request(&self) -> SourceAudioRequest {
        SourceAudioRequest {
            project_id: "p".into(),
            expected_revision: self.current.revision.clone(),
            sequence_id: "s".into(),
            track_id: "main".into(),
            element_id: "video".into(),
            idempotency_key: "separate".into(),
            label: "Separate source audio".into(),
            command: SourceAudioCommand::Separate {
                audio_element_id: "detached".into(),
                destination: SourceAudioDestination::Existing {
                    track_id: "audio".into(),
                },
            },
        }
    }
    pub fn source(&self) -> &TimelineElement {
        &self.current.project.sequences[0].tracks[0].elements[0]
    }
}
impl SceneRead for Fixture {
    fn authorize_project(&self, _: SceneCommitIntent<'_>) -> Result<(), EditorError> {
        if self.deny {
            Err(EditorError::new("NOT_AUTHORIZED", "Denied current caller"))
        } else {
            Ok(())
        }
    }
    fn head_scene_source(&self, _: &str) -> Result<StoredProjectRevision, EditorError> {
        self.reads.set(self.reads.get() + 1);
        Ok(StoredProjectRevision {
            document: StoredProjectDocument::Editable(Box::new(self.current.project.clone())),
            revision: self.current.revision.clone(),
        })
    }
    fn revision_scene_source(
        &self,
        _: &str,
        _: &str,
    ) -> Result<StoredProjectRevision, EditorError> {
        unreachable!("Source audio uses the fixed current head")
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
        intent: SceneCommitIntent<'_>,
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
impl ProjectPublicResources for Fixture {
    fn authorized_project_media_scene(
        &self,
        _: &str,
        _: &str,
        _: &AssetVersion,
    ) -> Result<ProjectAsset, EditorError> {
        unreachable!("Source audio must not acquire or rebind resources")
    }
}
impl SceneStore for Fixture {
    fn create_scene_atomic(
        &mut self,
        _: &CreateProjectRequest,
        _: PreparedCreate,
    ) -> Result<EditorSnapshot, EditorError> {
        unreachable!()
    }
    fn copy_scene_atomic(
        &mut self,
        _: &CopySceneProject,
        _: PreparedCreate,
    ) -> Result<EditorSnapshot, EditorError> {
        unreachable!()
    }
    fn commit_scene_atomic(
        &mut self,
        intent: SceneCommitIntent<'_>,
        prepared: PreparedSceneCommit,
    ) -> Result<TransactionResult, EditorError> {
        self.authorize_project(intent)?;
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
