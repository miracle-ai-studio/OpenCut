//! Projection of stored execution records into the public task contract.
use super::{JobRecord, JobState};
use opencut_api_contracts::{JobProgress, job_views::*};
use opencut_contracts::EditorError;
use opencut_job_contracts as jobs;
use opencut_media_contracts as media;
use serde::de::DeserializeOwned;
use serde_json::Value;

fn error(code: &str, message: &str) -> EditorError {
    EditorError::new(code, message)
}
fn bad_result() -> EditorError {
    error(
        "INVALID_STORED_RESULT",
        "Stored task result cannot be represented by this contract version",
    )
}
fn decode<T: DeserializeOwned>(value: Value) -> Result<T, EditorError> {
    serde_json::from_value(value).map_err(|_| bad_result())
}
fn public_failure(job: &str, code: &str) -> PublicJobFailure {
    let (code, message) = match code {
        "MEDIA_OUTPUT_LIMIT_EXCEEDED" => (
            "MEDIA_OUTPUT_LIMIT_EXCEEDED",
            "Media output reached its admitted file limit; ask the owner to inspect the output ceiling and workspace headroom before submitting a new task",
        ),
        "STORAGE_BUDGET_EXCEEDED" => (
            "STORAGE_BUDGET_EXCEEDED",
            "Workspace admission limit prevented task file allocation; ask the owner to inspect policy or release eligible data before submitting a new task",
        ),
        "STORAGE_CAPACITY_LOW" => (
            "STORAGE_CAPACITY_LOW",
            "Observed volume headroom is insufficient for task file allocation",
        ),
        "DATABASE_CAPACITY_EXHAUSTED" => (
            "DATABASE_CAPACITY_EXHAUSTED",
            "Workspace database has no space to commit task state; free capacity before retrying",
        ),
        "STORAGE_BUDGET_BUSY" => (
            "STORAGE_BUDGET_BUSY",
            "Workspace admission remained busy; inspect the terminal task before submitting a new task",
        ),
        "STORAGE_BUDGET_ERROR" => (
            "STORAGE_BUDGET_ERROR",
            "Workspace admission state requires owner inspection",
        ),
        "INVALID_ARGUMENT" | "TIME_OVERFLOW" | "CONSTRAINT_VIOLATION" => (
            "INVALID_ARGUMENT",
            "Task input is invalid for this operation",
        ),
        "MISSING_ASSET" | "ASSET_UNAVAILABLE" | "NOT_FOUND" => {
            ("MISSING_ASSET", "A required task dependency is unavailable")
        }
        "CAPABILITY_UNAVAILABLE" | "ASSET_LOCK_REQUIRED" => (
            "CAPABILITY_UNAVAILABLE",
            "A required task capability is unavailable",
        ),
        "ARTIFACT_VERIFICATION_FAILED" | "ASSET_CORRUPTED" | "UPLOAD_INTEGRITY_ERROR" => (
            "ARTIFACT_VERIFICATION_FAILED",
            "Task content failed integrity validation",
        ),
        "UNSUPPORTED_JOB_KIND" => (
            "UNSUPPORTED_JOB_KIND",
            "This contract version cannot represent the task kind",
        ),
        "INVALID_STORED_RESULT" => (
            "INVALID_STORED_RESULT",
            "Stored task result cannot be represented by this contract version",
        ),
        _ => (
            "JOB_FAILED",
            "Task execution failed; consult its authorized diagnostic record",
        ),
    };
    PublicJobFailure {
        code: code.into(),
        message: message.into(),
        diagnostic_job_id: job.into(),
    }
}
pub fn public_error(original: EditorError) -> EditorError {
    // Envelopes for new view tools may not leak worker/backend error text either.
    match original.code.as_str() {
        "NOT_AUTHORIZED" => crate::access::denied(),
        "NOT_FOUND" => error("NOT_FOUND", "Task or required object was not found"),
        "INVALID_ARGUMENT" => error("INVALID_ARGUMENT", "Invalid public task request"),
        "IDEMPOTENCY_KEY_REUSED" => error(
            "IDEMPOTENCY_KEY_REUSED",
            "Idempotency key was already used with another request",
        ),
        "INVALID_CURSOR" => error(
            "INVALID_CURSOR",
            "Cursor does not match this stream or filter",
        ),
        "CURSOR_EXPIRED" => error(
            "CURSOR_EXPIRED",
            "Cursor anchor is unavailable; start a new query",
        ),
        "REVISION_CONFLICT" => error(
            "REVISION_CONFLICT",
            "The expected revision is no longer current",
        ),
        "DATABASE_BUSY" => error(
            "DATABASE_BUSY",
            "Task state is temporarily locked; inspect it and retry the same request",
        ),
        "STORAGE_ERROR" | "INTERNAL_ERROR" => {
            error("INTERNAL_ERROR", "Task state could not be read or changed")
        }
        _ => {
            let failure = public_failure("", &original.code);
            error(&failure.code, &failure.message)
        }
    }
}
pub fn progress(value: JobProgress) -> ProgressView {
    let stage = match value.stage.as_str() {
        "resolving_assets" => PublicJobStage::ResolvingAssets,
        "probing" => PublicJobStage::Probing,
        "selecting_frame" => PublicJobStage::SelectingFrame,
        "decoding" => PublicJobStage::Decoding,
        "encoding" => PublicJobStage::Encoding,
        "verifying" => PublicJobStage::Verifying,
        _ => PublicJobStage::Unknown,
    };
    ProgressView {
        stage,
        fraction: value.fraction,
        media_time: value.media_time,
    }
}
fn metadata(value: media::MediaMetadata) -> MediaInfo {
    MediaInfo {
        format_name: value.format_name,
        duration: value.duration,
        bit_rate: value.bit_rate,
        streams: value.streams,
    }
}
fn verification(value: media::VerificationReport) -> VerificationView {
    VerificationView {
        passed: value.passed,
        metadata: metadata(value.metadata),
        decoded_samples: value.decoded_samples,
        checks: value
            .checks
            .into_iter()
            .map(|v| VerificationCheckView {
                name: v.name,
                passed: v.passed,
            })
            .collect(),
    }
}
fn layouts(values: Vec<media::TextLayoutReport>) -> Vec<TextLayoutView> {
    values
        .into_iter()
        .map(|v| TextLayoutView {
            requested_family: v.requested_family,
            policy: v.policy,
            used_faces: v
                .used_faces
                .into_iter()
                .map(|f| FontFaceView {
                    family: f.family,
                    postscript_name: f.postscript_name,
                    face_index: f.face_index,
                })
                .collect(),
            line_count: v.line_count,
            glyph_count: v.glyph_count,
            rtl_runs: v.rtl_runs,
            cluster_ranges: v.cluster_ranges,
            pixel_bounds: v.pixel_bounds,
        })
        .collect()
}
fn export(value: media::ExportResult) -> ExportView {
    ExportView {
        metadata: metadata(value.metadata),
        verification: verification(value.verification),
        text_layouts: layouts(value.text_layouts),
    }
}
pub fn valid_job_id(value: &str) -> bool {
    value.strip_prefix("job_").is_some_and(|id| {
        id.len() == 32
            && id
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    })
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
pub fn parse_artifact_id(value: &str) -> Result<(&str, &str), EditorError> {
    let mut parts = value.split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("artifact"), Some(job), Some(hash), None)
            if valid_job_id(job) && valid_hash(hash) =>
        {
            Ok((job, hash))
        }
        _ => Err(error("INVALID_ARGUMENT", "Invalid artifact identity")),
    }
}
fn artifact(
    job_id: &str,
    value: jobs::ArtifactDescriptor,
) -> Result<ArtifactReference, EditorError> {
    if !valid_hash(&value.content_hash) || !valid_job_id(job_id) {
        return Err(bad_result());
    }
    let mime = match value.mime_type.as_str() {
        "image/png" => "image/png",
        "video/mp4" => "video/mp4",
        "video/x-matroska" => "video/x-matroska",
        "audio/wav" => "audio/wav",
        "audio/mpeg" => "audio/mpeg",
        _ => "application/octet-stream",
    };
    Ok(ArtifactReference {
        artifact_id: format!("artifact:{job_id}:{}", value.content_hash),
        job_id: job_id.into(),
        role: ArtifactRole::Primary,
        content_hash: value.content_hash,
        size_bytes: value.size.to_string(),
        mime_type: mime.into(),
        availability: ArtifactAvailability::Unchecked,
    })
}
fn result(
    job_id: &str,
    kind: &str,
    value: Value,
) -> Result<(JobResultView, Vec<ArtifactReference>), EditorError> {
    let (result, file) = match kind {
        "media.probe" => return Ok((JobResultView::Probe(metadata(decode(value)?)), vec![])),
        "artifact.verify" => {
            return Ok((
                JobResultView::Verification(verification(decode(value)?)),
                vec![],
            ));
        }
        "preview.frame" => {
            let v: jobs::MediaFrameJobResult = decode(value)?;
            (
                JobResultView::SourceFrame(SourceFrameView {
                    requested_time: v.media.requested_time,
                    actual_time: v.media.actual_time,
                    source_pts: v.media.source_pts.to_string(),
                    source_time_base: v.media.source_time_base,
                    alignment: v.media.alignment,
                    exact: v.media.exact,
                    quality: v.media.quality,
                }),
                v.artifact,
            )
        }
        "render.export" => {
            let v: jobs::MediaExportJobResult = decode(value)?;
            (JobResultView::Export(export(v.media)), v.artifact)
        }
        "render.project" => {
            let v: jobs::ProjectRenderJobResult = decode(value)?;
            (
                JobResultView::ProjectRender(ProjectRenderView {
                    revision: v.revision,
                    asset_lock: v.asset_lock,
                    asset_locking: v.asset_locking,
                    media: export(v.media),
                }),
                v.artifact,
            )
        }
        "preview.project_frame" => {
            let v: jobs::ProjectFrameJobResult = decode(value)?;
            (
                JobResultView::ProjectFrame(ProjectFrameView {
                    revision: v.revision,
                    sequence_id: v.sequence_id,
                    requested_time: v.requested_time,
                    actual_sequence_time: v.actual_sequence_time,
                    alignment: v.alignment,
                    quality: v.quality,
                    asset_lock: v.asset_lock,
                    asset_locking: v.asset_locking,
                    width: v.width,
                    height: v.height,
                    text_layouts: layouts(v.text_layouts),
                }),
                v.artifact,
            )
        }
        "preview.composited_frame" => {
            let v: jobs::CompositedProjectFrameJobResult = decode(value)?;
            (
                JobResultView::CompositedFrame(CompositedFrameView {
                    revision: v.revision,
                    sequence_id: v.sequence_id,
                    requested_time: v.requested_time,
                    actual_sequence_time: v.actual_sequence_time,
                    alignment: v.alignment,
                    quality: v.quality,
                    asset_lock: v.asset_lock,
                    asset_locking: v.asset_locking,
                    width: v.width,
                    height: v.height,
                    text_layouts: layouts(v.text_layouts),
                }),
                v.artifact,
            )
        }
        "preview.project_range" => {
            let v: jobs::ProjectRangeJobResult = decode(value)?;
            (
                JobResultView::ProjectRange(ProjectRangeView {
                    revision: v.revision,
                    sequence_id: v.sequence_id,
                    requested_range: v.requested_range,
                    actual_sequence_range: v.actual_sequence_range,
                    alignment: v.alignment,
                    quality: v.quality,
                    asset_lock: v.asset_lock,
                    asset_locking: v.asset_locking,
                    media: export(v.media),
                }),
                v.artifact,
            )
        }
        _ => return Err(error("UNSUPPORTED_JOB_KIND", "Unsupported result kind")),
    };
    Ok((result, vec![artifact(job_id, file)?]))
}
pub fn project(record: JobRecord) -> JobView {
    // Public intent identity is useful even before a result exists; only these three
    // fields are selected from the legacy prepared input, never its snapshot or URIs.
    let target = if matches!(
        record.kind.as_str(),
        "render.project"
            | "preview.project_frame"
            | "preview.composited_frame"
            | "preview.project_range"
    ) {
        (|| {
            Some(ProjectTaskTarget {
                project_id: record
                    .input
                    .pointer("/snapshot/project/id")?
                    .as_str()?
                    .into(),
                revision: record.input.pointer("/snapshot/revision")?.as_str()?.into(),
                sequence_id: record.input.get("sequence_id")?.as_str()?.into(),
            })
        })()
    } else {
        None
    };
    let import = if record.kind == "media.import" {
        serde_json::from_value::<crate::uploads::PreparedMediaImportInput>(record.input.clone())
            .ok()
            .filter(|v| crate::uploads::validate_manifest(v).is_ok())
    } else {
        None
    };
    let resource_target = import.as_ref().map(|v| JobResourceTarget {
        project_id: v.project_id.clone(),
        resource_kind: "upload".into(),
        resource_id: v.upload_id.clone(),
    });
    let mut view = JobView {
        job_id: record.job_id.clone(),
        kind: record.kind,
        target,
        state: record.state,
        cancel_requested: record.cancel_requested,
        progress: record.progress.map(progress),
        result: None,
        extended_result: None,
        resource_target,
        result_unavailable: None,
        failure: None,
        artifacts: vec![],
    };
    if record.state == JobState::Succeeded && view.kind == "media.import" {
        let projected = (|| {
            let input = import.ok_or_else(bad_result)?;
            let result: jobs::MediaImportJobResult = decode(record.result.unwrap_or(Value::Null))?;
            crate::uploads::validate_import_result(&input, &view.job_id, &result)
                .map_err(|_| bad_result())?;
            Ok::<_, EditorError>(ExtendedJobResult {
                kind: "media.import".into(),
                schema_version: 1,
                data: serde_json::to_value(result).map_err(|_| bad_result())?,
            })
        })();
        match projected {
            Ok(result) => view.extended_result = Some(result),
            Err(e) => view.result_unavailable = Some(public_failure(&view.job_id, &e.code)),
        }
    } else if record.state == JobState::Succeeded {
        match result(
            &view.job_id,
            &view.kind,
            record.result.unwrap_or(Value::Null),
        ) {
            Ok((result, artifacts)) => {
                view.result = Some(result);
                view.artifacts = artifacts;
            }
            Err(error) => view.result_unavailable = Some(public_failure(&view.job_id, &error.code)),
        }
    } else if record.state == JobState::Failed {
        view.failure = Some(public_failure(
            &view.job_id,
            record
                .error
                .as_ref()
                .and_then(|e| e.get("code"))
                .and_then(Value::as_str)
                .unwrap_or("JOB_FAILED"),
        ));
    }
    view
}
pub fn change(e: crate::JobEvent) -> JobChange {
    let kind = match e.event_type.as_str() {
        "queued" => JobChangeKind::Queued,
        "running" | "started" => JobChangeKind::Started,
        "progress" => JobChangeKind::Progress,
        "succeeded" => JobChangeKind::Succeeded,
        "failed" => JobChangeKind::Failed,
        "cancel_requested" => JobChangeKind::CancelRequested,
        "cancelled" => JobChangeKind::Cancelled,
        "recovered" => JobChangeKind::Recovered,
        _ => JobChangeKind::Unknown,
    };
    let progress = if e.event_type == "progress" {
        serde_json::from_value::<JobProgress>(e.payload)
            .ok()
            .map(progress)
    } else {
        None
    };
    JobChange {
        event_id: e.event_id,
        job_id: e.job_id,
        event_type: kind,
        state: e.state,
        progress,
        created_at: e.created_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn record() -> JobRecord {
        JobRecord {
            job_id: format!("job_{}", "a".repeat(32)),
            kind: "media.probe".into(),
            state: JobState::Succeeded,
            input: json!({"snapshot":{"private":"/private/secret"}}),
            result: Some(
                json!({"input":"/private/secret", "format_name":"wav", "duration":null,"bit_rate":null,"streams":[],"future":{"private":"/private/secret"}}),
            ),
            error: None,
            attempt: 42,
            lease_owner: Some("worker-secret".into()),
            lease_until: Some(123),
            cancel_requested: false,
            progress: None,
        }
    }
    #[test]
    fn projection_is_an_allowlist_and_unknown_result_fields_do_not_leak() {
        let view = project(record());
        assert!(matches!(view.result, Some(JobResultView::Probe(_))));
        let encoded = serde_json::to_string(&view).unwrap();
        for private in [
            "/private/secret",
            "worker-secret",
            "lease_until",
            "attempt",
            "snapshot",
            "future",
        ] {
            assert!(!encoded.contains(private), "{encoded}");
        }
    }
    #[test]
    fn invalid_or_future_stored_results_remain_historical_success_with_explicit_unavailability() {
        let mut invalid = record();
        invalid.result = Some(json!({"input":"/private/secret"}));
        let view = project(invalid);
        assert_eq!(view.state, JobState::Succeeded);
        assert!(view.result.is_none());
        assert_eq!(
            view.result_unavailable.unwrap().code,
            "INVALID_STORED_RESULT"
        );
        let mut future = record();
        future.kind = "future.job".into();
        let view = project(future);
        assert_eq!(view.state, JobState::Succeeded);
        assert_eq!(
            view.result_unavailable.unwrap().code,
            "UNSUPPORTED_JOB_KIND"
        );
    }
    #[test]
    fn raw_failure_codes_text_and_progress_stage_are_not_public_diagnostics() {
        let mut failed = record();
        failed.state = JobState::Failed;
        failed.error = Some(
            json!({"code":"/private/secret", "message":"/private/secret", "details":{"private":"/private/secret"}}),
        );
        failed.progress = Some(JobProgress {
            stage: "/private/secret".into(),
            fraction: None,
            media_time: None,
        });
        let view = project(failed);
        assert_eq!(view.failure.as_ref().unwrap().code, "JOB_FAILED");
        assert!(view.result.is_none());
        assert!(
            !serde_json::to_string(&view)
                .unwrap()
                .contains("/private/secret")
        );
    }
    #[test]
    fn database_capacity_error_is_public_without_leaking_storage_details() {
        let error = EditorError::new("DATABASE_CAPACITY_EXHAUSTED", "/private/secret");
        let public = public_error(error);
        assert_eq!(public.code, "DATABASE_CAPACITY_EXHAUSTED");
        assert!(!public.message.contains("/private/secret"));

        let mut failed = record();
        failed.state = JobState::Failed;
        failed.error =
            Some(json!({"code":"DATABASE_CAPACITY_EXHAUSTED","message":"/private/secret"}));
        let view = project(failed);
        assert_eq!(view.failure.unwrap().code, "DATABASE_CAPACITY_EXHAUSTED");
    }
    #[test]
    fn transient_database_lock_remains_retryable_without_leaking_storage_details() {
        let public = public_error(EditorError::new("DATABASE_BUSY", "/private/secret"));
        assert_eq!(public.code, "DATABASE_BUSY");
        assert!(!public.message.contains("/private/secret"));
    }
}
