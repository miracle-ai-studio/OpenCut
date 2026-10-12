use super::*;
mod nodes;
use nodes::{
    active_transitions, audio, audio_exclusion, font_binding, format, media_asset, use_fonts,
    visibility, visual,
};
pub fn compile_frame_v4(
    snapshot: &ProjectSnapshotV4,
    sequence_id: &str,
    at: &Time,
    scope: CompositionScope,
) -> Result<CompositionV4, CompositionError> {
    if snapshot.revision.trim().is_empty() {
        return Err(invalid("V4 compilation requires a fixed revision identity"));
    }
    validate_project_v4(&snapshot.project).map_err(error)?;
    let at_time = time(at)?;
    if at_time.ticks() < 0 {
        return Err(invalid("Presentation time must be nonnegative"));
    }
    let project = &snapshot.project;
    let sequence = project
        .sequences
        .iter()
        .find(|s| s.id == sequence_id)
        .ok_or_else(|| invalid("V4 sequence does not exist"))?;
    let assets: HashMap<_, _> = project.assets.iter().map(|a| (a.id.as_str(), a)).collect();
    let (format, background) = format(project, sequence);
    let mut coverage = vec![];
    coverage::emit(
        &mut coverage,
        &project.id,
        SourceNodeKindV4::Project,
        &project.id,
    );
    coverage::emit(
        &mut coverage,
        &project.id,
        SourceNodeKindV4::Settings,
        &sequence.id,
    );
    coverage::emit(
        &mut coverage,
        &sequence.id,
        SourceNodeKindV4::Sequence,
        &sequence.id,
    );
    for other in &project.sequences {
        if other.id != sequence.id {
            coverage::sequence_excluded(&mut coverage, other);
        }
    }
    for bookmark in &sequence.bookmarks {
        coverage::exclude(
            &mut coverage,
            &bookmark.id,
            SourceNodeKindV4::Bookmark,
            ExclusionV4::EditorialOnly,
        );
    }
    let transitions = active_transitions(sequence, at_time, &mut coverage)?;
    let mut output = CompositionV4 {
        revision: snapshot.revision.clone(),
        project_id: project.id.clone(),
        sequence_id: sequence.id.clone(),
        scope,
        time: at.clone(),
        format,
        tracks: vec![],
        layers: vec![],
        audio: vec![],
        transitions,
        background: CompiledBackgroundV4::Color {
            color: ColorV4 {
                encoding: ColorEncodingV4::Srgb,
                red: 0.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0,
            },
        },
        resources: vec![],
        fonts: vec![],
        coverage: vec![],
    };
    let mut used_assets = HashSet::new();
    let mut used_fonts = HashSet::new();
    for (track_order, track) in sequence.tracks.iter().enumerate() {
        let excluded = if !track.enabled {
            Some(ExclusionV4::DisabledTrack)
        } else if scope == CompositionScope::Visual && track.kind == TrackKindV4::Audio {
            Some(ExclusionV4::VisualScope)
        } else {
            None
        };
        if let Some(reason) = excluded {
            coverage::exclude(&mut coverage, &track.id, SourceNodeKindV4::Track, reason);
            for channel in &track.channels {
                coverage::channel(&mut coverage, channel, None, reason);
            }
            for e in &track.elements {
                coverage::element_excluded(&mut coverage, e, reason);
            }
            continue;
        }
        let mut track_audio_exclusion = Some(if scope == CompositionScope::Visual {
            ExclusionV4::VisualScope
        } else {
            ExclusionV4::OutsideFrame
        });
        for element in &track.elements {
            if !element.enabled || !contains(&visibility(element, &output.transitions)?.0, at_time)?
            {
                continue;
            }
            let reason = audio_exclusion(element, track, media_asset(element, &assets), scope);
            if reason.is_none() {
                track_audio_exclusion = None;
                break;
            }
            track_audio_exclusion = reason;
        }
        let (opacity, gain) =
            animation::track(track, at, scope, track_audio_exclusion, &mut coverage)?;
        let sampled_track = CompiledTrackV4 {
            id: track.id.clone(),
            order: track_order,
            kind: track.kind,
            opacity,
            gain,
            muted: track.muted,
        };
        coverage::emit(&mut coverage, &track.id, SourceNodeKindV4::Track, &track.id);
        for (element_order, e) in track.elements.iter().enumerate() {
            if !e.enabled {
                coverage::element_excluded(&mut coverage, e, ExclusionV4::DisabledElement);
                continue;
            }
            let (extended, head, tail) = visibility(e, &output.transitions)?;
            if !contains(&extended, at_time)? {
                coverage::element_excluded(&mut coverage, e, ExclusionV4::OutsideFrame);
                continue;
            }
            if let ElementContentV4::EffectLayer { effect } = &e.content
                && !effect.enabled
            {
                coverage::element_excluded(&mut coverage, e, ExclusionV4::DisabledEffect);
                continue;
            }
            let audio_exclusion = audio_exclusion(e, track, media_asset(e, &assets), scope);
            if matches!(e.content, ElementContentV4::Audio { .. })
                && let Some(reason) = audio_exclusion
            {
                coverage::element_excluded(&mut coverage, e, reason);
                continue;
            }
            let local = element_local_time_v4(at, &e.timeline_range.start).map_err(error)?;
            let sampled = animation::element(e, &local, scope, audio_exclusion, &mut coverage)?;
            let asset = media_asset(&sampled, &assets);
            if let Some(asset) = asset {
                used_assets.insert(asset.id.clone());
            }
            use_fonts(&sampled, &mut used_fonts);
            let visual = visual(&sampled, opacity, &mut coverage)?;
            let content = match &sampled.content {
                ElementContentV4::Video {
                    asset_id,
                    video_stream,
                    ..
                } => {
                    let asset = asset.ok_or_else(|| invalid("Video CAS binding missing"))?;
                    let source = source::visual_clock(&sampled, asset, &local, head, tail)?;
                    let audio_source = source::clock(&sampled, asset, &local, head, tail)?;
                    let decision = audio(
                        &sampled,
                        &sampled_track,
                        asset,
                        &audio_source,
                        scope,
                        &mut output.audio,
                    )?;
                    SampledContentV4::Video {
                        asset_id: asset_id.clone(),
                        stream: *video_stream,
                        source,
                        source_audio: decision,
                    }
                }
                ElementContentV4::Audio { .. } => {
                    let asset = asset.ok_or_else(|| invalid("Audio CAS binding missing"))?;
                    let source = source::clock(&sampled, asset, &local, head, tail)?;
                    let decision = audio(
                        &sampled,
                        &sampled_track,
                        asset,
                        &source,
                        scope,
                        &mut output.audio,
                    )?;
                    if !matches!(
                        decision,
                        SourceAudioDecisionV4::Emitted | SourceAudioDecisionV4::RequiresProbe
                    ) {
                        return Err(invalid(
                            "Excluded audio element unexpectedly reached execution emission",
                        ));
                    }
                    coverage::emit(&mut coverage, &e.id, SourceNodeKindV4::Element, &e.id);
                    continue;
                }
                ElementContentV4::Image { asset_id } => SampledContentV4::Image {
                    asset_id: asset_id.clone(),
                },
                ElementContentV4::Sticker {
                    asset_id,
                    intrinsic_size,
                } => SampledContentV4::Sticker {
                    asset_id: asset_id.clone(),
                    intrinsic_size: *intrinsic_size,
                },
                ElementContentV4::Text { text } => SampledContentV4::Text { text: text.clone() },
                ElementContentV4::Graphic { graphic } => SampledContentV4::Graphic {
                    graphic: graphic.clone(),
                },
                ElementContentV4::EffectLayer { effect } => {
                    coverage::effect(
                        &mut coverage,
                        effect,
                        Some(&e.id),
                        ExclusionV4::DisabledEffect,
                    );
                    SampledContentV4::EffectLayer {
                        effect: effect.clone(),
                    }
                }
            };
            output.layers.push(CompiledLayerV4 {
                element_id: e.id.clone(),
                track_id: track.id.clone(),
                track_order,
                element_order,
                local_time: local,
                visual,
                content,
            });
            coverage::emit(&mut coverage, &e.id, SourceNodeKindV4::Element, &e.id);
        }
        output.tracks.push(sampled_track);
    }
    for (index, caption) in sequence.captions.iter().enumerate() {
        let track = sequence
            .tracks
            .iter()
            .find(|t| t.id == caption.track_id)
            .expect("validated caption ownership");
        let reason = if !track.enabled {
            Some(ExclusionV4::DisabledTrack)
        } else if !contains(&caption.timeline_range, at_time)? {
            Some(ExclusionV4::OutsideFrame)
        } else {
            None
        };
        if let Some(reason) = reason {
            coverage::exclude(
                &mut coverage,
                &caption.id,
                SourceNodeKindV4::Caption,
                reason,
            );
            continue;
        }
        let state = output
            .tracks
            .iter()
            .find(|t| t.id == track.id)
            .ok_or_else(|| invalid("Caption track state missing"))?;
        font_binding(&caption.style.font, &mut used_fonts);
        let mut style = caption.style.clone();
        style.font_size = match style.font_size {
            FontSizeV4::ClassicHeight90 { value } => FontSizeV4::ClassicHeight90 {
                value: property(ScalarParameterV4::FontSizeClassicHeight90, value)?,
            },
            FontSizeV4::CanvasPixels { value } => FontSizeV4::CanvasPixels {
                value: property(ScalarParameterV4::FontSizeCanvasPixels, value)?,
            },
        };
        output.layers.push(CompiledLayerV4 {
            element_id: caption.id.clone(),
            track_id: track.id.clone(),
            track_order: state.order,
            element_order: index,
            local_time: element_local_time_v4(at, &caption.timeline_range.start).map_err(error)?,
            visual: Some(SampledVisualV4 {
                profile: if style.layout == CaptionLayoutProfileV4::LegacyCaptionV2 {
                    VisualProfileV4::LegacyCompositionV3
                } else {
                    VisualProfileV4::ClassicCanvasV31
                },
                transform: TransformV4 {
                    position_x_px: 0.0,
                    position_y_px: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation_degrees: 0.0,
                },
                element_opacity: 1.0,
                track_opacity: state.opacity,
                effective_opacity: state.opacity,
                opacity_policy: LayerOpacityPolicyV4::MultiplyTrack,
                blend_mode: BlendModeV4::Normal,
                effects: vec![],
                masks: vec![],
            }),
            content: SampledContentV4::Caption {
                text: caption.text.clone(),
                style,
            },
        });
        coverage::emit(
            &mut coverage,
            &caption.id,
            SourceNodeKindV4::Caption,
            &caption.id,
        );
    }
    output
        .layers
        .sort_by_key(|layer| (layer.track_order, layer.element_order));
    output.background = match background {
        BackgroundV4::Color { color } => CompiledBackgroundV4::Color { color },
        BackgroundV4::MainTrackBlur { intensity } => CompiledBackgroundV4::MainTrackBlur {
            intensity: property(ScalarParameterV4::ClassicBlurIntensity, intensity)?,
            main_track_id: sequence.main_track_id.clone(),
            source_layers: output
                .layers
                .iter()
                .filter(|l| {
                    l.track_id == sequence.main_track_id
                        && (matches!(
                            l.content,
                            SampledContentV4::Video { .. } | SampledContentV4::Image { .. }
                        ) || (matches!(l.content, SampledContentV4::Sticker { .. })
                            && l.visual.as_ref().is_some_and(|v| {
                                v.profile == VisualProfileV4::LegacyCompositionV3
                            })))
                })
                .map(|l| l.element_id.clone())
                .collect(),
        },
    };
    for asset in &project.assets {
        if used_assets.contains(&asset.id) {
            output.resources.push(CasResourceV4 {
                asset_id: asset.id.clone(),
                version: asset.version.clone(),
                kind: asset.kind.clone(),
                duration: asset.duration.clone(),
                metadata: asset.metadata.clone(),
                license: asset.license.clone(),
            });
            coverage::emit(&mut coverage, &asset.id, SourceNodeKindV4::Asset, &asset.id);
        } else {
            coverage::exclude(
                &mut coverage,
                &asset.id,
                SourceNodeKindV4::Asset,
                ExclusionV4::UnusedResource,
            );
        }
    }
    for font in &project.fonts {
        if used_fonts.contains(&font.id) {
            output.fonts.push(font.clone());
            coverage::emit(&mut coverage, &font.id, SourceNodeKindV4::Font, &font.id);
        } else {
            coverage::exclude(
                &mut coverage,
                &font.id,
                SourceNodeKindV4::Font,
                ExclusionV4::UnusedResource,
            );
        }
    }
    output.coverage = coverage;
    Ok(output)
}
