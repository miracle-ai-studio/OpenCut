//! Actual isolated WebGPU execution of fixed Classic visual frames.
mod png;
mod process;
mod resources;
mod text_resources;
pub use text_resources::VerifiedClassicTextResources;
mod session_adapter;
pub use png::ClassicVisualPngSink;
pub use process::{ClassicVisualProcess, ClassicVisualProcessConfig};
pub use resources::VerifiedClassicVisualResources;
pub(crate) use session_adapter::VisualProcessSession;

pub(crate) use process::{validate_identity, validate_quota};
