//! Native assembly of the bounded Classic audio process adapter. This module
//! consumes fixed prepared facts; it never invokes an OpenCut CLI internally.
pub(crate) mod lifecycle;
mod process;
mod resources;
pub(crate) mod transport;
mod wav;
pub use process::{ClassicAudioProcess, ClassicAudioProcessConfig, ClassicAudioProcessResult};
pub use resources::VerifiedClassicAudioResources;
pub use wav::ClassicAudioWavSink;
