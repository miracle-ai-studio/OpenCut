//! Revision-owned V4 use cases. Adapters own CAS access, current authority,
//! serialization locks and atomic publication; no V4 side tables exist here.
use crate::{content_id, encode_json, require_idempotency_key, revision_conflict};
use opencut_contracts::{
    CreateProject, EditorError, ProjectDocument, TransactionResult, project_v3::ProjectDocumentV3,
    project_v4::*, project_v4_ops::*,
};
use opencut_editor_core::project_v4::validate_project_v4;
use std::collections::BTreeMap;
use uuid::Uuid;

mod bookmarks;
mod edits;
mod entities;
pub mod history;
mod locks;
mod persistence;
mod ports;
pub mod public;
pub mod resources;
pub use edits::plan;
pub use persistence::{apply, copy, create, import_font_v1, inspect, list, promote, restore};
pub use ports::*;
#[cfg(test)]
mod tests;

fn invalid(message: impl Into<String>) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn missing(entity: &str) -> EditorError {
    EditorError::new("NOT_FOUND", format!("V4 {entity} does not exist"))
}

/// Explicit ClassicGlobal initialization; requested canvas/FPS replace Classic
/// defaults, while black background/preset mode and an empty main video track
/// match Classic project-manager.ts, timeline/scenes.ts and placement/main-track.ts.
pub fn new_document(input: &CreateProject) -> Result<ProjectDocumentV4, EditorError> {
    opencut_editor_core::create_project(input)?;
    let mut main_track_id = format!("track_{}", Uuid::new_v4().simple());
    while main_track_id == input.id || main_track_id == input.sequence_id {
        main_track_id = format!("track_{}", Uuid::new_v4().simple());
    }
    let project = ProjectDocumentV4 {
        schema_version: 4,
        id: input.id.clone(),
        name: input.name.clone(),
        default_sequence_id: input.sequence_id.clone(),
        settings: SettingsProfileV4::ClassicGlobal {
            settings: ProjectSettingsV4 {
                fps: input.fps.clone(),
                canvas_size: CanvasSizeV4 {
                    width: input.width,
                    height: input.height,
                },
                canvas_size_mode: CanvasSizeModeV4::Preset,
                last_custom_canvas_size: None,
                original_canvas_size: None,
                background: BackgroundV4::Color {
                    color: ColorV4 {
                        encoding: ColorEncodingV4::Srgb,
                        red: 0.0,
                        green: 0.0,
                        blue: 0.0,
                        alpha: 1.0,
                    },
                },
            },
        },
        assets: vec![],
        fonts: vec![],
        sequences: vec![SequenceV4 {
            id: input.sequence_id.clone(),
            name: "Main scene".into(),
            main_track_id: main_track_id.clone(),
            tracks: vec![TrackV4 {
                id: main_track_id,
                name: "Main Track".into(),
                kind: TrackKindV4::Video,
                enabled: true,
                muted: false,
                locked: false,
                opacity: 1.0,
                gain: GainV4::Decibels { value: 0.0 },
                elements: vec![],
                channels: vec![],
            }],
            captions: vec![],
            transitions: vec![],
            bookmarks: vec![],
        }],
    };
    validate_project_v4(&project)?;
    Ok(project)
}

pub(super) fn converted(
    source: &StoredProjectDocumentV4,
    versions: &BTreeMap<String, opencut_contracts::AssetVersion>,
) -> Result<ProjectDocumentV4, EditorError> {
    use opencut_editor_core::project_v4::legacy::*;
    let input = match source {
        StoredProjectDocumentV4::Legacy(value) => LegacyProjectInput::Base(value),
        StoredProjectDocumentV4::V3(value) => LegacyProjectInput::V3(value),
        StoredProjectDocumentV4::V4(value) => {
            if !versions.is_empty() {
                return Err(invalid("V4 resources cannot acquire legacy overrides"));
            }
            validate_project_v4(value)?;
            return Ok((**value).clone());
        }
    };
    match convert_legacy_project(
        input,
        &LegacyConversionInputs {
            asset_versions: versions.clone(),
        },
    ) {
        LegacyConversionResult::Converted { project, .. } => Ok(*project),
        LegacyConversionResult::Rejected { report } => {
            let mut error = EditorError::new(
                "CONVERSION_REJECTED",
                "Legacy project cannot be promoted without resolving reported issues",
            );
            error.details = serde_json::json!({"issues": report.issues.iter().map(|issue| {
                serde_json::json!({"path":issue.path,"reason":format!("{:?}",issue.kind)})
            }).collect::<Vec<_>>()});
            Err(error)
        }
    }
}
