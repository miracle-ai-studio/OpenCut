//! Static font inspection and immutable resource preparation. Upload ownership,
//! read authority, CAS publication and revision commits stay with existing ports.
use opencut_contracts::{AssetVersion, EditorError, project_v4::*};
use sha2::{Digest, Sha256};

pub mod catalog;
pub mod recovery;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod transaction_tests;

pub const MAX_FONT_BYTES_V1: u64 = 32 * 1024 * 1024;
pub const MAX_FONT_TEXT_BYTES_V1: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub struct FontImportSpecV1 {
    pub font_id: String,
    pub family: String,
    pub weight: u16,
    pub style: FontStyleV4,
    pub origin: ResourceOriginV4,
    pub license: ResourceLicenseV4,
    /// Coverage evidence for these characters only. Every render must inspect
    /// its current text against the same locked bytes again.
    pub required_text: String,
}

#[derive(Clone, Copy, Debug)]
pub struct FontImportPolicyV1 {
    pub max_font_bytes: u64,
    pub max_text_bytes: usize,
}
impl Default for FontImportPolicyV1 {
    fn default() -> Self {
        Self {
            max_font_bytes: MAX_FONT_BYTES_V1,
            max_text_bytes: MAX_FONT_TEXT_BYTES_V1,
        }
    }
}

/// Callers propagate their existing cancellation, deadline and live authority
/// checks. Parsing never clears or replaces an upstream error.
pub trait FontImportCheckpointV1 {
    fn check(&mut self) -> Result<(), EditorError>;
}
impl<F: FnMut() -> Result<(), EditorError>> FontImportCheckpointV1 for F {
    fn check(&mut self) -> Result<(), EditorError> {
        self()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedFontFaceV1 {
    pub family: String,
    pub weight: u16,
    pub style: FontStyleV4,
    pub glyph_count: u16,
    pub checked_codepoints: u32,
}

/// Native parsing is injected by host assembly. No system-font lookup, URL
/// fetching, fallback face or filesystem locator crosses this port.
pub trait FontInspectorV1 {
    fn inspect(
        &self,
        bytes: &[u8],
        required_text: &str,
        policy: &FontImportPolicyV1,
        checkpoint: &mut dyn FontImportCheckpointV1,
    ) -> Result<ParsedFontFaceV1, EditorError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedFontV1 {
    pub resource: FontResourceV4,
    pub glyph_count: u16,
    pub checked_codepoints: u32,
}

/// Publishes already inspected bytes through the host's managed CAS. The
/// returned lease MUST retain its shared CAS gate until the project transaction
/// returns, including cancellation, replay and revision-conflict paths.
pub trait FontCasPublishV1 {
    type Lease;
    fn publish(
        &self,
        font: &PreparedFontV1,
        bytes: &[u8],
        checkpoint: &mut dyn FontImportCheckpointV1,
    ) -> Result<Self::Lease, EditorError>;
}

pub fn validate_import_request_v1(
    request: &opencut_contracts::font_import_v1::ImportFontV1,
) -> Result<(), EditorError> {
    crate::require_idempotency_key(&request.idempotency_key)?;
    if [
        &request.project_id,
        &request.expected_revision,
        &request.font_id,
        &request.catalog_id,
    ]
    .iter()
    .any(|value| !bounded_text(value, 1024))
    {
        return Err(invalid(
            "Font import identifiers must be nonempty bounded text",
        ));
    }
    let source = catalog::pinned_font_source_v1(&request.catalog_id)?;
    validate_font_envelope_v1(
        source.size_bytes,
        &request.required_text,
        &FontImportPolicyV1::default(),
    )
}

/// Exact admissible resource for this public request. A catalog supplies
/// provenance and licensed content; the caller supplies only its logical ID.
pub fn expected_import_resource_v1(
    request: &opencut_contracts::font_import_v1::ImportFontV1,
) -> Result<FontResourceV4, EditorError> {
    validate_import_request_v1(request)?;
    let source = catalog::pinned_font_source_v1(&request.catalog_id)?;
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

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn bounded_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

/// Structural provenance admission, not proof of a license grant. The source
/// adapter must obtain these exact bytes and the complete matching license from
/// the admitted upstream. URLs are provenance only, never rendering authority.
pub fn validate_font_spec_v1(spec: &FontImportSpecV1) -> Result<(), EditorError> {
    if !bounded_text(&spec.font_id, 1024)
        || !bounded_text(&spec.family, 512)
        || !(1..=1000).contains(&spec.weight)
    {
        return Err(invalid("Invalid font identifier, family or numeric weight"));
    }
    let ResourceOriginV4::Online {
        provider,
        source_url,
    } = &spec.origin
    else {
        return Err(invalid(
            "Font import requires retained Google Fonts provenance",
        ));
    };
    if provider != "google-fonts" || !google_source(source_url) {
        return Err(invalid("Unsupported font provenance"));
    }
    if !matches!(
        spec.license.identifier.as_deref(),
        Some("OFL-1.1" | "Apache-2.0")
    ) || spec
        .license
        .source_url
        .as_deref()
        .is_none_or(|url| !google_source(url))
        || spec.license.notice.as_deref().is_none_or(|notice| {
            notice.trim().is_empty() || notice.len() > 64 * 1024 || notice.contains('\0')
        })
        || spec
            .license
            .attribution
            .as_deref()
            .is_some_and(|text| text.len() > 16 * 1024 || text.contains('\0'))
    {
        return Err(invalid(
            "Font import requires a supported license, source and complete notice",
        ));
    }
    Ok(())
}

fn google_source(value: &str) -> bool {
    bounded_text(value, 4096)
        && !value.contains(['\\', '#', '@'])
        && [
            "https://fonts.gstatic.com/",
            "https://fonts.google.com/",
            "https://fonts.googleapis.com/",
            "https://raw.githubusercontent.com/google/fonts/",
            "https://github.com/google/fonts/",
            "https://raw.githubusercontent.com/notofonts/",
            "https://github.com/notofonts/",
        ]
        .iter()
        .any(|prefix| value.starts_with(prefix))
}

pub fn validate_font_input_v1(
    bytes: &[u8],
    required_text: &str,
    policy: &FontImportPolicyV1,
) -> Result<(), EditorError> {
    validate_font_envelope_v1(bytes.len() as u64, required_text, policy)
}

pub fn validate_font_envelope_v1(
    size_bytes: u64,
    required_text: &str,
    policy: &FontImportPolicyV1,
) -> Result<(), EditorError> {
    if !(1..=MAX_FONT_BYTES_V1).contains(&policy.max_font_bytes)
        || !(1..=MAX_FONT_TEXT_BYTES_V1).contains(&policy.max_text_bytes)
    {
        return Err(invalid(
            "Font inspection policy exceeds its bounded envelope",
        ));
    }
    if size_bytes == 0 || size_bytes > policy.max_font_bytes {
        return Err(EditorError::new(
            "FONT_BUDGET_EXCEEDED",
            "Font byte limit exceeded",
        ));
    }
    if required_text.len() > policy.max_text_bytes {
        return Err(EditorError::new(
            "FONT_BUDGET_EXCEEDED",
            "Font text inspection limit exceeded",
        ));
    }
    if required_text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r')
    {
        return Err(invalid("Font text contains unsupported control characters"));
    }
    Ok(())
}

pub fn prepare_font_v1(
    inspector: &impl FontInspectorV1,
    bytes: &[u8],
    spec: &FontImportSpecV1,
    policy: &FontImportPolicyV1,
    checkpoint: &mut dyn FontImportCheckpointV1,
) -> Result<PreparedFontV1, EditorError> {
    checkpoint.check()?;
    validate_font_spec_v1(spec)?;
    validate_font_input_v1(bytes, &spec.required_text, policy)?;
    let parsed = inspector.inspect(bytes, &spec.required_text, policy, checkpoint)?;
    if parsed.family != spec.family || parsed.weight != spec.weight || parsed.style != spec.style {
        return Err(EditorError::new(
            "FONT_FACE_MISMATCH",
            "Uploaded font does not match the requested family, weight and style",
        ));
    }
    if parsed.glyph_count == 0 {
        return Err(EditorError::new("INVALID_FONT", "Font contains no glyphs"));
    }
    let mut digest = Sha256::new();
    for chunk in bytes.chunks(64 * 1024) {
        checkpoint.check()?;
        digest.update(chunk);
    }
    checkpoint.check()?;
    let content_hash = format!("{:x}", digest.finalize());
    Ok(PreparedFontV1 {
        resource: FontResourceV4 {
            id: spec.font_id.clone(),
            family: parsed.family,
            weight: parsed.weight,
            style: parsed.style,
            face_index: 0,
            content: AssetVersion {
                id: format!("av_{content_hash}"),
                content_hash,
                size: bytes.len() as u64,
            },
            origin: spec.origin.clone(),
            license: Some(spec.license.clone()),
        },
        glyph_count: parsed.glyph_count,
        checked_codepoints: parsed.checked_codepoints,
    })
}
