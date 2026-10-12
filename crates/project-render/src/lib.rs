use opencut_composition::{
    Composition, CompositionScope,
    validation::{
        VIDEO_PARAMS, VISUAL_PARAMS, map_font_policy, map_text_align, time, validate_channels,
        validate_params,
    },
};
use opencut_contracts::{Asset, AssetKind, ProjectSnapshot, Time, TimeRange, TrackKind};
use opencut_media::{
    AspectMode, CancellationToken, Container, ExportResult, FfmpegBackend, MediaError,
    MediaMetadata, MediaTime, ProbeRequest, ProgressUpdate, StreamKind, TimelineCaption,
    TimelineExportRequest, TimelineFrameRenderRequest, TimelineRangeRenderRequest, VideoCodec,
    VideoOutput,
};
use opencut_time::RationalTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt, path::PathBuf};

pub mod project_v3;
pub mod project_v4;
pub mod project_v4_audio;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectRenderRequest {
    pub snapshot: ProjectSnapshot,
    pub sequence_id: String,
    pub output: PathBuf,
    pub container: Container,
    pub video_codec: VideoCodec,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectRenderPlan {
    pub revision: String,
    pub sequence_id: String,
    pub media_request: TimelineExportRequest,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectPreflightRequest {
    pub snapshot: ProjectSnapshot,
    pub sequence_id: String,
    pub container: Container,
    pub video_codec: VideoCodec,
}

pub use opencut_api_contracts::render::{PreflightIssue, ProjectPreflightReport};

/// A project snapshot whose required immutable asset versions have passed
/// through the caller's resolver. This type is intentionally not serializable
/// and cannot be constructed from a raw snapshot.
#[derive(Clone, Debug)]
pub struct ResolvedProjectSnapshot {
    _source: ProjectSnapshot,
    execution: ProjectSnapshot,
    sequence_id: String,
    resolution_issues: Vec<PreflightIssue>,
}

/// A fixed project revision whose visual inputs alone have passed through the
/// caller's resolver. It cannot be used for full audio/video export.
///
/// ```compile_fail
/// use opencut_media::{Container, VideoCodec};
/// use opencut_project_render::{ResolvedProjectRenderRequest, ResolvedVisualProjectSnapshot};
/// use std::path::PathBuf;
/// fn full_export(visual: &ResolvedVisualProjectSnapshot) {
///     let _ = ResolvedProjectRenderRequest {
///         resolved: visual,
///         output: PathBuf::from("render.mp4"),
///         container: Container::Mp4,
///         video_codec: VideoCodec::H264,
///         overwrite: false,
///     };
/// }
/// ```
#[derive(Clone, Debug)]
pub struct ResolvedVisualProjectSnapshot {
    _source: ProjectSnapshot,
    execution: ProjectSnapshot,
    sequence_id: String,
    resolution_issues: Vec<PreflightIssue>,
}

/// Request for preflighting an explicitly resolved snapshot.
///
/// ```compile_fail
/// use opencut_contracts::ProjectSnapshot;
/// use opencut_media::{Container, VideoCodec};
/// use opencut_project_render::ResolvedProjectPreflightRequest;
/// let raw: ProjectSnapshot = todo!();
/// let _ = ResolvedProjectPreflightRequest {
///     resolved: &raw,
///     container: Container::Mp4,
///     video_codec: VideoCodec::H264,
/// };
/// ```
pub struct ResolvedProjectPreflightRequest<'a> {
    pub resolved: &'a ResolvedProjectSnapshot,
    pub container: Container,
    pub video_codec: VideoCodec,
}

pub struct ResolvedProjectRenderRequest<'a> {
    pub resolved: &'a ResolvedProjectSnapshot,
    pub output: PathBuf,
    pub container: Container,
    pub video_codec: VideoCodec,
    pub overwrite: bool,
}

pub struct ResolvedProjectFrameRequest<'a> {
    pub resolved: &'a ResolvedVisualProjectSnapshot,
    pub requested_time: Time,
    pub alignment: opencut_media::FrameAlignment,
    pub output: PathBuf,
    pub overwrite: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectFramePlan {
    pub revision: String,
    pub sequence_id: String,
    pub actual_sequence_time: Time,
    pub media_request: TimelineFrameRenderRequest,
}

pub struct ResolvedProjectRangeRequest<'a> {
    pub resolved: &'a ResolvedProjectSnapshot,
    pub requested_range: TimeRange,
    pub alignment: opencut_media::FrameAlignment,
    pub output: PathBuf,
    pub overwrite: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectRangePlan {
    pub revision: String,
    pub sequence_id: String,
    pub actual_sequence_range: TimeRange,
    pub media_request: TimelineRangeRenderRequest,
}

#[derive(Debug)]
pub struct ProjectRenderError {
    pub code: &'static str,
    pub message: String,
}
impl fmt::Display for ProjectRenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ProjectRenderError {}
impl From<MediaError> for ProjectRenderError {
    fn from(value: MediaError) -> Self {
        Self {
            code: value.code(),
            message: value.to_string(),
        }
    }
}

pub struct ProjectRenderAdapter;
/// Visual contract: sequence tracks are composited bottom-to-top. Media is
/// aspect-contained, then scaled; positions are pixel offsets from canvas
/// center, positive right/down; positive rotation is clockwise around center.
/// Track and clip opacity multiply. Captions are composited above all tracks.
impl ProjectRenderAdapter {
    pub fn required_asset_ids(
        snapshot: &ProjectSnapshot,
        sequence_id: &str,
    ) -> Result<Vec<String>, ProjectRenderError> {
        opencut_editor_core::dependencies::required_asset_ids(
            snapshot,
            sequence_id,
            opencut_editor_core::dependencies::AssetScope::All,
        )
        .map_err(|problem| error("NOT_FOUND", problem.message))
    }

    pub fn required_visual_asset_ids(
        snapshot: &ProjectSnapshot,
        sequence_id: &str,
    ) -> Result<Vec<String>, ProjectRenderError> {
        opencut_editor_core::dependencies::required_asset_ids(
            snapshot,
            sequence_id,
            opencut_editor_core::dependencies::AssetScope::Visual,
        )
        .map_err(|problem| error("NOT_FOUND", problem.message))
    }

    pub fn preflight(request: &ProjectPreflightRequest) -> ProjectPreflightReport {
        Self::preflight_with_backend(request, FfmpegBackend::discover(), false)
    }

    /// Runs required immutable assets through the host's verification callback.
    /// The returned capability proves that this resolution flow ran; the host
    /// remains responsible for verifying the bytes behind each returned path.
    pub fn resolve_assets<F>(
        snapshot: ProjectSnapshot,
        sequence_id: &str,
        mut resolver: F,
    ) -> Result<ResolvedProjectSnapshot, ProjectRenderError>
    where
        F: FnMut(&Asset) -> Result<PathBuf, opencut_contracts::EditorError>,
    {
        opencut_editor_core::validate_project(&snapshot.project)
            .map_err(|problem| error("INVALID_PROJECT", problem.to_string()))?;
        let required = Self::required_asset_ids(&snapshot, sequence_id)?;
        let mut execution = snapshot.clone();
        let mut resolution_issues = Vec::new();
        for asset_id in required {
            let Some(source_asset) = snapshot
                .project
                .assets
                .iter()
                .find(|asset| asset.id == asset_id)
            else {
                continue;
            };
            if source_asset.version.is_none() {
                continue;
            }
            let execution_asset = execution
                .project
                .assets
                .iter_mut()
                .find(|asset| asset.id == asset_id)
                .expect("execution snapshot clones source assets");
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
        Ok(ResolvedProjectSnapshot {
            _source: snapshot,
            execution,
            sequence_id: sequence_id.into(),
            resolution_issues,
        })
    }

    pub fn resolve_visual_assets<F>(
        snapshot: ProjectSnapshot,
        sequence_id: &str,
        mut resolver: F,
    ) -> Result<ResolvedVisualProjectSnapshot, ProjectRenderError>
    where
        F: FnMut(&Asset) -> Result<PathBuf, opencut_contracts::EditorError>,
    {
        opencut_editor_core::validate_project(&snapshot.project)
            .map_err(|problem| error("INVALID_PROJECT", problem.to_string()))?;
        let required = Self::required_visual_asset_ids(&snapshot, sequence_id)?;
        let mut execution = snapshot.clone();
        let mut resolution_issues = Vec::new();
        for asset_id in required {
            let Some(source_asset) = snapshot
                .project
                .assets
                .iter()
                .find(|asset| asset.id == asset_id)
            else {
                continue;
            };
            if source_asset.version.is_none() {
                continue;
            }
            let execution_asset = execution
                .project
                .assets
                .iter_mut()
                .find(|asset| asset.id == asset_id)
                .expect("execution snapshot clones source assets");
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
        Ok(ResolvedVisualProjectSnapshot {
            _source: snapshot,
            execution,
            sequence_id: sequence_id.into(),
            resolution_issues,
        })
    }

    pub fn preflight_resolved(
        request: &ResolvedProjectPreflightRequest<'_>,
    ) -> ProjectPreflightReport {
        let resolved = request.resolved;
        let raw_request = ProjectPreflightRequest {
            snapshot: resolved.execution.clone(),
            sequence_id: resolved.sequence_id.clone(),
            container: request.container,
            video_codec: request.video_codec,
        };
        let mut report =
            Self::preflight_with_backend(&raw_request, FfmpegBackend::discover(), true);
        if !resolved.resolution_issues.is_empty() {
            let failed = resolved
                .resolution_issues
                .iter()
                .filter_map(|issue| issue.asset_id.as_deref())
                .collect::<std::collections::HashSet<_>>();
            report.issues.retain(|issue| {
                issue
                    .asset_id
                    .as_deref()
                    .is_none_or(|id| !failed.contains(id))
            });
            report
                .issues
                .splice(0..0, resolved.resolution_issues.clone());
            report.ready = false;
        }
        report
    }

    fn preflight_with_backend(
        request: &ProjectPreflightRequest,
        backend: Result<FfmpegBackend, MediaError>,
        resolved_assets: bool,
    ) -> ProjectPreflightReport {
        let mut issues = Vec::new();
        let revision = request.snapshot.revision.clone();
        let Some(sequence) = request
            .snapshot
            .project
            .sequences
            .iter()
            .find(|sequence| sequence.id == request.sequence_id)
        else {
            issues.push(preflight_issue(
                "SEQUENCE_NOT_FOUND",
                "sequence does not exist",
                None,
                None,
                None,
            ));
            return ProjectPreflightReport {
                revision,
                sequence_id: request.sequence_id.clone(),
                ready: false,
                issues,
            };
        };
        let assets: HashMap<_, _> = request
            .snapshot
            .project
            .assets
            .iter()
            .map(|asset| (asset.id.as_str(), asset))
            .collect();
        if let Err(problem) = opencut_editor_core::validate_project(&request.snapshot.project) {
            issues.push(preflight_issue(
                "INVALID_PROJECT",
                problem.to_string(),
                None,
                None,
                None,
            ));
        }
        collect_object_issues(sequence, &assets, &mut issues);
        let required_ids =
            Self::required_asset_ids(&request.snapshot, &request.sequence_id).unwrap_or_default();
        for id in &required_ids {
            match assets.get(id.as_str()) {
                None => issues.push(preflight_issue(
                    "MEDIA_NOT_FOUND",
                    format!("asset {id} does not exist"),
                    Some(id.clone()),
                    None,
                    None,
                )),
                Some(asset) => {
                    if let Err(problem) = execution_path(asset, resolved_assets) {
                        issues.push(preflight_issue(
                            problem.code,
                            problem.message,
                            Some(id.clone()),
                            None,
                            None,
                        ));
                    }
                }
            }
        }
        match &backend {
            Err(problem) => issues.push(preflight_issue(
                "TOOL_UNAVAILABLE",
                problem.to_string(),
                None,
                None,
                None,
            )),
            Ok(backend) => {
                match backend.supports_video_codec(request.video_codec) {
                    Ok(true) => {}
                    Ok(false) => issues.push(preflight_issue(
                        "CAPABILITY_UNAVAILABLE",
                        format!(
                            "video codec {:?} encoder is unavailable",
                            request.video_codec
                        ),
                        None,
                        None,
                        None,
                    )),
                    Err(problem) => issues.push(preflight_issue(
                        "TOOL_UNAVAILABLE",
                        problem.to_string(),
                        None,
                        None,
                        None,
                    )),
                }
                probe_required_assets(
                    backend,
                    sequence,
                    &assets,
                    &required_ids,
                    resolved_assets,
                    &mut issues,
                );
            }
        }
        for cue in &sequence.caption_cues {
            let caption = TimelineCaption {
                start: time(&cue.timeline_range.start).unwrap_or(MediaTime {
                    ticks: 0,
                    timebase: 1,
                }),
                duration: time(&cue.timeline_range.duration).unwrap_or(MediaTime {
                    ticks: 1,
                    timebase: 1,
                }),
                text: cue.text.clone(),
                font_family: cue.style.font_family.clone(),
                font_size: cue.style.font_size,
                color: cue.style.color.clone(),
                background_color: cue.style.background_color.clone(),
                font_policy: map_font_policy(&cue.style.font_policy),
                max_width: cue.style.max_width,
                line_height: cue.style.line_height,
                text_align: map_text_align(cue.style.text_align),
            };
            if let Err(problem) =
                opencut_media::layout_caption(&caption, sequence.width, sequence.height)
            {
                issues.push(preflight_issue(
                    problem.code(),
                    problem.to_string(),
                    None,
                    None,
                    Some(cue.id.clone()),
                ));
            }
        }
        let probe_request = ProjectRenderRequest {
            snapshot: request.snapshot.clone(),
            sequence_id: request.sequence_id.clone(),
            output: std::env::temp_dir().join("opencut-preflight-output"),
            container: request.container,
            video_codec: request.video_codec,
            overwrite: false,
        };
        match Self::plan_impl(&probe_request, resolved_assets) {
            Ok(plan) => {
                if let Ok(backend) = &backend
                    && let Err(problem) = backend.validate_timeline_request(&plan.media_request)
                    && !issues
                        .iter()
                        .any(|issue| issue.message == problem.to_string())
                {
                    issues.push(preflight_issue(
                        problem.code(),
                        problem.to_string(),
                        None,
                        None,
                        None,
                    ));
                }
            }
            Err(problem) if !issues.iter().any(|issue| issue.message == problem.message) => {
                issues.push(preflight_issue(
                    problem.code,
                    problem.message,
                    None,
                    None,
                    None,
                ));
            }
            Err(_) => {}
        }
        ProjectPreflightReport {
            revision,
            sequence_id: request.sequence_id.clone(),
            ready: issues.is_empty(),
            issues,
        }
    }

    pub fn plan(request: &ProjectRenderRequest) -> Result<ProjectRenderPlan, ProjectRenderError> {
        Self::plan_impl(request, false)
    }

    pub fn plan_resolved(
        request: &ResolvedProjectRenderRequest<'_>,
    ) -> Result<ProjectRenderPlan, ProjectRenderError> {
        if !request.resolved.resolution_issues.is_empty() {
            return Err(error(
                "ASSET_RESOLUTION_FAILED",
                "one or more required immutable assets could not be resolved",
            ));
        }
        Self::plan_impl(
            &ProjectRenderRequest {
                snapshot: request.resolved.execution.clone(),
                sequence_id: request.resolved.sequence_id.clone(),
                output: request.output.clone(),
                container: request.container,
                video_codec: request.video_codec,
                overwrite: request.overwrite,
            },
            true,
        )
    }

    pub fn plan_frame_resolved(
        request: &ResolvedProjectFrameRequest<'_>,
    ) -> Result<ProjectFramePlan, ProjectRenderError> {
        let resolved = request.resolved;
        if !resolved.resolution_issues.is_empty() {
            return Err(error(
                "ASSET_RESOLUTION_FAILED",
                "one or more required immutable visual assets could not be resolved",
            ));
        }
        let composition = opencut_composition::compile(
            &resolved.execution,
            &resolved.sequence_id,
            CompositionScope::Visual,
        )?;
        let fps = opencut_contracts::FrameRate {
            numerator: composition.timeline.frame_rate.numerator,
            denominator: composition.timeline.frame_rate.denominator,
        };
        let actual_sequence_time = select_project_frame_time(
            &request.requested_time,
            &fps,
            request.alignment,
            composition.timeline.duration,
        )
        .map_err(project_time_error)?;
        let selected = time(&actual_sequence_time)?;
        let timeline = bind_composition(
            composition,
            &resolved.execution,
            true,
            request.output.clone(),
            Container::Mp4,
            VideoCodec::H264,
            request.overwrite,
        )?;
        Ok(ProjectFramePlan {
            revision: resolved.execution.revision.clone(),
            sequence_id: resolved.sequence_id.clone(),
            actual_sequence_time,
            media_request: TimelineFrameRenderRequest {
                timeline,
                sequence_time: selected,
            },
        })
    }

    pub fn plan_range_resolved(
        request: &ResolvedProjectRangeRequest<'_>,
    ) -> Result<ProjectRangePlan, ProjectRenderError> {
        let resolved = request.resolved;
        let project_plan = Self::plan_resolved(&ResolvedProjectRenderRequest {
            resolved,
            output: request.output.clone(),
            container: Container::Mp4,
            video_codec: VideoCodec::H264,
            overwrite: request.overwrite,
        })?;
        let sequence = resolved
            .execution
            .project
            .sequences
            .iter()
            .find(|sequence| sequence.id == resolved.sequence_id)
            .ok_or_else(|| error("NOT_FOUND", "sequence does not exist"))?;
        let actual_sequence_range = select_project_range(
            &request.requested_range,
            &sequence.fps,
            request.alignment,
            project_plan.media_request.duration,
        )
        .map_err(project_time_error)?;
        let sequence_start = time(&actual_sequence_range.start)?;
        let sequence_duration = time(&actual_sequence_range.duration)?;
        Ok(ProjectRangePlan {
            revision: project_plan.revision,
            sequence_id: project_plan.sequence_id,
            actual_sequence_range,
            media_request: TimelineRangeRenderRequest {
                timeline: project_plan.media_request,
                sequence_start,
                sequence_duration,
            },
        })
    }

    fn plan_impl(
        request: &ProjectRenderRequest,
        resolved_assets: bool,
    ) -> Result<ProjectRenderPlan, ProjectRenderError> {
        let composition = opencut_composition::compile(
            &request.snapshot,
            &request.sequence_id,
            CompositionScope::AudioVisual,
        )?;
        Ok(ProjectRenderPlan {
            revision: composition.revision.clone(),
            sequence_id: composition.sequence_id.clone(),
            media_request: bind_composition(
                composition,
                &request.snapshot,
                resolved_assets,
                request.output.clone(),
                request.container,
                request.video_codec,
                request.overwrite,
            )?,
        })
    }

    pub fn execute(
        plan: &ProjectRenderPlan,
        backend: &FfmpegBackend,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(ProgressUpdate),
    ) -> Result<ExportResult, ProjectRenderError> {
        backend
            .render_timeline(&plan.media_request, cancel, progress)
            .map_err(Into::into)
    }
}

pub use opencut_composition::{select_project_frame_time, select_project_range};

fn project_time_error(problem: opencut_contracts::EditorError) -> ProjectRenderError {
    let code = match problem.code.as_str() {
        "TIME_OVERFLOW" => "TIME_OVERFLOW",
        _ => "INVALID_ARGUMENT",
    };
    error(code, problem.message)
}

fn error(code: &'static str, message: impl Into<String>) -> ProjectRenderError {
    ProjectRenderError {
        code,
        message: message.into(),
    }
}
fn capability(message: impl Into<String>) -> ProjectRenderError {
    error("CAPABILITY_UNAVAILABLE", message)
}

fn preflight_issue(
    code: impl Into<String>,
    message: impl Into<String>,
    asset_id: Option<String>,
    clip_id: Option<String>,
    cue_id: Option<String>,
) -> PreflightIssue {
    PreflightIssue {
        code: code.into(),
        message: message.into(),
        asset_id,
        clip_id,
        cue_id,
    }
}

fn collect_object_issues(
    sequence: &opencut_contracts::Sequence,
    assets: &HashMap<&str, &Asset>,
    issues: &mut Vec<PreflightIssue>,
) {
    for track in sequence.tracks.iter().filter(|track| track.enabled) {
        if track.kind == TrackKind::Caption {
            for clip in &track.clips {
                push_unique_issue(
                    issues,
                    preflight_issue(
                        "CAPABILITY_UNAVAILABLE",
                        "clip-backed caption tracks are not implemented; use typed caption cues",
                        clip.asset_id.clone(),
                        Some(clip.id.clone()),
                        None,
                    ),
                );
            }
            continue;
        }
        let (track_allowed, clip_allowed, rendered) = match track.kind {
            TrackKind::Video => (&["volume", "opacity"][..], VIDEO_PARAMS, true),
            TrackKind::Graphic => (&["opacity"][..], VISUAL_PARAMS, true),
            TrackKind::Audio if !track.muted => (&["volume"][..], &["volume"][..], true),
            TrackKind::Audio | TrackKind::Caption => (&[][..], &[][..], false),
        };
        if !rendered {
            continue;
        }
        if let Err(problem) = validate_params(&track.params, track_allowed, "track") {
            push_unique_issue(
                issues,
                preflight_issue(problem.code, problem.message, None, None, None),
            );
        }
        for clip in &track.clips {
            let asset_id = clip.asset_id.clone();
            let image = clip
                .asset_id
                .as_deref()
                .and_then(|id| assets.get(id))
                .is_some_and(|asset| asset.kind == AssetKind::Image);
            let clip_allowed = if image { VISUAL_PARAMS } else { clip_allowed };
            for result in [
                validate_params(&clip.params, clip_allowed, "clip"),
                validate_channels(clip, clip_allowed),
            ] {
                if let Err(problem) = result {
                    push_unique_issue(
                        issues,
                        preflight_issue(
                            problem.code,
                            problem.message,
                            asset_id.clone(),
                            Some(clip.id.clone()),
                            None,
                        ),
                    );
                }
            }
            let Some(asset_id_ref) = clip.asset_id.as_deref() else {
                push_unique_issue(
                    issues,
                    preflight_issue(
                        "INVALID_PROJECT",
                        "media clip has no asset",
                        None,
                        Some(clip.id.clone()),
                        None,
                    ),
                );
                continue;
            };
            if let Some(asset) = assets.get(asset_id_ref) {
                let kind_ok = match track.kind {
                    TrackKind::Video => matches!(asset.kind, AssetKind::Video | AssetKind::Image),
                    TrackKind::Graphic => asset.kind == AssetKind::Image,
                    TrackKind::Audio => matches!(asset.kind, AssetKind::Audio | AssetKind::Video),
                    TrackKind::Caption => true,
                };
                if !kind_ok {
                    push_unique_issue(
                        issues,
                        preflight_issue(
                            "INVALID_PROJECT",
                            format!("asset {} has an unsupported kind for this track", asset.id),
                            Some(asset.id.clone()),
                            Some(clip.id.clone()),
                            None,
                        ),
                    );
                }
            }
            if !matches!(track.kind, TrackKind::Graphic) && !image && clip.source_range.is_none() {
                push_unique_issue(
                    issues,
                    preflight_issue(
                        "INVALID_PROJECT",
                        "media clips require source_range",
                        asset_id,
                        Some(clip.id.clone()),
                        None,
                    ),
                );
            }
        }
    }
}

fn probe_required_assets(
    backend: &FfmpegBackend,
    sequence: &opencut_contracts::Sequence,
    assets: &HashMap<&str, &Asset>,
    required_ids: &[String],
    resolved_assets: bool,
    issues: &mut Vec<PreflightIssue>,
) {
    let mut metadata = HashMap::<&str, MediaMetadata>::new();
    for id in required_ids {
        let Some(asset) = assets.get(id.as_str()) else {
            continue;
        };
        let Ok(path) = execution_path(asset, resolved_assets) else {
            continue;
        };
        match backend.probe(&ProbeRequest { input: path }) {
            Ok(probe) => {
                metadata.insert(id, probe);
            }
            Err(problem) => push_unique_issue(
                issues,
                preflight_issue(
                    problem.code(),
                    problem.to_string(),
                    Some(id.clone()),
                    None,
                    None,
                ),
            ),
        }
    }

    for track in sequence.tracks.iter().filter(|track| track.enabled) {
        let required_kind = match track.kind {
            TrackKind::Video | TrackKind::Graphic => Some(StreamKind::Video),
            TrackKind::Audio if !track.muted => Some(StreamKind::Audio),
            TrackKind::Audio | TrackKind::Caption => None,
        };
        let Some(required_kind) = required_kind else {
            continue;
        };
        for clip in &track.clips {
            let Some(asset_id) = clip.asset_id.as_deref() else {
                continue;
            };
            let Some(probe) = metadata.get(asset_id) else {
                continue;
            };
            let stream = probe
                .streams
                .iter()
                .find(|stream| stream.codec_type == required_kind);
            let Some(_) = stream else {
                push_unique_issue(
                    issues,
                    preflight_issue(
                        "MEDIA_STREAM_MISSING",
                        format!(
                            "asset {asset_id} has no {:?} stream required by this track",
                            required_kind
                        ),
                        Some(asset_id.into()),
                        Some(clip.id.clone()),
                        None,
                    ),
                );
                continue;
            };
            // Image clips are looped and their source_range is not consumed by
            // the renderer, so media duration cannot constrain them.
            if matches!(track.kind, TrackKind::Graphic) {
                continue;
            }
            let Some(source_range) = &clip.source_range else {
                continue;
            };
            // Match FfmpegBackend::validate_source_range: format duration is
            // the common reliable boundary, while per-stream duration can be
            // rounded differently for VFR media.
            let Some(duration) = probe.duration else {
                continue;
            };
            let Ok(source_end) =
                opencut_time::range_end(source_range).and_then(|end| RationalTime::try_from(&end))
            else {
                continue;
            };
            let Ok(media_end) = RationalTime::new(duration.ticks, duration.timebase) else {
                continue;
            };
            let Ok(limit) = media_end.checked_add(RationalTime::new(1, 1_000_000).unwrap()) else {
                continue;
            };
            if source_end.cmp_exact(limit).is_gt() {
                push_unique_issue(
                    issues,
                    preflight_issue(
                        "SOURCE_RANGE_OUT_OF_BOUNDS",
                        format!("clip {} source range exceeds asset duration", clip.id),
                        Some(asset_id.into()),
                        Some(clip.id.clone()),
                        None,
                    ),
                );
            }
        }
    }
}

fn push_unique_issue(issues: &mut Vec<PreflightIssue>, issue: PreflightIssue) {
    if !issues.contains(&issue) {
        issues.push(issue);
    }
}

fn local_path(asset: &Asset) -> Result<PathBuf, ProjectRenderError> {
    let raw = asset.uri.strip_prefix("file://").unwrap_or(&asset.uri);
    if asset.uri.contains("://") && !asset.uri.starts_with("file://") {
        return Err(capability(
            "only local paths and file:// asset URIs are supported",
        ));
    }
    if raw.contains('%') {
        return Err(capability("percent-encoded file URIs are not implemented"));
    }
    let path = PathBuf::from(raw);
    if !path.is_file() {
        return Err(error(
            "MEDIA_NOT_FOUND",
            format!("asset {} is not a local file", asset.id),
        ));
    }
    Ok(path)
}

fn execution_path(asset: &Asset, resolved_assets: bool) -> Result<PathBuf, ProjectRenderError> {
    if asset.version.is_some() && !resolved_assets {
        return Err(error(
            "ASSET_VERSION_UNRESOLVED",
            format!(
                "asset {} has an immutable version that requires a verified execution URI",
                asset.id
            ),
        ));
    }
    local_path(asset)
}

impl From<opencut_composition::CompositionError> for ProjectRenderError {
    fn from(value: opencut_composition::CompositionError) -> Self {
        Self {
            code: value.code,
            message: value.message,
        }
    }
}
fn bind_composition(
    composition: Composition,
    snapshot: &ProjectSnapshot,
    resolved_assets: bool,
    output: PathBuf,
    container: Container,
    codec: VideoCodec,
    overwrite: bool,
) -> Result<TimelineExportRequest, ProjectRenderError> {
    let timeline = composition.timeline.try_map_inputs(|source| {
        let asset = snapshot
            .project
            .assets
            .iter()
            .find(|a| a.id == source.asset_id)
            .ok_or_else(|| error("NOT_FOUND", "composition asset does not exist"))?;
        if asset.version != source.version {
            return Err(error(
                "ASSET_VERSION_UNRESOLVED",
                "composition source identity changed",
            ));
        }
        execution_path(asset, resolved_assets)
    })?;
    Ok(TimelineExportRequest {
        output,
        container,
        overwrite,
        video: VideoOutput {
            width: timeline.width,
            height: timeline.height,
            frame_rate: timeline.frame_rate,
            aspect_mode: AspectMode::Fit,
            codec,
            crf: None,
        },
        duration: timeline.duration,
        video_clips: timeline.video_clips,
        image_clips: timeline.image_clips,
        audio_clips: timeline.audio_clips,
        captions: timeline.captions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::{
        CaptionCue, CaptionStyle, Clip, FrameRate as ProjectFrameRate, ProjectDocument, Sequence,
        Track,
    };

    fn project_time(ticks: i64) -> Time {
        Time {
            ticks: ticks.to_string(),
            timebase: 1,
        }
    }

    fn clip(id: &str, asset_id: &str) -> Clip {
        Clip {
            id: id.into(),
            asset_id: Some(asset_id.into()),
            timeline_range: TimeRange {
                start: project_time(0),
                duration: project_time(1),
            },
            source_range: Some(TimeRange {
                start: project_time(0),
                duration: project_time(1),
            }),
            params: Default::default(),
            channels: vec![],
        }
    }

    fn track(id: &str, kind: TrackKind, enabled: bool, muted: bool, clips: Vec<Clip>) -> Track {
        Track {
            id: id.into(),
            name: id.into(),
            kind,
            enabled,
            muted,
            locked: false,
            clips,
            params: Default::default(),
        }
    }

    fn snapshot(
        tracks: Vec<Track>,
        assets: Vec<Asset>,
        caption_cues: Vec<CaptionCue>,
    ) -> ProjectSnapshot {
        ProjectSnapshot {
            revision: "revision-1".into(),
            project: ProjectDocument {
                id: "project-1".into(),
                name: "project".into(),
                schema_version: 1,
                default_sequence_id: "sequence-1".into(),
                sequences: vec![Sequence {
                    id: "sequence-1".into(),
                    name: "sequence".into(),
                    width: 320,
                    height: 180,
                    fps: ProjectFrameRate {
                        numerator: 30,
                        denominator: 1,
                    },
                    tracks,
                    caption_cues,
                }],
                assets,
            },
        }
    }

    fn asset(id: &str, kind: AssetKind, uri: &str) -> Asset {
        Asset {
            id: id.into(),
            name: id.into(),
            kind,
            uri: uri.into(),
            duration: Some(project_time(1)),
            version: None,
            metadata: Default::default(),
        }
    }

    #[test]
    fn required_assets_match_enabled_and_muted_render_semantics() {
        let snapshot = snapshot(
            vec![
                track(
                    "video",
                    TrackKind::Video,
                    true,
                    true,
                    vec![clip("v", "z-video")],
                ),
                track(
                    "audio",
                    TrackKind::Audio,
                    true,
                    false,
                    vec![clip("a", "a-audio")],
                ),
                track(
                    "muted",
                    TrackKind::Audio,
                    true,
                    true,
                    vec![clip("m", "muted-audio")],
                ),
                track(
                    "disabled",
                    TrackKind::Graphic,
                    false,
                    false,
                    vec![clip("d", "disabled-image")],
                ),
                track(
                    "duplicate",
                    TrackKind::Video,
                    true,
                    false,
                    vec![clip("v2", "z-video")],
                ),
            ],
            vec![],
            vec![],
        );
        assert_eq!(
            ProjectRenderAdapter::required_asset_ids(&snapshot, "sequence-1").unwrap(),
            vec!["a-audio".to_string(), "z-video".to_string()]
        );
    }

    #[test]
    fn visual_dependencies_exclude_audio_and_disabled_tracks() {
        let snapshot = snapshot(
            vec![
                track(
                    "video",
                    TrackKind::Video,
                    true,
                    true,
                    vec![clip("v", "video")],
                ),
                track(
                    "graphic",
                    TrackKind::Graphic,
                    true,
                    false,
                    vec![clip("g", "image")],
                ),
                track(
                    "audio",
                    TrackKind::Audio,
                    true,
                    false,
                    vec![clip("a", "audio")],
                ),
                track(
                    "disabled",
                    TrackKind::Video,
                    false,
                    false,
                    vec![clip("d", "disabled")],
                ),
            ],
            vec![],
            vec![],
        );
        assert_eq!(
            ProjectRenderAdapter::required_visual_asset_ids(&snapshot, "sequence-1").unwrap(),
            vec!["image".to_string(), "video".to_string()]
        );
    }

    #[test]
    fn full_plan_rejects_clip_backed_caption_tracks_before_range_can_inherit_it() {
        let snapshot = snapshot(
            vec![track(
                "caption-track",
                TrackKind::Caption,
                true,
                false,
                vec![clip("subtitle-clip", "subtitle-asset")],
            )],
            vec![asset(
                "subtitle-asset",
                AssetKind::Subtitle,
                "/unused/subtitle.srt",
            )],
            vec![],
        );
        let problem = ProjectRenderAdapter::plan(&ProjectRenderRequest {
            snapshot,
            sequence_id: "sequence-1".into(),
            output: PathBuf::from("render.mp4"),
            container: Container::Mp4,
            video_codec: VideoCodec::H264,
            overwrite: false,
        })
        .unwrap_err();
        assert_eq!(problem.code, "CAPABILITY_UNAVAILABLE");
        assert!(problem.message.contains("typed caption cues"));
    }

    #[test]
    fn visual_resolution_never_requests_audio_content() {
        let hash = "a".repeat(64);
        let mut audio = asset("audio", AssetKind::Audio, "/missing/audio.wav");
        audio.version = Some(opencut_contracts::AssetVersion {
            id: format!("av_{hash}"),
            content_hash: hash,
            size: 10,
        });
        let mut snapshot = snapshot(
            vec![track(
                "audio-track",
                TrackKind::Audio,
                true,
                false,
                vec![clip("a", "audio")],
            )],
            vec![audio],
            vec![],
        );
        snapshot.project.schema_version = 2;
        let mut calls = 0;
        let resolved = ProjectRenderAdapter::resolve_visual_assets(snapshot, "sequence-1", |_| {
            calls += 1;
            unreachable!("audio assets are outside visual resolution")
        })
        .unwrap();
        assert_eq!(calls, 0);
        let plan = ProjectRenderAdapter::plan_frame_resolved(&ResolvedProjectFrameRequest {
            resolved: &resolved,
            requested_time: Time {
                ticks: "29".into(),
                timebase: 30,
            },
            alignment: opencut_media::FrameAlignment::Exact,
            output: PathBuf::from("frame.png"),
            overwrite: false,
        })
        .unwrap();
        assert!(plan.media_request.timeline.audio_clips.is_empty());
        assert!(plan.media_request.timeline.video_clips.is_empty());
        assert_eq!(plan.media_request.timeline.duration.ticks, 1);
    }

    #[test]
    fn frame_selection_preserves_fractional_grid_and_half_open_end() {
        let fps = opencut_contracts::FrameRate {
            numerator: 30_000,
            denominator: 1_001,
        };
        let duration = MediaTime {
            ticks: 1,
            timebase: 1,
        };
        let selected = select_project_frame_time(
            &Time {
                ticks: "1001".into(),
                timebase: 30_000,
            },
            &fps,
            opencut_media::FrameAlignment::Exact,
            duration,
        )
        .unwrap();
        assert_eq!(selected.ticks, "1001");
        assert_eq!(selected.timebase, 30_000);
        let non_grid = select_project_frame_time(
            &Time {
                ticks: "1".into(),
                timebase: 1_000,
            },
            &fps,
            opencut_media::FrameAlignment::Exact,
            duration,
        )
        .unwrap_err();
        assert_eq!(non_grid.code, "INVALID_ARGUMENT");
        assert!(non_grid.message.contains("not on the sequence frame grid"));

        let last = select_project_frame_time(
            &Time {
                ticks: "999".into(),
                timebase: 1_000,
            },
            &fps,
            opencut_media::FrameAlignment::Floor,
            duration,
        )
        .unwrap();
        assert!(
            RationalTime::try_from(&last)
                .unwrap()
                .cmp_exact(RationalTime::new(1, 1).unwrap())
                .is_lt()
        );
        assert!(
            select_project_frame_time(
                &Time {
                    ticks: "999".into(),
                    timebase: 1_000,
                },
                &fps,
                opencut_media::FrameAlignment::Ceil,
                duration,
            )
            .is_err()
        );
        assert!(
            select_project_frame_time(
                &project_time(1),
                &fps,
                opencut_media::FrameAlignment::Floor,
                duration,
            )
            .is_err()
        );
    }

    #[test]
    fn range_selection_aligns_fractional_endpoints_without_clamping() {
        let fps = opencut_contracts::FrameRate {
            numerator: 30_000,
            denominator: 1_001,
        };
        let full = MediaTime {
            ticks: 1_001,
            timebase: 1_000,
        };
        let exact = select_project_range(
            &TimeRange {
                start: Time {
                    ticks: "1001".into(),
                    timebase: 30_000,
                },
                duration: Time {
                    ticks: "29029".into(),
                    timebase: 30_000,
                },
            },
            &fps,
            opencut_media::FrameAlignment::Exact,
            full,
        )
        .unwrap();
        assert_eq!(exact.start.ticks, "1001");
        assert_eq!(exact.start.timebase, 30_000);
        assert_eq!(
            RationalTime::try_from(&opencut_time::range_end(&exact).unwrap())
                .unwrap()
                .cmp_exact(RationalTime::new(1_001, 1_000).unwrap()),
            std::cmp::Ordering::Equal
        );

        let non_grid = TimeRange {
            start: Time {
                ticks: "1".into(),
                timebase: 1_000,
            },
            duration: Time {
                ticks: "1".into(),
                timebase: 2,
            },
        };
        assert_eq!(
            select_project_range(&non_grid, &fps, opencut_media::FrameAlignment::Exact, full,)
                .unwrap_err()
                .code,
            "INVALID_ARGUMENT"
        );
        let fractional_full_end = MediaTime {
            ticks: 1,
            timebase: 1,
        };
        let whole_second = TimeRange {
            start: project_time(0),
            duration: project_time(1),
        };
        assert!(
            select_project_range(
                &whole_second,
                &fps,
                opencut_media::FrameAlignment::Floor,
                fractional_full_end,
            )
            .is_ok()
        );
        assert!(
            select_project_range(
                &whole_second,
                &fps,
                opencut_media::FrameAlignment::Ceil,
                fractional_full_end,
            )
            .is_err()
        );
        let subframe = TimeRange {
            start: project_time(0),
            duration: Time {
                ticks: "1".into(),
                timebase: 100_000,
            },
        };
        assert!(
            select_project_range(
                &subframe,
                &fps,
                opencut_media::FrameAlignment::Nearest,
                full,
            )
            .is_err()
        );
        assert!(
            select_project_range(
                &TimeRange {
                    start: project_time(0),
                    duration: project_time(0),
                },
                &fps,
                opencut_media::FrameAlignment::Floor,
                full,
            )
            .is_err()
        );
        assert!(
            select_project_range(
                &TimeRange {
                    start: project_time(-1),
                    duration: project_time(1),
                },
                &fps,
                opencut_media::FrameAlignment::Floor,
                full,
            )
            .is_err()
        );
    }

    #[test]
    fn frame_plan_keeps_split_clip_timing_at_shared_boundary() {
        let directory = tempfile::tempdir().unwrap();
        let visual = directory.path().join("visual.mp4");
        std::fs::write(&visual, b"planning only checks the resolved path").unwrap();
        let mut left = clip("left", "video");
        left.timeline_range.duration = Time {
            ticks: "1".into(),
            timebase: 2,
        };
        left.source_range.as_mut().unwrap().duration = left.timeline_range.duration.clone();
        let mut right = clip("right", "video");
        right.timeline_range.start = Time {
            ticks: "1".into(),
            timebase: 2,
        };
        right.timeline_range.duration = Time {
            ticks: "1".into(),
            timebase: 2,
        };
        right.source_range.as_mut().unwrap().start = Time {
            ticks: "1".into(),
            timebase: 2,
        };
        right.source_range.as_mut().unwrap().duration = right.timeline_range.duration.clone();
        let snapshot = snapshot(
            vec![track(
                "video-track",
                TrackKind::Video,
                true,
                false,
                vec![left, right],
            )],
            vec![asset("video", AssetKind::Video, visual.to_str().unwrap())],
            vec![],
        );
        let resolved = ProjectRenderAdapter::resolve_visual_assets(snapshot, "sequence-1", |_| {
            unreachable!("legacy asset needs no resolver")
        })
        .unwrap();
        let plan = ProjectRenderAdapter::plan_frame_resolved(&ResolvedProjectFrameRequest {
            resolved: &resolved,
            requested_time: Time {
                ticks: "1".into(),
                timebase: 2,
            },
            alignment: opencut_media::FrameAlignment::Exact,
            output: PathBuf::from("frame.png"),
            overwrite: false,
        })
        .unwrap();
        assert_eq!(plan.media_request.timeline.video_clips.len(), 2);
        assert_eq!(
            plan.media_request.timeline.video_clips[0]
                .timeline_duration
                .ticks,
            1
        );
        assert_eq!(
            plan.media_request.timeline.video_clips[1]
                .timeline_start
                .ticks,
            1
        );
        assert_eq!(plan.actual_sequence_time.ticks, "1");
        assert_eq!(plan.actual_sequence_time.timebase, 2);
    }

    #[test]
    fn preflight_collects_multiple_issues_without_mutating_or_writing() {
        let missing_output = std::env::temp_dir().join("opencut-preflight-output");
        let _ = std::fs::remove_file(&missing_output);
        let cue = CaptionCue {
            id: "cue-1".into(),
            timeline_range: TimeRange {
                start: project_time(0),
                duration: project_time(1),
            },
            text: "visible text".into(),
            source_anchor: None,
            style: CaptionStyle {
                font_family: "OpenCut Font That Must Not Exist 4D27C9".into(),
                font_size: 24.0,
                color: "#ffffff".into(),
                background_color: None,
                font_policy: opencut_contracts::FontResolutionPolicy::Strict {},
                max_width: None,
                line_height: None,
                text_align: opencut_contracts::TextAlign::Center,
            },
        };
        let snapshot = snapshot(
            vec![
                track(
                    "video",
                    TrackKind::Video,
                    true,
                    false,
                    vec![clip("missing", "missing")],
                ),
                track(
                    "remote",
                    TrackKind::Audio,
                    true,
                    false,
                    vec![clip("remote", "remote")],
                ),
                track(
                    "unused",
                    TrackKind::Audio,
                    true,
                    true,
                    vec![clip("unused", "unused")],
                ),
            ],
            vec![
                asset(
                    "remote",
                    AssetKind::Audio,
                    "https://example.invalid/media.wav",
                ),
                asset(
                    "unused",
                    AssetKind::Audio,
                    "/definitely/not/present/unused.wav",
                ),
            ],
            vec![cue],
        );
        let before = snapshot.clone();
        let report = ProjectRenderAdapter::preflight(&ProjectPreflightRequest {
            snapshot: snapshot.clone(),
            sequence_id: "sequence-1".into(),
            container: Container::Mp4,
            video_codec: VideoCodec::H264,
        });
        assert_eq!(snapshot, before);
        assert!(!missing_output.exists());
        assert!(!report.ready);
        assert!(report.issues.len() >= 3, "issues: {:?}", report.issues);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "MEDIA_NOT_FOUND"
                    && issue.asset_id.as_deref() == Some("missing"))
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "CAPABILITY_UNAVAILABLE"
                    && issue.asset_id.as_deref() == Some("remote"))
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "CAPABILITY_UNAVAILABLE"
                    && issue.cue_id.as_deref() == Some("cue-1"))
        );
        assert!(
            !report
                .issues
                .iter()
                .any(|issue| issue.asset_id.as_deref() == Some("unused"))
        );
    }

    #[test]
    fn optional_issue_identifiers_serialize_as_null() {
        let issue = preflight_issue("INVALID_PROJECT", "bad input", None, None, None);
        let value = serde_json::to_value(issue).unwrap();
        assert!(value["asset_id"].is_null());
        assert!(value["clip_id"].is_null());
        assert!(value["cue_id"].is_null());
    }

    #[test]
    fn preflight_reports_injected_missing_tool_without_process_environment_changes() {
        let snapshot = snapshot(vec![], vec![], vec![]);
        let report = ProjectRenderAdapter::preflight_with_backend(
            &ProjectPreflightRequest {
                snapshot,
                sequence_id: "sequence-1".into(),
                container: Container::Mp4,
                video_codec: VideoCodec::H264,
            },
            Err(MediaError::ToolUnavailable {
                tool: "ffmpeg".into(),
                detail: "test executable is absent".into(),
            }),
            false,
        );
        assert!(report.issues.iter().any(|issue| {
            issue.code == "TOOL_UNAVAILABLE"
                && issue.message.contains("ffmpeg")
                && issue.message.contains("absent")
        }));
        assert!(!report.ready);
    }

    #[test]
    fn preflight_reports_clip_backed_caption_before_planning() {
        let snapshot = snapshot(
            vec![track(
                "caption-track",
                TrackKind::Caption,
                true,
                false,
                vec![clip("subtitle-clip", "subtitle-asset")],
            )],
            vec![asset(
                "subtitle-asset",
                AssetKind::Subtitle,
                "/unused/subtitle.srt",
            )],
            vec![],
        );
        let report = ProjectRenderAdapter::preflight_with_backend(
            &ProjectPreflightRequest {
                snapshot,
                sequence_id: "sequence-1".into(),
                container: Container::Mp4,
                video_codec: VideoCodec::H264,
            },
            Err(MediaError::ToolUnavailable {
                tool: "ffmpeg".into(),
                detail: "not used by this assertion".into(),
            }),
            false,
        );
        assert!(!report.ready);
        assert!(report.issues.iter().any(|issue| {
            issue.code == "CAPABILITY_UNAVAILABLE"
                && issue.clip_id.as_deref() == Some("subtitle-clip")
                && issue.asset_id.as_deref() == Some("subtitle-asset")
                && issue.message.contains("typed caption cues")
        }));
    }

    #[test]
    fn installed_ffmpeg_reports_h264_encoder_capability() {
        let Ok(backend) = FfmpegBackend::discover() else {
            eprintln!("skipping: ffmpeg/ffprobe unavailable");
            return;
        };
        assert!(backend.supports_video_codec(VideoCodec::H264).unwrap());
    }

    #[test]
    fn source_entry_rejects_only_required_unresolved_asset_versions_without_reading_uri() {
        let directory = tempfile::tempdir().unwrap();
        let execution_path = directory.path().join("verified-execution.mp4");
        std::fs::write(&execution_path, b"plan only needs a resolved regular file").unwrap();
        let mut required = asset(
            "required",
            AssetKind::Video,
            execution_path.to_str().unwrap(),
        );
        let required_hash = "a".repeat(64);
        required.version = Some(opencut_contracts::AssetVersion {
            id: format!("av_{required_hash}"),
            content_hash: required_hash,
            size: 123,
        });
        let mut unused = asset("unused", AssetKind::Video, "/missing/original-unused.mp4");
        let unused_hash = "b".repeat(64);
        unused.version = Some(opencut_contracts::AssetVersion {
            id: format!("av_{unused_hash}"),
            content_hash: unused_hash,
            size: 456,
        });
        let mut snapshot = snapshot(
            vec![track(
                "video",
                TrackKind::Video,
                true,
                false,
                vec![clip("clip", "required")],
            )],
            vec![required, unused],
            vec![],
        );
        snapshot.project.schema_version = 2;
        let preflight = ProjectRenderAdapter::preflight(&ProjectPreflightRequest {
            snapshot: snapshot.clone(),
            sequence_id: "sequence-1".into(),
            container: Container::Mp4,
            video_codec: VideoCodec::H264,
        });
        assert!(preflight.issues.iter().any(|issue| {
            issue.code == "ASSET_VERSION_UNRESOLVED"
                && issue.asset_id.as_deref() == Some("required")
        }));
        assert!(!preflight.issues.iter().any(|issue| {
            issue.asset_id.as_deref() == Some("required") && issue.code == "MEDIA_NOT_FOUND"
        }));
        assert!(
            !preflight
                .issues
                .iter()
                .any(|issue| issue.asset_id.as_deref() == Some("unused"))
        );

        let request = ProjectRenderRequest {
            snapshot: snapshot.clone(),
            sequence_id: "sequence-1".into(),
            output: std::env::temp_dir().join("unresolved-version.mp4"),
            container: Container::Mp4,
            video_codec: VideoCodec::H264,
            overwrite: false,
        };
        assert_eq!(
            ProjectRenderAdapter::plan(&request).unwrap_err().code,
            "ASSET_VERSION_UNRESOLVED"
        );
        assert_eq!(
            ProjectRenderAdapter::resolve_assets(snapshot.clone(), "another-sequence", |_| {
                unreachable!("missing sequence must fail before resolving")
            })
            .unwrap_err()
            .code,
            "NOT_FOUND"
        );
        let mut callbacks = Vec::new();
        let resolved_snapshot =
            ProjectRenderAdapter::resolve_assets(snapshot, "sequence-1", |asset| {
                callbacks.push(asset.id.clone());
                Ok(execution_path.clone())
            })
            .unwrap();
        assert_eq!(callbacks, vec!["required"]);
        assert_eq!(
            resolved_snapshot._source.project.assets[0].version,
            resolved_snapshot.execution.project.assets[0].version
        );
        let resolved = ProjectRenderAdapter::plan_resolved(&ResolvedProjectRenderRequest {
            resolved: &resolved_snapshot,
            output: request.output.clone(),
            container: request.container,
            video_codec: request.video_codec,
            overwrite: request.overwrite,
        })
        .unwrap();
        assert_eq!(resolved.media_request.video_clips[0].input, execution_path);
        assert!(resolved.media_request.video_clips[0].input.is_file());
    }

    #[cfg(unix)]
    #[test]
    fn resolved_non_utf8_paths_are_rejected_without_lossy_source_fallback() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let mut source = snapshot(
            vec![track(
                "video",
                TrackKind::Video,
                true,
                false,
                vec![clip("clip", "asset")],
            )],
            vec![asset(
                "asset",
                AssetKind::Video,
                "legacy-source-must-not-be-used",
            )],
            vec![],
        );
        source.project.schema_version = 2;
        let hash = "a".repeat(64);
        source.project.assets[0].version = Some(opencut_contracts::AssetVersion {
            id: format!("av_{hash}"),
            content_hash: hash,
            size: 1,
        });
        let invalid_path = || PathBuf::from(OsString::from_vec(vec![b'/', b'x', 0xff]));
        let full = ProjectRenderAdapter::resolve_assets(source.clone(), "sequence-1", |_| {
            Ok(invalid_path())
        })
        .unwrap();
        let visual = ProjectRenderAdapter::resolve_visual_assets(source, "sequence-1", |_| {
            Ok(invalid_path())
        })
        .unwrap();
        for (issues, snapshot) in [
            (&full.resolution_issues, &full.execution),
            (&visual.resolution_issues, &visual.execution),
        ] {
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].code, "INVALID_ARGUMENT");
            assert_eq!(issues[0].asset_id.as_deref(), Some("asset"));
            assert_eq!(snapshot.project.assets[0].uri, "opencut-unresolved://asset");
        }
    }

    #[test]
    fn resolved_preflight_collects_each_resolution_error_and_plan_rejects_them() {
        let hash = |value: char| value.to_string().repeat(64);
        let mut first = asset("first", AssetKind::Video, "/original/first.mp4");
        first.version = Some(opencut_contracts::AssetVersion {
            id: format!("av_{}", hash('c')),
            content_hash: hash('c'),
            size: 1,
        });
        let mut second = asset("second", AssetKind::Video, "/original/second.mp4");
        second.version = Some(opencut_contracts::AssetVersion {
            id: format!("av_{}", hash('d')),
            content_hash: hash('d'),
            size: 2,
        });
        let mut snapshot = snapshot(
            vec![
                track(
                    "video-1",
                    TrackKind::Video,
                    true,
                    false,
                    vec![clip("first-clip", "first")],
                ),
                track(
                    "video-2",
                    TrackKind::Video,
                    true,
                    false,
                    vec![clip("second-clip", "second")],
                ),
            ],
            vec![first, second],
            vec![],
        );
        snapshot.project.schema_version = 2;
        let resolved = ProjectRenderAdapter::resolve_assets(snapshot, "sequence-1", |asset| {
            Err(opencut_contracts::EditorError::new(
                "ASSET_INTEGRITY_ERROR",
                format!("{} is corrupt", asset.id),
            ))
        })
        .unwrap();
        assert_eq!(resolved.resolution_issues.len(), 2);
        let report = ProjectRenderAdapter::preflight_resolved(&ResolvedProjectPreflightRequest {
            resolved: &resolved,
            container: Container::Mp4,
            video_codec: VideoCodec::H264,
        });
        assert!(!report.ready);
        for id in ["first", "second"] {
            assert!(report.issues.iter().any(|issue| {
                issue.code == "ASSET_INTEGRITY_ERROR" && issue.asset_id.as_deref() == Some(id)
            }));
        }
        assert_eq!(
            ProjectRenderAdapter::plan_resolved(&ResolvedProjectRenderRequest {
                resolved: &resolved,
                output: std::env::temp_dir().join("resolution-failed.mp4"),
                container: Container::Mp4,
                video_codec: VideoCodec::H264,
                overwrite: false,
            })
            .unwrap_err()
            .code,
            "ASSET_RESOLUTION_FAILED"
        );
    }

    #[test]
    fn preflight_probes_corrupt_media_and_rejects_declared_video_without_video_stream() {
        let Ok(_backend) = FfmpegBackend::discover() else {
            eprintln!("skipping: ffmpeg/ffprobe unavailable");
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let corrupt = temp.path().join("corrupt.mp4");
        let audio_only = temp.path().join("audio-only.wav");
        std::fs::write(&corrupt, b"not a media container").unwrap();
        let ffmpeg = if std::path::Path::new("/usr/local/bin/ffmpeg").is_file() {
            "/usr/local/bin/ffmpeg"
        } else {
            "ffmpeg"
        };
        assert!(
            std::process::Command::new(ffmpeg)
                .args(["-v", "error", "-f", "lavfi", "-i", "sine=d=1", "-y"])
                .arg(&audio_only)
                .status()
                .unwrap()
                .success()
        );
        let snapshot = snapshot(
            vec![track(
                "video",
                TrackKind::Video,
                true,
                false,
                vec![clip("corrupt-clip", "corrupt"), clip("audio-clip", "audio")],
            )],
            vec![
                asset("corrupt", AssetKind::Video, corrupt.to_str().unwrap()),
                asset("audio", AssetKind::Video, audio_only.to_str().unwrap()),
            ],
            vec![],
        );
        let report = ProjectRenderAdapter::preflight(&ProjectPreflightRequest {
            snapshot,
            sequence_id: "sequence-1".into(),
            container: Container::Mp4,
            video_codec: VideoCodec::H264,
        });
        assert!(!report.ready);
        assert!(report.issues.iter().any(|issue| {
            issue.code == "MEDIA_PROCESS_FAILED" && issue.asset_id.as_deref() == Some("corrupt")
        }));
        assert!(report.issues.iter().any(|issue| {
            issue.code == "MEDIA_STREAM_MISSING"
                && issue.asset_id.as_deref() == Some("audio")
                && issue.clip_id.as_deref() == Some("audio-clip")
        }));
    }
}
