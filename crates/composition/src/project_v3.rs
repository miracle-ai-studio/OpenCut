//! One pure schema 3 graph for frame, range and export adapters.
use crate::{Composition, CompositionError, CompositionScope};
use opencut_contracts::{ProjectSnapshot, TrackKind, project_v3::*};
use opencut_media_contracts::MediaTime;
use opencut_time::RationalTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompositionV3 {
    pub format: CompositionV3Format,
    /// Shared schema 2 audio, media, caption and canvas compilation.
    pub base: Composition,
    pub visual: CompiledVisualGraph,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CompositionV3Format {
    #[serde(rename = "opencut.composition.v3")]
    V3,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledVisualGraph {
    /// Bindings retain stable clip IDs erased by the base media Timeline.
    pub targets: Vec<VisualTargetBinding>,
    pub shapes: Vec<CompiledShape>,
    pub effects: Vec<EffectNode>,
    pub masks: Vec<MaskNode>,
    pub transitions: Vec<TransitionNode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualTargetBinding {
    pub target_id: String,
    pub kind: VisualTargetKind,
    /// Index in Composition.timeline.video_clips/image_clips or visual.shapes.
    pub index: usize,
    pub z_index: i32,
    /// Base clips use their track index; shapes follow them in document order.
    pub layer_order: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VisualTargetKind {
    VideoClip,
    ImageClip,
    Shape,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledShape {
    pub id: String,
    pub track_id: String,
    pub start: MediaTime,
    pub duration: MediaTime,
    pub primitive: ShapePrimitive,
    pub fill: Rgba,
    pub position_x: f64,
    pub position_y: f64,
    pub rotation_degrees: f64,
    pub opacity: f64,
}

fn invalid(message: impl Into<String>) -> CompositionError {
    CompositionError {
        code: "INVALID_PROJECT",
        message: message.into(),
    }
}
fn media_time(time: &opencut_contracts::Time) -> Result<MediaTime, CompositionError> {
    crate::validation::time(time)
}

pub fn compile_v3(
    snapshot: &ProjectSnapshotV3,
    sequence_id: &str,
    scope: CompositionScope,
) -> Result<CompositionV3, CompositionError> {
    opencut_editor_core::project_v3::validate_project_v3(&snapshot.project)
        .map_err(|e| invalid(e.message))?;
    let sequence = snapshot
        .project
        .base
        .sequences
        .iter()
        .find(|sequence| sequence.id == sequence_id)
        .ok_or_else(|| invalid("sequence does not exist"))?;
    let visual = snapshot
        .project
        .visual
        .iter()
        .find(|visual| visual.sequence_id == sequence_id);
    let mut minimum_end = None;
    if let Some(visual) = visual {
        for shape in &visual.shapes {
            if !sequence
                .tracks
                .iter()
                .any(|track| track.id == shape.track_id && track.enabled)
            {
                continue;
            }
            let end = RationalTime::try_from(
                &opencut_time::range_end(&shape.timeline_range).map_err(|e| invalid(e.message))?,
            )
            .map_err(|e| invalid(e.message))?;
            if minimum_end.is_none_or(|current: RationalTime| current.cmp_exact(end).is_lt()) {
                minimum_end = Some(end);
            }
        }
    }
    let base = crate::compiler::compile_with_min_end_and_curves(
        &ProjectSnapshot {
            project: snapshot.project.base.clone(),
            revision: snapshot.revision.clone(),
        },
        sequence_id,
        scope,
        minimum_end,
        &snapshot.project.keyframe_curves,
    )?;
    let mut graph = CompiledVisualGraph {
        targets: Vec::new(),
        shapes: Vec::new(),
        effects: Vec::new(),
        masks: Vec::new(),
        transitions: Vec::new(),
    };
    let mut indices = HashMap::new();
    let mut video_index = 0;
    let mut image_index = 0;
    for (track_index, track) in sequence.tracks.iter().enumerate() {
        if !track.enabled {
            continue;
        }
        let kind = match track.kind {
            TrackKind::Video => VisualTargetKind::VideoClip,
            TrackKind::Graphic => VisualTargetKind::ImageClip,
            _ => continue,
        };
        for (clip_index, clip) in track.clips.iter().enumerate() {
            let index = match kind {
                VisualTargetKind::VideoClip => {
                    let index = video_index;
                    video_index += 1;
                    index
                }
                VisualTargetKind::ImageClip => {
                    let index = image_index;
                    image_index += 1;
                    index
                }
                VisualTargetKind::Shape => unreachable!(),
            };
            indices.insert(clip.id.as_str(), graph.targets.len());
            graph.targets.push(VisualTargetBinding {
                target_id: clip.id.clone(),
                kind,
                index,
                z_index: i32::try_from(track_index)
                    .map_err(|_| invalid("too many visual tracks"))?,
                layer_order: u32::try_from(clip_index).map_err(|_| invalid("too many clips"))?,
            });
        }
    }
    if let Some(visual) = visual {
        for shape in &visual.shapes {
            let (track_index, track) = sequence
                .tracks
                .iter()
                .enumerate()
                .find(|(_, track)| track.id == shape.track_id)
                .expect("validated shape track");
            if !track.enabled {
                continue;
            }
            indices.insert(shape.id.as_str(), graph.targets.len());
            graph.targets.push(VisualTargetBinding {
                target_id: shape.id.clone(),
                kind: VisualTargetKind::Shape,
                index: graph.shapes.len(),
                z_index: i32::try_from(track_index)
                    .map_err(|_| invalid("too many visual tracks"))?,
                layer_order: u32::try_from(track.clips.len() + graph.shapes.len())
                    .map_err(|_| invalid("too many shapes"))?,
            });
            graph.shapes.push(CompiledShape {
                id: shape.id.clone(),
                track_id: shape.track_id.clone(),
                start: media_time(&shape.timeline_range.start)?,
                duration: media_time(&shape.timeline_range.duration)?,
                primitive: shape.primitive.clone(),
                fill: shape.fill,
                position_x: shape.position_x,
                position_y: shape.position_y,
                rotation_degrees: shape.rotation_degrees,
                opacity: shape.opacity,
            });
        }
        graph.effects.extend(
            visual
                .effects
                .iter()
                .filter(|effect| indices.contains_key(effect.target_id.as_str()))
                .cloned(),
        );
        graph.masks.extend(
            visual
                .masks
                .iter()
                .filter(|mask| indices.contains_key(mask.target_id.as_str()))
                .cloned(),
        );
        graph.transitions.extend(
            visual
                .transitions
                .iter()
                .filter(|transition| {
                    indices.contains_key(transition.outgoing_clip_id.as_str())
                        && indices.contains_key(transition.incoming_clip_id.as_str())
                })
                .cloned(),
        );
    }
    let compiled = CompositionV3 {
        format: CompositionV3Format::V3,
        base,
        visual: graph,
    };
    crate::coverage_v3::audit_visual_coverage(snapshot, &compiled)?;
    Ok(compiled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{
        Asset, AssetKind, Clip, CreateProject, FrameRate, Keyframe, KeyframeChannel,
        KeyframeInterpolation, Time, TimeRange, Track,
        project_v3::{CubicBezierV1, KeyframeCurveV1},
    };
    use serde_json::json;

    fn time(ticks: i64) -> Time {
        Time {
            ticks: ticks.to_string(),
            timebase: 1000,
        }
    }
    fn fixture() -> ProjectSnapshotV3 {
        let mut base = opencut_editor_core::create_project(&CreateProject {
            id: "p".into(),
            name: "Project".into(),
            sequence_id: "main".into(),
            width: 640,
            height: 360,
            fps: FrameRate {
                numerator: 25,
                denominator: 1,
            },
        })
        .unwrap();
        base.sequences[0].tracks.push(Track {
            id: "graphics".into(),
            name: "Graphics".into(),
            kind: TrackKind::Graphic,
            enabled: true,
            muted: false,
            locked: false,
            clips: vec![],
            params: Default::default(),
        });
        ProjectSnapshotV3 {
            revision: "rev".into(),
            project: ProjectDocumentV3 {
                schema_version: 3,
                base,
                keyframe_curves: vec![],
                visual: vec![SequenceVisualV3 {
                    sequence_id: "main".into(),
                    shapes: vec![ShapeClip {
                        id: "shape".into(),
                        track_id: "graphics".into(),
                        timeline_range: TimeRange {
                            start: time(0),
                            duration: time(1000),
                        },
                        primitive: ShapePrimitive::Ellipse {
                            radius_x_px: 40.0,
                            radius_y_px: 30.0,
                        },
                        fill: Rgba {
                            red: 255,
                            green: 0,
                            blue: 0,
                            alpha: 255,
                        },
                        position_x: 5.0,
                        position_y: 0.0,
                        rotation_degrees: 0.0,
                        opacity: 1.0,
                    }],
                    effects: vec![EffectNode {
                        id: "grade".into(),
                        target_id: "shape".into(),
                        effect: VisualEffect::ColorAdjust {
                            exposure_ev: 0.5,
                            contrast: 1.0,
                            saturation: 1.0,
                        },
                    }],
                    masks: vec![MaskNode {
                        id: "mask".into(),
                        target_id: "shape".into(),
                        shape: MaskShape::Rectangle {
                            x: 0.1,
                            y: 0.1,
                            width: 0.8,
                            height: 0.8,
                        },
                        inverted: false,
                        feather: 0.0,
                    }],
                    transitions: vec![],
                }],
            },
        }
    }
    #[test]
    fn shape_only_project_compiles_one_shared_visual_graph_for_all_scopes() {
        let source = fixture();
        let visual = compile_v3(&source, "main", CompositionScope::Visual).unwrap();
        let full = compile_v3(&source, "main", CompositionScope::AudioVisual).unwrap();
        assert_eq!(visual.visual, full.visual);
        assert_eq!(visual.base.timeline.duration.ticks, 1000);
        assert_eq!(visual.visual.targets[0].kind, VisualTargetKind::Shape);
        assert_eq!(visual.visual.effects[0].target_id, "shape");
        assert_eq!(visual.visual.masks[0].target_id, "shape");
    }

    #[test]
    fn coverage_rejects_lost_injected_or_changed_nodes() {
        use crate::coverage_v3::audit_visual_coverage;
        let source = fixture();
        let original = compile_v3(&source, "main", CompositionScope::Visual).unwrap();
        let mutations: [fn(&mut CompositionV3); 9] = [
            |c| c.visual.effects.clear(),
            |c| c.visual.masks.clear(),
            |c| c.visual.shapes.clear(),
            |c| c.visual.targets.clear(),
            |c| c.visual.targets[0].index = 99,
            |c| c.visual.masks[0].inverted = true,
            |c| c.visual.effects.push(c.visual.effects[0].clone()),
            |c| c.visual.shapes[0].opacity = 0.5,
            |c| c.base.revision = "wrong-revision".into(),
        ];
        for mutate in mutations {
            let mut broken = original.clone();
            mutate(&mut broken);
            assert_eq!(
                audit_visual_coverage(&source, &broken).unwrap_err().code,
                "COMPOSITION_COVERAGE_MISMATCH"
            );
        }
    }

    #[test]
    fn enabled_clip_transitions_reach_both_render_scopes() {
        let mut source = fixture();
        source.project.base.assets.push(Asset {
            id: "footage".into(),
            name: "Footage".into(),
            kind: AssetKind::Video,
            uri: "asset://footage".into(),
            duration: Some(time(3000)),
            version: None,
            metadata: Default::default(),
        });
        source.project.base.sequences[0].tracks.push(Track {
            id: "video".into(),
            name: "Video".into(),
            kind: TrackKind::Video,
            enabled: true,
            muted: true,
            locked: false,
            clips: ["out", "in"]
                .into_iter()
                .enumerate()
                .map(|(index, id)| Clip {
                    id: id.into(),
                    asset_id: Some("footage".into()),
                    timeline_range: TimeRange {
                        start: time(index as i64 * 1000),
                        duration: time(1000),
                    },
                    source_range: Some(TimeRange {
                        start: time(500 + index as i64 * 1000),
                        duration: time(1000),
                    }),
                    params: Default::default(),
                    channels: vec![],
                })
                .collect(),
            params: Default::default(),
        });
        let transition = TransitionNode {
            id: "crossfade".into(),
            outgoing_clip_id: "out".into(),
            incoming_clip_id: "in".into(),
            duration: time(600),
            transition: Transition::Crossfade,
        };
        source.project.visual[0]
            .transitions
            .push(transition.clone());
        for scope in [CompositionScope::Visual, CompositionScope::AudioVisual] {
            let compiled = compile_v3(&source, "main", scope).unwrap();
            assert_eq!(compiled.visual.transitions, vec![transition.clone()]);
            let mut lost = compiled.clone();
            lost.visual.transitions.clear();
            assert_eq!(
                crate::coverage_v3::audit_visual_coverage(&source, &lost)
                    .unwrap_err()
                    .code,
                "COMPOSITION_COVERAGE_MISMATCH"
            );
        }
        source.project.base.sequences[0].tracks[1].enabled = false;
        for scope in [CompositionScope::Visual, CompositionScope::AudioVisual] {
            let compiled = compile_v3(&source, "main", scope).unwrap();
            assert!(compiled.visual.transitions.is_empty());
            let receipt = crate::coverage_v3::audit_visual_coverage(&source, &compiled).unwrap();
            assert!(
                receipt
                    .entries
                    .iter()
                    .any(|entry| entry.source_id == "crossfade"
                        && entry.disposition
                            == crate::coverage_v3::VisualDisposition::DisabledTrack)
            );
            let mut injected = compiled;
            injected.visual.transitions.push(transition.clone());
            assert!(crate::coverage_v3::audit_visual_coverage(&source, &injected).is_err());
        }
    }

    #[test]
    fn schema_3_keyframe_curves_reach_the_media_composition_contract() {
        let mut source = fixture();
        source.project.base.assets.push(Asset {
            id: "footage".into(),
            name: "Footage".into(),
            kind: AssetKind::Video,
            uri: "asset://footage".into(),
            duration: Some(time(1000)),
            version: None,
            metadata: Default::default(),
        });
        source.project.base.sequences[0].tracks.push(Track {
            id: "video".into(),
            name: "Video".into(),
            kind: TrackKind::Video,
            enabled: true,
            muted: false,
            locked: false,
            clips: vec![Clip {
                id: "clip".into(),
                asset_id: Some("footage".into()),
                timeline_range: TimeRange {
                    start: time(0),
                    duration: time(1000),
                },
                source_range: Some(TimeRange {
                    start: time(0),
                    duration: time(1000),
                }),
                params: Default::default(),
                channels: vec![KeyframeChannel {
                    parameter: "opacity".into(),
                    keyframes: vec![
                        Keyframe {
                            id: "start".into(),
                            time: time(0),
                            value: json!(0.0),
                            interpolation: KeyframeInterpolation::Linear,
                        },
                        Keyframe {
                            id: "end".into(),
                            time: time(1000),
                            value: json!(1.0),
                            interpolation: KeyframeInterpolation::Linear,
                        },
                    ],
                }],
            }],
            params: Default::default(),
        });
        source.project.keyframe_curves.push(KeyframeCurveV1 {
            id: "opacity-ease".into(),
            schema_version: 1,
            sequence_id: "main".into(),
            clip_id: "clip".into(),
            parameter: "opacity".into(),
            keyframe_id: "start".into(),
            curve: CubicBezierV1 {
                x1: 0.0,
                y1: 0.0,
                x2: 1.0,
                y2: 0.0,
            },
        });

        let composition = compile_v3(&source, "main", CompositionScope::Visual).unwrap();
        assert_eq!(composition.base.timeline.video_clips.len(), 1);
        assert_eq!(
            composition.base.timeline.video_clips[0]
                .curves
                .opacity
                .segments[0]
                .cubic_bezier,
            Some(opencut_media_contracts::composition::CubicBezierCurve {
                x1: 0.0,
                y1: 0.0,
                x2: 1.0,
                y2: 0.0,
            })
        );
    }
    #[test]
    fn feathered_mask_is_preserved_in_compiled_visual_graph() {
        let mut source = fixture();
        source.project.visual[0].masks[0].feather = 0.1;
        assert_eq!(
            compile_v3(&source, "main", CompositionScope::Visual)
                .unwrap()
                .visual
                .masks[0]
                .feather,
            0.1
        );
    }
}
