//! Schema 3 project to portable media-graph adapter. Resolution is required
//! before any plan can be executed against immutable asset versions.
use super::*;
use opencut_composition::project_v3::{CompiledVisualGraph, VisualTargetKind};
use opencut_contracts::project_v3::{
    MaskShape, ProjectSnapshotV3, ShapePrimitive, Transition, VisualEffect,
};
use opencut_media::{
    CompiledShapeV3, CompiledVisualGraphV3, EffectNodeV3, MaskNodeV3, MaskShapeV3, RgbaV3,
    ShapePrimitiveV3, TimelineExportRequestV3, TimelineFrameRenderRequestV3,
    TimelineRangeRenderRequestV3, TransitionNodeV3, TransitionV3, VisualEffectV3,
    VisualTargetBindingV3, VisualTargetKindV3,
};

#[derive(Clone, Debug)]
pub struct ResolvedProjectSnapshotV3 {
    _source: ProjectSnapshotV3,
    execution: ProjectSnapshotV3,
    sequence_id: String,
    resolution_issues: Vec<PreflightIssue>,
    visual_only: bool,
}

pub struct ResolvedProjectRenderRequestV3<'a> {
    pub resolved: &'a ResolvedProjectSnapshotV3,
    pub output: PathBuf,
    pub container: Container,
    pub video_codec: VideoCodec,
    pub overwrite: bool,
}

pub struct ResolvedProjectPreflightRequestV3<'a> {
    pub resolved: &'a ResolvedProjectSnapshotV3,
    pub container: Container,
    pub video_codec: VideoCodec,
}

pub struct ResolvedProjectFrameRequestV3<'a> {
    pub resolved: &'a ResolvedProjectSnapshotV3,
    pub requested_time: Time,
    pub alignment: opencut_media::FrameAlignment,
    pub output: PathBuf,
    pub overwrite: bool,
}

pub struct ResolvedProjectRangeRequestV3<'a> {
    pub resolved: &'a ResolvedProjectSnapshotV3,
    pub requested_range: TimeRange,
    pub alignment: opencut_media::FrameAlignment,
    pub output: PathBuf,
    pub overwrite: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectRenderPlanV3 {
    pub revision: String,
    pub sequence_id: String,
    pub media_request: TimelineExportRequestV3,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectFramePlanV3 {
    pub revision: String,
    pub sequence_id: String,
    pub actual_sequence_time: Time,
    pub media_request: TimelineFrameRenderRequestV3,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectRangePlanV3 {
    pub revision: String,
    pub sequence_id: String,
    pub actual_sequence_range: TimeRange,
    pub media_request: TimelineRangeRenderRequestV3,
}

pub struct ProjectRenderAdapterV3;
impl ProjectRenderAdapterV3 {
    pub fn preflight_resolved(
        request: &ResolvedProjectPreflightRequestV3<'_>,
    ) -> ProjectPreflightReport {
        let resolved = request.resolved;
        let base_resolved = ResolvedProjectSnapshot {
            _source: base_snapshot(&resolved._source),
            execution: base_snapshot(&resolved.execution),
            sequence_id: resolved.sequence_id.clone(),
            resolution_issues: resolved.resolution_issues.clone(),
        };
        let mut report =
            ProjectRenderAdapter::preflight_resolved(&ResolvedProjectPreflightRequest {
                resolved: &base_resolved,
                container: request.container,
                video_codec: request.video_codec,
            });
        let output = std::env::temp_dir().join("opencut-v3-preflight-output");
        let base_plan = ProjectRenderAdapter::plan_resolved(&ResolvedProjectRenderRequest {
            resolved: &base_resolved,
            output: output.clone(),
            container: request.container,
            video_codec: request.video_codec,
            overwrite: false,
        });
        let visual_plan = Self::plan_resolved(&ResolvedProjectRenderRequestV3 {
            resolved,
            output,
            container: request.container,
            video_codec: request.video_codec,
            overwrite: false,
        });
        match visual_plan {
            Ok(plan) => {
                // An empty schema-2 base can be a valid shape-only schema-3
                // graph. Keep every independent source/font/codec diagnostic.
                if let Err(problem) = base_plan {
                    report.issues.retain(|issue| {
                        issue.code != problem.code || issue.message != problem.message
                    });
                }
                let mut filters = vec!["geq", "overlay", "color", "fps", "pad"];
                if plan
                    .media_request
                    .visual
                    .effects
                    .iter()
                    .any(|node| matches!(node.effect, VisualEffectV3::ColorAdjust { .. }))
                {
                    filters.push("eq");
                }
                if plan
                    .media_request
                    .visual
                    .effects
                    .iter()
                    .any(|node| matches!(node.effect, VisualEffectV3::GaussianBlur { .. }))
                {
                    filters.push("gblur");
                }
                if let Ok(backend) = FfmpegBackend::discover() {
                    match backend.missing_v3_filters(&filters) {
                        Ok(missing) => {
                            for name in missing {
                                report.issues.push(preflight_issue(
                                    "CAPABILITY_UNAVAILABLE",
                                    format!("FFmpeg {name} filter is unavailable"),
                                    None,
                                    None,
                                    None,
                                ));
                            }
                        }
                        Err(problem) => report.issues.push(preflight_issue(
                            "TOOL_UNAVAILABLE",
                            problem.to_string(),
                            None,
                            None,
                            None,
                        )),
                    }
                }
            }
            Err(problem) => {
                if !report
                    .issues
                    .iter()
                    .any(|issue| issue.code == problem.code && issue.message == problem.message)
                {
                    report.issues.push(preflight_issue(
                        problem.code,
                        problem.message,
                        None,
                        None,
                        None,
                    ));
                }
            }
        }
        report.ready = report.issues.is_empty();
        report
    }

    pub fn required_asset_ids(
        snapshot: &ProjectSnapshotV3,
        sequence_id: &str,
    ) -> Result<Vec<String>, ProjectRenderError> {
        ProjectRenderAdapter::required_asset_ids(&base_snapshot(snapshot), sequence_id)
    }

    pub fn required_visual_asset_ids(
        snapshot: &ProjectSnapshotV3,
        sequence_id: &str,
    ) -> Result<Vec<String>, ProjectRenderError> {
        ProjectRenderAdapter::required_visual_asset_ids(&base_snapshot(snapshot), sequence_id)
    }

    pub fn resolve_assets<F>(
        snapshot: ProjectSnapshotV3,
        sequence_id: &str,
        mut resolver: F,
    ) -> Result<ResolvedProjectSnapshotV3, ProjectRenderError>
    where
        F: FnMut(&Asset) -> Result<PathBuf, opencut_contracts::EditorError>,
    {
        Self::resolve_scoped(snapshot, sequence_id, &mut resolver, false)
    }

    pub fn resolve_visual_assets<F>(
        snapshot: ProjectSnapshotV3,
        sequence_id: &str,
        mut resolver: F,
    ) -> Result<ResolvedProjectSnapshotV3, ProjectRenderError>
    where
        F: FnMut(&Asset) -> Result<PathBuf, opencut_contracts::EditorError>,
    {
        Self::resolve_scoped(snapshot, sequence_id, &mut resolver, true)
    }

    fn resolve_scoped<F>(
        snapshot: ProjectSnapshotV3,
        sequence_id: &str,
        resolver: &mut F,
        visual_only: bool,
    ) -> Result<ResolvedProjectSnapshotV3, ProjectRenderError>
    where
        F: FnMut(&Asset) -> Result<PathBuf, opencut_contracts::EditorError>,
    {
        opencut_editor_core::project_v3::validate_project_v3(&snapshot.project)
            .map_err(|e| error("INVALID_PROJECT", e.message))?;
        let required = if visual_only {
            Self::required_visual_asset_ids(&snapshot, sequence_id)?
        } else {
            Self::required_asset_ids(&snapshot, sequence_id)?
        };
        let mut execution = snapshot.clone();
        let mut resolution_issues = Vec::new();
        for asset_id in required {
            let Some(source_asset) = snapshot
                .project
                .base
                .assets
                .iter()
                .find(|a| a.id == asset_id)
            else {
                continue;
            };
            if source_asset.version.is_none() {
                continue;
            }
            let execution_asset = execution
                .project
                .base
                .assets
                .iter_mut()
                .find(|a| a.id == asset_id)
                .expect("execution clones assets");
            match resolver(source_asset).and_then(|path| {
                path.into_os_string().into_string().map_err(|_| {
                    opencut_contracts::EditorError::new(
                        "INVALID_ARGUMENT",
                        "Resolved asset path is not valid UTF-8",
                    )
                })
            }) {
                Ok(uri) => execution_asset.uri = uri,
                Err(problem) => {
                    execution_asset.uri = format!("opencut-unresolved://{asset_id}");
                    resolution_issues.push(preflight_issue(
                        problem.code,
                        problem.message,
                        Some(asset_id),
                        None,
                        None,
                    ));
                }
            }
        }
        Ok(ResolvedProjectSnapshotV3 {
            _source: snapshot,
            execution,
            sequence_id: sequence_id.into(),
            resolution_issues,
            visual_only,
        })
    }

    pub fn plan_resolved(
        request: &ResolvedProjectRenderRequestV3<'_>,
    ) -> Result<ProjectRenderPlanV3, ProjectRenderError> {
        let resolved = request.resolved;
        if resolved.visual_only {
            return Err(error(
                "ASSET_VERSION_UNRESOLVED",
                "full render requires all assets to be resolved",
            ));
        }
        if !resolved.resolution_issues.is_empty() {
            return Err(error(
                "ASSET_RESOLUTION_FAILED",
                "one or more required immutable assets could not be resolved",
            ));
        }
        let composition = opencut_composition::project_v3::compile_v3(
            &resolved.execution,
            &resolved.sequence_id,
            CompositionScope::AudioVisual,
        )?;
        let base = bind_composition(
            composition.base,
            &base_snapshot(&resolved.execution),
            true,
            request.output.clone(),
            request.container,
            request.video_codec,
            request.overwrite,
        )?;
        Ok(ProjectRenderPlanV3 {
            revision: resolved.execution.revision.clone(),
            sequence_id: resolved.sequence_id.clone(),
            media_request: TimelineExportRequestV3 {
                base,
                visual: map_graph(composition.visual),
            },
        })
    }

    pub fn plan_frame_resolved(
        request: &ResolvedProjectFrameRequestV3<'_>,
    ) -> Result<ProjectFramePlanV3, ProjectRenderError> {
        let resolved = request.resolved;
        if !resolved.resolution_issues.is_empty() {
            return Err(error(
                "ASSET_RESOLUTION_FAILED",
                "one or more required immutable visual assets could not be resolved",
            ));
        }
        let composition = opencut_composition::project_v3::compile_v3(
            &resolved.execution,
            &resolved.sequence_id,
            CompositionScope::Visual,
        )?;
        let fps = opencut_contracts::FrameRate {
            numerator: composition.base.timeline.frame_rate.numerator,
            denominator: composition.base.timeline.frame_rate.denominator,
        };
        let actual_sequence_time = select_project_frame_time(
            &request.requested_time,
            &fps,
            request.alignment,
            composition.base.timeline.duration,
        )
        .map_err(project_time_error)?;
        let sequence_time = time(&actual_sequence_time)?;
        let base = bind_composition(
            composition.base,
            &base_snapshot(&resolved.execution),
            true,
            request.output.clone(),
            Container::Mp4,
            VideoCodec::H264,
            request.overwrite,
        )?;
        Ok(ProjectFramePlanV3 {
            revision: resolved.execution.revision.clone(),
            sequence_id: resolved.sequence_id.clone(),
            actual_sequence_time,
            media_request: TimelineFrameRenderRequestV3 {
                timeline: TimelineExportRequestV3 {
                    base,
                    visual: map_graph(composition.visual),
                },
                sequence_time,
            },
        })
    }

    pub fn plan_range_resolved(
        request: &ResolvedProjectRangeRequestV3<'_>,
    ) -> Result<ProjectRangePlanV3, ProjectRenderError> {
        let plan = Self::plan_resolved(&ResolvedProjectRenderRequestV3 {
            resolved: request.resolved,
            output: request.output.clone(),
            container: Container::Mp4,
            video_codec: VideoCodec::H264,
            overwrite: request.overwrite,
        })?;
        let sequence = request
            .resolved
            .execution
            .project
            .base
            .sequences
            .iter()
            .find(|s| s.id == request.resolved.sequence_id)
            .ok_or_else(|| error("NOT_FOUND", "sequence does not exist"))?;
        let actual_sequence_range = select_project_range(
            &request.requested_range,
            &sequence.fps,
            request.alignment,
            plan.media_request.base.duration,
        )
        .map_err(project_time_error)?;
        let sequence_start = time(&actual_sequence_range.start)?;
        let sequence_duration = time(&actual_sequence_range.duration)?;
        Ok(ProjectRangePlanV3 {
            revision: plan.revision,
            sequence_id: plan.sequence_id,
            actual_sequence_range,
            media_request: TimelineRangeRenderRequestV3 {
                timeline: plan.media_request,
                sequence_start,
                sequence_duration,
            },
        })
    }
}

fn base_snapshot(snapshot: &ProjectSnapshotV3) -> ProjectSnapshot {
    ProjectSnapshot {
        project: snapshot.project.base.clone(),
        revision: snapshot.revision.clone(),
    }
}

fn map_graph(graph: CompiledVisualGraph) -> CompiledVisualGraphV3 {
    CompiledVisualGraphV3 {
        targets: graph
            .targets
            .into_iter()
            .map(|target| VisualTargetBindingV3 {
                target_id: target.target_id,
                index: target.index,
                z_index: target.z_index,
                layer_order: target.layer_order,
                kind: match target.kind {
                    VisualTargetKind::VideoClip => VisualTargetKindV3::VideoClip,
                    VisualTargetKind::ImageClip => VisualTargetKindV3::ImageClip,
                    VisualTargetKind::Shape => VisualTargetKindV3::Shape,
                },
            })
            .collect(),
        shapes: graph
            .shapes
            .into_iter()
            .map(|shape| CompiledShapeV3 {
                id: shape.id,
                start: shape.start,
                duration: shape.duration,
                primitive: match shape.primitive {
                    ShapePrimitive::Rectangle {
                        width_px,
                        height_px,
                    } => ShapePrimitiveV3::Rectangle {
                        width_px,
                        height_px,
                    },
                    ShapePrimitive::Ellipse {
                        radius_x_px,
                        radius_y_px,
                    } => ShapePrimitiveV3::Ellipse {
                        radius_x_px,
                        radius_y_px,
                    },
                },
                fill: RgbaV3 {
                    red: shape.fill.red,
                    green: shape.fill.green,
                    blue: shape.fill.blue,
                    alpha: shape.fill.alpha,
                },
                position_x: shape.position_x,
                position_y: shape.position_y,
                rotation_degrees: shape.rotation_degrees,
                opacity: shape.opacity,
            })
            .collect(),
        effects: graph
            .effects
            .into_iter()
            .map(|node| EffectNodeV3 {
                target_id: node.target_id,
                effect: match node.effect {
                    VisualEffect::ColorAdjust {
                        exposure_ev,
                        contrast,
                        saturation,
                    } => VisualEffectV3::ColorAdjust {
                        exposure_ev,
                        contrast,
                        saturation,
                    },
                    VisualEffect::GaussianBlur { radius_px } => {
                        VisualEffectV3::GaussianBlur { radius_px }
                    }
                },
            })
            .collect(),
        masks: graph
            .masks
            .into_iter()
            .map(|node| MaskNodeV3 {
                target_id: node.target_id,
                inverted: node.inverted,
                feather: node.feather,
                shape: match node.shape {
                    MaskShape::Rectangle {
                        x,
                        y,
                        width,
                        height,
                    } => MaskShapeV3::Rectangle {
                        x,
                        y,
                        width,
                        height,
                    },
                    MaskShape::Ellipse {
                        x,
                        y,
                        width,
                        height,
                    } => MaskShapeV3::Ellipse {
                        x,
                        y,
                        width,
                        height,
                    },
                },
            })
            .collect(),
        transitions: graph
            .transitions
            .into_iter()
            .map(|node| TransitionNodeV3 {
                outgoing_clip_id: node.outgoing_clip_id,
                incoming_clip_id: node.incoming_clip_id,
                duration: time(&node.duration).expect("validated project transition time"),
                transition: match node.transition {
                    Transition::Crossfade => TransitionV3::Crossfade,
                },
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::project_v3::{ProjectDocumentV3, Rgba, SequenceVisualV3, ShapeClip};
    use opencut_contracts::{CreateProject, FrameRate, TimeRange, Track};

    #[test]
    fn shape_only_v3_preflight_uses_visual_graph_and_native_capabilities() {
        let Ok(backend) = FfmpegBackend::discover() else {
            return;
        };
        if !backend
            .supports_video_codec(VideoCodec::H264)
            .unwrap_or(false)
        {
            return;
        }
        let mut base = opencut_editor_core::create_project(&CreateProject {
            id: "shape-film".into(),
            name: "Shape film".into(),
            sequence_id: "main".into(),
            width: 160,
            height: 90,
            fps: FrameRate {
                numerator: 10,
                denominator: 1,
            },
        })
        .unwrap();
        base.schema_version = 2;
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
        let snapshot = ProjectSnapshotV3 {
            revision: "revision-1".into(),
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
                            start: Time {
                                ticks: "0".into(),
                                timebase: 1,
                            },
                            duration: Time {
                                ticks: "1".into(),
                                timebase: 1,
                            },
                        },
                        primitive: ShapePrimitive::Rectangle {
                            width_px: 40.0,
                            height_px: 30.0,
                        },
                        fill: Rgba {
                            red: 255,
                            green: 0,
                            blue: 0,
                            alpha: 255,
                        },
                        position_x: 0.0,
                        position_y: 0.0,
                        rotation_degrees: 0.0,
                        opacity: 1.0,
                    }],
                    effects: vec![],
                    masks: vec![],
                    transitions: vec![],
                }],
            },
        };
        let resolved =
            ProjectRenderAdapterV3::resolve_assets(snapshot, "main", |_| unreachable!()).unwrap();
        let report =
            ProjectRenderAdapterV3::preflight_resolved(&ResolvedProjectPreflightRequestV3 {
                resolved: &resolved,
                container: Container::Mp4,
                video_codec: VideoCodec::H264,
            });
        assert!(report.ready, "{:?}", report.issues);
    }
}
