use super::*;
use opencut_media_contracts::ClassicPacketRateV1;

fn greater(a: &FrameRate, b: &FrameRate) -> bool {
    u64::from(a.numerator) * u64::from(b.denominator)
        > u64::from(b.numerator) * u64::from(a.denominator)
}

enum ObservedFps {
    Header(FrameRate),
    Packet(f64),
}
impl ObservedFps {
    fn value(&self) -> f64 {
        match self {
            Self::Header(rate) => f64::from(rate.numerator) / f64::from(rate.denominator),
            Self::Packet(rate) => *rate,
        }
    }
    fn greater(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Header(a), Self::Header(b)) => greater(a, b),
            _ => self.value() > other.value(),
        }
    }
    fn normalize(&self) -> Result<FrameRate, EditorError> {
        match self {
            Self::Header(rate) => crate::media_import::normalize_import_fps(rate),
            Self::Packet(rate) => crate::media_import::normalize_classic_packet_fps(*rate),
        }
    }
}
fn observed(
    asset: &AssetV4,
    packets: &BTreeMap<String, ClassicPacketRateV1>,
) -> Option<ObservedFps> {
    match packets.get(&asset.id) {
        Some(packet) => packet.average_packet_rate.map(ObservedFps::Packet),
        None => asset.metadata.frame_rate.clone().map(ObservedFps::Header),
    }
}

pub(super) fn decide(
    project: &ProjectDocumentV4,
    sequence: &SequenceV4,
    input: &MediaImportRequestV4,
    assets: &BTreeMap<String, AssetV4>,
    placement_assets: &BTreeMap<String, AssetV4>,
    packet_rates: &BTreeMap<String, ClassicPacketRateV1>,
) -> Result<(SettingsProfileV4, MediaImportDecisionsV4), EditorError> {
    let mut settings = project.settings.clone();
    let (scope, previous) = match &settings {
        SettingsProfileV4::ClassicGlobal { settings } => {
            (MediaSettingsScopeV4::ClassicGlobal, settings.fps.clone())
        }
        SettingsProfileV4::LegacyPerScene { formats } => (
            MediaSettingsScopeV4::LegacyPerScene {
                sequence_id: sequence.id.clone(),
            },
            formats
                .iter()
                .find(|f| f.sequence_id == sequence.id)
                .ok_or_else(|| invalid("Legacy sequence format is missing"))?
                .fps
                .clone(),
        ),
    };
    let mut highest: Option<ObservedFps> = None;
    for asset in assets.values().filter(|a| a.kind == AssetKind::Video) {
        if let Some(rate) = observed(asset, packet_rates) {
            if !rate.value().is_finite() || rate.value() <= 0.0 {
                return Err(invalid("Observed media FPS must be positive"));
            }
            if highest.as_ref().is_none_or(|current| rate.greater(current)) {
                highest = Some(rate);
            }
        }
    }
    // Reusing an existing asset is not an import-FPS decision, but an actual
    // placement still needs the same missing-metadata warning. Do not warn for
    // unrelated revision-owned assets or duplicate references to one video.
    let missing_fps = assets
        .values()
        .chain(
            input
                .placements
                .iter()
                .filter_map(|placement| placement_assets.get(&placement.asset_id)),
        )
        .filter(|asset| asset.kind == AssetKind::Video && observed(asset, packet_rates).is_none())
        .map(|asset| asset.id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut raised = None;
    if let Some(highest) =
        highest.filter(|highest| highest.greater(&ObservedFps::Header(previous.clone())))
    {
        let normalized = highest.normalize()?;
        if matches!(settings, SettingsProfileV4::ClassicGlobal { .. })
            || greater(&normalized, &previous)
        {
            match &mut settings {
                SettingsProfileV4::ClassicGlobal { settings } => settings.fps = normalized.clone(),
                SettingsProfileV4::LegacyPerScene { formats } => {
                    formats
                        .iter_mut()
                        .find(|f| f.sequence_id == sequence.id)
                        .expect("validated format")
                        .fps = normalized.clone()
                }
            }
            raised = Some(normalized);
        }
    }
    let mut first_visual_fps = None;
    let mut canvas_change = None;
    // Classic tests the target scene immediately before each insertion. An
    // audio-first batch already has an element when the following visual lands.
    if sequence.tracks.iter().all(|t| t.elements.is_empty())
        && sequence.captions.is_empty()
        && let Some(first) = input.placements.first()
        && matches!(
            first.content,
            MediaPlacementContentV4::Video { .. } | MediaPlacementContentV4::Image { .. }
        )
        && let Some(asset) = placement_assets.get(&first.asset_id)
        && let SettingsProfileV4::ClassicGlobal { settings } = &mut settings
    {
        if let Some(size) = asset.metadata.intrinsic_size {
            if size.width == 0 || size.height == 0 {
                return Err(invalid("Native media dimensions must be positive"));
            }
            canvas_change = Some(MediaCanvasChangeV4 {
                previous: settings.canvas_size,
                next: size,
                source_asset_id: asset.id.clone(),
            });
            settings.canvas_size = size;
            // This field remembers a user's explicit custom choice; media
            // insertion only changes current/original canvas dimensions.
            if settings.original_canvas_size.is_none() {
                settings.original_canvas_size = Some(size);
            }
        }
        // Independent of observed dimensions. The original insert command
        // overrides FPS for its first video after applying placement.
        if matches!(first.content, MediaPlacementContentV4::Video { .. })
            && let Some(fps) = observed(asset, packet_rates)
        {
            let fps = fps.normalize()?;
            settings.fps = fps.clone();
            first_visual_fps = Some(fps);
        }
    }
    Ok((
        settings,
        MediaImportDecisionsV4 {
            settings_scope: scope,
            previous_fps: previous,
            raised_fps: raised,
            first_visual_fps,
            canvas_change,
            placements: vec![],
            bound_asset_ids: vec![],
            replaced_asset_ids: vec![],
            video_asset_ids_without_fps: missing_fps,
        },
    ))
}
