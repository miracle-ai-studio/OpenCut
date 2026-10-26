//! Revision-owned V4 use cases. Adapters own CAS access, current authority,
//! serialization locks and atomic publication; no V4 side tables exist here.
use crate::{content_id, encode_json, require_idempotency_key, revision_conflict};
use opencut_contracts::{
    CreateProject, EditorError, ProjectDocument, TransactionResult,
    historical_visual::HistoricalVisualDocument, scene::*, scene_ops::*,
};
use opencut_editor_core::scene::validate_project_scene;
use std::collections::BTreeMap;
use uuid::Uuid;

mod bookmarks;
pub mod brand_resources;
pub mod catalog_resources;
mod edits;
mod entities;
pub mod history;
mod locks;
pub(crate) mod persistence;
mod ports;
pub mod public;

pub use edits::plan;
pub use persistence::{
    apply, copy, create, import_derived_font, import_font_resource, inspect,
    prepare_caption_commit, promote, restore,
};
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
pub fn new_document(input: &CreateProject) -> Result<EditorDocument, EditorError> {
    opencut_editor_core::create_project(input)?;
    let mut main_track_id = format!("track_{}", Uuid::new_v4().simple());
    while main_track_id == input.id || main_track_id == input.sequence_id {
        main_track_id = format!("track_{}", Uuid::new_v4().simple());
    }
    let project = EditorDocument {
        cover: None,
        schema_version: 4,
        id: input.id.clone(),
        name: input.name.clone(),
        default_sequence_id: input.sequence_id.clone(),
        settings: SettingsProfile::ClassicGlobal {
            settings: ProjectSettings {
                fps: input.fps.clone(),
                canvas_size: CanvasSize {
                    width: input.width,
                    height: input.height,
                },
                canvas_size_mode: CanvasSizeMode::Preset,
                last_custom_canvas_size: None,
                original_canvas_size: None,
                background: Background::Color {
                    color: Color {
                        encoding: ColorEncoding::Srgb,
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
        sequences: vec![Scene {
            id: input.sequence_id.clone(),
            name: "Main scene".into(),
            main_track_id: main_track_id.clone(),
            tracks: vec![SceneTrack {
                id: main_track_id,
                name: "Main Track".into(),
                kind: SceneTrackKind::Video,
                enabled: true,
                muted: false,
                locked: false,
                opacity: 1.0,
                gain: Gain::Decibels { value: 0.0 },
                elements: vec![],
                channels: vec![],
            }],
            captions: vec![],
            transitions: vec![],
            bookmarks: vec![],
        }],
    };
    validate_project_scene(&project)?;
    Ok(project)
}

pub(crate) fn converted(
    source: &StoredProjectDocument,
    versions: &BTreeMap<String, opencut_contracts::AssetVersion>,
) -> Result<EditorDocument, EditorError> {
    use opencut_editor_core::scene::legacy::*;
    let input = match source {
        StoredProjectDocument::Legacy(value) => LegacyProjectInput::Base(value),
        StoredProjectDocument::HistoricalVisual(value) => LegacyProjectInput::V3(value),
        StoredProjectDocument::Editable(value) => {
            if !versions.is_empty() {
                return Err(invalid("V4 resources cannot acquire legacy overrides"));
            }
            validate_project_scene(value)?;
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

/// Branch merges use the same prepared publication as every other edit.
pub(crate) fn prepare_branch_merge(
    source: &StoredProjectRevision,
    document: EditorDocument,
    request: &opencut_contracts::branching::BranchMergeRequest,
    base: &str,
    changes: &[opencut_contracts::project_history::HistoryChange],
) -> Result<PreparedSceneCommit, EditorError> {
    let mut prepared =
        persistence::prepare(source, document, SceneCommitIntent::BranchMerge(request))?;
    prepared.journal_json = Some(crate::encode_json(
        &opencut_contracts::branching::BranchJournal::BranchMerge {
            request: request.clone(),
            base_revision: base.into(),
            changes: changes.to_vec(),
        },
        "branch merge causal journal",
    )?);
    Ok(prepared)
}
