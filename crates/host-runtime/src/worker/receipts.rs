//! Bounded attempt-local receipts. The scheduler publishes only after this file
//! is durable; readers authorize the job and require the published attempt.
use super::*;
use opencut_api_contracts::{
    execution_receipt::ExecutionReceipt, historical_visual_package::PackageToolLock,
};
use opencut_composition::CompositionScope;
const MAX_RECEIPT: u64 = 2 * 1024 * 1024;
fn hash(value: &impl serde::Serialize) -> Result<String, EditorError> {
    Ok(format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(value)
                .map_err(|e| EditorError::new("INTERNAL_ERROR", e.to_string()))?
        )
    ))
}
fn identity(
    backend: &FfmpegBackend,
) -> Result<opencut_api_contracts::reproducibility::ExecutionIdentity, EditorError> {
    let tools = backend
        .tool_fingerprints()
        .map_err(media_error)?
        .into_iter()
        .map(|v| PackageToolLock {
            name: v.name,
            version: v.version,
            binary_sha256: v.binary_sha256,
        })
        .collect();
    opencut_native_services::reproducibility::execution_identity(tools)
}
pub(super) fn prepare(
    job: &JobRecord,
    backend: &FfmpegBackend,
) -> Result<Option<ExecutionReceipt>, EditorError> {
    let visual = matches!(
        job.kind.as_str(),
        "preview.project_frame" | "preview.project_frame.v3"
    );
    if !matches!(
        job.kind.as_str(),
        "render.project"
            | "render.project.v3"
            | "preview.project_frame"
            | "preview.project_frame.v3"
            | "preview.project_range"
            | "preview.project_range.v3"
    ) {
        return Ok(None);
    }
    let scope = if visual {
        CompositionScope::Visual
    } else {
        CompositionScope::AudioVisual
    };
    let seq = job
        .input
        .get("sequence_id")
        .and_then(Value::as_str)
        .ok_or_else(|| EditorError::new("INVALID_ARGUMENT", "Missing prepared sequence"))?;
    let snapshot = job
        .input
        .get("snapshot")
        .cloned()
        .ok_or_else(|| EditorError::new("INVALID_ARGUMENT", "Missing prepared snapshot"))?;
    let (revision, composition_sha256, covered) = if job.kind.ends_with(".v3") {
        let source: opencut_contracts::historical_visual::HistoricalVisualSnapshot =
            parse(snapshot)?;
        let compiled =
            opencut_composition::historical_visual::compile_historical_visual(&source, seq, scope)
                .map_err(|e| EditorError::new(e.code, e.message))?;
        let base = ProjectSnapshot {
            project: source.project.base.clone(),
            revision: source.revision.clone(),
        };
        let semantics = opencut_composition::semantic_coverage::audit(
            &base,
            &compiled.base,
            &source.project.keyframe_curves,
        )
        .map_err(|e| EditorError::new(e.code, e.message))?;
        let visual = opencut_composition::historical_visual_coverage::audit_visual_coverage(
            &source, &compiled,
        )
        .map_err(|e| EditorError::new(e.code, e.message))?;
        let mut covered: Vec<String> = semantics
            .entries
            .into_iter()
            .map(|v| format!("{}:{}:{}", v.semantic, v.source_id, v.disposition))
            .collect();
        covered.extend(
            visual
                .entries
                .into_iter()
                .map(|v| format!("{}:{}:{:?}", v.kind, v.source_id, v.disposition)),
        );
        (source.revision, hash(&compiled)?, covered)
    } else {
        let source: ProjectSnapshot = parse(snapshot)?;
        let compiled = opencut_composition::compile(&source, seq, scope)
            .map_err(|e| EditorError::new(e.code, e.message))?;
        let semantics = opencut_composition::semantic_coverage::audit(&source, &compiled, &[])
            .map_err(|e| EditorError::new(e.code, e.message))?;
        (
            source.revision,
            hash(&compiled)?,
            semantics
                .entries
                .into_iter()
                .map(|v| format!("{}:{}:{}", v.semantic, v.source_id, v.disposition))
                .collect(),
        )
    };
    Ok(Some(ExecutionReceipt {
        job_id: job.job_id.clone(),
        attempt: job.attempt,
        kind: job.kind.clone(),
        request_sha256: hash(&job.input)?,
        source_revision: revision,
        sequence_id: seq.into(),
        composition_sha256,
        execution: identity(backend)?,
        artifact_sha256: String::new(),
        artifact_size: 0,
        covered_semantics: covered,
        semantic_output_verified: false,
    }))
}
pub(super) fn finish(
    mut receipt: ExecutionReceipt,
    backend: &FfmpegBackend,
    result: &Value,
) -> Result<Vec<u8>, EditorError> {
    let after = identity(backend)?;
    if hash(&after)? != hash(&receipt.execution)? {
        return Err(EditorError::new(
            "EXECUTION_IDENTITY_CHANGED",
            "Host executable or media tools changed during execution",
        ));
    }
    let artifact = result
        .get("artifact")
        .ok_or_else(|| EditorError::new("INTERNAL_ERROR", "Project result has no artifact"))?;
    receipt.artifact_sha256 = artifact
        .get("content_hash")
        .and_then(Value::as_str)
        .ok_or_else(|| EditorError::new("INTERNAL_ERROR", "Missing output hash"))?
        .into();
    receipt.artifact_size = artifact
        .get("size")
        .and_then(Value::as_u64)
        .ok_or_else(|| EditorError::new("INTERNAL_ERROR", "Missing output size"))?;
    let bytes = serde_json::to_vec(&receipt)
        .map_err(|e| EditorError::new("INTERNAL_ERROR", e.to_string()))?;
    if bytes.len() as u64 > MAX_RECEIPT {
        return Err(EditorError::new(
            "CAPABILITY_UNAVAILABLE",
            "Execution receipt exceeds bounded storage",
        ));
    }
    Ok(bytes)
}
pub(crate) fn inspect(database: &Path, job: &JobRecord) -> Result<ExecutionReceipt, EditorError> {
    if job.state != JobState::Succeeded {
        return Err(EditorError::new(
            "NOT_FOUND",
            "No published successful execution receipt",
        ));
    }
    let root = database
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("artifacts");
    let artifact = job
        .result
        .as_ref()
        .and_then(|r| r.get("artifact"))
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Job has no artifact"))?;
    let directory = artifact
        .get("path")
        .and_then(Value::as_str)
        .and_then(|s| Path::new(s).parent())
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Job has no artifact path"))?;
    let current = directory.join("execution.json");
    let historical = matches!(
        std::fs::symlink_metadata(&current),
        Err(ref error) if error.kind() == std::io::ErrorKind::NotFound
    );
    let path = if historical {
        directory.join("execution-v1.json")
    } else {
        current
    };
    let mut source = opencut_native_services::artifacts::ArtifactSource::open(
        &root,
        &job.job_id,
        job.attempt,
        &path,
    )?;
    let mut bytes = Vec::new();
    (&mut source)
        .take(MAX_RECEIPT + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > MAX_RECEIPT {
        return Err(EditorError::new(
            "ARTIFACT_VERIFICATION_FAILED",
            "Oversized execution receipt",
        ));
    }
    let invalid = || EditorError::new("ARTIFACT_VERIFICATION_FAILED", "Invalid execution receipt");
    let mut value: Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if historical
        && value
            .as_object_mut()
            .and_then(|object| object.remove("schema_version"))
            != Some(serde_json::json!(1))
    {
        return Err(invalid());
    }
    let receipt: ExecutionReceipt = serde_json::from_value(value).map_err(|_| invalid())?;
    if receipt.job_id != job.job_id
        || receipt.attempt != job.attempt
        || receipt.kind != job.kind
        || receipt.request_sha256 != hash(&job.input)?
        || Some(receipt.artifact_sha256.as_str())
            != artifact.get("content_hash").and_then(Value::as_str)
        || Some(receipt.artifact_size) != artifact.get("size").and_then(Value::as_u64)
    {
        return Err(EditorError::new(
            "ARTIFACT_VERIFICATION_FAILED",
            "Execution receipt does not match published attempt",
        ));
    }
    Ok(receipt)
}
