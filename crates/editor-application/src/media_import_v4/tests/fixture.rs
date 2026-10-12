use super::*;
use crate::project_v4::{ProjectV4Read, StoredProjectRevisionV4};
use opencut_contracts::{AssetVersion, CreateProject, TransactionResult};
use std::cell::Cell;

pub(super) struct Fixture {
    pub snapshot: ProjectSnapshotV4,
    pub media: BTreeMap<String, AssetV4>,
    pub media_packets: BTreeMap<String, opencut_media_contracts::ClassicPacketRateV1>,
    pub asset_packets: BTreeMap<String, opencut_media_contracts::ClassicPacketRateV1>,
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
pub(super) fn asset() -> AssetV4 {
    AssetV4 {
        id: "observation".into(),
        name: "Source".into(),
        kind: AssetKind::Video,
        version: version('1'),
        duration: Some(t(10, 1)),
        origin: ResourceOriginV4::LegacyLocator {
            uri: "/private/source.mp4".into(),
        },
        metadata: MediaMetadataV4 {
            mime_type: Some("video/mp4".into()),
            intrinsic_size: Some(CanvasSizeV4 {
                width: 1920,
                height: 1080,
            }),
            frame_rate: Some(FrameRate {
                numerator: 2997,
                denominator: 100,
            }),
            rotation_degrees: Some(0.0),
            audio: AudioPresenceV4::Present { metadata: None },
        },
        license: None,
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
        audio.kind = TrackKindV4::Audio;
        project.sequences[0].tracks.push(audio);
        Self {
            snapshot: ProjectSnapshotV4 {
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
    pub fn request(&self) -> MediaImportRequestV4 {
        MediaImportRequestV4 {
            project_id: "p".into(),
            expected_revision: "fixed".into(),
            sequence_id: "s".into(),
            idempotency_key: "import-key".into(),
            label: "Insert observed media".into(),
            bindings: vec![MediaBindingV4 {
                media_id: "available".into(),
                version: self.media["available"].version.clone(),
                asset_id: "asset".into(),
                name: "Video".into(),
                mode: MediaBindingModeV4::Bind,
            }],
            placements: vec![],
            ripple: false,
        }
    }
    pub fn video(&self, timing: MediaTimingV4) -> MediaPlacementV4 {
        MediaPlacementV4 {
            element_id: "video".into(),
            name: "Video".into(),
            track_id: "main".into(),
            asset_id: "asset".into(),
            start: t(7, 1),
            content: MediaPlacementContentV4::Video {
                timing,
                source_audio_enabled: true,
            },
        }
    }
    pub fn source_timing(&self) -> MediaTimingV4 {
        MediaTimingV4::SourceRange {
            range: TimeRange {
                start: t(1, 1),
                duration: t(1, 7),
            },
            rate: RationalRateV4 {
                numerator: 3,
                denominator: 2,
            },
            maintain_pitch: true,
        }
    }
}
impl ProjectV4Read for Fixture {
    fn authorize_v4(&self, _: V4Intent<'_>) -> Result<(), EditorError> {
        if self.deny {
            Err(EditorError::new("NOT_AUTHORIZED", "Current caller denied"))
        } else {
            Ok(())
        }
    }
    fn head_v4_source(&self, _: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        self.reads.set(self.reads.get() + 1);
        Ok(StoredProjectRevisionV4 {
            document: StoredProjectDocumentV4::V4(Box::new(self.snapshot.project.clone())),
            revision: self.snapshot.revision.clone(),
        })
    }
    fn revision_v4_source(&self, _: &str, _: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        unreachable!("Import never follows an unpinned resource revision")
    }
    fn replay_v4_create(
        &self,
        _: &CreateV4Project,
    ) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        unreachable!("Read-only plan has no receipt")
    }
    fn replay_v4_copy(&self, _: &CopyV4Project) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        unreachable!("Read-only plan has no receipt")
    }
    fn replay_v4_edit(&self, _: V4Intent<'_>) -> Result<Option<TransactionResult>, EditorError> {
        unreachable!("Read-only plan has no receipt")
    }
}
impl ProjectV4PublicResources for Fixture {
    fn authorized_project_media_packet_rate_v4(
        &self,
        project: &str,
        media: &str,
        version: &AssetVersion,
    ) -> Result<Option<opencut_media_contracts::ClassicPacketRateV1>, EditorError> {
        let observation = self.media_packets.get(media).cloned();
        if observation.is_some() {
            assert_eq!(project, "p");
            assert_eq!(&self.media[media].version, version);
        }
        Ok(observation)
    }
    fn project_asset_packet_rate_v4(
        &self,
        project: &str,
        revision: &str,
        asset: &str,
    ) -> Result<Option<opencut_media_contracts::ClassicPacketRateV1>, EditorError> {
        assert_eq!(project, "p");
        assert_eq!(revision, self.snapshot.revision);
        assert!(self.snapshot.project.assets.iter().any(|a| a.id == asset));
        Ok(self.asset_packets.get(asset).cloned())
    }
    fn authorized_project_media_v4(
        &self,
        project: &str,
        media: &str,
        version: &AssetVersion,
    ) -> Result<AssetV4, EditorError> {
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
