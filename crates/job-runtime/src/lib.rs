#[cfg(unix)]
mod media_output;
use opencut_job_execution::{
    PreparedProjectFrameInput, PreparedProjectRangeInput, PreparedProjectRenderInput,
};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use opencut_asset_store::{AssetLockManifest, AssetStore};
use opencut_contracts::{EditorError, ProjectSnapshot};
use opencut_job_contracts::{
    ArtifactDescriptor, AssetLockingMode, CompositedProjectFrameJobResult,
    CompositedProjectFrameQuality, ExportJobInput, FrameJobInput, MediaExportJobResult,
    MediaFrameJobResult, ProjectFrameJobResult, ProjectFrameQuality, ProjectRangeJobResult,
    ProjectRangeQuality, ProjectRenderJobResult,
};
use opencut_media::{
    CancellationToken, Container, ExportRequest, FfmpegBackend, FrameAlignment,
    FrameExtractRequest, MediaTime, ProbeRequest, ProgressStage, ProgressUpdate, VerifyRequest,
    VideoCodec,
};
use opencut_project_render::{
    ProjectRenderAdapter, ResolvedProjectFrameRequest, ResolvedProjectRangeRequest,
    ResolvedProjectRenderRequest, ResolvedProjectSnapshot, ResolvedVisualProjectSnapshot,
    select_project_frame_time,
};
use opencut_sqlite_store::{EditorApplication, JobProgress, JobRecord, JobState};
use opencut_time::RationalTime;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const LEASE_MS: i64 = 30_000;

pub fn run(database: &Path, once: bool, stop: Arc<AtomicBool>) -> Result<(), EditorError> {
    let mut app = EditorApplication::open(database)?;
    let worker = format!("worker_{}", uuid::Uuid::new_v4().simple());
    let output_root = database
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("artifacts");
    let mut initial_cleanup = true;
    loop {
        if stop.load(Ordering::SeqCst) {
            return Ok(());
        }
        let recovered = app.live_jobs().recover()?;
        if initial_cleanup || recovered > 0 {
            if let Err(error) = clean_abandoned_inputs(&app, &output_root) {
                eprintln!("Could not clean abandoned private inputs: {error}");
            }
            initial_cleanup = false;
        }
        let claimed = app.live_jobs().claim(&worker, LEASE_MS)?;
        if let Some(job) = claimed {
            let result = execute_monitored(database, &output_root, &worker, &job, stop.clone());
            let state = app.get_job(&job.job_id)?;
            // Host shutdown abandons the lease; recovery retries the immutable input.
            // A user cancellation, in contrast, must reach the cancelled state.
            if stop.load(Ordering::SeqCst) && !state.cancel_requested {
                return Ok(());
            }
            if state.lease_owner.as_deref() != Some(&worker)
                || state.attempt != job.attempt
                || state.state != JobState::Running
            {
                eprintln!(
                    "Job {} attempt {} lost its lease; private output was not published",
                    job.job_id, job.attempt
                );
            } else {
                let completion = match result {
                    Ok(result) => {
                        app.live_jobs()
                            .complete(&job.job_id, &worker, job.attempt, result)
                    }
                    Err(error) if state.cancel_requested => app
                        .live_jobs()
                        .finish_cancelled(&job.job_id, &worker, job.attempt)
                        .inspect_err(|_| {
                            eprintln!("Task cancellation: {error}");
                        }),
                    Err(error) => {
                        app.live_jobs()
                            .fail(&job.job_id, &worker, job.attempt, json!(error))
                    }
                };
                if let Err(error) = completion {
                    eprintln!("Could not publish job {}: {error}", job.job_id);
                }
            }
            if once {
                return Ok(());
            }
        } else {
            if once {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

fn execute_monitored(
    database: &Path,
    root: &Path,
    worker: &str,
    job: &JobRecord,
    stop: Arc<AtomicBool>,
) -> Result<Value, EditorError> {
    let cancel = CancellationToken::new();
    let done = Arc::new(AtomicBool::new(false));
    let monitor_app = EditorApplication::open(database)?;
    let mut progress_app = EditorApplication::open(database)?;
    std::thread::scope(|scope| {
        let monitor_cancel = cancel.clone();
        let monitor_done = done.clone();
        scope.spawn(move || {
            let mut app = monitor_app;
            while !monitor_done.load(Ordering::SeqCst) {
                if stop.load(Ordering::SeqCst) {
                    monitor_cancel.cancel();
                    break;
                }
                match app
                    .live_jobs()
                    .heartbeat(&job.job_id, worker, job.attempt, LEASE_MS)
                {
                    Ok(state) if state.cancel_requested => {
                        monitor_cancel.cancel();
                        break;
                    }
                    Err(_) => {
                        monitor_cancel.cancel();
                        break;
                    }
                    _ => {}
                }
                for _ in 0..5 {
                    if monitor_done.load(Ordering::SeqCst) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        });
        let mut reporter = ProgressReporter::new(&mut progress_app, worker, job, &cancel);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            execute(root, job, &cancel, &mut |update| reporter.report(update))
        }));
        done.store(true, Ordering::SeqCst);
        if let Some(error) = reporter.error {
            Err(error)
        } else {
            outcome.unwrap_or_else(|_| {
                Err(EditorError::new("INTERNAL_ERROR", "Media worker panicked"))
            })
        }
    })
}

/// The fraction belongs to the reported stage, not to total job completion.
/// Stage changes and the stage's final sample bypass the write-rate limit.
struct ProgressReporter<'a> {
    app: &'a mut EditorApplication,
    worker: &'a str,
    job: &'a JobRecord,
    cancel: &'a CancellationToken,
    last: Option<(JobProgress, Instant)>,
    error: Option<EditorError>,
}

impl<'a> ProgressReporter<'a> {
    fn new(
        app: &'a mut EditorApplication,
        worker: &'a str,
        job: &'a JobRecord,
        cancel: &'a CancellationToken,
    ) -> Self {
        Self {
            app,
            worker,
            job,
            cancel,
            last: None,
            error: None,
        }
    }

    fn report(&mut self, update: ProgressUpdate) {
        self.report_at(update, Instant::now());
    }

    fn report_at(&mut self, update: ProgressUpdate, observed_at: Instant) {
        if self.error.is_some() || self.cancel.is_cancelled() {
            return;
        }
        let progress = JobProgress {
            stage: match update.stage {
                ProgressStage::ResolvingAssets => "resolving_assets",
                ProgressStage::Probing => "probing",
                ProgressStage::SelectingFrame => "selecting_frame",
                ProgressStage::Decoding => "decoding",
                ProgressStage::Encoding => "encoding",
                ProgressStage::Verifying => "verifying",
            }
            .into(),
            fraction: update.fraction.map(f64::from),
            media_time: update.media_time.map(|time| opencut_contracts::Time {
                ticks: time.ticks.to_string(),
                timebase: time.timebase,
            }),
        };
        if let Some((last, at)) = &self.last {
            if last == &progress {
                return;
            }
            if last.stage == progress.stage
                && !(progress.fraction == Some(1.0) && last.fraction != Some(1.0))
                && observed_at.saturating_duration_since(*at) < Duration::from_millis(250)
            {
                return;
            }
        }
        match self.app.live_jobs().progress(
            &self.job.job_id,
            self.worker,
            self.job.attempt,
            progress.clone(),
        ) {
            Ok(state) => {
                self.last = Some((progress, observed_at));
                if state.cancel_requested {
                    self.cancel.cancel();
                }
            }
            Err(error) => {
                // Do not keep rendering or publish a successful result after losing
                // the lease or failing to durably report the attempt's state.
                self.cancel.cancel();
                self.error = Some(error);
            }
        }
    }
}

struct PrivateInputs(PathBuf);
impl Drop for PrivateInputs {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Attempts increase monotonically. A queued attempt can only be claimed under
/// a NEW number, so deleting its old inputs remains safe if another worker claims
/// concurrently. Never remove current running inputs or public artifacts.
fn clean_abandoned_inputs(app: &EditorApplication, root: &Path) -> Result<(), EditorError> {
    let io = |e: std::io::Error| EditorError::new("PRIVATE_INPUT_CLEANUP_FAILED", e.to_string());
    match fs::symlink_metadata(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io(error)),
        Ok(metadata) if !metadata.is_dir() => {
            return Err(EditorError::new(
                "PRIVATE_INPUT_CLEANUP_FAILED",
                "Artifact root must be a real directory",
            ));
        }
        Ok(_) => (),
    }
    for job_directory in fs::read_dir(root).map_err(io)? {
        let job_directory = job_directory.map_err(io)?;
        if !job_directory.file_type().map_err(io)?.is_dir() {
            continue;
        }
        let Ok(job) = app.get_job(&job_directory.file_name().to_string_lossy()) else {
            continue;
        };
        for attempt_directory in fs::read_dir(job_directory.path()).map_err(io)? {
            let attempt_directory = attempt_directory.map_err(io)?;
            if !attempt_directory.file_type().map_err(io)?.is_dir() {
                continue;
            }
            let name = attempt_directory.file_name();
            let Some(attempt) = name
                .to_str()
                .and_then(|name| name.strip_prefix("attempt-"))
                .and_then(|n| n.parse::<u32>().ok())
            else {
                continue;
            };
            if attempt < job.attempt || (attempt == job.attempt && job.state != JobState::Running) {
                #[cfg(unix)]
                let _cleanup = {
                    let Some(guard) = opencut_storage_budget::AttemptLease::try_cleanup(
                        &attempt_directory.path(),
                    )
                    .map_err(io)?
                    else {
                        continue;
                    };
                    if !guard.still_names(&attempt_directory.path()).map_err(io)? {
                        continue;
                    }
                    guard
                };
                let published = attempt == job.attempt
                    && job.state == JobState::Succeeded
                    && job
                        .result
                        .as_ref()
                        .and_then(|result| result.pointer("/artifact/path"))
                        .and_then(Value::as_str)
                        .is_some_and(|path| Path::new(path).starts_with(attempt_directory.path()));
                // An expired or terminal unsuccessful attempt has no published
                // references and is entirely private. A successful attempt keeps
                // only the directory containing its referenced public artifact.
                if !published {
                    fs::remove_dir_all(attempt_directory.path()).map_err(io)?;
                    continue;
                }
                let inputs = attempt_directory.path().join("inputs");
                match fs::symlink_metadata(&inputs) {
                    Ok(metadata) if metadata.is_dir() => {
                        fs::remove_dir_all(inputs).map_err(io)?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Ok(_) => {}
                    Err(error) => return Err(io(error)),
                }
            }
        }
    }
    Ok(())
}

fn resolve_inputs(
    root: &Path,
    directory: &Path,
    snapshot: &ProjectSnapshot,
    sequence_id: &str,
    lock: Option<&AssetLockManifest>,
    cancel: &CancellationToken,
    progress: &mut dyn FnMut(ProgressUpdate),
) -> Result<ResolvedProjectSnapshot, EditorError> {
    let verified = prepare_inputs(
        root,
        directory,
        snapshot,
        InputScope::Full(sequence_id),
        lock,
        cancel,
        progress,
    )?;
    resolve_verified_inputs(verified, sequence_id)
}

fn resolve_visual_inputs(
    root: &Path,
    directory: &Path,
    snapshot: &ProjectSnapshot,
    sequence_id: &str,
    lock: Option<&AssetLockManifest>,
    cancel: &CancellationToken,
    progress: &mut dyn FnMut(ProgressUpdate),
) -> Result<ResolvedVisualProjectSnapshot, EditorError> {
    let verified = prepare_inputs(
        root,
        directory,
        snapshot,
        InputScope::VisualFrame(sequence_id),
        lock,
        cancel,
        progress,
    )?;
    ProjectRenderAdapter::resolve_visual_assets(verified, sequence_id, |asset| {
        Ok(PathBuf::from(&asset.uri))
    })
    .map_err(|error| EditorError::new(error.code, error.message))
}

enum InputScope<'a> {
    Full(&'a str),
    VisualFrame(&'a str),
}

/// The private preparation path verifies exactly the dependency set selected by
/// the job kind. Callers wrap these bytes in a purpose-specific resolved type.
fn prepare_inputs(
    root: &Path,
    directory: &Path,
    snapshot: &ProjectSnapshot,
    scope: InputScope<'_>,
    lock: Option<&AssetLockManifest>,
    cancel: &CancellationToken,
    progress: &mut dyn FnMut(ProgressUpdate),
) -> Result<ProjectSnapshot, EditorError> {
    let (sequence_id, ids) = match scope {
        InputScope::Full(sequence_id) => (
            sequence_id,
            ProjectRenderAdapter::required_asset_ids(snapshot, sequence_id),
        ),
        InputScope::VisualFrame(sequence_id) => (
            sequence_id,
            ProjectRenderAdapter::required_visual_asset_ids(snapshot, sequence_id),
        ),
    };
    let ids = ids.map_err(|error| EditorError::new(error.code, error.message))?;
    let Some(lock) = lock else {
        if snapshot
            .project
            .assets
            .iter()
            .any(|asset| asset.version.is_some() && ids.contains(&asset.id))
        {
            return Err(EditorError::new(
                "ASSET_LOCK_REQUIRED",
                "A versioned project job requires a verified media lock",
            ));
        }
        return Ok(snapshot.clone());
    };
    progress(ProgressUpdate {
        stage: ProgressStage::ResolvingAssets,
        fraction: None,
        media_time: None,
    });
    let store = AssetStore::open_managed(
        root.parent()
            .unwrap_or_else(|| Path::new("."))
            .join("asset-store"),
    )?;
    store.materialize(snapshot, sequence_id, lock, &ids, directory, &|| {
        cancel.is_cancelled()
    })
}

/// The caller supplies legacy unversioned inputs, or private copies whose lock
/// and bytes were checked by AssetStore. Resolving preserves version provenance.
fn resolve_verified_inputs(
    verified: ProjectSnapshot,
    sequence_id: &str,
) -> Result<ResolvedProjectSnapshot, EditorError> {
    ProjectRenderAdapter::resolve_assets(verified, sequence_id, |asset| {
        Ok(PathBuf::from(&asset.uri))
    })
    .map_err(|error| EditorError::new(error.code, error.message))
}

/// Also runs while unwinding a backend panic, before execute_monitored turns it
/// into a failed job. Only a successful frame survives this private workspace.
struct PreviewFiles {
    intermediate: Option<PathBuf>,
    output: PathBuf,
    keep_output: bool,
}

impl Drop for PreviewFiles {
    fn drop(&mut self) {
        if let Some(intermediate) = &self.intermediate {
            let _ = fs::remove_file(intermediate);
        }
        if !self.keep_output {
            let _ = fs::remove_file(&self.output);
        }
    }
}

fn execute(
    root: &Path,
    job: &JobRecord,
    cancel: &CancellationToken,
    progress: &mut dyn FnMut(ProgressUpdate),
) -> Result<Value, EditorError> {
    progress(ProgressUpdate {
        stage: ProgressStage::Probing,
        fraction: None,
        media_time: None,
    });
    if cancel.is_cancelled() {
        return Err(EditorError::new(
            "JOB_CANCELLED",
            "Job was cancelled before media execution",
        ));
    }
    let directory = root
        .join(&job.job_id)
        .join(format!("attempt-{}", job.attempt));
    for path in [
        root.to_path_buf(),
        root.join(&job.job_id),
        directory.clone(),
    ] {
        fs::create_dir_all(&path).map_err(io_error)?;
        if !fs::symlink_metadata(&path).map_err(io_error)?.is_dir() {
            return Err(EditorError::new(
                "STORAGE_BUDGET_ERROR",
                "Private attempt roots must be real directories",
            ));
        }
    }
    let directory = fs::canonicalize(directory).map_err(io_error)?;
    let backend = FfmpegBackend::discover().map_err(media_error)?;
    #[cfg(unix)]
    let backend = {
        let attempt =
            opencut_storage_budget::AttemptLease::acquire(&directory).map_err(io_error)?;
        backend.with_output_admission(std::sync::Arc::new(media_output::TaskOutput {
            budget: opencut_storage_budget::Budget::new(
                root.parent().unwrap_or_else(|| Path::new(".")),
            ),
            attempt,
            directory: directory.clone(),
            writer: opencut_editor_application::budget::Writer {
                job_id: job.job_id.clone(),
                attempt: job.attempt,
            },
        }))
    };
    match job.kind.as_str() {
        "media.import" => {
            use opencut_editor_application::uploads::{self, PreparedMediaImportInput};
            let input: PreparedMediaImportInput = parse(job.input.clone())?;
            uploads::validate_manifest(&input)?;
            let private = PrivateInputs(directory.join("inputs"));
            fs::create_dir_all(&private.0).map_err(io_error)?;
            let file = private.0.join("upload.media");
            let workspace = root
                .parent()
                .ok_or_else(|| EditorError::new("INTERNAL_ERROR", "Missing workspace root"))?;
            opencut_asset_store::uploads::UploadChunks::managed(workspace.join("uploads"))
                .assemble(&input, &file, &|| cancel.is_cancelled())?;
            let metadata = backend.probe_upload(&file, cancel).map_err(media_error)?;
            let mut asset = opencut_editor_application::media::describe(
                &metadata,
                &public_path(file.clone())?,
                &input.name,
                &input.content_hash,
                input.size_bytes,
            )?;
            asset.id = input.media_id.clone();
            // This project resource does not grant access to the owner-only catalog.
            asset.metadata.remove("media_url");
            if cancel.is_cancelled() {
                return Err(EditorError::new("JOB_CANCELLED", "Import was cancelled"));
            }
            let mut asset =
                AssetStore::open_managed(workspace.join("asset-store"))?.pin_asset(&asset)?;
            asset.uri = format!("opencut-media://{}", input.media_id);
            let result = opencut_job_contracts::MediaImportJobResult {
                media_id: input.media_id.clone(),
                project_id: input.project_id.clone(),
                upload_id: input.upload_id.clone(),
                job_id: job.job_id.clone(),
                asset,
            };
            uploads::validate_import_result(&input, &job.job_id, &result)?;
            Ok(json!(result))
        }
        "media.probe" => {
            let request: ProbeRequest = parse(job.input.clone())?;
            Ok(json!(backend.probe(&request).map_err(media_error)?))
        }
        "preview.frame" => {
            let request: FrameJobInput = parse(job.input.clone())?;
            let output = directory.join("frame.png");
            let result = backend
                .extract_frame(
                    &FrameExtractRequest {
                        input: request.input.into(),
                        output: output.clone(),
                        requested_time: request.requested_time,
                        video_stream: request.video_stream,
                        alignment: request.alignment,
                        overwrite: false,
                    },
                    cancel,
                    progress,
                )
                .map_err(media_error)?;
            Ok(json!(MediaFrameJobResult {
                media: result.try_map_paths(public_path)?,
                artifact: artifact(&output, "image/png")?,
            }))
        }
        "preview.project_frame" => {
            let input: PreparedProjectFrameInput = parse(job.input.clone())?;
            let private_inputs = PrivateInputs(directory.join("inputs"));
            let snapshot = resolve_inputs(
                root,
                &private_inputs.0,
                &input.snapshot,
                &input.sequence_id,
                input.asset_lock.as_ref(),
                cancel,
                progress,
            )?;
            let sequence = input
                .snapshot
                .project
                .sequences
                .iter()
                .find(|sequence| sequence.id == input.sequence_id)
                .ok_or_else(|| {
                    EditorError::new("NOT_FOUND", "Sequence does not exist in the fixed revision")
                })?;
            let fps = sequence.fps.clone();
            let (width, height) = (sequence.width, sequence.height);
            let revision = input.snapshot.revision.clone();
            let output = directory.join("frame.png");
            // This first project preview uses the exact same composition/export
            // path as a full render. It is deliberately a job, not a real-time
            // seek API. The intermediate encoded movie is never published.
            let intermediate = directory.join("preview-source.mp4");
            let mut private_files = PreviewFiles {
                intermediate: Some(intermediate.clone()),
                output: output.clone(),
                keep_output: false,
            };
            let plan = ProjectRenderAdapter::plan_resolved(&ResolvedProjectRenderRequest {
                resolved: &snapshot,
                output: intermediate.clone(),
                container: Container::Mp4,
                video_codec: VideoCodec::H264,
                overwrite: false,
            })
            .map_err(|error| EditorError::new(error.code, error.message))?;
            let actual_time = select_project_frame_time(
                &input.requested_time,
                &fps,
                input.alignment,
                plan.media_request.duration,
            )
            .map_err(|error| EditorError::new(error.code, error.message))?;
            let result = (|| {
                let rendered = ProjectRenderAdapter::execute(&plan, &backend, cancel, progress)
                    .map_err(|error| EditorError::new(error.code, error.message))?;
                let selected = RationalTime::try_from(&actual_time)?;
                let extracted = backend
                    .extract_frame(
                        &FrameExtractRequest {
                            input: intermediate.clone(),
                            output: output.clone(),
                            requested_time: MediaTime {
                                ticks: selected.ticks(),
                                timebase: selected.timebase(),
                            },
                            video_stream: None,
                            alignment: FrameAlignment::Exact,
                            overwrite: false,
                        },
                        cancel,
                        progress,
                    )
                    .map_err(media_error)?;
                let observed =
                    RationalTime::new(extracted.actual_time.ticks, extracted.actual_time.timebase)?;
                if observed.cmp_exact(selected) != std::cmp::Ordering::Equal {
                    return Err(EditorError::new(
                        "ARTIFACT_VERIFICATION_FAILED",
                        "Rendered preview frame timestamp differs from the selected sequence time",
                    ));
                }
                if cancel.is_cancelled() {
                    return Err(EditorError::new(
                        "JOB_CANCELLED",
                        "Project preview was cancelled",
                    ));
                }
                let result = ProjectFrameJobResult {
                    revision,
                    sequence_id: input.sequence_id,
                    requested_time: input.requested_time,
                    actual_sequence_time: actual_time,
                    alignment: input.alignment,
                    quality: ProjectFrameQuality::EncodedProjectFrame,
                    asset_locking: locking_mode(input.asset_lock.as_ref()),
                    asset_lock: input.asset_lock,
                    width,
                    height,
                    text_layouts: rendered.text_layouts,
                    artifact: artifact(&output, "image/png")?,
                };
                result.validate()?;
                Ok(json!(result))
            })();
            private_files.keep_output = result.is_ok();
            result
        }
        "preview.composited_frame" => {
            let input: PreparedProjectFrameInput = parse(job.input.clone())?;
            let private_inputs = PrivateInputs(directory.join("inputs"));
            let snapshot = resolve_visual_inputs(
                root,
                &private_inputs.0,
                &input.snapshot,
                &input.sequence_id,
                input.asset_lock.as_ref(),
                cancel,
                progress,
            )?;
            let output = directory.join("frame.png");
            let mut private_files = PreviewFiles {
                intermediate: None,
                output: output.clone(),
                keep_output: false,
            };
            let plan = ProjectRenderAdapter::plan_frame_resolved(&ResolvedProjectFrameRequest {
                resolved: &snapshot,
                requested_time: input.requested_time.clone(),
                alignment: input.alignment,
                output: output.clone(),
                overwrite: false,
            })
            .map_err(|error| EditorError::new(error.code, error.message))?;
            let rendered = backend
                .render_timeline_frame(&plan.media_request, cancel, progress)
                .map_err(media_error)?;
            let observed = RationalTime::new(
                rendered.actual_sequence_time.ticks,
                rendered.actual_sequence_time.timebase,
            )?;
            if observed.cmp_exact(RationalTime::try_from(&plan.actual_sequence_time)?)
                != std::cmp::Ordering::Equal
            {
                return Err(EditorError::new(
                    "ARTIFACT_VERIFICATION_FAILED",
                    "Composited frame timestamp differs from the selected sequence time",
                ));
            }
            if cancel.is_cancelled() {
                return Err(EditorError::new(
                    "JOB_CANCELLED",
                    "Project preview was cancelled",
                ));
            }
            let result = CompositedProjectFrameJobResult {
                revision: plan.revision,
                sequence_id: plan.sequence_id,
                requested_time: input.requested_time,
                actual_sequence_time: plan.actual_sequence_time,
                alignment: input.alignment,
                quality: CompositedProjectFrameQuality::CompositedProjectFrame,
                asset_locking: locking_mode(input.asset_lock.as_ref()),
                asset_lock: input.asset_lock,
                width: rendered.width,
                height: rendered.height,
                text_layouts: rendered.text_layouts,
                artifact: artifact(&output, "image/png")?,
            };
            result.validate()?;
            private_files.keep_output = true;
            Ok(json!(result))
        }
        "preview.project_range" => {
            let input: PreparedProjectRangeInput = parse(job.input.clone())?;
            let private_inputs = PrivateInputs(directory.join("inputs"));
            let snapshot = resolve_inputs(
                root,
                &private_inputs.0,
                &input.snapshot,
                &input.sequence_id,
                input.asset_lock.as_ref(),
                cancel,
                progress,
            )?;
            let output = directory.join("preview.mp4");
            let mut private_files = PreviewFiles {
                intermediate: None,
                output: output.clone(),
                keep_output: false,
            };
            let plan = ProjectRenderAdapter::plan_range_resolved(&ResolvedProjectRangeRequest {
                resolved: &snapshot,
                requested_range: input.requested_range.clone(),
                alignment: input.alignment,
                output: output.clone(),
                overwrite: false,
            })
            .map_err(|error| EditorError::new(error.code, error.message))?;
            let rendered = backend
                .render_timeline_range(&plan.media_request, cancel, progress)
                .map_err(media_error)?;
            if cancel.is_cancelled() {
                return Err(EditorError::new(
                    "JOB_CANCELLED",
                    "Project range preview was cancelled",
                ));
            }
            let result = ProjectRangeJobResult {
                revision: plan.revision,
                sequence_id: plan.sequence_id,
                requested_range: input.requested_range,
                actual_sequence_range: plan.actual_sequence_range,
                alignment: input.alignment,
                quality: ProjectRangeQuality::EncodedProjectRange,
                asset_locking: locking_mode(input.asset_lock.as_ref()),
                asset_lock: input.asset_lock,
                media: rendered.try_map_paths(public_path)?,
                artifact: artifact(&output, "video/mp4")?,
            };
            result.validate()?;
            private_files.keep_output = true;
            Ok(json!(result))
        }
        "render.export" => {
            let request: ExportJobInput = parse(job.input.clone())?;
            let output = directory.join(output_name(request.container));
            let result = backend
                .export(
                    &ExportRequest {
                        segments: request
                            .segments
                            .into_iter()
                            .map(|segment| segment.map_paths(PathBuf::from))
                            .collect(),
                        output: output.clone(),
                        container: request.container,
                        video: request.video,
                        audio: request.audio,
                        requested_features: vec![],
                        overwrite: false,
                    },
                    cancel,
                    progress,
                )
                .map_err(media_error)?;
            Ok(json!(MediaExportJobResult {
                media: result.try_map_paths(public_path)?,
                artifact: artifact(&output, output_mime(request.container))?,
            }))
        }
        "render.project" => {
            let input: PreparedProjectRenderInput = parse(job.input.clone())?;
            let private_inputs = PrivateInputs(directory.join("inputs"));
            let snapshot = resolve_inputs(
                root,
                &private_inputs.0,
                &input.snapshot,
                &input.sequence_id,
                input.asset_lock.as_ref(),
                cancel,
                progress,
            )?;
            let output = directory.join(output_name(input.container));
            let revision = input.snapshot.revision.clone();
            let request = ResolvedProjectRenderRequest {
                resolved: &snapshot,
                output: output.clone(),
                container: input.container,
                video_codec: input.video_codec,
                overwrite: false,
            };
            let plan = ProjectRenderAdapter::plan_resolved(&request)
                .map_err(|e| EditorError::new(e.code, e.message))?;
            let result = ProjectRenderAdapter::execute(&plan, &backend, cancel, progress)
                .map_err(|e| EditorError::new(e.code, e.message))?;
            let result = ProjectRenderJobResult {
                revision,
                asset_locking: locking_mode(input.asset_lock.as_ref()),
                asset_lock: input.asset_lock,
                media: result.try_map_paths(public_path)?,
                artifact: artifact(&output, output_mime(input.container))?,
            };
            result.validate()?;
            Ok(json!(result))
        }
        "artifact.verify" => {
            let input: VerifyRequest = parse(job.input.clone())?;
            Ok(json!(
                backend
                    .verify(&input, cancel, progress)
                    .map_err(media_error)?
            ))
        }
        _ => Err(EditorError::new(
            "CAPABILITY_UNAVAILABLE",
            "Worker cannot execute this job kind",
        )),
    }
}

fn locking_mode(lock: Option<&AssetLockManifest>) -> AssetLockingMode {
    if lock.is_some() {
        AssetLockingMode::MediaContentLocked
    } else {
        AssetLockingMode::LegacyUnlocked
    }
}

// The native boundary interprets a wire location as a local path.
fn public_path(path: PathBuf) -> Result<String, EditorError> {
    path.into_os_string().into_string().map_err(|_| {
        EditorError::new(
            "INVALID_ARGUMENT",
            "A public media location must be valid UTF-8",
        )
    })
}

fn artifact(path: &Path, mime: &str) -> Result<ArtifactDescriptor, EditorError> {
    let mut file = fs::File::open(path).map_err(io_error)?;
    let size = file.metadata().map_err(io_error)?.len();
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(ArtifactDescriptor {
        path: public_path(path.to_path_buf())?,
        mime_type: mime.into(),
        size,
        content_hash: format!("{:x}", hash.finalize()),
    })
}
fn output_name(container: Container) -> &'static str {
    match container {
        Container::Mp4 => "render.mp4",
        Container::Matroska => "render.mkv",
    }
}
fn output_mime(container: Container) -> &'static str {
    match container {
        Container::Mp4 => "video/mp4",
        Container::Matroska => "video/x-matroska",
    }
}
fn parse<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, EditorError> {
    serde_json::from_value(value).map_err(|e| EditorError::new("INVALID_ARGUMENT", e.to_string()))
}
fn media_error(error: opencut_media::MediaError) -> EditorError {
    EditorError::new(error.code(), error.to_string())
}
fn io_error(error: std::io::Error) -> EditorError {
    EditorError::new("MEDIA_IO_ERROR", error.to_string())
}
#[cfg(test)]
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_contracts::Time;
    use opencut_editor_application::JobRequest;

    #[test]
    fn versioned_inputs_require_lock_but_unused_versions_do_not_block_legacy_jobs() {
        let hash = "a".repeat(64);
        let mut snapshot: ProjectSnapshot = serde_json::from_value(json!({
            "revision": "revision-1",
            "project": {
                "id": "project", "name": "Project", "schema_version": 2,
                "default_sequence_id": "main",
                "assets": [{
                    "id": "source", "name": "Source", "kind": "video",
                    "uri": "/never-read-provenance.mov", "duration": null,
                    "version": {"id": format!("av_{hash}"), "content_hash": hash, "size": 10}
                }],
                "sequences": [{
                    "id": "main", "name": "Main", "width": 64, "height": 48,
                    "fps": {"numerator": 10, "denominator": 1},
                    "tracks": [{
                        "id": "video", "name": "Video", "kind": "video",
                        "enabled": true, "muted": false, "locked": false,
                        "clips": [{
                            "id": "clip", "asset_id": "source",
                            "timeline_range": {"start": {"ticks": "0", "timebase": 1},
                                "duration": {"ticks": "1", "timebase": 1}},
                            "source_range": {"start": {"ticks": "0", "timebase": 1},
                                "duration": {"ticks": "1", "timebase": 1}},
                            "params": {}, "channels": []
                        }]
                    }]
                }]
            }
        }))
        .unwrap();
        let root = std::env::temp_dir().join(format!("opencut-no-lock-{}", uuid::Uuid::new_v4()));
        let cancel = CancellationToken::new();
        let mut progress = |_| panic!("Unlocked input must not start resolving assets");
        assert_eq!(
            resolve_inputs(
                &root,
                &root.join("inputs"),
                &snapshot,
                "main",
                None,
                &cancel,
                &mut progress
            )
            .unwrap_err()
            .code,
            "ASSET_LOCK_REQUIRED"
        );
        assert_eq!(
            resolve_visual_inputs(
                &root,
                &root.join("inputs"),
                &snapshot,
                "main",
                None,
                &cancel,
                &mut progress
            )
            .unwrap_err()
            .code,
            "ASSET_LOCK_REQUIRED"
        );
        let mut audio_only = snapshot.clone();
        audio_only.project.sequences[0].tracks[0].kind = opencut_contracts::TrackKind::Audio;
        audio_only.project.assets[0].kind = opencut_contracts::AssetKind::Audio;
        assert!(
            resolve_visual_inputs(
                &root,
                &root.join("inputs"),
                &audio_only,
                "main",
                None,
                &cancel,
                &mut progress
            )
            .is_ok()
        );
        assert_eq!(
            resolve_inputs(
                &root,
                &root.join("inputs"),
                &audio_only,
                "main",
                None,
                &cancel,
                &mut progress
            )
            .unwrap_err()
            .code,
            "ASSET_LOCK_REQUIRED"
        );
        snapshot.project.sequences[0].tracks[0].enabled = false;
        assert!(
            resolve_inputs(
                &root,
                &root.join("inputs"),
                &snapshot,
                "main",
                None,
                &cancel,
                &mut progress
            )
            .is_ok()
        );
        snapshot.project.sequences[0].tracks[0].enabled = true;
        snapshot.project.assets[0].version = None;
        assert!(
            resolve_inputs(
                &root,
                &root.join("inputs"),
                &snapshot,
                "main",
                None,
                &cancel,
                &mut progress
            )
            .is_ok()
        );
        assert!(!root.exists());
    }

    #[test]
    fn project_frame_selection_rejects_end_and_does_not_clamp_alignment() {
        let fps = opencut_contracts::FrameRate {
            numerator: 10,
            denominator: 1,
        };
        let duration = MediaTime {
            ticks: 1,
            timebase: 1,
        };
        let requested = Time {
            ticks: "99".into(),
            timebase: 100,
        };
        let floor =
            select_project_frame_time(&requested, &fps, FrameAlignment::Floor, duration).unwrap();
        assert_eq!(
            floor,
            Time {
                ticks: "9".into(),
                timebase: 10
            }
        );
        assert!(
            select_project_frame_time(&requested, &fps, FrameAlignment::Nearest, duration).is_err()
        );
        assert!(
            select_project_frame_time(&requested, &fps, FrameAlignment::Ceil, duration).is_err()
        );
        assert!(
            select_project_frame_time(&requested, &fps, FrameAlignment::Exact, duration).is_err()
        );
        let end = Time {
            ticks: "1".into(),
            timebase: 1,
        };
        assert!(select_project_frame_time(&end, &fps, FrameAlignment::Floor, duration).is_err());
    }

    #[test]
    fn preview_private_files_are_removed_when_backend_unwinds() {
        let directory =
            std::env::temp_dir().join(format!("opencut-preview-cleanup-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let intermediate = directory.join("preview-source.mp4");
        let frame = directory.join("frame.png");
        let inputs = directory.join("inputs");
        let result = std::panic::catch_unwind(|| {
            let _inputs = PrivateInputs(inputs.clone());
            fs::create_dir(&inputs).unwrap();
            fs::write(inputs.join("locked-input"), b"private verified copy").unwrap();
            let _private_files = PreviewFiles {
                intermediate: Some(intermediate.clone()),
                output: frame.clone(),
                keep_output: false,
            };
            fs::write(&intermediate, b"partial encoded movie").unwrap();
            fs::write(&frame, b"partial image").unwrap();
            panic!("injected media backend failure");
        });
        assert!(!inputs.exists());
        assert!(result.is_err());
        assert!(!intermediate.exists());
        assert!(!frame.exists());
        fs::remove_dir(directory).unwrap();
    }

    fn claimed_job(app: &mut EditorApplication) -> JobRecord {
        app.create_job(JobRequest {
            kind: "render.project".into(),
            input: json!({}),
            idempotency_key: "progress-test".into(),
        })
        .unwrap();
        app.claim_job("worker", now(), LEASE_MS).unwrap().unwrap()
    }

    fn encoding(fraction: f32) -> ProgressUpdate {
        ProgressUpdate {
            stage: ProgressStage::Encoding,
            fraction: Some(fraction),
            media_time: Some(opencut_media::MediaTime {
                ticks: 123,
                timebase: 1000,
            }),
        }
    }

    #[test]
    fn recovery_cleanup_removes_abandoned_attempts_and_preserves_live_and_future_attempts() {
        let mut app = EditorApplication::open(":memory:").unwrap();
        let job = claimed_job(&mut app);
        let root =
            std::env::temp_dir().join(format!("opencut-input-recovery-{}", uuid::Uuid::new_v4()));
        for attempt in 0..=2 {
            let directory = root.join(&job.job_id).join(format!("attempt-{attempt}"));
            fs::create_dir_all(directory.join("inputs")).unwrap();
            fs::write(directory.join("inputs/source"), b"private media").unwrap();
            fs::write(directory.join("render.mp4"), b"artifact").unwrap();
        }
        clean_abandoned_inputs(&app, &root).unwrap();
        assert!(!root.join(&job.job_id).join("attempt-0").exists());
        assert!(
            root.join(&job.job_id)
                .join("attempt-1/inputs/source")
                .exists()
        );
        assert!(
            root.join(&job.job_id)
                .join("attempt-2/inputs/source")
                .exists()
        );
        assert_eq!(app.recover_expired(now() + LEASE_MS + 1).unwrap(), 1);
        clean_abandoned_inputs(&app, &root).unwrap();
        assert!(!root.join(&job.job_id).join("attempt-0").exists());
        assert!(!root.join(&job.job_id).join("attempt-1").exists());
        assert!(
            root.join(&job.job_id).join("attempt-2/render.mp4").exists(),
            "a directory newer than the database attempt may belong to a concurrent claimant"
        );
        let next = app.claim_job("next", now(), LEASE_MS).unwrap().unwrap();
        assert_eq!(next.attempt, 2);
        app.cancel_job(&next.job_id).unwrap();
        app.recover_expired(now() + LEASE_MS + 1).unwrap();
        assert_eq!(
            app.get_job(&next.job_id).unwrap().state,
            JobState::Cancelled
        );
        clean_abandoned_inputs(&app, &root).unwrap();
        assert!(!root.join(&job.job_id).join("attempt-2").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_cleans_unpublished_attempts_and_preserves_only_published_artifacts() {
        for (kind, output_name) in [
            ("preview.composited_frame", "frame.png"),
            ("preview.project_range", "preview.mp4"),
            ("render.project", "render.mp4"),
            ("preview.project_frame", "frame.png"),
        ] {
            let mut app = EditorApplication::open(":memory:").unwrap();
            app.create_job(JobRequest {
                kind: kind.into(),
                input: json!({}),
                idempotency_key: "direct-cleanup".into(),
            })
            .unwrap();
            let job = app.claim_job("first", now(), LEASE_MS).unwrap().unwrap();
            let root = std::env::temp_dir()
                .join(format!("opencut-direct-cleanup-{}", uuid::Uuid::new_v4()));
            let first = root.join(&job.job_id).join("attempt-1");
            fs::create_dir_all(first.join("inputs")).unwrap();
            fs::write(first.join(output_name), b"unpublished preview").unwrap();
            fs::write(
                first.join(format!(".{output_name}.partial.caption-0.png")),
                b"private caption",
            )
            .unwrap();
            clean_abandoned_inputs(&app, &root).unwrap();
            assert!(first.join(output_name).exists());
            app.recover_expired(now() + LEASE_MS + 1).unwrap();
            clean_abandoned_inputs(&app, &root).unwrap();
            assert!(!first.exists());

            let next = app.claim_job("second", now(), LEASE_MS).unwrap().unwrap();
            let second = root.join(&job.job_id).join("attempt-2");
            fs::create_dir_all(second.join("inputs")).unwrap();
            let published = second.join(output_name);
            fs::write(&published, b"published preview").unwrap();
            app.complete_job(
                &job.job_id,
                "second",
                next.attempt,
                now(),
                json!({
                    "artifact": { "path": published }
                }),
            )
            .unwrap();
            clean_abandoned_inputs(&app, &root).unwrap();
            assert!(published.exists());
            assert!(!second.join("inputs").exists());
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn progress_throttles_samples_but_persists_stage_boundaries_and_exact_time() {
        let mut app = EditorApplication::open(":memory:").unwrap();
        let job = claimed_job(&mut app);
        let cancel = CancellationToken::new();
        let start = Instant::now();
        {
            let mut reporter = ProgressReporter::new(&mut app, "worker", &job, &cancel);
            reporter.report_at(encoding(0.1), start);
            reporter.report_at(encoding(0.2), start + Duration::from_millis(1));
            reporter.report_at(encoding(0.5), start + Duration::from_millis(300));
            reporter.report_at(encoding(1.0), start + Duration::from_millis(301));
            reporter.report_at(encoding(1.0), start + Duration::from_millis(302));
            reporter.report_at(
                ProgressUpdate {
                    stage: ProgressStage::Verifying,
                    fraction: None,
                    media_time: None,
                },
                start + Duration::from_millis(303),
            );
            assert!(reporter.error.is_none());
        }
        let events = app.job_events_after(0, 100, Some(&job.job_id)).unwrap();
        let progress: Vec<_> = events
            .iter()
            .filter(|event| event.event_type == "progress")
            .collect();
        assert_eq!(progress.len(), 4);
        assert_eq!(
            progress[0].payload["media_time"],
            json!({"ticks":"123","timebase":1000})
        );
        assert_eq!(progress[1].payload["fraction"], 0.5);
        assert_eq!(progress[2].payload["fraction"], 1.0);
        assert_eq!(progress[3].payload["stage"], "verifying");
        assert_eq!(progress[3].payload["fraction"], Value::Null);
        assert_eq!(app.get_job(&job.job_id).unwrap().state, JobState::Running);
        assert!(!cancel.is_cancelled());
    }

    #[test]
    fn stale_attempt_cancels_media_without_publishing_progress() {
        let mut app = EditorApplication::open(":memory:").unwrap();
        let job = claimed_job(&mut app);
        app.recover_expired(now() + LEASE_MS + 1).unwrap();
        let replacement = app
            .claim_job("replacement", now(), LEASE_MS)
            .unwrap()
            .unwrap();
        let before = app.job_events_after(0, 100, None).unwrap().len();
        let cancel = CancellationToken::new();
        {
            let mut reporter = ProgressReporter::new(&mut app, "worker", &job, &cancel);
            reporter.report(encoding(0.5));
            assert_eq!(reporter.error.as_ref().unwrap().code, "JOB_LEASE_LOST");
            reporter.report(encoding(1.0));
        }
        assert!(cancel.is_cancelled());
        assert_eq!(
            app.get_job(&job.job_id).unwrap().attempt,
            replacement.attempt
        );
        assert!(app.get_job(&job.job_id).unwrap().progress.is_none());
        assert_eq!(app.job_events_after(0, 100, None).unwrap().len(), before);
    }
}
