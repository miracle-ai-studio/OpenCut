//! Actual isolated WebGPU execution of fixed Classic visual frames.
mod png;
mod process;
mod resources;
pub use png::ClassicVisualPngSink;
pub use process::{ClassicVisualProcess, ClassicVisualProcessConfig};
pub use resources::VerifiedClassicVisualResources;
