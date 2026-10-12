use super::*;
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn create(root: &Path) -> Result<Self, ProjectRenderError> {
        fs::create_dir_all(root).map_err(MediaError::from)?;
        for _ in 0..32 {
            let path = root.join(format!(
                "ir4-frame-{}-{}",
                std::process::id(),
                NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(MediaError::from(e).into()),
            }
        }
        Err(error(
            "IO_ERROR",
            "Could not allocate exclusive IR4 scratch directory",
        ))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) fn render(
    backend: &FfmpegBackend,
    resolved: &ResolvedFrameV4,
    request: &FrameExecutionV4,
    cancel: &CancellationToken,
    progress: &mut dyn FnMut(ProgressUpdate),
) -> Result<FrameResultV4, ProjectRenderError> {
    lowering::validate(&resolved.graph)?;
    if cancel.is_cancelled() {
        return Err(MediaError::Cancelled.into());
    }
    if !request.output.is_absolute() || !request.scratch_root.is_absolute() {
        return Err(error(
            "INVALID_ARGUMENT",
            "Native IR4 output and scratch root must be absolute",
        ));
    }
    // Decoded PNGs hide their original input from the existing backend's
    // output-alias checks. Check original CAS inputs before creating scratch.
    if request.output.exists() {
        let output = fs::canonicalize(&request.output).map_err(MediaError::from)?;
        for input in resolved.resources.values() {
            if fs::canonicalize(input).map_err(MediaError::from)? == output
                || same_file(input, &request.output)?
            {
                return Err(error(
                    "INVALID_ARGUMENT",
                    "Frame output aliases immutable CAS input",
                ));
            }
        }
        if !request.overwrite {
            return Err(error("INVALID_ARGUMENT", "Frame output already exists"));
        }
    }
    let scratch = Scratch::create(&request.scratch_root)?;
    let parent = request
        .output
        .parent()
        .ok_or_else(|| error("INVALID_ARGUMENT", "Frame output requires parent directory"))?;
    fs::create_dir_all(parent).map_err(MediaError::from)?;
    if fs::canonicalize(parent)
        .map_err(MediaError::from)?
        .starts_with(fs::canonicalize(&scratch.0).map_err(MediaError::from)?)
    {
        return Err(error(
            "INVALID_ARGUMENT",
            "Frame output must be outside temporary decode directory",
        ));
    }
    let mut inputs = HashMap::new();
    let mut decoded_sources = vec![];
    for (index, layer) in resolved.graph.layers.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled.into());
        }
        match &layer.content {
            SampledContentV4::Image { asset_id } => {
                let metadata = backend.probe_cancellable(
                    &ProbeRequest {
                        input: resolved.resources[asset_id].clone(),
                    },
                    cancel,
                )?;
                if !matches!(
                    metadata.format_name.as_deref(),
                    Some("png_pipe" | "jpeg_pipe" | "ppm_pipe" | "bmp_pipe")
                ) {
                    return Err(unsupported(
                        &layer.element_id,
                        "animated or unverified still-image container",
                    ));
                }
                inputs.insert(
                    layer.element_id.clone(),
                    resolved.resources[asset_id].clone(),
                );
            }
            SampledContentV4::Video {
                asset_id,
                stream,
                source,
                ..
            } => {
                let decoded = backend.extract_frame(
                    &FrameExtractRequest {
                        input: resolved.resources[asset_id].clone(),
                        output: scratch.0.join(format!("source-{index}.png")),
                        requested_time: media_time(&source.source_time)?,
                        video_stream: *stream,
                        alignment: request.source_alignment,
                        overwrite: false,
                    },
                    cancel,
                    progress,
                )?;
                inputs.insert(layer.element_id.clone(), decoded.output);
                decoded_sources.push(DecodedSourceV4 {
                    element_id: layer.element_id.clone(),
                    asset_id: asset_id.clone(),
                    requested_time: source.source_time.clone(),
                    actual_time: decoded.actual_time,
                    source_pts: decoded.source_pts,
                    source_time_base: decoded.source_time_base,
                    exact: decoded.exact,
                });
            }
            _ => {}
        }
    }
    let plan = lowering::lower(
        &resolved.graph,
        &inputs,
        request.output.clone(),
        request.overwrite,
    )?;
    let mut frame = backend.render_timeline_frame_v3(&plan, cancel, progress)?;
    // Native graph zero is an execution-local clock. Presentation time remains
    // the exact IR4 rational, including non-frame-aligned authoring requests.
    frame.actual_sequence_time = media_time(&resolved.graph.time)?;
    Ok(FrameResultV4 {
        revision: resolved.graph.revision.clone(),
        actual_sequence_time: resolved.graph.time.clone(),
        frame,
        decoded_sources,
    })
}

pub(super) fn check_filters(
    backend: &FfmpegBackend,
    graph: &CompositionV4,
) -> Result<(), ProjectRenderError> {
    let mut filters = vec!["geq", "overlay", "color", "fps", "pad"];
    for style in graph.layers.iter().filter_map(|l| l.visual.as_ref()) {
        for effect in style.effects.iter().filter(|e| e.enabled) {
            match effect.effect {
                EffectKindV4::LegacyColorAdjust { .. } => filters.push("eq"),
                EffectKindV4::LegacyGaussianBlur { .. } => filters.push("gblur"),
                _ => {}
            }
        }
    }
    filters.sort_unstable();
    filters.dedup();
    let missing = backend.missing_v3_filters(&filters)?;
    if !missing.is_empty() {
        return Err(unsupported(
            "frame",
            &format!("missing FFmpeg filters: {}", missing.join(", ")),
        ));
    }
    Ok(())
}
fn same_file(first: &Path, second: &Path) -> Result<bool, ProjectRenderError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let a = fs::metadata(first).map_err(MediaError::from)?;
        let b = fs::metadata(second).map_err(MediaError::from)?;
        Ok(a.dev() == b.dev() && a.ino() == b.ino())
    }
    #[cfg(not(unix))]
    {
        Ok(fs::canonicalize(first).map_err(MediaError::from)?
            == fs::canonicalize(second).map_err(MediaError::from)?)
    }
}
