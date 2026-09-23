//! Native media probing, exact frame extraction, export, and verification.
//!
//! This crate deliberately owns transport-neutral DTOs. It does not depend on
//! OpenCut's project model or job scheduler; those layers adapt these blocking
//! worker operations and persist their progress and results.

mod admission;
mod backend;
mod caption;
mod dto;
mod error;
mod process;
pub use admission::{OutputAdmission, OutputProcess};

pub use backend::FfmpegBackend;
pub use dto::*;
pub use error::{MediaError, MediaResult};
pub use process::CancellationToken;

/// Shapes and rasterizes a caption fully in memory, returning the actual layout/font report.
pub fn layout_caption(
    caption: &TimelineCaption,
    width: u32,
    height: u32,
) -> MediaResult<TextLayoutReport> {
    caption::layout(caption, width, height)
}
