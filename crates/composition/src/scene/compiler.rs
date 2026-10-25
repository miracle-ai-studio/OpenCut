use super::*;
mod nodes;
pub use nodes::resolve_scene_format;
use nodes::{
    active_transitions, audio, audio_exclusion, font_binding, media_asset, use_fonts, visibility,
    visual,
};
pub fn compile_frame_scene(
    snapshot: &EditorSnapshot,
    sequence_id: &str,
    at: &Time,
    scope: CompositionScope,
) -> Result<SceneComposition, CompositionError> {
    if snapshot.revision.trim().is_empty() {
        return Err(invalid("V4 compilation requires a fixed revision identity"));
    }
    validate_project_scene(&snapshot.project).map_err(error)?;
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
    let (format, background) = resolve_scene_format(project, sequence)?;
    let mut coverage = vec![];
    coverage::emit(
        &mut coverage,
        &project.id,
        SourceNodeKind::Project,
        &project.id,
    );
    coverage::emit(
        &mut coverage,
        &project.id,
        SourceNodeKind::Settings,
        &sequence.id,
    );
    coverage::emit(
        &mut coverage,
        &sequence.id,
        SourceNodeKind::Sequence,
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
            SourceNodeKind::Bookmark,
            Exclusion::EditorialOnly,
        );
    }
    let transitions = active_transitions(sequence, at_time, &mut coverage)?;
    let mut output = SceneComposition {
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
        background: CompiledBackground::Color {
            color: Color {
                encoding: ColorEncoding::Srgb,
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
            Some(Exclusion::DisabledTrack)
        } else if scope == CompositionScope::Visual && track.kind == SceneTrackKind::Audio {
            Some(Exclusion::VisualScope)
        } else {
            None
        };
        if let Some(reason) = excluded {
            coverage::exclude(&mut coverage, &track.id, SourceNodeKind::Track, reason);
            for channel in &track.channels {
                coverage::channel(&mut coverage, channel, None, reason);
            }
            for e in &track.elements {
                coverage::element_excluded(&mut coverage, e, reason);
            }
            continue;
        }
        let mut track_audio_exclusion = Some(if scope == CompositionScope::Visual {
            Exclusion::VisualScope
        } else {
            Exclusion::OutsideFrame
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
        let sampled_track = CompiledTrack {
            id: track.id.clone(),
            order: track_order,
            kind: track.kind,
            opacity,
            gain,
            muted: track.muted,
        };
        coverage::emit(&mut coverage, &track.id, SourceNodeKind::Track, &track.id);
        for (element_order, e) in track.elements.iter().enumerate() {
            if !e.enabled {
                coverage::element_excluded(&mut coverage, e, Exclusion::DisabledElement);
                continue;
            }
            let (extended, head, tail) = visibility(e, &output.transitions)?;
            if !contains(&extended, at_time)? {
                coverage::element_excluded(&mut coverage, e, Exclusion::OutsideFrame);
                continue;
            }
            if let ElementContent::EffectLayer { effect } = &e.content
                && !effect.enabled
            {
                coverage::element_excluded(&mut coverage, e, Exclusion::DisabledEffect);
                continue;
            }
            let audio_exclusion = audio_exclusion(e, track, media_asset(e, &assets), scope);
            if matches!(e.content, ElementContent::Audio { .. })
                && let Some(reason) = audio_exclusion
            {
                coverage::element_excluded(&mut coverage, e, reason);
                continue;
            }
            let local = element_local_time_scene(at, &e.timeline_range.start).map_err(error)?;
            let sampled = animation::element(e, &local, scope, audio_exclusion, &mut coverage)?;
            let asset = media_asset(&sampled, &assets);
            if let Some(asset) = asset {
                used_assets.insert(asset.id.clone());
            }
            use_fonts(&sampled, &mut used_fonts);
            let visual = visual(&sampled, opacity, &mut coverage)?;
            let content = match &sampled.content {
                ElementContent::Video {
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
                    SampledContent::Video {
                        asset_id: asset_id.clone(),
                        stream: *video_stream,
                        source,
                        source_audio: decision,
                    }
                }
                ElementContent::Audio { .. } => {
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
                        SourceAudioDecision::Emitted | SourceAudioDecision::RequiresProbe
                    ) {
                        return Err(invalid(
                            "Excluded audio element unexpectedly reached execution emission",
                        ));
                    }
                    coverage::emit(&mut coverage, &e.id, SourceNodeKind::Element, &e.id);
                    continue;
                }
                ElementContent::Image { asset_id } => SampledContent::Image {
                    asset_id: asset_id.clone(),
                },
                ElementContent::Sticker {
                    asset_id,
                    intrinsic_size,
                } => SampledContent::Sticker {
                    asset_id: asset_id.clone(),
                    intrinsic_size: *intrinsic_size,
                },
                ElementContent::Text { text } => SampledContent::Text { text: text.clone() },
                ElementContent::Graphic { graphic } => SampledContent::Graphic {
                    graphic: graphic.clone(),
                },
                ElementContent::EffectLayer { effect } => {
                    coverage::effect(
                        &mut coverage,
                        effect,
                        Some(&e.id),
                        Exclusion::DisabledEffect,
                    );
                    SampledContent::EffectLayer {
                        effect: effect.clone(),
                    }
                }
            };
            output.layers.push(CompiledLayer {
                element_id: e.id.clone(),
                track_id: track.id.clone(),
                track_order,
                element_order,
                local_time: local,
                visual,
                content,
            });
            coverage::emit(&mut coverage, &e.id, SourceNodeKind::Element, &e.id);
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
            Some(Exclusion::DisabledTrack)
        } else if !contains(&caption.timeline_range, at_time)? {
            Some(Exclusion::OutsideFrame)
        } else {
            None
        };
        if let Some(reason) = reason {
            coverage::exclude(&mut coverage, &caption.id, SourceNodeKind::Caption, reason);
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
            FontSize::ClassicHeight90 { value } => FontSize::ClassicHeight90 {
                value: property(ScalarParameter::FontSizeClassicHeight90, value)?,
            },
            FontSize::CanvasPixels { value } => FontSize::CanvasPixels {
                value: property(ScalarParameter::FontSizeCanvasPixels, value)?,
            },
        };
        output.layers.push(CompiledLayer {
            element_id: caption.id.clone(),
            track_id: track.id.clone(),
            track_order: state.order,
            element_order: index,
            local_time: element_local_time_scene(at, &caption.timeline_range.start)
                .map_err(error)?,
            visual: Some(SampledVisual {
                profile: if style.layout == CaptionLayoutProfile::LegacyCaptionV2 {
                    VisualProfile::LegacyCompositionV3
                } else {
                    VisualProfile::ClassicCanvasV31
                },
                transform: Transform {
                    position_x_px: 0.0,
                    position_y_px: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation_degrees: 0.0,
                },
                element_opacity: 1.0,
                track_opacity: state.opacity,
                effective_opacity: state.opacity,
                opacity_policy: LayerOpacityPolicy::MultiplyTrack,
                blend_mode: BlendMode::Normal,
                effects: vec![],
                masks: vec![],
            }),
            content: SampledContent::Caption {
                text: caption.text.clone(),
                style,
            },
        });
        coverage::emit(
            &mut coverage,
            &caption.id,
            SourceNodeKind::Caption,
            &caption.id,
        );
    }
    output
        .layers
        .sort_by_key(|layer| (layer.track_order, layer.element_order));
    output.background = match background {
        Background::Color { color } => CompiledBackground::Color { color },
        Background::MainTrackBlur { intensity } => CompiledBackground::MainTrackBlur {
            intensity: property(ScalarParameter::ClassicBlurIntensity, intensity)?,
            main_track_id: sequence.main_track_id.clone(),
            source_layers: output
                .layers
                .iter()
                .filter(|l| {
                    l.track_id == sequence.main_track_id
                        && (matches!(
                            l.content,
                            SampledContent::Video { .. } | SampledContent::Image { .. }
                        ) || (matches!(l.content, SampledContent::Sticker { .. })
                            && l.visual
                                .as_ref()
                                .is_some_and(|v| v.profile == VisualProfile::LegacyCompositionV3)))
                })
                .map(|l| l.element_id.clone())
                .collect(),
        },
    };
    for asset in &project.assets {
        if used_assets.contains(&asset.id) {
            output.resources.push(CasResource {
                asset_id: asset.id.clone(),
                version: asset.version.clone(),
                kind: asset.kind.clone(),
                duration: asset.duration.clone(),
                metadata: asset.metadata.clone(),
                license: asset.license.clone(),
            });
            coverage::emit(&mut coverage, &asset.id, SourceNodeKind::Asset, &asset.id);
        } else {
            coverage::exclude(
                &mut coverage,
                &asset.id,
                SourceNodeKind::Asset,
                Exclusion::UnusedResource,
            );
        }
    }
    for font in &project.fonts {
        if used_fonts.contains(&font.id) {
            output.fonts.push(font.clone());
            coverage::emit(&mut coverage, &font.id, SourceNodeKind::Font, &font.id);
        } else {
            coverage::exclude(
                &mut coverage,
                &font.id,
                SourceNodeKind::Font,
                Exclusion::UnusedResource,
            );
        }
    }
    output.coverage = coverage;
    Ok(output)
}
