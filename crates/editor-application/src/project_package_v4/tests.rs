use super::*;
use opencut_contracts::{AssetVersion, FontResolutionPolicy};
fn fixture() -> PreparedImportV4 {
    let fixed = crate::project_v4_video_jobs::tests::input();
    let mut document = fixed.snapshot.project;
    document.fonts.push(FontResourceV4 {
        id: "font".into(),
        family: "Fixture".into(),
        weight: 400,
        style: FontStyleV4::Normal,
        face_index: 0,
        content: AssetVersion {
            id: format!("av_{}", "f".repeat(64)),
            content_hash: "f".repeat(64),
            size: 100,
        },
        origin: ResourceOriginV4::LocalImport {
            original_name: "fixture.ttf".into(),
        },
        license: None,
    });
    let sequence = &mut document.sequences[0];
    let mut graphics = sequence.tracks[0].clone();
    graphics.id = "graphics".into();
    graphics.kind = TrackKindV4::Graphic;
    graphics.elements[0].id = "graphic".into();
    graphics.elements[0].content = ElementContentV4::Graphic {
        graphic: crate::project_v4_graphic_video_jobs::tests::graphic(),
    };
    sequence.tracks.push(graphics);
    let mut texts = sequence.tracks[0].clone();
    texts.id = "texts".into();
    texts.kind = TrackKindV4::Text;
    texts.elements[0].id = "text".into();
    let graph = crate::classic_text_frame_v2::tests::graph();
    let opencut_composition::project_v4::SampledContentV4::Text { text } =
        graph.layers[1].content.clone()
    else {
        panic!()
    };
    texts.elements[0].content = ElementContentV4::Text { text: text.clone() };
    sequence.tracks.push(texts);
    let mut captions = sequence.tracks[0].clone();
    captions.id = "captions".into();
    captions.kind = TrackKindV4::Caption;
    captions.elements.clear();
    sequence.tracks.push(captions);
    sequence.captions.push(CaptionV4 {
        id: "caption".into(),
        track_id: "captions".into(),
        timeline_range: sequence.tracks[0].elements[0].timeline_range.clone(),
        text: "A caption".into(),
        source_anchor: None,
        style: CaptionStyleV4 {
            font: text.font,
            font_size: text.font_size,
            color: text.color,
            background_color: None,
            max_width_px: None,
            line_height_px: None,
            text_align: TextAlignV4::Center,
            layout: CaptionLayoutProfileV4::PinnedCaptionV4,
        },
    });
    let mut prepared = PreparedImportV4 {
        manifest_sha256: String::new(),
        manifest: PackageManifestV4 {
            schema_version: 6,
            project_schema_version: 4,
            source_project_id: document.id.clone(),
            source_revision: fixed.snapshot.revision,
            project_path: "project.json".into(),
            project_sha256: String::new(),
            assets: document
                .assets
                .iter()
                .map(|a| PackageResourceV4 {
                    resource_id: a.id.clone(),
                    content_hash: a.version.content_hash.clone(),
                    size_bytes: a.version.size,
                    path: format!("media/{}", a.version.content_hash),
                })
                .collect(),
            fonts: document
                .fonts
                .iter()
                .map(|f| PackageResourceV4 {
                    resource_id: f.id.clone(),
                    content_hash: f.content.content_hash.clone(),
                    size_bytes: f.content.size,
                    path: format!("fonts/{}", f.content.content_hash),
                })
                .collect(),
            derived_fonts: vec![],
            self_contained_resources: true,
            history_included: false,
            fully_reproducible: false,
        },
        document,
    };
    rehash(&mut prepared);
    prepared
}
fn rehash(p: &mut PreparedImportV4) {
    p.manifest.project_sha256 = digest(&serde_json::to_vec(&p.document).unwrap());
    p.manifest_sha256 = digest(&serde_json::to_vec(&p.manifest).unwrap());
}
#[test]
fn single_revision_preserves_all_typed_content_and_only_replaces_target_project_identity() {
    let p = fixture();
    validate_contents(&p.manifest, &p.document).unwrap();
    validate_export_source(&PackageExportSourceV4 {
        snapshot: ProjectSnapshotV4 {
            project: p.document.clone(),
            revision: p.manifest.source_revision.clone(),
        },
        derived_fonts: vec![],
    })
    .unwrap();
    let restored = prepare_import(&p, "restored").unwrap();
    let mut expected = p.document.clone();
    expected.id = "restored".into();
    assert_eq!(restored, expected);
    assert_eq!(restored.sequences[0].captions.len(), 1);
    assert!(matches!(
        restored.sequences[0].tracks[1].elements[0].content,
        ElementContentV4::Graphic { .. }
    ));
}
#[test]
fn every_resource_and_both_canonical_digests_are_required() {
    for case in 0..6 {
        let mut p = fixture();
        match case {
            0 => p.manifest.assets.clear(),
            1 => p.manifest.fonts[0].size_bytes += 1,
            2 => p.manifest.fonts.push(p.manifest.fonts[0].clone()),
            3 => p.manifest.assets[0].path = "../outside".into(),
            4 => p.document.name = "changed-after-read".into(),
            _ => p.manifest_sha256 = "a".repeat(64),
        }
        assert!(prepare_import(&p, "restored").is_err(), "case {case}");
    }
    let mut p = fixture();
    p.manifest.fonts.clear();
    p.manifest_sha256 = digest(&serde_json::to_vec(&p.manifest).unwrap());
    assert!(prepare_import(&p, "restored").is_err());
}
#[test]
fn legacy_package_scope_and_unresolved_text_caption_and_mask_fonts_are_not_portable() {
    for case in 0..4 {
        let mut p = fixture();
        match case {
            0 => p.manifest.schema_version = 5,
            1 => p.manifest.project_schema_version = 3,
            2 => p.manifest.history_included = true,
            _ => p.manifest.fully_reproducible = true,
        }
        assert!(validate_manifest(&p.manifest).is_err());
    }
    let unresolved = || FontBindingV4::UnresolvedFamily {
        family: "System only".into(),
        policy: FontResolutionPolicy::Strict {},
    };
    for case in 0..3 {
        let mut p = fixture();
        let sequence = &mut p.document.sequences[0];
        if case == 0 {
            let ElementContentV4::Text { text } = &mut sequence.tracks[2].elements[0].content
            else {
                panic!()
            };
            text.font = unresolved();
        } else if case == 1 {
            sequence.captions[0].style.font = unresolved();
            // Pinned captions already reject unresolved fonts in the core.
            // Legacy caption layout is valid V4 and reaches package policy.
            sequence.captions[0].style.layout = CaptionLayoutProfileV4::LegacyCaptionV2;
        } else {
            let ElementContentV4::Text { text } = sequence.tracks[2].elements[0].content.clone()
            else {
                panic!()
            };
            sequence.tracks[0].elements[0]
                .visual
                .as_mut()
                .unwrap()
                .masks
                .push(MaskV4 {
                    id: "mask".into(),
                    enabled: true,
                    inverted: false,
                    feather: MaskFeatherV4::ClassicPixels { value: 0.0 },
                    stroke: crate::project_v4_graphic_video_jobs::tests::graphic().stroke,
                    shape: MaskShapeV4::Text {
                        mask: TextMaskV4 {
                            content: "Mask".into(),
                            font: unresolved(),
                            font_size: text.font_size,
                            font_weight: text.font_weight,
                            font_style: text.font_style,
                            decoration: text.decoration,
                            letter_spacing_px: 0.0,
                            line_height_multiplier: 1.0,
                            center_x: 0.5,
                            center_y: 0.5,
                            rotation_degrees: 0.0,
                            scale: 1.0,
                        },
                    },
                });
        }
        opencut_editor_core::project_v4::validate_project_v4(&p.document).unwrap();
        rehash(&mut p);
        assert_eq!(
            prepare_import(&p, "restored").unwrap_err().code,
            "PACKAGE_DEPENDENCY_MISSING"
        );
    }
}
#[test]
fn derived_proof_inventory_cannot_be_missing_extra_or_detached_from_font_identity() {
    let mut p = fixture();
    let request = opencut_contracts::font_derived_v1::ImportDerivedFontV1 {
        project_id: p.document.id.clone(),
        expected_revision: p.manifest.source_revision.clone(),
        idempotency_key: "source".into(),
        catalog_id: crate::font_derived_v1::catalog()
            .unwrap()
            .into_iter()
            .find(|p| p.source.face.weight == 400 && p.source.face.style == FontStyleV4::Normal)
            .unwrap()
            .source
            .face
            .id,
        font_id: "font".into(),
        required_text: "A".into(),
    };
    p.document.fonts[0] = crate::font_derived_v1::expected_import_resource_v1(&request).unwrap();
    let font = &p.document.fonts[0];
    p.manifest.fonts[0] = PackageResourceV4 {
        resource_id: font.id.clone(),
        content_hash: font.content.content_hash.clone(),
        size_bytes: font.content.size,
        path: format!("fonts/{}", font.content.content_hash),
    };
    p.manifest.derived_fonts = vec![
        crate::font_derived_v1::source(&request.catalog_id)
            .unwrap()
            .source,
    ];
    rehash(&mut p);
    validate_contents(&p.manifest, &p.document).unwrap();
    let mut missing = p.clone();
    missing.manifest.derived_fonts.clear();
    rehash(&mut missing);
    assert!(prepare_import(&missing, "restored").is_err());
    let mut detached = p.clone();
    detached.document.fonts[0].family = "Other".into();
    rehash(&mut detached);
    assert!(prepare_import(&detached, "restored").is_err());
    let mut extra = fixture();
    extra.manifest.derived_fonts = p.manifest.derived_fonts;
    rehash(&mut extra);
    assert!(prepare_import(&extra, "restored").is_err());
}
