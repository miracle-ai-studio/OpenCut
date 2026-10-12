//! Native capability adapters for portable application use cases.
pub mod media;
pub mod plugin_v2;
pub mod plugins;
pub mod preview;
pub mod project_package;
pub mod project_package_v3;
pub mod render;
pub mod render_v3;
pub mod reproducibility;
pub mod uploads;

pub mod storage;

mod browser_process;
mod browser_resources;
pub mod classic_audio;
pub mod classic_audio_preview;
pub mod classic_visual;
pub mod gc;

pub mod artifacts;
pub mod budget;

mod preview_inputs;
pub mod source_art_v1;
