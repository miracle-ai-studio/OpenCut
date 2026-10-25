use super::*;
use crate::scene::{SceneRead, StoredProjectRevision};
use opencut_contracts::{AssetVersion, CreateProject, TransactionResult};
use std::cell::Cell;

pub(super) struct Fixture {
    pub snapshot: EditorSnapshot,
    pub media: BTreeMap<String, ProjectAsset>,
    pub media_packets: BTreeMap<String, opencut_media_contracts::ClassicPacketRate>,
    pub asset_packets: BTreeMap<String, opencut_media_contracts::ClassicPacketRate>,
    pub deny: bool,
    pub reads: Cell<usize>,
}
pub(super) fn t(n: i64, d: u32) -> Time {
    RationalTime::new(n, d).unwrap().into()
}
pub(super) fn version(digit: char) -> AssetVersion {
    let hash = digit.to_string().repeat(64);
    AssetVersion {
        id: format!("av_{hash}"),
        content_hash: hash,
        size: 1,
    }
}
pub(super) fn asset() -> ProjectAsset {
    ProjectAsset {
        id: "observation".into(),
        name: "Source".into(),
        kind: AssetKind::Video,
        version: version('1'),
        duration: Some(t(10, 1)),
        origin: ResourceOrigin::LegacyLocator {
            uri: "/private/source.mp4".into(),
        },
        metadata: ProjectMediaMetadata {
            mime_type: Some("video/mp4".into()),
            intrinsic_size: Some(CanvasSize {
                width: 1920,
                height: 1080,
            }),
            frame_rate: Some(FrameRate {
                numerator: 2997,
                denominator: 100,
            }),
            rotation_degrees: Some(0.0),
            audio: AudioPresence::Present { metadata: None },
        },
        license: None,
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
                numerator: 24,
                denominator: 1,
            },
        })
        .unwrap();
        project.sequences[0].main_track_id = "main".into();
        project.sequences[0].tracks[0].id = "main".into();
        let mut audio = project.sequences[0].tracks[0].clone();
        audio.id = "audio".into();
        audio.name = "Audio".into();
        audio.kind = SceneTrackKind::Audio;
        project.sequences[0].tracks.push(audio);
        Self {
            snapshot: EditorSnapshot {
                project,
                revision: "fixed".into(),
            },
            media: BTreeMap::from([("available".into(), asset())]),
            media_packets: BTreeMap::new(),
            asset_packets: BTreeMap::new(),
            deny: false,
            reads: Cell::new(0),
        }
    }
    pub fn request(&self) -> MediaImportRequest {
        MediaImportRequest {
            project_id: "p".into(),
            expected_revision: "fixed".into(),
            sequence_id: "s".into(),
            idempotency_key: "import-key".into(),
            label: "Insert observed media".into(),
            bindings: vec![MediaBinding {
                media_id: "available".into(),
                version: self.media["available"].version.clone(),
                asset_id: "asset".into(),
                name: "Video".into(),
                mode: MediaBindingMode::Bind,
            }],
            placements: vec![],
            ripple: false,
            new_tracks: vec![],
        }
    }
    pub fn video(&self, timing: MediaTiming) -> MediaPlacement {
        MediaPlacement {
            element_id: "video".into(),
            name: "Video".into(),
            track_id: "main".into(),
            asset_id: "asset".into(),
            start: t(7, 1),
            content: MediaPlacementContent::Video {
                timing,
                source_audio_enabled: true,
            },
        }
    }
    pub fn source_timing(&self) -> MediaTiming {
        MediaTiming::SourceRange {
            range: TimeRange {
                start: t(1, 1),
                duration: t(1, 7),
            },
            rate: RationalRate {
                numerator: 3,
                denominator: 2,
            },
            maintain_pitch: true,
        }
    }
}
impl SceneRead for Fixture {
    fn authorize_project(&self, _: SceneCommitIntent<'_>) -> Result<(), EditorError> {
        if self.deny {
            Err(EditorError::new("NOT_AUTHORIZED", "Current caller denied"))
        } else {
            Ok(())
        }
    }
    fn head_scene_source(&self, _: &str) -> Result<StoredProjectRevision, EditorError> {
        self.reads.set(self.reads.get() + 1);
        Ok(StoredProjectRevision {
            document: StoredProjectDocument::Editable(Box::new(self.snapshot.project.clone())),
            revision: self.snapshot.revision.clone(),
        })
    }
    fn revision_scene_source(
        &self,
        _: &str,
        _: &str,
    ) -> Result<StoredProjectRevision, EditorError> {
        unreachable!("Import never follows an unpinned resource revision")
    }
    fn replay_scene_create(
        &self,
        _: &CreateProjectRequest,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        unreachable!("Read-only plan has no receipt")
    }
    fn replay_scene_copy(
        &self,
        _: &CopySceneProject,
    ) -> Result<Option<EditorSnapshot>, EditorError> {
        unreachable!("Read-only plan has no receipt")
    }
    fn replay_scene_edit(
        &self,
        _: SceneCommitIntent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        unreachable!("Read-only plan has no receipt")
    }
}
impl ProjectPublicResources for Fixture {
    fn authorized_project_media_packet_rate_scene(
        &self,
        project: &str,
        media: &str,
        version: &AssetVersion,
    ) -> Result<Option<opencut_media_contracts::ClassicPacketRate>, EditorError> {
        let observation = self.media_packets.get(media).cloned();
        if observation.is_some() {
            assert_eq!(project, "p");
            assert_eq!(&self.media[media].version, version);
        }
        Ok(observation)
    }
    fn project_asset_packet_rate_scene(
        &self,
        project: &str,
        revision: &str,
        asset: &str,
    ) -> Result<Option<opencut_media_contracts::ClassicPacketRate>, EditorError> {
        assert_eq!(project, "p");
        assert_eq!(revision, self.snapshot.revision);
        assert!(self.snapshot.project.assets.iter().any(|a| a.id == asset));
        Ok(self.asset_packets.get(asset).cloned())
    }
    fn authorized_project_media_scene(
        &self,
        project: &str,
        media: &str,
        version: &AssetVersion,
    ) -> Result<ProjectAsset, EditorError> {
        self.reads.set(self.reads.get() + 1);
        if project != "p" {
            return Err(EditorError::new(
                "NOT_AUTHORIZED",
                "Observation belongs to another project",
            ));
        }
        let asset = self
            .media
            .get(media)
            .ok_or_else(|| missing("Available media missing"))?;
        if &asset.version != version {
            return Err(EditorError::new(
                "NOT_AUTHORIZED",
                "Observed immutable identity differs",
            ));
        }
        Ok(asset.clone())
    }
}
