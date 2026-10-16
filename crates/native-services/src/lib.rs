//! Native capability adapters for portable application use cases.
pub mod classic_mask_frame_v1;
pub mod classic_scene_frame_v1;
pub mod export_video_v2;
pub mod media;
pub mod plugin_v2;
pub mod plugins;
pub mod preview;
pub mod project_history_package_v1;
pub mod project_package;
pub mod project_package_v3;
pub mod project_package_v4;
pub mod render;
pub mod render_v3;
pub mod reproducibility;
pub mod uploads;

pub mod storage;

mod browser_process;
mod browser_resources;
pub mod classic_audio;
pub mod classic_audio_preview;
pub mod classic_video;
pub mod classic_visual;
pub mod font_catalog_v3;
pub mod font_derived_v1;
pub mod font_derived_v2;
pub mod font_import_v1;
pub mod font_import_v2;
pub mod font_synthesis_v1;
pub mod gc;

pub mod artifacts;
pub mod budget;

mod preview_inputs;
pub mod source_art_v1;

mod classic_blend_frame_v1;
pub mod classic_graphic_frame_v1;
pub mod classic_text_frame_v3;

pub mod catalog_resources_v1;
