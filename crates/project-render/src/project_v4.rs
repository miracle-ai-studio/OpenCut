//! Native per-frame execution of the sampled IR4 graph. No project schema
//! projection, public paths or range/audio completion claims are made here.
use crate::{ProjectRenderError, error, project_time_error};
use opencut_composition::project_v4::*;
use opencut_contracts::{EditorError, Time, project_v4::*};
use opencut_media::*;
use std::{collections::HashMap, path::PathBuf};

mod execution;
mod lowering;
#[cfg(test)]
mod tests;

/// Constructed only by a host resolver that verifies immutable CAS bytes and
/// enforces its processing policy. Paths never enter the portable IR.
#[derive(Clone, Debug)]
pub struct ResolvedFrameV4 {
    graph: CompositionV4,
    resources: HashMap<String, PathBuf>,
}

pub struct FrameExecutionV4 {
    pub output: PathBuf,
    /// Host-owned scratch root; only a newly created child is removed.
    pub scratch_root: PathBuf,
    pub overwrite: bool,
    /// Explicit source PTS selection. This is not a declaration of Classic's
    /// frame-rounding profile; the result records selected source PTS.
    pub source_alignment: FrameAlignment,
}

#[derive(Clone, Debug)]
pub struct FrameResultV4 {
    pub revision: String,
    pub actual_sequence_time: Time,
    pub frame: RenderedTimelineFrame,
    pub decoded_sources: Vec<DecodedSourceV4>,
}

#[derive(Clone, Debug)]
pub struct DecodedSourceV4 {
    pub element_id: String,
    pub asset_id: String,
    pub requested_time: Time,
    pub actual_time: MediaTime,
    pub source_pts: i64,
    pub source_time_base: FrameRate,
    pub exact: bool,
}

pub struct ProjectFrameAdapterV4;
impl ProjectFrameAdapterV4 {
    pub fn resolve(
        graph: CompositionV4,
        mut resolver: impl FnMut(&CasResourceV4) -> Result<PathBuf, EditorError>,
    ) -> Result<ResolvedFrameV4, ProjectRenderError> {
        // Reject unsupported semantics before resolving or starting processes.
        lowering::validate(&graph)?;
        let mut resources = HashMap::new();
        for resource in &graph.resources {
            if !lowering::visual_resource_used(&graph, &resource.asset_id) {
                continue;
            }
            if resources.contains_key(&resource.asset_id) {
                return Err(error("INVALID_PROJECT", "Duplicate IR4 resource identity"));
            }
            let path = resolver(resource).map_err(resolution_error)?;
            if !path.is_absolute() || !path.is_file() {
                return Err(error(
                    "ASSET_RESOLUTION_FAILED",
                    "CAS resolver must return an absolute existing file",
                ));
            }
            resources.insert(resource.asset_id.clone(), path);
        }
        for layer in &graph.layers {
            if let Some(id) = lowering::asset_id(layer)
                && !resources.contains_key(id)
            {
                return Err(error(
                    "ASSET_RESOLUTION_FAILED",
                    format!("Missing IR4 resource {id}"),
                ));
            }
        }
        Ok(ResolvedFrameV4 { graph, resources })
    }

    pub fn preflight_resolved(resolved: &ResolvedFrameV4) -> Result<(), ProjectRenderError> {
        lowering::validate(&resolved.graph)
    }

    /// Tool availability probe, separate from cancellable rendering. The old
    /// backend filter inventory currently owns its bounded probe token.
    pub fn preflight_native(
        backend: &FfmpegBackend,
        resolved: &ResolvedFrameV4,
    ) -> Result<(), ProjectRenderError> {
        Self::preflight_resolved(resolved)?;
        execution::check_filters(backend, &resolved.graph)
    }

    pub fn render_resolved(
        backend: &FfmpegBackend,
        resolved: &ResolvedFrameV4,
        request: &FrameExecutionV4,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(ProgressUpdate),
    ) -> Result<FrameResultV4, ProjectRenderError> {
        execution::render(backend, resolved, request, cancel, progress)
    }
}

fn unsupported(node: &str, field: &str) -> ProjectRenderError {
    error(
        "UNSUPPORTED_V4",
        format!("IR4 node {node}: native frame execution does not support {field}"),
    )
}
fn media_time(time: &Time) -> Result<MediaTime, ProjectRenderError> {
    let rational = opencut_time::RationalTime::try_from(time).map_err(project_time_error)?;
    Ok(MediaTime {
        ticks: rational.ticks(),
        timebase: rational.timebase(),
    })
}

fn resolution_error(problem: EditorError) -> ProjectRenderError {
    let code = match problem.code.as_str() {
        "NOT_AUTHORIZED" => "NOT_AUTHORIZED",
        "POLICY_DENIED" => "POLICY_DENIED",
        "ASSET_INTEGRITY_ERROR" => "ASSET_INTEGRITY_ERROR",
        "ASSET_IO_ERROR" => "ASSET_IO_ERROR",
        "INVALID_ARGUMENT" => "INVALID_ARGUMENT",
        _ => "ASSET_RESOLUTION_FAILED",
    };
    error(code, problem.message)
}
