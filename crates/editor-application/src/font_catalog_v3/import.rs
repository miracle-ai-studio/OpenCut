use super::*;
use crate::{
    font_import_v1::{catalog::*, *},
    project_v4::{
        persistence::{begin, prepare},
        *,
    },
};
use opencut_contracts::{
    AssetVersion, TransactionResult,
    font_import_v1::ImportFontV1,
    project_v4::{FontResourceV4, ProjectSnapshotV4},
    project_v4_ops::{ApplyV4Project, ProjectOperationV4},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceLicenseV3 {
    Ofl11,
    Apache20,
    UbuntuFont10,
}
fn source_license(pin: &PinnedFontSourceV1) -> Result<SourceLicenseV3, EditorError> {
    let license = match pin.license_identifier {
        "OFL-1.1" => SourceLicenseV3::Ofl11,
        "Apache-2.0" => SourceLicenseV3::Apache20,
        "Ubuntu-font-1.0" => SourceLicenseV3::UbuntuFont10,
        _ => {
            return Err(EditorError::new(
                "FONT_LICENSE_REQUIRED",
                "Compiled V3 source has no admitted license",
            ));
        }
    };
    if pin.license_notice.trim().is_empty()
        || pin.license_notice.len() > 64 * 1024
        || pin.attribution.len() > 16 * 1024
    {
        return Err(EditorError::new(
            "FONT_LICENSE_REQUIRED",
            "Compiled V3 source must retain its complete bounded license",
        ));
    }
    Ok(license)
}

pub fn validate_import_request_v3(request: &ImportFontV1) -> Result<(), EditorError> {
    crate::require_idempotency_key(&request.idempotency_key)?;
    for value in [
        &request.project_id,
        &request.expected_revision,
        &request.font_id,
    ] {
        if value.trim().is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Font import identity is invalid",
            ));
        }
    }
    let source = &native_source_v3(&request.catalog_id)?.source;
    source_license(source)?;
    if source.family.trim().is_empty()
        || source.family.len() > 512
        || source.family.chars().any(char::is_control)
        || !matches!(source.weight, 400 | 700)
    {
        return Err(EditorError::new(
            "FONT_FACE_MISMATCH",
            "Compiled V3 source face is not expressible by the current text model",
        ));
    }
    validate_catalog_font_envelope_v3(
        source.content_hash,
        source.size_bytes,
        &request.required_text,
        &FontImportPolicyV1 {
            max_font_bytes: MAX_CATALOG_FONT_BYTES_V3,
            ..FontImportPolicyV1::default()
        },
    )
}
pub fn expected_import_resource_v3(request: &ImportFontV1) -> Result<FontResourceV4, EditorError> {
    validate_import_request_v3(request)?;
    let source = &native_source_v3(&request.catalog_id)?.source;
    let spec = source.import_spec(request.font_id.clone(), request.required_text.clone());
    Ok(FontResourceV4 {
        id: spec.font_id,
        family: spec.family,
        weight: spec.weight,
        style: spec.style,
        face_index: 0,
        content: AssetVersion {
            id: format!("av_{}", source.content_hash),
            content_hash: source.content_hash.into(),
            size: source.size_bytes,
        },
        origin: spec.origin,
        license: Some(spec.license),
    })
}
pub fn validate_fetch_v3(
    id: &str,
    policy: &FontFetchPolicyV1,
) -> Result<&'static ApprovedFontSourceV3, EditorError> {
    let pin = native_source_v3(id)?;
    if !policy.allow_public_font_download {
        return Err(EditorError::new(
            "FONT_NETWORK_DENIED",
            "Task policy does not allow public font downloads",
        ));
    }
    if !(1..=30000).contains(&policy.deadline_ms)
        || !(1..=MAX_CATALOG_FONT_BYTES_V3).contains(&policy.max_bytes)
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Font download policy exceeds its bounded envelope",
        ));
    }
    if pin.source.size_bytes > policy.max_bytes {
        return Err(EditorError::new(
            "FONT_BUDGET_EXCEEDED",
            "Pinned font exceeds the download byte budget",
        ));
    }
    Ok(pin)
}

/// Uses the existing original-request receipt, planner, CAS lease and atomic
/// commit. Only source selection and native inspector are versioned.
pub fn import_static_font_v3(
    store: &mut impl ProjectV4Store,
    fetcher: &impl FontSourceFetchV1,
    inspector: &impl FontInspectorV1,
    publisher: &impl FontCasPublishV1,
    request: &ImportFontV1,
    fetch_policy: &FontFetchPolicyV1,
    inspection_policy: &FontImportPolicyV1,
    checkpoint: &mut dyn FontImportCheckpointV1,
) -> Result<TransactionResult, EditorError> {
    validate_import_request_v3(request)?;
    let intent = V4Intent::FontImport(request);
    let source = match begin(store, intent)? {
        Ok(value) => value,
        Err(result) => return Ok(result),
    };
    checkpoint.check()?;
    let font = expected_import_resource_v3(request)?;
    let StoredProjectDocumentV4::V4(project) = &source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Promote the project before importing fonts",
        ));
    };
    let operation = ApplyV4Project {
        project_id: request.project_id.clone(),
        expected_revision: request.expected_revision.clone(),
        idempotency_key: request.idempotency_key.clone(),
        label: format!("Import {}", font.family),
        operations: vec![ProjectOperationV4::FontPut { font: font.clone() }],
    };
    let proposed = plan(
        &ProjectSnapshotV4 {
            project: (**project).clone(),
            revision: source.revision.clone(),
        },
        &operation,
    )?;
    let commit = prepare(&source, proposed, intent)?;
    let pin = validate_fetch_v3(&request.catalog_id, fetch_policy)?;
    validate_catalog_font_envelope_v3(
        pin.source.content_hash,
        pin.source.size_bytes,
        &request.required_text,
        inspection_policy,
    )?;
    let bytes = fetcher.fetch(&request.catalog_id, fetch_policy, checkpoint)?;
    if bytes.len() as u64 != font.content.size {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Font bytes differ from the pinned size",
        ));
    }
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    for chunk in bytes.chunks(64 * 1024) {
        checkpoint.check()?;
        digest.update(chunk);
    }
    if format!("{:x}", digest.finalize()) != font.content.content_hash {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Font bytes differ from the pinned hash",
        ));
    }
    let parsed = inspector.inspect(
        &bytes,
        &request.required_text,
        inspection_policy,
        checkpoint,
    )?;
    if parsed.family != font.family
        || parsed.weight != font.weight
        || parsed.style != font.style
        || parsed.glyph_count == 0
    {
        return Err(EditorError::new(
            "FONT_FACE_MISMATCH",
            "Inspected font differs from its admitted catalog resource",
        ));
    }
    let acquired = PreparedFontV1 {
        resource: font,
        glyph_count: parsed.glyph_count,
        checked_codepoints: parsed.checked_codepoints,
    };
    checkpoint.check()?;
    let _publication = publisher.publish(&acquired, &bytes, checkpoint)?;
    checkpoint.check()?;
    store.commit_v4_atomic(intent, commit)
}
