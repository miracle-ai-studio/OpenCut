//! Bounded static OpenType inspection. Only caller-supplied authorized bytes are
//! read; no native font discovery, subprocess, URL fetch or fallback is used.
use opencut_contracts::{EditorError, scene::FontStyle};
use opencut_editor_application::font_import::*;
use std::collections::BTreeSet;
use ttf_parser::{Face, GlyphId, Language, RawFace, Tag, name_id};

pub(crate) mod fetch;
pub use fetch::GoogleFontsFetch;

#[cfg(test)]
pub(crate) mod tests;

pub struct StaticFontInspector;

fn malformed(message: &str) -> EditorError {
    EditorError::new("INVALID_FONT", message)
}
fn unsupported(message: &str) -> EditorError {
    EditorError::new("UNSUPPORTED_FONT", message)
}

/// ttf-parser intentionally ignores malformed optional tables. Check the SFNT
/// directory and checksums before parsing so damaged bytes cannot silently
/// become a different face. This is format validation, not a shaping oracle.
pub(crate) fn validate_sfnt(
    bytes: &[u8],
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<(), EditorError> {
    validate_sfnt_with(bytes, checkpoint, |tag| {
        if matches!(
            &tag,
            b"COLR" | b"SVG " | b"CBDT" | b"sbix" | b"EBDT" | b"bdat"
        ) {
            return Err(unsupported(
                "Color and bitmap glyph rendering requires an explicit capability",
            ));
        }
        Ok(())
    })
}

/// Shared byte integrity; the caller supplies only its extra table admission.
/// The original V1 color gate above and unconditional variation gate stay frozen.
pub(crate) fn validate_sfnt_with(
    bytes: &[u8],
    checkpoint: &mut dyn FontImportCheckpoint,
    mut admit: impl FnMut([u8; 4]) -> Result<(), EditorError>,
) -> Result<(), EditorError> {
    if bytes.starts_with(b"ttcf") {
        return Err(unsupported(
            "Font collections need an explicit collection contract",
        ));
    }
    if !bytes.starts_with(&[0, 1, 0, 0]) && !bytes.starts_with(b"OTTO") {
        return Err(unsupported(
            "Only static TrueType or OpenType SFNT files are supported",
        ));
    }
    let raw = RawFace::parse(bytes, 0).map_err(|_| malformed("Invalid SFNT directory"))?;
    let count = raw.table_records.len();
    if count == 0 || count > 128 {
        return Err(malformed("Font table count exceeds the supported envelope"));
    }
    let directory_end = 12 + usize::from(count) * 16;
    let mut prior = None;
    let mut ranges = Vec::with_capacity(usize::from(count));
    for record in raw.table_records {
        checkpoint.check()?;
        if prior.is_some_and(|tag| tag >= record.tag) {
            return Err(malformed("Font tables must be uniquely sorted"));
        }
        prior = Some(record.tag);
        let tag = record.tag.to_bytes();
        if matches!(
            &tag,
            b"fvar" | b"gvar" | b"CFF2" | b"avar" | b"HVAR" | b"VVAR" | b"MVAR"
        ) {
            return Err(unsupported(
                "Variable-font instances are not represented by this contract",
            ));
        }
        admit(tag)?;
        let start = record.offset as usize;
        let end = start
            .checked_add(record.length as usize)
            .ok_or_else(|| malformed("Font table length overflow"))?;
        if start < directory_end || start % 4 != 0 || end > bytes.len() {
            return Err(malformed("Font table points outside its byte envelope"));
        }
        if record.length > 0 {
            ranges.push((start, end));
        }
        let table = &bytes[start..end];
        let mut checksum = 0u32;
        for (i, word) in table.chunks(4).enumerate() {
            if i % 16384 == 0 {
                checkpoint.check()?;
            }
            let mut padded = [0; 4];
            padded[..word.len()].copy_from_slice(word);
            if tag == *b"head" && i == 2 {
                padded = [0; 4];
            }
            checksum = checksum.wrapping_add(u32::from_be_bytes(padded));
        }
        if checksum != record.check_sum {
            return Err(malformed("Font table checksum does not match its bytes"));
        }
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|r| r[0].1 > r[1].0) {
        return Err(malformed("Font tables overlap"));
    }
    Ok(())
}

fn family(
    face: &Face<'_>,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<String, EditorError> {
    let mut best = None;
    for (index, name) in face.names().into_iter().enumerate() {
        if index % 32 == 0 {
            checkpoint.check()?;
        }
        if !matches!(name.name_id, name_id::TYPOGRAPHIC_FAMILY | name_id::FAMILY)
            || !name.is_unicode()
        {
            continue;
        }
        if name.name.len() > 1024 || name.name.len() % 2 != 0 {
            return Err(malformed("Font family name is malformed or too long"));
        }
        let value = name
            .to_string()
            .ok_or_else(|| malformed("Font family is not valid Unicode"))?;
        if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
            return Err(malformed("Invalid font family name"));
        }
        let priority = (
            name.language() != Language::English_UnitedStates,
            name.name_id != name_id::TYPOGRAPHIC_FAMILY,
        );
        let candidate = (priority, value);
        if best.as_ref().is_none_or(|current| &candidate < current) {
            best = Some(candidate);
        }
    }
    best.map(|(_, value)| value)
        .ok_or_else(|| malformed("Font has no supported Unicode family name"))
}

impl FontInspector for StaticFontInspector {
    fn inspect(
        &self,
        bytes: &[u8],
        required_text: &str,
        policy: &FontImportPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<ParsedFontFace, EditorError> {
        checkpoint.check()?;
        validate_font_input(bytes, required_text, policy)?;
        validate_sfnt(bytes, checkpoint)?;
        inspect_static_face(bytes, required_text, checkpoint)
    }
}

/// Existing metadata, Unicode coverage and outline requirements shared verbatim.
pub(crate) fn inspect_static_face(
    bytes: &[u8],
    required_text: &str,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<ParsedFontFace, EditorError> {
    let face =
        Face::parse(bytes, 0).map_err(|_| malformed("Font face tables could not be parsed"))?;
    let tables = face.tables();
    if tables.os2.is_none()
        || tables.name.is_none()
        || tables.cmap.is_none()
        || tables.hmtx.is_none()
        || (tables.glyf.is_none() && tables.cff.is_none())
    {
        return Err(malformed(
            "Font requires valid names, style, Unicode cmap, metrics and outlines",
        ));
    }
    if !tables
        .cmap
        .unwrap()
        .subtables
        .into_iter()
        .any(|subtable| subtable.is_unicode())
    {
        return Err(unsupported("Font has no Unicode character map"));
    }
    // Shaping tables may be absent, but a declared malformed table is not
    // silently discarded while claiming the original face was inspected.
    for (tag, parsed) in [
        (*b"GSUB", tables.gsub.is_some()),
        (*b"GPOS", tables.gpos.is_some()),
        (*b"GDEF", tables.gdef.is_some()),
    ] {
        if face.raw_face().table(Tag::from_bytes(&tag)).is_some() && !parsed {
            return Err(malformed("Declared font shaping table could not be parsed"));
        }
    }
    if face.is_oblique() {
        return Err(unsupported(
            "Oblique is not an expressed font style in this version",
        ));
    }
    let weight = face.weight().to_number();
    if !(1..=1000).contains(&weight) {
        return Err(malformed(
            "Font numeric weight is outside the OpenType range",
        ));
    }
    let mut checked = BTreeSet::new();
    let mut missing = Vec::new();
    let mut missing_count = 0u32;
    for c in required_text.chars().filter(|c| *c != '\n' && *c != '\r') {
        if !checked.insert(c) {
            continue;
        }
        checkpoint.check()?;
        let glyph = face.glyph_index(c);
        if glyph.is_none_or(|g| g == GlyphId(0) || g.0 >= face.number_of_glyphs()) {
            missing_count += 1;
            if missing.len() < 32 {
                missing.push(format!("U+{:04X}", c as u32));
            }
        } else if face.glyph_hor_advance(glyph.unwrap()).is_none() {
            return Err(malformed("Mapped font glyph has no horizontal metrics"));
        }
    }
    if missing_count != 0 {
        let mut error = EditorError::new(
            "FONT_GLYPH_MISSING",
            "Locked font does not cover the required text",
        );
        error.details =
            serde_json::json!({"missing_codepoints": missing, "missing_count": missing_count});
        return Err(error);
    }
    checkpoint.check()?;
    Ok(ParsedFontFace {
        family: family(&face, checkpoint)?,
        weight,
        style: if face.is_italic() {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        },
        glyph_count: face.number_of_glyphs(),
        checked_codepoints: checked.len() as u32,
    })
}
