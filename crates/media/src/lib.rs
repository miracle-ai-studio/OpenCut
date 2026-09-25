//! Native media probing, exact frame extraction, export, and verification.
//!
//! This crate deliberately owns transport-neutral DTOs. It does not depend on
//! OpenCut's project model or job scheduler; those layers adapt these blocking
//! worker operations and persist their progress and results.

mod admission;
mod analysis;
mod backend;
mod caption;
mod composition_v3;
mod dto;
mod error;
mod preview_stream;
mod process;
mod speaker_turns;
mod transcription;
pub use admission::{OutputAdmission, OutputProcess};
pub use analysis::FfmpegAnalysisBackend;
pub use speaker_turns::TinydiarizeBackend;

pub use backend::{FfmpegBackend, MediaToolFingerprint};
pub use dto::*;
pub use error::{MediaError, MediaResult};
pub use preview_stream::{
    ContinuousPreviewProducer, PreviewStreamLimits, PreviewStreamRead, PreviewStreamStatus,
};
pub use process::CancellationToken;
pub use transcription::{WhisperBackend, validate_transcription_language};

/// Shapes and rasterizes a caption fully in memory, returning the actual layout/font report.
pub fn layout_caption(
    caption: &TimelineCaption,
    width: u32,
    height: u32,
) -> MediaResult<TextLayoutReport> {
    caption::layout(caption, width, height)
}
