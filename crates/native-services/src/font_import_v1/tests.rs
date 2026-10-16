use super::*;
use std::collections::BTreeMap;

// Construct our own tiny static font. It is a parser/identity fixture, not an
// expected-pixel oracle or a copy of a third-party redistributable font.
pub(crate) fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_be_bytes());
}
pub(crate) fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
}
pub(crate) fn table_map() -> BTreeMap<[u8; 4], Vec<u8>> {
    let mut tables = BTreeMap::new();
    let mut head = vec![0; 54];
    put32(&mut head, 0, 0x10000);
    put32(&mut head, 12, 0x5f0f3cf5);
    put16(&mut head, 18, 1000);
    let mut hhea = vec![0; 36];
    put32(&mut hhea, 0, 0x10000);
    put16(&mut hhea, 4, 800);
    put16(&mut hhea, 6, (-200i16) as u16);
    put16(&mut hhea, 34, 3);
    let mut maxp = vec![0; 32];
    put32(&mut maxp, 0, 0x10000);
    put16(&mut maxp, 4, 3);
    let mut os2 = vec![0; 78];
    put16(&mut os2, 4, 400);
    put16(&mut os2, 6, 5);
    put16(&mut os2, 62, 0x40);
    let mut name = vec![0; 18];
    put16(&mut name, 2, 1);
    put16(&mut name, 4, 18);
    put16(&mut name, 6, 3);
    put16(&mut name, 8, 1);
    put16(&mut name, 10, 0x409);
    put16(&mut name, 12, 1);
    let family: Vec<_> = "Fixture Sans"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    put16(&mut name, 14, family.len() as u16);
    name.extend(family);
    let mut chars: Vec<_> = " AVTo中文编辑".chars().collect();
    chars.sort_unstable();
    chars.dedup();
    let mut cmap = vec![0; 28 + chars.len() * 12];
    put16(&mut cmap, 2, 1);
    put16(&mut cmap, 4, 3);
    put16(&mut cmap, 6, 10);
    put32(&mut cmap, 8, 12);
    put16(&mut cmap, 12, 12);
    let cmap_len = cmap.len();
    put32(&mut cmap, 16, (cmap_len - 12) as u32);
    put32(&mut cmap, 24, chars.len() as u32);
    for (i, c) in chars.into_iter().enumerate() {
        put32(&mut cmap, 28 + i * 12, c as u32);
        put32(&mut cmap, 32 + i * 12, c as u32);
        put32(&mut cmap, 36 + i * 12, if c == ' ' { 1 } else { 2 });
    }
    let mut glyf = vec![0; 34];
    put16(&mut glyf, 0, 1);
    put16(&mut glyf, 6, 100);
    put16(&mut glyf, 8, 100);
    put16(&mut glyf, 10, 3);
    glyf[14..18].fill(1);
    for (i, n) in [0i16, 100, 0, -100, 0, 0, 100, 0].into_iter().enumerate() {
        put16(&mut glyf, 18 + i * 2, n as u16);
    }
    let mut loca = vec![0; 8];
    put16(&mut loca, 6, (glyf.len() / 2) as u16);
    let mut hmtx = vec![0; 12];
    for i in 0..3 {
        put16(&mut hmtx, i * 4, 600);
    }
    let mut post = vec![0; 32];
    put32(&mut post, 0, 0x30000);
    for (tag, value) in [
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"maxp", maxp),
        (*b"OS/2", os2),
        (*b"name", name),
        (*b"cmap", cmap),
        (*b"glyf", glyf),
        (*b"loca", loca),
        (*b"hmtx", hmtx),
        (*b"post", post),
    ] {
        tables.insert(tag, value);
    }
    tables
}
pub(crate) fn encode(tables: BTreeMap<[u8; 4], Vec<u8>>) -> Vec<u8> {
    let mut bytes = vec![0; 12 + tables.len() * 16];
    put32(&mut bytes, 0, 0x10000);
    put16(&mut bytes, 4, tables.len() as u16);
    for (i, (tag, data)) in tables.into_iter().enumerate() {
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }
        let offset = bytes.len();
        let checksum = data.chunks(4).fold(0u32, |sum, word| {
            let mut padded = [0; 4];
            padded[..word.len()].copy_from_slice(word);
            sum.wrapping_add(u32::from_be_bytes(padded))
        });
        let at = 12 + i * 16;
        bytes[at..at + 4].copy_from_slice(&tag);
        put32(&mut bytes, at + 4, checksum);
        put32(&mut bytes, at + 8, offset as u32);
        put32(&mut bytes, at + 12, data.len() as u32);
        bytes.extend(data);
    }
    bytes
}
fn inspect(bytes: &[u8], text: &str) -> Result<ParsedFontFaceV1, EditorError> {
    StaticFontInspectorV1.inspect(bytes, text, &FontImportPolicyV1::default(), &mut || Ok(()))
}

#[test]
fn reads_real_static_sfnt_metadata_and_latin_cjk_cmap() {
    let bytes = encode(table_map());
    let parsed = inspect(&bytes, "AV To\n中文编辑").unwrap();
    assert_eq!(parsed.family, "Fixture Sans");
    assert_eq!(parsed.weight, 400);
    assert_eq!(parsed.style, FontStyleV4::Normal);
    assert_eq!(parsed.glyph_count, 3);
    assert_eq!(parsed.checked_codepoints, 9);
    assert!(
        Face::parse(&bytes, 0)
            .unwrap()
            .glyph_bounding_box(GlyphId(2))
            .is_some()
    );
}

#[test]
fn reports_missing_glyphs_without_system_fallback_and_rechecks_new_text() {
    let bytes = encode(table_map());
    inspect(&bytes, "中文").unwrap();
    let error = inspect(&bytes, "中文Z🦀Z").unwrap_err();
    assert_eq!(error.code, "FONT_GLYPH_MISSING");
    assert_eq!(error.details["missing_count"], 2);
    assert_eq!(
        error.details["missing_codepoints"],
        serde_json::json!(["U+005A", "U+1F980"])
    );
}

#[test]
fn rejects_variable_collections_and_unexpressed_oblique_faces() {
    let mut tables = table_map();
    tables.insert(*b"fvar", vec![0; 16]);
    assert_eq!(
        inspect(&encode(tables), "A").unwrap_err().code,
        "UNSUPPORTED_FONT"
    );
    assert_eq!(
        inspect(b"ttcf\0\0\0\0", "A").unwrap_err().code,
        "UNSUPPORTED_FONT"
    );
    let mut tables = table_map();
    let os2 = tables.get_mut(b"OS/2").unwrap();
    os2.resize(96, 0);
    put16(os2, 0, 4);
    put16(os2, 62, 1 << 9);
    assert_eq!(
        inspect(&encode(tables), "A").unwrap_err().code,
        "UNSUPPORTED_FONT"
    );
}

#[test]
fn parses_bold_italic_from_bytes_instead_of_declared_names() {
    let mut tables = table_map();
    let os2 = tables.get_mut(b"OS/2").unwrap();
    put16(os2, 4, 700);
    put16(os2, 62, 0x21);
    let parsed = inspect(&encode(tables), "A").unwrap();
    assert_eq!(parsed.weight, 700);
    assert_eq!(parsed.style, FontStyleV4::Italic);
}

#[test]
fn rejects_corrupt_checksums_and_silently_skipped_optional_tables() {
    let mut bytes = encode(table_map());
    *bytes.last_mut().unwrap() ^= 1;
    assert_eq!(inspect(&bytes, "A").unwrap_err().code, "INVALID_FONT");
    let mut tables = table_map();
    tables.insert(*b"cmap", vec![0; 3]);
    assert_eq!(
        inspect(&encode(tables), "A").unwrap_err().code,
        "INVALID_FONT"
    );
    let mut tables = table_map();
    tables.insert(*b"GSUB", vec![0; 3]);
    assert_eq!(
        inspect(&encode(tables), "A").unwrap_err().code,
        "INVALID_FONT"
    );
}

#[test]
fn zero_glyph_and_out_of_range_cmap_never_count_as_coverage() {
    for glyph in [0, 3] {
        let mut tables = table_map();
        let cmap = tables.get_mut(b"cmap").unwrap();
        // A is the second sorted group, following space.
        put32(cmap, 48, glyph);
        assert_eq!(
            inspect(&encode(tables), "A").unwrap_err().code,
            "FONT_GLYPH_MISSING"
        );
    }
}

#[test]
fn bounded_input_and_cancellation_errors_survive_native_parsing() {
    let bytes = encode(table_map());
    let policy = FontImportPolicyV1 {
        max_font_bytes: 10,
        ..Default::default()
    };
    assert_eq!(
        StaticFontInspectorV1
            .inspect(&bytes, "A", &policy, &mut || Ok(()))
            .unwrap_err()
            .code,
        "FONT_BUDGET_EXCEEDED"
    );
    let mut checkpoints = 0;
    let error = StaticFontInspectorV1
        .inspect(&bytes, "A", &FontImportPolicyV1::default(), &mut || {
            checkpoints += 1;
            if checkpoints == 4 {
                Err(EditorError::new("JOB_CANCELLED", "cancel parser"))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
    assert_eq!(error.code, "JOB_CANCELLED");
}

#[test]
#[ignore = "requires unchanged licensed public fixtures in OPENCUT_FONT_FIXTURE_DIR"]
fn pinned_google_fonts_parse_with_their_actual_metadata_and_current_text() {
    use opencut_editor_application::font_import_v1::catalog::*;
    use sha2::{Digest, Sha256};
    use std::{fs::File, io::Read, path::PathBuf};
    let root = PathBuf::from(
        std::env::var_os("OPENCUT_FONT_FIXTURE_DIR").expect("explicit font fixture directory"),
    );
    for (catalog_id, filename, text) in [
        ("google-lato-regular-v1", "lato-Lato-Regular.ttf", "AV To"),
        (
            "google-mashanzheng-regular-v1",
            "mashanzheng-MaShanZheng-Regular.ttf",
            "AV To\n中文编辑",
        ),
    ] {
        let source = pinned_font_source_v1(catalog_id).unwrap();
        let mut bytes = Vec::new();
        File::open(root.join(filename))
            .unwrap()
            .take(MAX_FONT_BYTES_V1 + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes.len() as u64, source.size_bytes);
        assert_eq!(format!("{:x}", Sha256::digest(&bytes)), source.content_hash);
        let prepared = prepare_font_v1(
            &StaticFontInspectorV1,
            &bytes,
            &source.import_spec("font".into(), text.into()),
            &FontImportPolicyV1::default(),
            &mut || Ok(()),
        )
        .unwrap();
        assert_eq!(prepared.resource.family, source.family);
        assert_eq!(prepared.resource.weight, 400);
        assert_eq!(prepared.resource.style, FontStyleV4::Normal);
        assert_eq!(
            prepared.resource.license.unwrap().notice.as_deref(),
            Some(source.license_notice)
        );
        assert_eq!(
            inspect(&bytes, "\u{10ffff}").unwrap_err().code,
            "FONT_GLYPH_MISSING"
        );
    }
}

#[test]
#[ignore = "requires fixed Google source cache in OPENCUT_FONT_CATALOG_CACHE; never fetches"]
fn every_pinned_classic_static_face_matches_native_inspector() {
    use opencut_editor_application::font_import_v1::catalog::*;
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;
    let root = PathBuf::from(
        std::env::var_os("OPENCUT_FONT_CATALOG_CACHE").expect("explicit catalog fixture cache"),
    );
    let prefix =
        format!("https://raw.githubusercontent.com/google/fonts/{GOOGLE_FONTS_COMMIT_V1}/");
    let mut failures = Vec::new();
    for source in font_catalog_v1() {
        let encoded = source.source_url.strip_prefix(&prefix).unwrap().as_bytes();
        let mut path = Vec::new();
        let mut offset = 0;
        while offset < encoded.len() {
            if encoded[offset] == b'%' {
                let hex = std::str::from_utf8(&encoded[offset + 1..offset + 3]).unwrap();
                path.push(u8::from_str_radix(hex, 16).unwrap());
                offset += 3;
            } else {
                path.push(encoded[offset]);
                offset += 1;
            }
        }
        let path = String::from_utf8(path).unwrap();
        assert!(!path.contains(".."));
        let bytes = match std::fs::read(root.join(path)) {
            Ok(bytes) => bytes,
            Err(error) => {
                failures.push(format!("{}: fixture {error}", source.catalog_id));
                continue;
            }
        };
        if bytes.len() as u64 != source.size_bytes
            || format!("{:x}", Sha256::digest(&bytes)) != source.content_hash
        {
            failures.push(format!("{}: source hash/size mismatch", source.catalog_id));
            continue;
        }
        // Aggregate all native failures in one scan; glyph coverage remains
        // a per-import/per-render check for actual required text.
        match StaticFontInspectorV1
            .inspect(&bytes, "", &FontImportPolicyV1::default(), &mut || Ok(()))
        {
            Ok(parsed)
                if parsed.family == source.family
                    && parsed.weight == source.weight
                    && parsed.style == source.style => {}
            Ok(parsed) => {
                failures.push(format!("{}: parsed identity {parsed:?}", source.catalog_id))
            }
            Err(error) => failures.push(format!("{}: {error:?}", source.catalog_id)),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} pinned fonts failed native inspection:\n{}",
        failures.len(),
        font_catalog_v1().len(),
        failures.join("\n")
    );
}
