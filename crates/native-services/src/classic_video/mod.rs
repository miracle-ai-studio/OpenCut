//! Persistent V2 frame execution and encoding adapters. FFmpeg consumes the
//! verified shared RGBA/PCM only; it never compiles a project or lays out text.
mod encoder;
mod process;
pub use encoder::{
    ClassicVideoEncoder, ClassicVideoEncoderConfig, ClassicVideoEncoderObservation, resolve_tool,
};
pub use process::ClassicVideoProcess;
