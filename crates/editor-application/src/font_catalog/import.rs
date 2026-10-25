use super::*;
use crate::{
    font_import::{catalog::*, *},
    scene::{
        persistence::{begin, prepare},
        *,
    },
};
use opencut_contracts::{
    AssetVersion, TransactionResult,
    font_import::ImportFont,
    scene::{EditorSnapshot, FontResource},
    scene_ops::{ApplySceneProject, ProjectOperation},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceLicense {
    Ofl11,
    Apache20,
    UbuntuFont10,
}
fn source_license(pin: &PinnedFontSource) -> Result<SourceLicense, EditorError> {
    let license = match pin.license_identifier {
        "OFL-1.1" => SourceLicense::Ofl11,
        "Apache-2.0" => SourceLicense::Apache20,
        "Ubuntu-font-1.0" => SourceLicense::UbuntuFont10,
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

pub fn validate_import_request(request: &ImportFont) -> Result<(), EditorError> {
    validate_import_identity(request)?;
    validate_selected_request(request, native_source(&request.catalog_id)?)
}
pub(super) fn validate_import_identity(request: &ImportFont) -> Result<(), EditorError> {
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
    Ok(())
}
pub(super) fn validate_selected_request(
    request: &ImportFont,
    approved: &ApprovedFontSource,
) -> Result<(), EditorError> {
    validate_import_identity(request)?;
    let source = &approved.source;
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
    validate_catalog_font_envelope(
        source.content_hash,
        source.size_bytes,
        &request.required_text,
        &FontImportPolicy {
            max_font_bytes: MAX_CATALOG_FONT_BYTES,
            ..FontImportPolicy::default()
        },
    )
}
pub fn expected_import_resource(request: &ImportFont) -> Result<FontResource, EditorError> {
    validate_import_request(request)?;
    expected_selected_resource(request, native_source(&request.catalog_id)?)
}
pub(super) fn expected_selected_resource(
    request: &ImportFont,
    approved: &ApprovedFontSource,
) -> Result<FontResource, EditorError> {
    validate_selected_request(request, approved)?;
    let source = &approved.source;
    let spec = source.import_spec(request.font_id.clone(), request.required_text.clone());
    Ok(FontResource {
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
pub fn validate_fetch(
    id: &str,
    policy: &FontFetchPolicy,
) -> Result<&'static ApprovedFontSource, EditorError> {
    let pin = native_source(id)?;
    validate_selected_fetch(pin, policy)?;
    Ok(pin)
}
pub(super) fn validate_selected_fetch(
    pin: &ApprovedFontSource,
    policy: &FontFetchPolicy,
) -> Result<(), EditorError> {
    if !policy.allow_public_font_download {
        return Err(EditorError::new(
            "FONT_NETWORK_DENIED",
            "Task policy does not allow public font downloads",
        ));
    }
    if !(1..=30000).contains(&policy.deadline_ms)
        || !(1..=MAX_CATALOG_FONT_BYTES).contains(&policy.max_bytes)
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
    Ok(())
}

/// Uses the existing original-request receipt, planner, CAS lease and atomic
/// commit. Only source selection and native inspector are versioned.
pub fn import_static_font(
    store: &mut impl SceneStore,
    fetcher: &impl FontSourceFetch,
    inspector: &impl FontInspector,
    publisher: &impl FontCasPublish,
    request: &ImportFont,
    fetch_policy: &FontFetchPolicy,
    inspection_policy: &FontImportPolicy,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<TransactionResult, EditorError> {
    validate_import_request(request)?;
    import_selected_static_font(
        store,
        fetcher,
        inspector,
        publisher,
        request,
        fetch_policy,
        inspection_policy,
        checkpoint,
        native_source(&request.catalog_id)?,
    )
}
pub(super) fn import_selected_static_font(
    store: &mut impl SceneStore,
    fetcher: &impl FontSourceFetch,
    inspector: &impl FontInspector,
    publisher: &impl FontCasPublish,
    request: &ImportFont,
    fetch_policy: &FontFetchPolicy,
    inspection_policy: &FontImportPolicy,
    checkpoint: &mut dyn FontImportCheckpoint,
    pin: &ApprovedFontSource,
) -> Result<TransactionResult, EditorError> {
    validate_selected_request(request, pin)?;
    let intent = SceneCommitIntent::FontImport(request);
    let source = match begin(store, intent)? {
        Ok(value) => value,
        Err(result) => return Ok(result),
    };
    checkpoint.check()?;
    let font = expected_selected_resource(request, pin)?;
    let StoredProjectDocument::Editable(project) = &source.document else {
        return Err(EditorError::new(
            "UNSUPPORTED_PROJECT_VERSION",
            "Promote the project before importing fonts",
        ));
    };
    let operation = ApplySceneProject {
        project_id: request.project_id.clone(),
        expected_revision: request.expected_revision.clone(),
        idempotency_key: request.idempotency_key.clone(),
        label: format!("Import {}", font.family),
        operations: vec![ProjectOperation::FontPut { font: font.clone() }],
    };
    let proposed = plan(
        &EditorSnapshot {
            project: (**project).clone(),
            revision: source.revision.clone(),
        },
        &operation,
    )?;
    let commit = prepare(&source, proposed, intent)?;
    validate_selected_fetch(pin, fetch_policy)?;
    validate_catalog_font_envelope(
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
    let acquired = PreparedFont {
        resource: font,
        glyph_count: parsed.glyph_count,
        checked_codepoints: parsed.checked_codepoints,
    };
    checkpoint.check()?;
    let _publication = publisher.publish(&acquired, &bytes, checkpoint)?;
    checkpoint.check()?;
    store.commit_scene_atomic(intent, commit)
}
