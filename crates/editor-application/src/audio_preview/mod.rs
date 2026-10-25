//! Fixed-revision, process-local audio session semantics. Native hosts own CAS,
//! processes, current authorization and binary transport; none is stored here.
mod machine;
mod ports;
pub use machine::*;
pub use opencut_api_contracts::audio_preview::*;
pub use ports::*;
#[cfg(test)]
mod tests;
