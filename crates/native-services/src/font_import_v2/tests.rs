use super::*;
use crate::font_import_v1::tests::{encode, put16, put32, table_map};
use opencut_contracts::project_v4::FontStyleV4;
use opencut_editor_application::font_import_v1::catalog::PinnedFontSourceV1;

fn cpal() -> Vec<u8> {
    let mut data = vec![0; 18];
    put16(&mut data, 2, 1);
    put16(&mut data, 4, 1);
    put16(&mut data, 6, 1);
    put32(&mut data, 8, 14);
    data[14..18].copy_from_slice(&[0, 0, 255, 255]);
    data
}
fn colr(version: u16) -> Vec<u8> {
    if version == 0 {
        let mut data = vec![0; 24];
        put16(&mut data, 2, 1);
        put32(&mut data, 4, 14);
        put32(&mut data, 8, 20);
        put16(&mut data, 12, 1);
        put16(&mut data, 14, 2);
        put16(&mut data, 18, 1);
        put16(&mut data, 20, 2);
        data
    } else {
        let mut data = vec![0; 55];
        put16(&mut data, 0, 1);
        put32(&mut data, 14, 34);
        put32(&mut data, 34, 1);
        put16(&mut data, 38, 2);
        put32(&mut data, 40, 10);
        data[44] = 10;
        data[47] = 6;
        put16(&mut data, 48, 2);
        data[50] = 2;
        put16(&mut data, 53, 0x4000);
        data
    }
}
fn color_font(version: u16) -> Vec<u8> {
    let mut tables = table_map();
    tables.insert(*b"CPAL", cpal());
    tables.insert(*b"COLR", colr(version));
    encode(tables)
}
fn mono_tables() -> std::collections::BTreeMap<[u8; 4], Vec<u8>> {
    let mut tables = table_map();
    let mut loc = vec![0; 76];
    put32(&mut loc, 0, 0x20000);
    put32(&mut loc, 4, 1);
    put32(&mut loc, 8, 56);
    put32(&mut loc, 12, 20);
    put32(&mut loc, 16, 1);
    put16(&mut loc, 48, 2);
    put16(&mut loc, 50, 2);
    loc[52..56].copy_from_slice(&[10, 10, 1, 1]);
    put16(&mut loc, 56, 2);
    put16(&mut loc, 58, 2);
    put32(&mut loc, 60, 8);
    put16(&mut loc, 64, 3);
    put16(&mut loc, 66, 1);
    put32(&mut loc, 68, 4);
    put16(&mut loc, 74, 6);
    let mut data = vec![0; 10];
    put32(&mut data, 0, 0x20000);
    data[4..10].copy_from_slice(&[1, 1, 0, 1, 1, 0x80]);
    tables.insert(*b"EBLC", loc);
    tables.insert(*b"EBDT", data);
    tables
}
fn pin(bytes: &[u8], profile: FontRenderProfileV3) -> ApprovedFontSourceV3 {
    ApprovedFontSourceV3 {
        source: PinnedFontSourceV1 {
            catalog_id: "private-unit-fixture",
            family: "Fixture Sans",
            weight: 400,
            style: FontStyleV4::Normal,
            source_commit: "fixture",
            source_url: "https://example.invalid/fixture",
            content_hash: Box::leak(format!("{:x}", Sha256::digest(bytes)).into_boxed_str()),
            size_bytes: bytes.len() as u64,
            license_url: "https://example.invalid/license",
            license_sha256: "",
            license_identifier: "fixture",
            license_notice: "Independent parser fixture",
            attribution: "OpenCut test",
        },
        render_profile: profile,
    }
}
fn inspect(
    bytes: &[u8],
    profile: FontRenderProfileV3,
    text: &str,
) -> Result<ParsedFontFaceV1, EditorError> {
    inspect_approved(
        bytes,
        text,
        &FontImportPolicyV1::default(),
        &pin(bytes, profile),
        &mut || Ok(()),
    )
}

#[test]
fn approved_color_profiles_validate_real_palette_graph_and_preserve_v1_negative_control() {
    for (version, profile) in [
        (0, FontRenderProfileV3::ColrV0),
        (1, FontRenderProfileV3::ColrV1),
    ] {
        let bytes = color_font(version);
        assert_eq!(
            inspect(&bytes, profile, "AV").unwrap().family,
            "Fixture Sans"
        );
        assert_eq!(
            StaticFontInspectorV1
                .inspect(&bytes, "AV", &FontImportPolicyV1::default(), &mut || Ok(()))
                .unwrap_err()
                .code,
            "UNSUPPORTED_FONT"
        );
        // The public V2 inspector cannot admit a valid color font merely because
        // a test can invoke its private approved-source implementation.
        assert_eq!(
            CatalogFontInspectorV2
                .inspect(&bytes, "AV", &FontImportPolicyV1::default(), &mut || Ok(()))
                .unwrap_err()
                .code,
            "UNSUPPORTED_FONT"
        );
    }
}
#[test]
fn colrv1_nested_damage_palette_bounds_cycles_and_variable_paints_are_not_silently_dropped() {
    for mutation in 0..5 {
        let mut tables = table_map();
        let mut color = colr(1);
        let mut palette = cpal();
        match mutation {
            0 => color[50] = 255,
            1 => put16(&mut color, 51, 1),
            2 => {
                color[44] = 11;
                put16(&mut color, 45, 2)
            }
            3 => color[50] = 3,
            4 => put16(&mut palette, 12, 1),
            _ => unreachable!(),
        }
        tables.insert(*b"COLR", color);
        tables.insert(*b"CPAL", palette);
        assert!(
            inspect(&encode(tables), FontRenderProfileV3::ColrV1, "A").is_err(),
            "mutation {mutation}"
        );
    }
    assert!(inspect(&color_font(0), FontRenderProfileV3::ColrV1, "A").is_err());
}
#[test]
fn color_plus_svg_retains_both_tables_but_svg_only_and_variable_sources_stay_outside_profile() {
    let mut tables = table_map();
    tables.insert(*b"COLR", colr(0));
    tables.insert(*b"CPAL", cpal());
    let document=b"<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"100\" height=\"100\" fill=\"red\"/></svg>";
    let mut svg = vec![0; 24];
    put32(&mut svg, 2, 10);
    put16(&mut svg, 10, 1);
    put16(&mut svg, 12, 2);
    put16(&mut svg, 14, 2);
    put32(&mut svg, 16, 14);
    put32(&mut svg, 20, document.len() as u32);
    svg.extend(document);
    tables.insert(*b"SVG ", svg);
    let bytes = encode(tables.clone());
    inspect(&bytes, FontRenderProfileV3::ColrV0, "A").unwrap();
    assert!(Face::parse(&bytes, 0).unwrap().tables().svg.is_some());
    tables.remove(b"COLR");
    tables.remove(b"CPAL");
    assert!(inspect(&encode(tables), FontRenderProfileV3::ColrV0, "A").is_err());
    let mut tables = table_map();
    tables.insert(*b"COLR", colr(0));
    tables.insert(*b"CPAL", cpal());
    tables.insert(*b"fvar", vec![0; 16]);
    assert_eq!(
        inspect(&encode(tables), FontRenderProfileV3::ColrV0, "A")
            .unwrap_err()
            .code,
        "UNSUPPORTED_FONT"
    );
}
#[test]
fn approved_fonts_still_reject_missing_glyphs_identity_budget_and_cancellation() {
    let bytes = color_font(0);
    let profile = FontRenderProfileV3::ColrV0;
    assert_eq!(
        inspect(&bytes, profile, "🦄").unwrap_err().code,
        "FONT_GLYPH_MISSING"
    );
    let mut approved = pin(&bytes, profile);
    approved.source.weight = 700;
    assert_eq!(
        inspect_approved(
            &bytes,
            "A",
            &FontImportPolicyV1::default(),
            &approved,
            &mut || Ok(())
        )
        .unwrap_err()
        .code,
        "FONT_FACE_MISMATCH"
    );
    let policy = FontImportPolicyV1 {
        max_font_bytes: 1,
        ..FontImportPolicyV1::default()
    };
    assert_eq!(
        CatalogFontInspectorV2
            .inspect(&bytes, "A", &policy, &mut || Ok(()))
            .unwrap_err()
            .code,
        "FONT_BUDGET_EXCEEDED"
    );
    assert_eq!(
        CatalogFontInspectorV2
            .inspect(&bytes, "A", &FontImportPolicyV1::default(), &mut || Err(
                EditorError::new("CANCELLED", "stopped")
            ))
            .unwrap_err()
            .code,
        "CANCELLED"
    );
}

#[test]
fn monochrome_bitmap_requires_real_bounded_one_bit_strikes_and_keeps_v1_strict() {
    let bytes = encode(mono_tables());
    inspect(&bytes, FontRenderProfileV3::MonochromeBitmap, "A").unwrap();
    assert_eq!(
        StaticFontInspectorV1
            .inspect(&bytes, "A", &FontImportPolicyV1::default(), &mut || Ok(()))
            .unwrap_err()
            .code,
        "UNSUPPORTED_FONT"
    );
    assert_eq!(
        CatalogFontInspectorV2
            .inspect(&bytes, "A", &FontImportPolicyV1::default(), &mut || Ok(()))
            .unwrap_err()
            .code,
        "UNSUPPORTED_FONT"
    );
    for mutation in 0..5 {
        let mut tables = mono_tables();
        match mutation {
            0 => tables.get_mut(b"EBLC").unwrap()[54] = 8,
            1 => put16(tables.get_mut(b"EBLC").unwrap(), 66, 17),
            2 => put16(tables.get_mut(b"EBLC").unwrap(), 74, 7),
            3 => tables.get_mut(b"EBDT").unwrap()[5] = 16,
            4 => {
                tables.remove(b"EBDT");
            }
            _ => unreachable!(),
        }
        assert!(
            inspect(&encode(tables), FontRenderProfileV3::MonochromeBitmap, "A").is_err(),
            "mutation {mutation}"
        );
    }
    assert!(inspect(&bytes, FontRenderProfileV3::ColrV0, "A").is_err());
}

#[test]
fn approved_source_integrity_and_cancel_are_not_skipped_by_profile_selection() {
    let mut bytes = color_font(0);
    let offset = Face::parse(&bytes, 0)
        .unwrap()
        .raw_face()
        .table(Tag::from_bytes(b"COLR"))
        .unwrap()
        .as_ptr() as usize
        - bytes.as_ptr() as usize;
    bytes[offset + 23] ^= 1;
    assert_eq!(
        inspect(&bytes, FontRenderProfileV3::ColrV0, "A")
            .unwrap_err()
            .code,
        "INVALID_FONT"
    );
    let mut checks = 0;
    let bytes = color_font(1);
    let approved = pin(&bytes, FontRenderProfileV3::ColrV1);
    let error = inspect_approved(
        &bytes,
        "A",
        &FontImportPolicyV1::default(),
        &approved,
        &mut || {
            checks += 1;
            if checks == 5 {
                Err(EditorError::new("CANCELLED", "during integrity scan"))
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "CANCELLED");
    assert_eq!(checks, 5);
}

#[test]
fn catalog_policy_does_not_widen_unknown_large_fonts_or_the_old_v1_policy() {
    let bytes = encode(table_map());
    let policy = FontImportPolicyV1 {
        max_font_bytes: 64 * 1024 * 1024,
        ..FontImportPolicyV1::default()
    };
    assert_eq!(
        StaticFontInspectorV1
            .inspect(&bytes, "A", &policy, &mut || Ok(()))
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
    CatalogFontInspectorV2
        .inspect(&bytes, "A", &policy, &mut || Ok(()))
        .unwrap();
    assert!(
        validate_catalog_font_envelope_v3(&"0".repeat(64), MAX_FONT_BYTES_V1 + 1, "A", &policy)
            .is_err()
    );
    let small = FontImportPolicyV1 {
        max_font_bytes: 1,
        ..policy
    };
    assert_eq!(
        CatalogFontInspectorV2
            .inspect(&bytes, "A", &small, &mut || Ok(()))
            .unwrap_err()
            .code,
        "FONT_BUDGET_EXCEEDED"
    );
    let overflow = FontImportPolicyV1 {
        max_font_bytes: policy.max_font_bytes + 1,
        ..policy
    };
    assert_eq!(
        CatalogFontInspectorV2
            .inspect(&bytes, "A", &overflow, &mut || Ok(()))
            .unwrap_err()
            .code,
        "INVALID_ARGUMENT"
    );
}

#[test]
#[ignore = "requires the existing offline fixed-source cache; no downloading"]
fn approved_static_color_and_mono_sources_match_compiled_profiles_without_v1_widening() {
    let cache = std::path::PathBuf::from(
        std::env::var_os("OPENCUT_FONT_CATALOG_CACHE").expect("existing catalog cache"),
    );
    let pins: serde_json::Value = serde_json::from_str(include_str!(
        "../../../editor-application/src/font_catalog_v3/color_source_pins.json"
    ))
    .unwrap();
    for pin in pins.as_array().unwrap() {
        let path = pin["source_path"].as_str().unwrap();
        let bytes = std::fs::read(cache.join(path)).unwrap();
        let result =
            CatalogFontInspectorV2
                .inspect(&bytes, "", &FontImportPolicyV1::default(), &mut || Ok(()));
        assert!(result.is_ok(), "{path}: {result:?}");
        assert_eq!(
            result.unwrap().family,
            pin["source"]["family"].as_str().unwrap()
        );
        assert!(
            StaticFontInspectorV1
                .inspect(&bytes, "", &FontImportPolicyV1::default(), &mut || Ok(()))
                .is_err(),
            "{path}"
        );
        let mut changed = bytes;
        let last = changed.len() - 1;
        changed[last] ^= 1;
        assert!(
            CatalogFontInspectorV2
                .inspect(&changed, "", &FontImportPolicyV1::default(), &mut || Ok(()))
                .is_err(),
            "unknown altered pin {path}"
        );
    }
}

#[test]
#[ignore = "requires existing locally generated V2 outputs; never derives or downloads"]
fn compiled_v2_outputs_match_native_profile_identity_and_separate_large_source_budget() {
    let cache = std::path::PathBuf::from(
        std::env::var_os("OPENCUT_FONT_CATALOG_CACHE").expect("existing catalog cache"),
    );
    let policy = FontImportPolicyV1 {
        max_font_bytes: 64 * 1024 * 1024,
        ..FontImportPolicyV1::default()
    };
    for pin in opencut_editor_application::font_derived_v2::compiled_sources_v2().unwrap() {
        let face = &pin.source.face;
        let bytes = std::fs::read(cache.join("derived").join(&face.content_hash)).unwrap();
        let source = approved_native_source_v3(&face.content_hash, bytes.len() as u64)
            .expect("compiled V2 output admitted by V3 identity");
        let result = CatalogFontInspectorV2.inspect(&bytes, "", &policy, &mut || Ok(()));
        assert!(result.is_ok(), "{}: {result:?}", face.id);
        let parsed = result.unwrap();
        assert_eq!(parsed.family, face.family);
        assert_eq!(parsed.weight, face.weight);
        assert_eq!(parsed.style, face.style);
        if !matches!(source.render_profile, FontRenderProfileV3::StaticOutline) {
            assert_eq!(
                StaticFontInspectorV1
                    .inspect(&bytes, "", &FontImportPolicyV1::default(), &mut || Ok(()))
                    .unwrap_err()
                    .code,
                "UNSUPPORTED_FONT",
                "{}",
                face.id
            );
        }
        // The source may be 48.8 MiB while its frozen output fits V1's 32 MiB
        // byte envelope. The output identity, never source size, selects it.
        assert_eq!(bytes.len().to_string(), face.size_bytes);
    }
}

#[test]
#[ignore = "writes an independent SVG/pixel-reference fixture only when integrator requests it"]
fn write_independent_red_square_color_glyph_reference() {
    let root = std::path::PathBuf::from(
        std::env::var_os("OPENCUT_COLOR_FONT_ORACLE_DIR").expect("external oracle directory"),
    );
    std::fs::create_dir_all(&root).unwrap();
    // This is an independently authored square at x=0..100, y=0..100 font
    // units, not pixels sampled from the renderer under test. At font-size 1000
    // and alphabetic baseline y=100, interior (50,50) is opaque red, exterior
    // (150,50) transparent. Both COLR versions express exactly that geometry.
    for version in 0..=1 {
        std::fs::write(
            root.join(format!("red-square-colr-v{version}.ttf")),
            color_font(version),
        )
        .unwrap();
    }
    std::fs::write(root.join("expected.svg"),"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"200\"><rect width=\"100\" height=\"100\" fill=\"#ff0000\"/></svg>").unwrap();
    std::fs::write(root.join("expected.json"),r#"{"font_size":1000,"text":"A","baseline":[0,100],"canvas":[200,200],"pixels":[{"x":50,"y":50,"rgba":[255,0,0,255]},{"x":150,"y":50,"rgba":[0,0,0,0]}],"authority":"authored geometry and CPAL bytes; no rendered output read"}"#).unwrap();
}
