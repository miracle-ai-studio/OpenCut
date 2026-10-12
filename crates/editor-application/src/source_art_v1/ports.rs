use super::*;
use opencut_contracts::{AssetVersion, project_v4::ProjectSnapshotV4};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceArtPolicyV1 {
    pub max_encoded_bytes: u64,
    pub max_pcm_bytes: u64,
    pub max_rss_bytes: u64,
    pub deadline_ms: u64,
}

#[derive(Clone, Debug)]
pub struct AuthorizedSourceArtV1 {
    pub snapshot: ProjectSnapshotV4,
    pub authority: TrustedJobAuthorityV1,
    /// Trusted workspace/principal/credential/project cache partition. Never
    /// caller-supplied, never returned as a public capability or native path.
    pub scope: String,
    pub policy: SourceArtPolicyV1,
}

pub trait SourceArtReadV1 {
    /// One fixed-revision read with current Read + RunJobs, live project status,
    /// original caller identity and local execution policy checked together.
    fn fixed_source_art_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<AuthorizedSourceArtV1, EditorError>;
    /// Used on cache hit, completion and native checkpoints for the original
    /// credential and this waiter's own fixed revision; a hash grants no access.
    fn revalidate_source_art(
        &self,
        project_id: &str,
        revision: &str,
        authority: &TrustedJobAuthorityV1,
    ) -> Result<(), EditorError>;
}

pub trait SourceArtCheckpointV1 {
    /// The host combines live authorization, deadline and waiter cancellation.
    /// Native adapters must acknowledge real process exit before releasing CAS.
    fn checkpoint(&mut self) -> Result<(), EditorError>;
}

pub trait SourceArtExecutorV1 {
    fn execute(
        &self,
        prepared: &PreparedSourceArtV1,
        checkpoint: &mut dyn SourceArtCheckpointV1,
    ) -> Result<DecodedSourceArtV1, EditorError>;
}

#[derive(Clone, Debug, PartialEq)]
pub enum DecodedSourceArtV1 {
    Waveform {
        identity: SourceArtIdentityV1,
        version: AssetVersion,
        sample_rate: u32,
        channels: u32,
        total_samples: u64,
        bucket_size: u32,
        /// The whole-source summary, not a caller's viewport projection.
        amplitudes: Vec<f32>,
    },
    ThumbnailReady {
        identity: SourceArtIdentityV1,
        version: AssetVersion,
        width: u32,
        height: u32,
        jpeg_bytes: Vec<u8>,
        observation: SourceArtThumbnailObservationV1,
    },
    ThumbnailNoFrame {
        identity: SourceArtIdentityV1,
        version: AssetVersion,
        observation: SourceArtThumbnailObservationV1,
        reason: SourceArtNoFrameReasonV1,
    },
}

impl DecodedSourceArtV1 {
    pub fn version(&self) -> &AssetVersion {
        match self {
            Self::Waveform { version, .. }
            | Self::ThumbnailReady { version, .. }
            | Self::ThumbnailNoFrame { version, .. } => version,
        }
    }
    pub fn identity(&self) -> &SourceArtIdentityV1 {
        match self {
            Self::Waveform { identity, .. }
            | Self::ThumbnailReady { identity, .. }
            | Self::ThumbnailNoFrame { identity, .. } => identity,
        }
    }
}
