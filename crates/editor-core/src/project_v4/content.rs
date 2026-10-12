//! Pure V4 content rules. No I/O or renderer capability claims.
use super::*;

pub(super) fn element(
    value: &TimelineElementV4,
    track: &TrackV4,
    assets: &HashMap<&str, &AssetV4>,
    fonts: &HashMap<&str, &FontResourceV4>,
    ids: &mut HashSet<String>,
) -> Result<(), EditorError> {
    unique(ids, &value.id)?;
    required(&value.name, "Element name")?;
    range(&value.timeline_range)?;
    let allowed = matches!(
        (&track.kind, &value.content),
        (
            TrackKindV4::Video,
            ElementContentV4::Video { .. }
                | ElementContentV4::Image { .. }
                | ElementContentV4::Sticker { .. }
        ) | (TrackKindV4::Audio, ElementContentV4::Audio { .. })
            | (TrackKindV4::Text, ElementContentV4::Text { .. })
            | (
                TrackKindV4::Graphic,
                ElementContentV4::Graphic { .. }
                    | ElementContentV4::Image { .. }
                    | ElementContentV4::Sticker { .. }
            )
            | (TrackKindV4::Effect, ElementContentV4::EffectLayer { .. })
    );
    if !allowed {
        return Err(invalid("Element content is incompatible with its track"));
    }
    let get_asset = |id: &str| {
        assets
            .get(id)
            .copied()
            .ok_or_else(|| invalid("Element asset reference is missing"))
    };
    match &value.content {
        ElementContentV4::Video {
            asset_id,
            audio,
            retime,
            ..
        } => {
            let asset = get_asset(asset_id)?;
            if asset.kind != AssetKind::Video {
                return Err(invalid("Video element requires a video asset"));
            }
            element_source(value, Some(retime), Some(asset))?;
            gain_amplitude(&audio.gain)?;
        }
        ElementContentV4::Audio {
            asset_id,
            audio,
            retime,
        } => {
            let asset = get_asset(asset_id)?;
            if asset.kind != AssetKind::Audio && asset.kind != AssetKind::Video {
                return Err(invalid("Audio element requires an audio-bearing asset"));
            }
            if matches!(asset.metadata.audio, AudioPresenceV4::Absent {}) {
                return Err(invalid(
                    "Audio element cannot reference a resource with known absent audio",
                ));
            }
            element_source(value, Some(retime), Some(asset))?;
            gain_amplitude(&audio.gain)?;
        }
        ElementContentV4::Image { asset_id } | ElementContentV4::Sticker { asset_id, .. } => {
            let asset = get_asset(asset_id)?;
            if !matches!(asset.kind, AssetKind::Image | AssetKind::Graphic) {
                return Err(invalid("Image or sticker requires a visual image asset"));
            }
            element_source(value, None, Some(asset))?;
            if let ElementContentV4::Sticker { intrinsic_size, .. } = &value.content {
                canvas(intrinsic_size)?;
            }
        }
        ElementContentV4::Text { text: content } => {
            text(content, fonts)?;
            element_source(value, None, None)?;
        }
        ElementContentV4::Graphic { graphic: content } => {
            graphic(content)?;
            element_source(value, None, None)?;
        }
        ElementContentV4::EffectLayer { effect: content } => {
            effect(content, ids)?;
            element_source(value, None, None)?;
        }
    }
    match (&value.content, &value.visual) {
        (ElementContentV4::Audio { .. }, Some(_)) => {
            return Err(invalid("Audio elements cannot own visual style"));
        }
        (ElementContentV4::Audio { .. } | ElementContentV4::EffectLayer { .. }, None) => {}
        (_, Some(style)) => visual(style, fonts, ids)?,
        _ => return Err(invalid("Visual elements require explicit visual style")),
    }
    channels::element_channels(value, ids)
}

pub(super) fn transition(
    value: &TransitionV4,
    elements: &HashMap<&str, (&TrackV4, &TimelineElementV4)>,
    assets: &HashMap<&str, &AssetV4>,
    ids: &mut HashSet<String>,
) -> Result<(), EditorError> {
    unique(ids, &value.id)?;
    if value.outgoing_element_id == value.incoming_element_id {
        return Err(invalid("Transition endpoints must differ"));
    }
    let (out_track, out) = elements
        .get(value.outgoing_element_id.as_str())
        .copied()
        .ok_or_else(|| invalid("Outgoing transition endpoint is missing"))?;
    let (in_track, inc) = elements
        .get(value.incoming_element_id.as_str())
        .copied()
        .ok_or_else(|| invalid("Incoming transition endpoint is missing"))?;
    if out_track.id != in_track.id
        || range(&out.timeline_range)?.cmp_exact(time(&inc.timeline_range.start)?)
            != Ordering::Equal
    {
        return Err(invalid(
            "Legacy transition endpoints must share a track and a cut",
        ));
    }
    let half = duration(&value.duration)?
        .checked_scale(RationalTime::new(1, 1)?, RationalTime::new(2, 1)?)
        .map_err(|e| invalid(e.message))?;
    for (element, outgoing) in [(out, true), (inc, false)] {
        let ElementContentV4::Video {
            asset_id, retime, ..
        } = &element.content
        else {
            return Err(invalid("Legacy crossfade requires two video elements"));
        };
        let asset = assets
            .get(asset_id.as_str())
            .copied()
            .ok_or_else(|| invalid("Transition asset reference is missing"))?;
        let source = element_source(element, Some(retime), Some(asset))?;
        if duration(&source.duration)?.cmp_exact(duration(&element.timeline_range.duration)?)
            != Ordering::Equal
            || duration(&element.timeline_range.duration)?.cmp_exact(half) == Ordering::Less
        {
            return Err(invalid(
                "Legacy crossfade requires one-to-one media and sufficient visible time",
            ));
        }
        let available = if outgoing {
            duration(
                asset
                    .duration
                    .as_ref()
                    .ok_or_else(|| invalid("Transition asset duration is unknown"))?,
            )?
            .checked_sub(range(&source)?)
            .map_err(|e| invalid(e.message))?
        } else {
            time(&source.start)?
        };
        if available.cmp_exact(half) == Ordering::Less {
            return Err(invalid("Legacy crossfade lacks source handles"));
        }
    }
    Ok(())
}
