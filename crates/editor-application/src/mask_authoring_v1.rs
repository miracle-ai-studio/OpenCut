//! Read-only, fixed-head bounds for static Classic box-mask authoring. These
//! metadata bounds never substitute for the renderer's actual decoded binding.
use crate::project_v4::{StoredProjectDocumentV4, V4Intent, public::ProjectV4PublicResources};
use opencut_contracts::{
    EditorError, mask_authoring_v1::*, mask_frame_v1::MaskQuadV1, project_v4::*,
    project_v4_public::ApplyPublicV4Project,
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
    project: &ProjectDocumentV4,
    element: &TimelineElementV4,
) -> Result<CanvasSizeV4, EditorError> {
    match &element.content {
        ElementContentV4::Graphic { graphic } => Ok(graphic.source_size),
        ElementContentV4::Image { asset_id } | ElementContentV4::Video { asset_id, .. } => {
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
                Ok(CanvasSizeV4 {
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
    store: &impl ProjectV4PublicResources,
    input: &MaskGeometryRequestV1,
) -> Result<MaskGeometryResultV1, EditorError> {
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
    let intent = ApplyPublicV4Project {
        project_id: input.project_id.clone(),
        expected_revision: input.expected_revision.clone(),
        idempotency_key: "mask-geometry-read-v1".into(),
        label: "Edit mask".into(),
        operations: vec![],
    };
    store.authorize_v4(V4Intent::PublicApply(&intent))?;
    let source = store.head_v4_source(&input.project_id)?;
    if source.document.project_id() != input.project_id {
        return Err(EditorError::new("STORAGE_ERROR", "Mask head scope differs"));
    }
    if source.revision != input.expected_revision {
        return Err(crate::revision_conflict(
            &input.expected_revision,
            &source.revision,
        ));
    }
    let StoredProjectDocumentV4::V4(project) = source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Mask authoring requires a V4 head",
        ));
    };
    opencut_editor_core::project_v4::validate_project_v4(&project)?;
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
    if visual.profile != VisualProfileV4::ClassicCanvasV31
        || visual.effects.iter().any(|effect| effect.enabled)
    {
        return Err(unsupported(
            "Mask authoring requires Classic geometry without active effects",
        ));
    }
    let enabled: Vec<_> = visual.masks.iter().filter(|mask| mask.enabled).collect();
    if enabled.len() > 1
        || enabled.iter().any(|mask| {
            !matches!(
                mask.shape,
                MaskShapeV4::Rectangle { .. } | MaskShapeV4::Ellipse { .. }
            ) || !matches!(mask.feather, MaskFeatherV4::ClassicPixels { .. })
                || mask.inverted
                || mask.stroke.width_px != 0.0
        })
    {
        return Err(unsupported(
            "Mask authoring supports one enabled Classic rectangle or ellipse without inversion or stroke",
        ));
    }
    let canvas = match &project.settings {
        SettingsProfileV4::ClassicGlobal { settings } => settings.canvas_size,
        SettingsProfileV4::LegacyPerScene { formats } => {
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
    let quad = MaskQuadV1 {
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
    let default_geometry = BoxMaskGeometryV4 {
        center_x: 0.0,
        center_y: 0.0,
        width: side / width.abs(),
        height: side / height.abs(),
        rotation_degrees: 0.0,
        scale: 1.0,
    };
    store.authorize_v4(V4Intent::PublicApply(&intent))?;
    let current = store.head_v4_source(&input.project_id)?;
    if current.document.project_id() != input.project_id {
        return Err(EditorError::new("STORAGE_ERROR", "Mask head scope differs"));
    }
    if current.revision != input.expected_revision {
        return Err(crate::revision_conflict(
            &input.expected_revision,
            &current.revision,
        ));
    }
    Ok(MaskGeometryResultV1 {
        schema_version: 1,
        revision: source.revision,
        element_id: input.element_id.clone(),
        quad,
        default_geometry,
    })
}

#[cfg(test)]
mod tests;
