//! Pure validation for the unpublished revision-owned V4 aggregate.
//! Structural validity is deliberately separate from renderer capabilities.
use opencut_contracts::{
    AssetKind, EditorError, FontResolutionPolicy, FrameRate, Time, TimeRange, scene::*,
};
use opencut_time::{RationalTime, range_end, validate_range};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

mod channels;
mod content;
mod evaluation;
mod font_synthesis;
pub mod legacy;
mod parameters;
mod resources;
mod text;
mod timing;
mod transform_animation;
mod visual;
use content::{element, transition};
pub use evaluation::*;
pub use font_synthesis::{is_font_synthesis_resource, validate_font_synthesis_resource};
pub use parameters::*;
pub use resources::gain_amplitude;
use resources::{background, origin};
use text::{caption_validate, font_request, font_size, text};
use timing::{checked_sum, element_source};
pub use transform_animation::*;
use visual::{effect, graphic, visual};
#[cfg(test)]
mod tests;

fn invalid(message: impl Into<String>) -> EditorError {
    EditorError::new("INVALID_PROJECT", message)
}

/// Map a nonnegative duration using the declared V4 rate and rounding profile.
/// Source-pinned inputs use inverse=true; timeline-pinned inputs use false.
pub fn map_retime_duration_scene(
    value: RationalTime,
    retime: &Retime,
    inverse: bool,
) -> Result<RationalTime, EditorError> {
    if value.ticks() < 0 {
        return Err(invalid("Retime duration must be nonnegative"));
    }
    timing::mapped_duration(value, retime, inverse)
}

/// Resolve a source request after aggregate validation. This is a quantized
/// request span, not a decoder read window; consumers must bound reads and
/// handle EOF independently. The resource must match the content reference.
pub fn resolve_element_source_request_scene(
    value: &TimelineElement,
    asset: Option<&ProjectAsset>,
) -> Result<TimeRange, EditorError> {
    range(&value.timeline_range)?;
    let (asset_id, retime) = match &value.content {
        ElementContent::Video {
            asset_id, retime, ..
        }
        | ElementContent::Audio {
            asset_id, retime, ..
        } => (Some(asset_id), Some(retime)),
        ElementContent::Image { asset_id } | ElementContent::Sticker { asset_id, .. } => {
            (Some(asset_id), None)
        }
        _ => (None, None),
    };
    if asset_id.map(String::as_str) != asset.map(|a| a.id.as_str()) {
        return Err(invalid(
            "Source request resource differs from element content",
        ));
    }
    element_source(value, retime, asset)
}
fn required(value: &str, label: &str) -> Result<(), EditorError> {
    if value.trim().is_empty() {
        return Err(invalid(format!("{label} must be nonempty")));
    }
    Ok(())
}
fn unique(ids: &mut HashSet<String>, id: &str) -> Result<(), EditorError> {
    if id.trim().is_empty() || id.len() > 128 || !ids.insert(id.to_owned()) {
        return Err(invalid(format!("Invalid or duplicate V4 identity: {id}")));
    }
    Ok(())
}
fn finite(value: f64, label: &str) -> Result<(), EditorError> {
    if !value.is_finite() {
        return Err(invalid(format!("{label} must be finite")));
    }
    Ok(())
}
fn bound(value: f64, low: f64, high: f64, label: &str) -> Result<(), EditorError> {
    if !value.is_finite() || value < low || value > high {
        return Err(invalid(format!("{label} is outside its semantic range")));
    }
    Ok(())
}
fn positive(value: f64, label: &str) -> Result<(), EditorError> {
    finite(value, label)?;
    if value <= 0.0 {
        return Err(invalid(format!("{label} must be positive")));
    }
    Ok(())
}
fn nonnegative(value: f64, label: &str) -> Result<(), EditorError> {
    finite(value, label)?;
    if value < 0.0 {
        return Err(invalid(format!("{label} must be nonnegative")));
    }
    Ok(())
}
fn time(value: &Time) -> Result<RationalTime, EditorError> {
    RationalTime::try_from(value).map_err(|e| invalid(e.message))
}
fn duration(value: &Time) -> Result<RationalTime, EditorError> {
    let value = time(value)?;
    if value.ticks() <= 0 {
        return Err(invalid("Duration must be positive"));
    }
    Ok(value)
}
fn range(value: &TimeRange) -> Result<RationalTime, EditorError> {
    validate_range(value, true).map_err(|e| invalid(e.message))?;
    if time(&value.start)?.ticks() < 0 {
        return Err(invalid("Range start must be nonnegative"));
    }
    time(&range_end(value).map_err(|e| invalid(e.message))?)
}
fn fps(value: &FrameRate) -> Result<(), EditorError> {
    if value.numerator == 0 || value.denominator == 0 {
        return Err(invalid("FPS must be positive"));
    }
    Ok(())
}
fn canvas(value: &CanvasSize) -> Result<(), EditorError> {
    if value.width == 0 || value.height == 0 {
        return Err(invalid("Canvas dimensions must be positive"));
    }
    Ok(())
}
fn color(value: &Color) -> Result<(), EditorError> {
    for component in [value.red, value.green, value.blue, value.alpha] {
        bound(component, 0.0, 1.0, "Color component")?;
    }
    Ok(())
}
/// Validate content, ownership and exact semantic relationships only. This does
/// not resolve font bytes or assert that any declared effect is renderable.
pub fn validate_project_scene(project: &EditorDocument) -> Result<(), EditorError> {
    validated_identities(project).map(|_| ())
}
/// The existing validator already collects the complete domain namespace.
/// Reuse it for fragment allocation;CAS version IDs are separate identities.
pub(crate) fn validated_identities(
    project: &EditorDocument,
) -> Result<HashSet<String>, EditorError> {
    if project.schema_version != 4 {
        return Err(invalid("Expected project schema version 4"));
    }
    required(&project.name, "Project name")?;
    let mut ids = HashSet::new();
    unique(&mut ids, &project.id)?;
    let assets: HashMap<_, _> = project.assets.iter().map(|a| (a.id.as_str(), a)).collect();
    let fonts: HashMap<_, _> = project.fonts.iter().map(|f| (f.id.as_str(), f)).collect();
    for asset in &project.assets {
        unique(&mut ids, &asset.id)?;
        required(&asset.name, "Resource name")?;
        crate::validate_asset_version(&asset.version).map_err(|e| invalid(e.message))?;
        origin(&asset.origin)?;
        if let Some(value) = &asset.duration {
            duration(value)?;
        }
        if let Some(value) = &asset.metadata.intrinsic_size {
            canvas(value)?;
        }
        if let Some(value) = &asset.metadata.frame_rate {
            fps(value)?;
            if asset.kind != AssetKind::Video {
                return Err(invalid("Only video assets declare frame rates"));
            }
        }
        if let Some(value) = asset.metadata.rotation_degrees {
            finite(value, "Resource rotation")?;
        }
        if let Some(mime_type) = &asset.metadata.mime_type {
            required(mime_type, "Resource MIME type")?;
        }
        if let AudioPresence::Present { .. } = &asset.metadata.audio
            && !matches!(asset.kind, AssetKind::Audio | AssetKind::Video)
        {
            return Err(invalid("Audio presence requires an audio-bearing resource"));
        }
        if let AudioPresence::Present {
            metadata: Some(value),
        } = &asset.metadata.audio
            && (value.channels == 0 || value.sample_rate == 0)
        {
            return Err(invalid(
                "Audio metadata requires an audio-bearing resource and positive format",
            ));
        }
    }
    for font in &project.fonts {
        unique(&mut ids, &font.id)?;
        required(&font.family, "Font family")?;
        if !(1..=1000).contains(&font.weight) || font.content.size == 0 {
            return Err(invalid("Font weight and content size are invalid"));
        }
        crate::validate_asset_version(&font.content).map_err(|e| invalid(e.message))?;
        origin(&font.origin)?;
        validate_font_synthesis_resource(font)?;
    }
    let sequence_ids: HashSet<_> = project.sequences.iter().map(|s| s.id.as_str()).collect();
    if !sequence_ids.contains(project.default_sequence_id.as_str()) {
        return Err(invalid("Default sequence reference is missing"));
    }
    if project
        .sequences
        .iter()
        .map(|s| s.bookmarks.len())
        .sum::<usize>()
        > crate::scene_bookmarks::MAX_BOOKMARKS
    {
        return Err(invalid("Project exceeds the bookmark limit"));
    }
    match &project.settings {
        SettingsProfile::ClassicGlobal { settings } => {
            fps(&settings.fps)?;
            canvas(&settings.canvas_size)?;
            background(&settings.background)?;
            for size in [
                &settings.last_custom_canvas_size,
                &settings.original_canvas_size,
            ]
            .into_iter()
            .flatten()
            {
                canvas(size)?;
            }
            // Remembered custom size is user intent. Classic first visual media
            // may replace current canvas without replacing this remembered size.
        }
        SettingsProfile::LegacyPerScene { formats } => {
            let mut seen = HashSet::new();
            for format in formats {
                if !sequence_ids.contains(format.sequence_id.as_str())
                    || !seen.insert(format.sequence_id.as_str())
                {
                    return Err(invalid(
                        "Legacy formats require distinct existing sequence references",
                    ));
                }
                fps(&format.fps)?;
                canvas(&format.canvas_size)?;
                background(&format.background)?;
            }
            if seen.len() != sequence_ids.len() {
                return Err(invalid(
                    "Legacy formats must cover every sequence exactly once",
                ));
            }
        }
    }
    for sequence in &project.sequences {
        unique(&mut ids, &sequence.id)?;
        if !sequence
            .tracks
            .iter()
            .any(|t| t.id == sequence.main_track_id && t.kind == SceneTrackKind::Video)
        {
            return Err(invalid(
                "Sequence main track must reference its video track",
            ));
        }
        let mut end = RationalTime::new(0, 1)?;
        let mut elements = HashMap::new();
        for track in &sequence.tracks {
            unique(&mut ids, &track.id)?;
            bound(track.opacity, 0.0, 1.0, "Track opacity")?;
            gain_amplitude(&track.gain)?;
            let mut intervals = Vec::new();
            for value in &track.elements {
                element(value, track, &assets, &fonts, &mut ids)?;
                let finish = range(&value.timeline_range)?;
                if finish.cmp_exact(end) == Ordering::Greater {
                    end = finish;
                }
                intervals.push((time(&value.timeline_range.start)?, finish));
                elements.insert(value.id.as_str(), (track, value));
            }
            intervals.sort_by(|a, b| a.0.cmp_exact(b.0));
            let legacy_graphic_layers = track.kind == SceneTrackKind::Graphic
                && track.elements.iter().all(|element| {
                    element
                        .visual
                        .as_ref()
                        .is_some_and(|visual| visual.profile == VisualProfile::LegacyCompositionV3)
                });
            if !legacy_graphic_layers
                && intervals
                    .windows(2)
                    .any(|w| w[0].1.cmp_exact(w[1].0) == Ordering::Greater)
            {
                return Err(invalid("Elements on one track cannot overlap"));
            }
        }
        for caption in &sequence.captions {
            if !sequence
                .tracks
                .iter()
                .any(|track| track.id == caption.track_id && track.kind == SceneTrackKind::Caption)
            {
                return Err(invalid(
                    "Caption must reference its owning caption track in this sequence",
                ));
            }
            caption_validate(caption, &assets, &fonts, &mut ids)?;
            let finish = range(&caption.timeline_range)?;
            if finish.cmp_exact(end) == Ordering::Greater {
                end = finish;
            }
        }
        for track in &sequence.tracks {
            channels::track_channels(track, end, &mut ids)?;
        }
        for value in &sequence.transitions {
            transition(value, &elements, &assets, &mut ids)?;
        }
        for bookmark in &sequence.bookmarks {
            unique(&mut ids, &bookmark.id)?;
            if bookmark.sequence_id != sequence.id || time(&bookmark.time)?.ticks() < 0 {
                return Err(invalid(
                    "Bookmark must have nonnegative time in its owning sequence",
                ));
            }
            if let Some(value) = &bookmark.duration {
                let value = time(value)?;
                if value.ticks() < 0 {
                    return Err(invalid("Bookmark duration cannot be negative"));
                }
                time(&bookmark.time)?
                    .checked_add(value)
                    .map_err(|e| invalid(e.message))?;
            }
            if bookmark
                .note
                .as_ref()
                .is_some_and(|n| n.len() > crate::scene_bookmarks::MAX_NOTE_BYTES)
                || bookmark.color.as_ref().is_some_and(|c| {
                    c.len() != 7
                        || !c.starts_with('#')
                        || !c.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
                })
            {
                return Err(invalid("Bookmark note or color is invalid"));
            }
        }
    }
    Ok(ids)
}
