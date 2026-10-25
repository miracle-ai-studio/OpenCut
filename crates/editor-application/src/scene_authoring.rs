//! Read-only, fixed-head bounds for static Classic visual-stack authoring. These
//! metadata bounds never substitute for the renderer's actual decoded binding.
use crate::scene::{SceneCommitIntent, StoredProjectDocument, public::ProjectPublicResources};
use opencut_contracts::{
    EditorError, mask_frame::MaskQuad, project_public::ApplyProject, scene::*, scene_authoring::*,
};
use opencut_time::RationalTime;

fn unsupported(message: &str) -> EditorError {
    EditorError::new("UNSUPPORTED_VISUAL_FEATURE", message)
}
fn finite(value: f64) -> Result<f64, EditorError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(unsupported(
            "Mask authoring geometry exceeds finite logical coordinates",
        ))
    }
}

fn intrinsic_size(
    project: &EditorDocument,
    element: &TimelineElement,
) -> Result<CanvasSize, EditorError> {
    match &element.content {
        ElementContent::Graphic { graphic } => Ok(graphic.source_size),
        ElementContent::Image { asset_id } | ElementContent::Video { asset_id, .. } => {
            let asset = project
                .assets
                .iter()
                .find(|asset| asset.id == *asset_id)
                .ok_or_else(|| EditorError::new("NOT_FOUND", "Mask media resource is missing"))?;
            let size = asset
                .metadata
                .intrinsic_size
                .ok_or_else(|| unsupported("Mask authoring requires observed media dimensions"))?;
            // Metadata is the encoded extent; the decoder exposes upright
            // dimensions. Non-quarter-turn display matrices need a real decode.
            let rotation = asset
                .metadata
                .rotation_degrees
                .unwrap_or(0.0)
                .rem_euclid(360.0);
            if rotation == 0.0 || rotation == 180.0 {
                Ok(size)
            } else if rotation == 90.0 || rotation == 270.0 {
                Ok(CanvasSize {
                    width: size.height,
                    height: size.width,
                })
            } else {
                Err(unsupported(
                    "Mask authoring requires known quarter-turn media orientation",
                ))
            }
        }
        _ => Err(unsupported(
            "Only video, image and graphic elements support box-mask authoring",
        )),
    }
}

pub fn plan(
    store: &impl ProjectPublicResources,
    input: &SceneGeometryRequest,
) -> Result<SceneGeometryResult, EditorError> {
    if [
        &input.project_id,
        &input.expected_revision,
        &input.sequence_id,
        &input.element_id,
    ]
    .iter()
    .any(|id| id.trim().is_empty() || id.len() > 128 || id.chars().any(char::is_control))
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Mask authoring requires bounded fixed identities",
        ));
    }
    let time = RationalTime::try_from(&input.time)?;
    if time.ticks() < 0 {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Mask authoring requires nonnegative exact time",
        ));
    }
    // Authoring reads use the same edit authorization as the eventual public
    // element_put. This empty intent is never submitted or given a receipt.
    let intent = ApplyProject {
        branch_name: None,
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: "scene-geometry-read-v1".into(),
        label: "Edit mask".into(),
        operations: vec![],
    };
    store.authorize_project(SceneCommitIntent::PublicApply(&intent))?;
    let source = store.head_scene_source(&input.project_id)?;
    if source.document.project_id() != input.project_id {
        return Err(EditorError::new("STORAGE_ERROR", "Mask head scope differs"));
    }
    if source.revision != input.expected_revision {
        return Err(crate::revision_conflict(
            &input.expected_revision,
            &source.revision,
        ));
    }
    let StoredProjectDocument::Editable(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Mask authoring requires a V4 head",
        ));
    };
    opencut_editor_core::scene::validate_project_scene(&project)?;
    let sequence = project
        .sequences
        .iter()
        .find(|sequence| sequence.id == input.sequence_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Mask scene is missing"))?;
    let (track, element) = sequence
        .tracks
        .iter()
        .find_map(|track| {
            track
                .elements
                .iter()
                .find(|element| element.id == input.element_id)
                .map(|element| (track, element))
        })
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Mask element is missing"))?;
    if track.locked {
        return Err(EditorError::new(
            "TRACK_LOCKED",
            "Unlock the track before editing its mask",
        ));
    }
    let start = RationalTime::try_from(&element.timeline_range.start)?;
    let end = start.checked_add(RationalTime::try_from(&element.timeline_range.duration)?)?;
    if !track.enabled
        || !element.enabled
        || time.cmp_exact(start).is_lt()
        || !time.cmp_exact(end).is_lt()
    {
        return Err(unsupported(
            "Mask element is inactive at the requested time",
        ));
    }
    if !track.channels.is_empty()
        || !element.channels.is_empty()
        || sequence.transitions.iter().any(|transition| {
            transition.outgoing_element_id == element.id
                || transition.incoming_element_id == element.id
        })
    {
        return Err(unsupported(
            "Animated or transitioning mask targets cannot be edited statically",
        ));
    }
    let visual = element
        .visual
        .as_ref()
        .ok_or_else(|| unsupported("Mask target has no visual style"))?;
    // Effects and mask order are authoring data, never evaluated by this read.
    // The Scene renderer independently admits the complete sampled stack.
    if visual.profile != VisualProfile::ClassicCanvasV31 {
        return Err(unsupported(
            "Scene authoring requires Classic logical geometry",
        ));
    }
    let canvas = match &project.settings {
        SettingsProfile::ClassicGlobal { settings } => settings.canvas_size,
        SettingsProfile::LegacyPerScene { formats } => {
            formats
                .iter()
                .find(|format| format.sequence_id == sequence.id)
                .ok_or_else(|| unsupported("Mask scene format is missing"))?
                .canvas_size
        }
    };
    let size = intrinsic_size(&project, element)?;
    if size.width == 0 || size.height == 0 {
        return Err(unsupported(
            "Mask authoring requires positive observed dimensions",
        ));
    }
    let transform = visual.transform;
    let contain = (f64::from(canvas.width) / f64::from(size.width))
        .min(f64::from(canvas.height) / f64::from(size.height));
    let width = f64::from(size.width) * contain * transform.scale_x;
    let height = f64::from(size.height) * contain * transform.scale_y;
    let quad = MaskQuad {
        center_x: finite(f64::from(canvas.width) / 2.0 + transform.position_x_px)?,
        center_y: finite(f64::from(canvas.height) / 2.0 + transform.position_y_px)?,
        width: finite(width.abs())?,
        height: finite(height.abs())?,
        rotation_radians: finite(transform.rotation_degrees * std::f64::consts::PI / 180.0)?,
        flip_x: width < 0.0,
        flip_y: height < 0.0,
        opacity: finite(track.opacity * visual.opacity)?,
    };
    if quad.width <= 0.0 || quad.height <= 0.0 || !quad.rotation_radians.is_finite() {
        return Err(unsupported("Zero-sized mask targets cannot be edited"));
    }
    let side = width.abs().min(height.abs()) * 0.6;
    let default_geometry = BoxMaskGeometry {
        center_x: 0.0,
        center_y: 0.0,
        width: side / width.abs(),
        height: side / height.abs(),
        rotation_degrees: 0.0,
        scale: 1.0,
    };
    store.authorize_project(SceneCommitIntent::PublicApply(&intent))?;
    let current = store.head_scene_source(&input.project_id)?;
    if current.document.project_id() != input.project_id {
        return Err(EditorError::new("STORAGE_ERROR", "Mask head scope differs"));
    }
    if current.revision != input.expected_revision {
        return Err(crate::revision_conflict(
            &input.expected_revision,
            &current.revision,
        ));
    }
    Ok(SceneGeometryResult {
        revision: source.revision,
        element_id: input.element_id.clone(),
        quad,
        default_geometry,
    })
}

#[cfg(test)]
mod tests;
