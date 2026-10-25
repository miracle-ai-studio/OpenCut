use super::*;
use opencut_contracts::{AssetVersion, scene::EditorSnapshot};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceArtPolicy {
    pub max_encoded_bytes: u64,
    pub max_pcm_bytes: u64,
    pub max_rss_bytes: u64,
    pub deadline_ms: u64,
}

#[derive(Clone, Debug)]
pub struct AuthorizedSourceArt {
    pub snapshot: EditorSnapshot,
    pub authority: TrustedJobAuthority,
    /// Trusted workspace/principal/credential/project cache partition. Never
    /// caller-supplied, never returned as a public capability or native path.
    pub scope: String,
    pub policy: SourceArtPolicy,
}

pub trait SourceArtRead {
    /// One fixed-revision read with current Read + RunJobs, live project status,
    /// original caller identity and local execution policy checked together.
    fn fixed_source_art_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<AuthorizedSourceArt, EditorError>;
    /// Used on cache hit, completion and native checkpoints for the original
    /// credential and this waiter's own fixed revision; a hash grants no access.
    fn revalidate_source_art(
        &self,
        project_id: &str,
        revision: &str,
        authority: &TrustedJobAuthority,
    ) -> Result<(), EditorError>;
}

pub trait SourceArtCheckpoint {
    /// The host combines live authorization, deadline and waiter cancellation.
    /// Native adapters must acknowledge real process exit before releasing CAS.
    fn checkpoint(&mut self) -> Result<(), EditorError>;
}

pub trait SourceArtExecutor {
    fn execute(
        &self,
        prepared: &PreparedSourceArt,
        checkpoint: &mut dyn SourceArtCheckpoint,
    ) -> Result<DecodedSourceArt, EditorError>;
}

#[derive(Clone, Debug, PartialEq)]
pub enum DecodedSourceArt {
    Waveform {
        identity: SourceArtIdentity,
        version: AssetVersion,
        sample_rate: u32,
        channels: u32,
        total_samples: u64,
        bucket_size: u32,
        /// The whole-source summary, not a caller's viewport projection.
        amplitudes: Vec<f32>,
    },
    ThumbnailReady {
        identity: SourceArtIdentity,
        version: AssetVersion,
        width: u32,
        height: u32,
        jpeg_bytes: Vec<u8>,
        observation: SourceArtThumbnailObservation,
    },
    ThumbnailNoFrame {
        identity: SourceArtIdentity,
        version: AssetVersion,
        observation: SourceArtThumbnailObservation,
        reason: SourceArtNoFrameReason,
    },
}

impl DecodedSourceArt {
    pub fn version(&self) -> &AssetVersion {
        match self {
            Self::Waveform { version, .. }
            | Self::ThumbnailReady { version, .. }
            | Self::ThumbnailNoFrame { version, .. } => version,
        }
    }
    pub fn identity(&self) -> &SourceArtIdentity {
        match self {
            Self::Waveform { identity, .. }
            | Self::ThumbnailReady { identity, .. }
            | Self::ThumbnailNoFrame { identity, .. } => identity,
        }
    }
}
