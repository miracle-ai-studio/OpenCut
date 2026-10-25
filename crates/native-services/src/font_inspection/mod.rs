//! Explicit compiled-source admission for unchanged color/monochrome font bytes.
//! Unrecognized bytes retain V1's original strict static-outline behavior.
use crate::font_import::{
    StaticFontInspector, inspect_static_face, validate_sfnt, validate_sfnt_with,
};
use opencut_contracts::{EditorError, font_catalog::FontRenderProfile};
use opencut_editor_application::{
    font_catalog::{
        ApprovedFontSource, approved_native_source, validate_catalog_font_envelope,
        validate_catalog_font_policy,
    },
    font_import::*,
};
use sha2::{Digest, Sha256};
use ttf_parser::{Face, Tag};

mod bitmap;
mod color;
#[cfg(test)]
mod tests;

pub struct CatalogFontInspector;
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_FONT", message)
}
fn unsupported(message: &str) -> EditorError {
    EditorError::new("UNSUPPORTED_FONT", message)
}
fn digest(bytes: &[u8], checkpoint: &mut dyn FontImportCheckpoint) -> Result<String, EditorError> {
    let mut hash = Sha256::new();
    for bytes in bytes.chunks(65536) {
        checkpoint.check()?;
        hash.update(bytes)
    }
    Ok(format!("{:x}", hash.finalize()))
}
impl FontInspector for CatalogFontInspector {
    fn inspect(
        &self,
        bytes: &[u8],
        text: &str,
        policy: &FontImportPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<ParsedFontFace, EditorError> {
        checkpoint.check()?;
        validate_catalog_font_policy(policy)?;
        validate_font_envelope(1, text, &historical_font_policy(policy))?;
        // Check the caller's bound before hashing. The exact compiled identity
        // is still required below before any >32 MiB face can be parsed.
        if bytes.is_empty() || bytes.len() as u64 > policy.max_font_bytes {
            return Err(EditorError::new(
                "FONT_BUDGET_EXCEEDED",
                "Font byte limit exceeded",
            ));
        }
        let hash = digest(bytes, checkpoint)?;
        validate_catalog_font_envelope(&hash, bytes.len() as u64, text, policy)?;
        match approved_native_source(&hash, bytes.len() as u64) {
            Some(pin) => inspect_approved(bytes, text, policy, pin, checkpoint),
            None => StaticFontInspector.inspect(
                bytes,
                text,
                &historical_font_policy(policy),
                checkpoint,
            ),
        }
    }
}
/// Resource readers already checked this hash against every byte of their CAS
/// transfer. Avoid hashing ordinary V1 fonts again just to select the inspector.
pub(crate) fn inspect_verified_resource(
    bytes: &[u8],
    content_hash: &str,
    text: &str,
    policy: &FontImportPolicy,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<ParsedFontFace, EditorError> {
    checkpoint.check()?;
    validate_catalog_font_envelope(content_hash, bytes.len() as u64, text, policy)?;
    match approved_native_source(content_hash, bytes.len() as u64) {
        Some(pin) => inspect_approved(bytes, text, policy, pin, checkpoint),
        None => {
            StaticFontInspector.inspect(bytes, text, &historical_font_policy(policy), checkpoint)
        }
    }
}
fn historical_font_policy(policy: &FontImportPolicy) -> FontImportPolicy {
    FontImportPolicy {
        max_font_bytes: policy.max_font_bytes.min(MAX_FONT_BYTES),
        ..*policy
    }
}
fn inspect_approved(
    bytes: &[u8],
    text: &str,
    policy: &FontImportPolicy,
    pin: &ApprovedFontSource,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<ParsedFontFace, EditorError> {
    use FontRenderProfile::*;
    checkpoint.check()?;
    validate_catalog_font_envelope(pin.source.content_hash, bytes.len() as u64, text, policy)?;
    if bytes.len() as u64 != pin.source.size_bytes {
        return Err(invalid("Approved font size differs from its bytes"));
    }
    if matches!(pin.render_profile, StaticOutline) {
        // Reuse V1's unchanged table, metadata and coverage checks. Only the
        // exact compiled V3 identity above can select a larger byte envelope.
        validate_sfnt(bytes, checkpoint)?;
        let parsed = inspect_static_face(bytes, text, checkpoint)?;
        return match_identity(parsed, pin);
    }
    validate_sfnt_with(bytes, checkpoint, |tag| {
        let color = matches!(pin.render_profile, ColrV0 | ColrV1);
        if matches!(&tag, b"CBDT" | b"CBLC" | b"sbix" | b"bdat" | b"bloc")
            || (matches!(&tag, b"COLR" | b"CPAL" | b"SVG ") && !color)
            || (matches!(&tag, b"EBDT" | b"EBLC") && color)
        {
            return Err(unsupported(
                "Declared font table is outside its approved rendering profile",
            ));
        }
        Ok(())
    })?;
    let face =
        Face::parse(bytes, 0).map_err(|_| invalid("Approved font face could not be parsed"))?;
    match pin.render_profile {
        ColrV0 | ColrV1 => color::validate(&face, pin.render_profile, checkpoint)?,
        MonochromeBitmap => bitmap::validate(&face, checkpoint)?,
        StaticOutline => unreachable!(),
    }
    // A simultaneous SVG table is retained byte-for-byte. This profile consumes
    // COLR, never SVG-only fonts; malformed declared SVG tables are not ignored.
    if let Some(svg) = face.raw_face().table(Tag::from_bytes(b"SVG ")) {
        if face.tables().svg.is_none() {
            return Err(invalid("Declared SVG font table could not be parsed"));
        }
        color::validate_svg_envelope(svg, face.number_of_glyphs(), checkpoint)?;
    }
    match_identity(inspect_static_face(bytes, text, checkpoint)?, pin)
}
fn match_identity(
    parsed: ParsedFontFace,
    pin: &ApprovedFontSource,
) -> Result<ParsedFontFace, EditorError> {
    if parsed.family != pin.source.family
        || parsed.weight != pin.source.weight
        || parsed.style != pin.source.style
    {
        return Err(EditorError::new(
            "FONT_FACE_MISMATCH",
            "Approved font metadata differs from the actual fixed source",
        ));
    }
    Ok(parsed)
}
fn u16_at(data: &[u8], offset: usize) -> Result<u16, EditorError> {
    Ok(u16::from_be_bytes(
        data.get(offset..)
            .and_then(|data| data.get(..2))
            .ok_or_else(|| invalid("Truncated font table"))?
            .try_into()
            .unwrap(),
    ))
}
fn u32_at(data: &[u8], offset: usize) -> Result<u32, EditorError> {
    Ok(u32::from_be_bytes(
        data.get(offset..)
            .and_then(|data| data.get(..4))
            .ok_or_else(|| invalid("Truncated font table"))?
            .try_into()
            .unwrap(),
    ))
}
fn span(data: &[u8], offset: usize, count: usize, width: usize) -> Result<(), EditorError> {
    let end = count
        .checked_mul(width)
        .and_then(|len| offset.checked_add(len))
        .ok_or_else(|| invalid("Font table length overflow"))?;
    data.get(offset..end)
        .ok_or_else(|| invalid("Font table reference exceeds its bytes"))?;
    Ok(())
}
