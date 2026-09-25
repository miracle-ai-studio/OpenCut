//! Pure fixed-revision composition compiler. No files, codecs, processes or database.
mod compiler;
pub mod project_v3;
mod selection;
pub use compiler::compile;
pub use selection::{select_project_frame_time, select_project_range};
/// Shared diagnostics used by native preflight as well as compilation.
pub mod validation {
    pub use crate::compiler::{
        VIDEO_PARAMS, VISUAL_PARAMS, map_font_policy, map_text_align, time, validate_channels,
        validate_params,
    };
}
use opencut_contracts::{AssetVersion, ProjectSnapshot, Time};
use opencut_media_contracts::{FrameRate, MediaTime, composition::Timeline};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceRef {
    pub asset_id: String,
    pub version: Option<AssetVersion>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompositionScope {
    Visual,
    AudioVisual,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Composition {
    pub format: CompositionFormat,
    pub revision: String,
    pub sequence_id: String,
    pub scope: CompositionScope,
    pub timeline: Timeline<SourceRef>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum CompositionFormat {
    #[serde(rename = "opencut.composition.v1")]
    V1,
}
#[derive(Debug)]
pub struct CompositionError {
    pub code: &'static str,
    pub message: String,
}
impl std::fmt::Display for CompositionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for CompositionError {}
