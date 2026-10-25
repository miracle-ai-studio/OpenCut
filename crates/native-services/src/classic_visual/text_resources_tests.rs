use super::*;
use opencut_asset_store::{AssetStore, font_import::ManagedFontPublisher};
use opencut_composition::{CompositionScope, scene::*};
use opencut_contracts::{AssetVersion, FrameRate, Time, scene::*};
use opencut_editor_application::{
    classic_visual::VisualResourceKind,
    font_import::{FontCasPublish, PreparedFont, catalog::pinned_font_source, prepare_font},
};
use std::{cell::Cell, path::PathBuf, rc::Rc};

fn color() -> Color {
    Color {
        encoding: ColorEncoding::Srgb,
        red: 1.0,
        green: 1.0,
        blue: 1.0,
        alpha: 1.0,
    }
}
fn graph(font: &FontResource, contents: &[&str]) -> SceneComposition {
    let layers = contents
        .iter()
        .enumerate()
        .map(|(index, content)| CompiledLayer {
            element_id: format!("title-{index}"),
            track_id: "text-track".into(),
            track_order: 0,
            element_order: index,
            local_time: Time {
                ticks: "0".into(),
                timebase: 1,
            },
            content: SampledContent::Text {
                text: Text {
                    content: (*content).into(),
                    font: FontBinding::Locked {
                        font_id: font.id.clone(),
                    },
                    font_weight: if font.weight == 700 {
                        FontWeight::Bold
                    } else {
                        FontWeight::Normal
                    },
                    font_style: font.style,
                    decoration: TextDecoration::None,
                    align: SceneTextAlign::Center,
                    font_size: FontSize::ClassicHeight90 { value: 15.0 },
                    color: color(),
                    letter_spacing_px: 0.0,
                    line_height_multiplier: 1.2,
                    background: TextBackground {
                        enabled: false,
                        color: color(),
                        corner_radius_percent: 0.0,
                        padding_x_at_size15: 0.0,
                        padding_y_at_size15: 0.0,
                        offset_x_px: 0.0,
                        offset_y_px: 0.0,
                    },
                },
            },
            visual: Some(SampledVisual {
                profile: VisualProfile::ClassicCanvasV31,
                transform: Transform {
                    position_x_px: 0.0,
                    position_y_px: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation_degrees: 0.0,
                },
                element_opacity: 1.0,
                track_opacity: 1.0,
                effective_opacity: 1.0,
                opacity_policy: LayerOpacityPolicy::MultiplyTrack,
                blend_mode: BlendMode::Normal,
                effects: vec![],
                masks: vec![],
            }),
        })
        .collect();
    SceneComposition {
        project_id: "project".into(),
        revision: "fixed-revision".into(),
        sequence_id: "scene".into(),
        scope: CompositionScope::Visual,
        time: Time {
            ticks: "0".into(),
            timebase: 1,
        },
        format: CompiledFormat {
            fps: FrameRate {
                numerator: 24,
                denominator: 1,
            },
            canvas_size: CanvasSize {
                width: 320,
                height: 180,
            },
            profile: FormatProfile::ClassicGlobal,
        },
        tracks: vec![],
        layers,
        audio: vec![],
        transitions: vec![],
        background: CompiledBackground::Color { color: color() },
        resources: vec![],
        fonts: vec![font.clone()],
        coverage: vec![],
    }
}
fn version(bytes: &[u8]) -> AssetVersion {
    let hash = format!("{:x}", Sha256::digest(bytes));
    AssetVersion {
        id: format!("av_{hash}"),
        content_hash: hash,
        size: bytes.len() as u64,
    }
}
fn font(bytes: &[u8]) -> FontResource {
    FontResource {
        id: "same".into(),
        family: "Test".into(),
        weight: 400,
        style: FontStyle::Normal,
        face_index: 0,
        content: version(bytes),
        origin: ResourceOrigin::LocalImport {
            original_name: "test.ttf".into(),
        },
        license: None,
    }
}
fn publish(root: &Path, resource: &FontResource, bytes: &[u8]) -> AssetStore {
    // This CAS port verifies bytes only. Deliberately malformed test bytes must
    // still be rejected by the actual native inspector before any coverage.
    ManagedFontPublisher::new(root)
        .publish(
            &PreparedFont {
                resource: resource.clone(),
                glyph_count: 1,
                checked_codepoints: 0,
            },
            bytes,
            &mut || Ok(()),
        )
        .unwrap()
}

#[test]
fn text_resources_budget_and_cancellation_precede_cas_access() {
    let font = font(b"invalid font");
    let prepared = prepare_text_frame(&graph(&font, &["ABCD", "EFGH"])).unwrap();
    let policy = FontImportPolicy {
        max_text_bytes: 7,
        ..Default::default()
    };
    let directory = tempfile::tempdir().unwrap();
    let absent = directory.path().join("does-not-exist");
    let error = VerifiedClassicTextResources::new(&absent, &prepared, &policy, || Ok(()))
        .err()
        .unwrap();
    assert_eq!(error.code, "FONT_BUDGET_EXCEEDED");
    assert!(!absent.exists());
    let policy = FontImportPolicy {
        max_font_bytes: 1,
        ..Default::default()
    };
    let error = VerifiedClassicTextResources::new(&absent, &prepared, &policy, || Ok(()))
        .err()
        .unwrap();
    assert_eq!(error.code, "FONT_BUDGET_EXCEEDED");
    let error =
        VerifiedClassicTextResources::new(&absent, &prepared, &FontImportPolicy::default(), || {
            Err(EditorError::new("JOB_CANCELLED", "Stopped"))
        })
        .err()
        .unwrap();
    assert_eq!(error.code, "JOB_CANCELLED");
    assert!(!absent.exists());
}

#[test]
fn hash_verified_bytes_are_not_a_font_coverage_claim() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("cas");
    let bytes = b"not a font";
    let resource = font(bytes);
    let _gate = publish(&root, &resource, bytes);
    let prepared = prepare_text_frame(&graph(&resource, &["AV To"])).unwrap();
    let error = VerifiedClassicTextResources::new(
        &root,
        &prepared,
        &FontImportPolicy::default(),
        || Ok(()),
    )
    .err()
    .unwrap();
    assert_eq!(error.code, "UNSUPPORTED_FONT");
}

#[test]
fn video_font_proof_reuses_only_identical_text_and_keeps_integrity_policy_and_live_fences() {
    use crate::font_import::tests::{encode, table_map};
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("cas");
    let bytes = encode(table_map());
    let mut resource = font(&bytes);
    resource.family = "Fixture Sans".into();
    let _gate = publish(&root, &resource, &bytes);
    let frame = |resource: &FontResource, text: &str| {
        PreparedVideoFrame::Caption(
            prepare_caption_frame(&caption_graph(resource, &[text], &[0])).unwrap(),
        )
    };
    let prepared = frame(&resource, "AV To");
    let policy = FontImportPolicy::default();
    let mut cache = VideoResourceCache::default();
    let calls = Cell::new(0);
    let checkpoint = || {
        calls.set(calls.get() + 1);
        Ok(())
    };
    let first = VerifiedClassicTextResources::new_video_frame(
        &root, &prepared, &policy, checkpoint, &mut cache,
    )
    .unwrap()
    .coverage;
    let initial_calls = calls.replace(0);
    let second = VerifiedClassicTextResources::new_video_frame(
        &root, &prepared, &policy, checkpoint, &mut cache,
    )
    .unwrap()
    .coverage;
    assert_eq!(first, second);
    assert!(calls.get() > 0 && calls.get() < initial_calls);
    assert_eq!(
        VerifiedClassicTextResources::new_video_frame(
            &root,
            &frame(&resource, "B"),
            &policy,
            || Ok(()),
            &mut cache,
        )
        .err()
        .unwrap()
        .code,
        "FONT_GLYPH_MISSING"
    );
    let mut false_face = resource.clone();
    false_face.family = "False face".into();
    assert_eq!(
        VerifiedClassicTextResources::new_video_frame(
            &root,
            &frame(&false_face, "AV To"),
            &policy,
            || Ok(()),
            &mut cache,
        )
        .err()
        .unwrap()
        .code,
        "FONT_FACE_MISMATCH"
    );
    let strict = FontImportPolicy {
        max_font_bytes: 1,
        ..policy
    };
    assert_eq!(
        VerifiedClassicTextResources::new_video_frame(
            &root,
            &prepared,
            &strict,
            || Ok(()),
            &mut cache,
        )
        .err()
        .unwrap()
        .code,
        "FONT_BUDGET_EXCEEDED"
    );
    assert_eq!(
        VerifiedClassicTextResources::new_video_frame(
            &root,
            &prepared,
            &policy,
            || Err(EditorError::new("NOT_AUTHORIZED", "Revoked")),
            &mut cache,
        )
        .err()
        .unwrap()
        .code,
        "NOT_AUTHORIZED"
    );
    let object = root
        .join(&resource.content.content_hash[..2])
        .join(&resource.content.content_hash);
    let mut corrupt = bytes;
    corrupt[0] ^= 1;
    // Published CAS files are read-only. Model replacement by the local owner
    // rather than bypassing the publisher's file permissions.
    std::fs::remove_file(&object).unwrap();
    std::fs::write(object, corrupt).unwrap();
    assert_eq!(
        VerifiedClassicTextResources::new_video_frame(
            &root,
            &prepared,
            &policy,
            || Ok(()),
            &mut cache,
        )
        .err()
        .unwrap()
        .code,
        "ASSET_INTEGRITY_ERROR"
    );
}

#[test]
fn version_descriptors_keep_media_and_font_handles_distinct_and_bounded() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("cas");
    let bytes = vec![42u8; 70000];
    let resource = font(&bytes);
    let _gate = publish(&root, &resource, &bytes);
    let cancelled = Rc::new(Cell::new(false));
    let flag = cancelled.clone();
    let mut store = VerifiedBrowserResources::new_versions(
        &root,
        [
            ("media:same", &resource.content),
            ("font:same", &resource.content),
        ],
        move || {
            if flag.get() {
                Err(EditorError::new("NOT_AUTHORIZED", "Credential revoked"))
            } else {
                Ok(())
            }
        },
        INVALID,
    )
    .unwrap();
    let handles: BTreeMap<String, String> = store
        .handles()
        .map(|(id, handle)| (id.into(), handle.into()))
        .collect();
    assert_ne!(handles["media:same"], handles["font:same"]);
    assert_eq!(
        store
            .resolve(
                "font:same",
                &resource.content.content_hash,
                resource.content.size,
                &handles["media:same"]
            )
            .unwrap_err()
            .code,
        INVALID
    );
    let (transfer, _) = store
        .resolve(
            "font:same",
            &resource.content.content_hash,
            resource.content.size,
            &handles["font:same"],
        )
        .unwrap();
    assert_eq!(store.read(&transfer, 0, 65537).unwrap_err().code, INVALID);
    let (first, eof) = store.read(&transfer, 0, 65536).unwrap();
    assert_eq!(first, bytes[..65536]);
    assert!(!eof);
    cancelled.set(true);
    assert_eq!(
        store.read(&transfer, 65536, 65536).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    cancelled.set(false);
    let (last, eof) = store.read(&transfer, 65536, 65536).unwrap();
    assert_eq!(last, bytes[65536..]);
    assert!(eof);
    store.release(&transfer).unwrap();
    assert_eq!(
        store.read(&transfer, 0, 1).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
}

#[test]
#[ignore = "requires pinned licensed public fixture in OPENCUT_FONT_FIXTURE_DIR"]
fn licensed_text_resources_recheck_each_current_text_and_actual_static_face() {
    let fixture = PathBuf::from(
        std::env::var_os("OPENCUT_FONT_FIXTURE_DIR").expect("font fixture directory"),
    );
    for (catalog, file, contents) in [
        (
            "google-lato-regular-v1",
            "lato-Lato-Regular.ttf",
            ["AV To", "Hello"],
        ),
        (
            "google-mashanzheng-regular-v1",
            "mashanzheng-MaShanZheng-Regular.ttf",
            ["中文编辑", "你好世界"],
        ),
    ] {
        let source = pinned_font_source(catalog).unwrap();
        let bytes = std::fs::read(fixture.join(file)).unwrap();
        assert_eq!(version(&bytes).content_hash, source.content_hash);
        assert_eq!(bytes.len() as u64, source.size_bytes);
        let spec = source.import_spec("font".into(), contents.join(""));
        let font = prepare_font(
            &StaticFontInspector,
            &bytes,
            &spec,
            &FontImportPolicy::default(),
            &mut || Ok(()),
        )
        .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("cas");
        let _gate = ManagedFontPublisher::new(&root)
            .publish(&font, &bytes, &mut || Ok(()))
            .unwrap();
        let prepared = prepare_text_frame(&graph(&font.resource, &contents)).unwrap();
        let cancelled = Rc::new(Cell::new(false));
        let flag = cancelled.clone();
        let mut resources = VerifiedClassicTextResources::new(
            &root,
            &prepared,
            &FontImportPolicy::default(),
            move || {
                if flag.get() {
                    Err(EditorError::new("JOB_CANCELLED", "Stopped"))
                } else {
                    Ok(())
                }
            },
        )
        .unwrap();
        assert_eq!(resources.coverage().len(), 2);
        for (index, coverage) in resources.coverage().iter().enumerate() {
            assert_eq!(coverage.element_id, format!("title-{index}"));
            assert_eq!(coverage.font, font.resource);
            assert_eq!(coverage.content, contents[index]);
        }
        assert!(resources.handles().is_empty());
        let handles = resources.font_handles();
        assert_eq!(handles.len(), 1);
        assert_eq!(handles[0].font_id, "font");
        let wire = font_resource(&font.resource);
        let mut forged = wire.clone();
        forged.weight = 700;
        assert_eq!(
            resources
                .resolve_font(&forged, &handles[0].handle)
                .unwrap_err()
                .code,
            INVALID
        );
        let media = VisualResource {
            asset_id: wire.font_id.clone(),
            content_hash: wire.content_hash.clone(),
            size: wire.size.clone(),
            kind: VisualResourceKind::Image,
        };
        assert_eq!(
            resources
                .resolve(&media, &handles[0].handle)
                .unwrap_err()
                .code,
            "NOT_AUTHORIZED"
        );
        let (transfer, size) = resources.resolve_font(&wire, &handles[0].handle).unwrap();
        let mut actual = Sha256::new();
        let mut offset = 0;
        while offset < size {
            let (chunk, _) = resources.read(&transfer, offset, 65536).unwrap();
            offset += chunk.len() as u64;
            actual.update(&chunk);
        }
        assert_eq!(format!("{:x}", actual.finalize()), wire.content_hash);
        cancelled.set(true);
        assert_eq!(
            resources.read(&transfer, 0, 1).unwrap_err().code,
            "JOB_CANCELLED"
        );
        cancelled.set(false);
        resources.release(&transfer).unwrap();
        let missing = prepare_text_frame(&graph(&font.resource, &["\u{10ffff}"])).unwrap();
        assert_eq!(
            VerifiedClassicTextResources::new(
                &root,
                &missing,
                &FontImportPolicy::default(),
                || Ok(())
            )
            .err()
            .unwrap()
            .code,
            "FONT_GLYPH_MISSING"
        );
        let mut wrong = font.resource.clone();
        wrong.family = "Different family".into();
        let wrong = prepare_text_frame(&graph(&wrong, &contents)).unwrap();
        assert_eq!(
            VerifiedClassicTextResources::new(&root, &wrong, &FontImportPolicy::default(), || Ok(
                ()
            ))
            .err()
            .unwrap()
            .code,
            "FONT_FACE_MISMATCH"
        );
    }
}

#[test]
fn caption_inspection_covers_rendered_separators_without_changing_free_text() {
    let resource = font(b"identity-only; no parser or CAS access in this planning check");
    let source = "A\tB\u{00a0}C\u{feff}D";
    let prepared = prepare_text_frame(&graph(&resource, &[source, "Free Text"])).unwrap();
    let caption_ids = std::collections::BTreeSet::from(["title-0".to_string()]);
    let batches = font_texts(
        &prepared,
        &FontImportPolicy::default(),
        &mut || Ok(()),
        Some(&caption_ids),
    )
    .unwrap();
    assert_eq!(batches["same"].content, "A B C DFree Text");
    assert_eq!(prepared.texts()[0].text.content, source);
    // Preserve ECMAScript's exact separator set and the unchanged V2 rejection
    // of source control characters; V3 only normalizes actual caption content.
    assert!(caption_separator('\u{feff}'));
    assert!(!caption_separator('\u{0085}'));
    let original = font_texts(
        &prepared,
        &FontImportPolicy::default(),
        &mut || Ok(()),
        None,
    );
    assert_eq!(original.err().unwrap().code, "INVALID_ARGUMENT");
}

fn caption_graph(font: &FontResource, contents: &[&str], captions: &[usize]) -> SceneComposition {
    let mut graph = graph(font, contents);
    for &index in captions {
        let SampledContent::Text { text } = &graph.layers[index].content else {
            unreachable!()
        };
        graph.layers[index].content = SampledContent::Caption {
            text: text.content.clone(),
            style: SceneCaptionStyle {
                font: text.font.clone(),
                font_size: text.font_size,
                color: text.color.clone(),
                background_color: None,
                max_width_px: None,
                line_height_px: None,
                text_align: SceneTextAlign::Center,
                layout: CaptionLayoutProfile::PinnedCaptionV4,
            },
        };
    }
    graph
}

#[test]
fn caption_font_subset_empty_checks_complete_envelope_without_cas() {
    let resource = font(b"unopened font");
    let prepared =
        prepare_caption_frame(&caption_graph(&resource, &["ABCD", "EFGH"], &[1])).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let absent = directory.path().join("never-opened");
    let empty = BTreeSet::new();
    let proof = VerifiedClassicTextResources::new_caption_fonts_subset(
        &absent,
        &prepared,
        &empty,
        &FontImportPolicy::default(),
        || Ok(()),
    )
    .unwrap();
    assert!(proof.is_empty());
    assert!(!absent.exists());
    for selected in [empty, BTreeSet::from(["title-1".into()])] {
        for policy in [
            FontImportPolicy {
                max_text_bytes: 7,
                ..Default::default()
            },
            FontImportPolicy {
                max_font_bytes: 1,
                ..Default::default()
            },
        ] {
            assert_eq!(
                VerifiedClassicTextResources::new_caption_fonts_subset(
                    &absent,
                    &prepared,
                    &selected,
                    &policy,
                    || Ok(())
                )
                .unwrap_err()
                .code,
                "FONT_BUDGET_EXCEEDED"
            );
            assert!(!absent.exists());
        }
    }
}

#[test]
fn caption_font_subset_rejects_unknown_selectors_before_cas() {
    let resource = font(b"unopened font");
    let prepared = prepare_caption_frame(&graph(&resource, &["Title"])).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let absent = directory.path().join("never-opened");
    // The API accepts a set, so duplicate selectors cannot reach the reader.
    let selected = BTreeSet::from(["foreign-element".into()]);
    assert_eq!(
        VerifiedClassicTextResources::new_caption_fonts_subset(
            &absent,
            &prepared,
            &selected,
            &FontImportPolicy::default(),
            || Ok(())
        )
        .unwrap_err()
        .code,
        INVALID
    );
    assert!(!absent.exists());
}

#[test]
fn caption_font_subset_all_hits_still_check_early_and_final_cancellation() {
    let resource = font(b"unopened font");
    let prepared = prepare_caption_frame(&graph(&resource, &["Title", "Other"])).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let absent = directory.path().join("never-opened");
    // Start, both current texts, and the final no-CAS return checkpoint.
    for fail_at in [1, 4] {
        let calls = Cell::new(0);
        assert_eq!(
            VerifiedClassicTextResources::new_caption_fonts_subset(
                &absent,
                &prepared,
                &BTreeSet::new(),
                &FontImportPolicy::default(),
                || {
                    calls.set(calls.get() + 1);
                    if calls.get() == fail_at {
                        Err(EditorError::new("JOB_CANCELLED", "Cancelled proof"))
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap_err()
            .code,
            "JOB_CANCELLED"
        );
        assert_eq!(calls.get(), fail_at);
        assert!(!absent.exists());
    }
}

#[test]
fn caption_font_subset_batches_only_missing_text_and_preserves_whitespace_proof() {
    let resource = font(b"unopened font");
    let prepared = prepare_caption_frame(&caption_graph(
        &resource,
        &["Already inspected", "New\tcaption\u{00a0}word"],
        &[1],
    ))
    .unwrap();
    let caption_ids = BTreeSet::from(["title-1".into()]);
    let batches = selected_font_texts(
        prepared.execution(),
        &FontImportPolicy::default(),
        &mut || Ok(()),
        Some(&caption_ids),
        Some(&caption_ids),
    )
    .unwrap();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches["same"].content, "New caption word");
    assert_eq!(
        prepared.execution().texts()[1].text.content,
        "New\tcaption\u{00a0}word"
    );
}

#[test]
#[ignore = "requires pinned licensed public fixture in OPENCUT_FONT_FIXTURE_DIR"]
fn licensed_caption_font_subset_skips_unchanged_font_and_checks_new_glyphs_and_face() {
    let fixture = PathBuf::from(
        std::env::var_os("OPENCUT_FONT_FIXTURE_DIR").expect("font fixture directory"),
    );
    let load = |catalog: &str, file: &str, id: &str, content: &str| {
        let source = pinned_font_source(catalog).unwrap();
        let bytes = std::fs::read(fixture.join(file)).unwrap();
        assert_eq!(version(&bytes).content_hash, source.content_hash);
        let prepared = prepare_font(
            &StaticFontInspector,
            &bytes,
            &source.import_spec(id.into(), content.into()),
            &FontImportPolicy::default(),
            &mut || Ok(()),
        )
        .unwrap();
        (prepared, bytes)
    };
    let (latin, latin_bytes) = load(
        "google-lato-regular-v1",
        "lato-Lato-Regular.ttf",
        "latin",
        "Title New caption",
    );
    let (chinese, _) = load(
        "google-mashanzheng-regular-v1",
        "mashanzheng-MaShanZheng-Regular.ttf",
        "chinese",
        "中文编辑",
    );
    let mut mixed = caption_graph(&latin.resource, &["Title", "New\tcaption"], &[1]);
    let mut chinese_layer = graph(&chinese.resource, &["中文编辑"]).layers.remove(0);
    chinese_layer.element_id = "unchanged-chinese".into();
    mixed.layers.insert(1, chinese_layer);
    mixed.fonts.push(chinese.resource.clone());
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("cas");
    // Only the new caption's font exists in this CAS. An accidental full-frame
    // read must fail on the unchanged Chinese font, rather than silently pass.
    let _gate = publish(&root, &latin.resource, &latin_bytes);
    let current = prepare_caption_frame(&mixed).unwrap();
    let selected = BTreeSet::from(["title-1".into()]);
    let proof = VerifiedClassicTextResources::new_caption_fonts_subset(
        &root,
        &current,
        &selected,
        &FontImportPolicy::default(),
        || Ok(()),
    )
    .unwrap();
    assert_eq!(
        proof,
        vec![NativeTextCoverage {
            element_id: "title-1".into(),
            font: latin.resource.clone(),
            content: "New\tcaption".into(),
        }]
    );
    assert!(
        VerifiedClassicTextResources::new_caption_fonts(
            &root,
            &current,
            &FontImportPolicy::default(),
            || Ok(()),
        )
        .is_err()
    );
    // The unselected large font remains part of the complete envelope.
    assert!(chinese.resource.content.size > latin.resource.content.size);
    let policy = FontImportPolicy {
        max_font_bytes: latin.resource.content.size,
        ..Default::default()
    };
    assert_eq!(
        VerifiedClassicTextResources::new_caption_fonts_subset(
            &root,
            &current,
            &selected,
            &policy,
            || Ok(()),
        )
        .unwrap_err()
        .code,
        "FONT_BUDGET_EXCEEDED"
    );
    let SampledContent::Caption { text, .. } = &mut mixed.layers[2].content else {
        unreachable!()
    };
    *text = "\u{10ffff}".into();
    let missing = prepare_caption_frame(&mixed).unwrap();
    assert_eq!(
        VerifiedClassicTextResources::new_caption_fonts_subset(
            &root,
            &missing,
            &selected,
            &FontImportPolicy::default(),
            || Ok(()),
        )
        .unwrap_err()
        .code,
        "FONT_GLYPH_MISSING"
    );
    let SampledContent::Caption { text, .. } = &mut mixed.layers[2].content else {
        unreachable!()
    };
    *text = "New caption".into();
    mixed.fonts[0].family = "Wrong fixed face".into();
    let wrong = prepare_caption_frame(&mixed).unwrap();
    assert_eq!(
        VerifiedClassicTextResources::new_caption_fonts_subset(
            &root,
            &wrong,
            &selected,
            &FontImportPolicy::default(),
            || Ok(()),
        )
        .unwrap_err()
        .code,
        "FONT_FACE_MISMATCH"
    );
}

fn scene_text_mask_graph(resource: &FontResource, content: &str) -> SceneComposition {
    let mut graph = graph(resource, &["unused"]);
    let layer = &mut graph.layers[0];
    layer.content = SampledContent::Graphic {
        graphic: Graphic {
            geometry: GraphicGeometry::Ellipse,
            source_size: CanvasSize {
                width: 80,
                height: 60,
            },
            fill: color(),
            stroke: Stroke {
                color: color(),
                width_px: 0.,
                align: StrokeAlign::Center,
            },
        },
    };
    layer.visual.as_mut().unwrap().masks.push(Mask {
        id: "text-mask".into(),
        enabled: true,
        inverted: false,
        feather: MaskFeather::ClassicPixels { value: 12. },
        stroke: Stroke {
            color: color(),
            width_px: 2.,
            align: StrokeAlign::Inside,
        },
        shape: LayerMaskShape::Text {
            mask: TextMask {
                content: content.into(),
                font: FontBinding::Locked {
                    font_id: resource.id.clone(),
                },
                font_size: FontSize::ClassicHeight90 { value: 15. },
                font_weight: FontWeight::Normal,
                font_style: FontStyle::Normal,
                decoration: TextDecoration::Underline,
                letter_spacing_px: 1.5,
                line_height_multiplier: 1.2,
                center_x: 0.1,
                center_y: -0.2,
                rotation_degrees: 15.,
                scale: 1.2,
            },
        },
    });
    graph
}
#[test]
fn scene_mask_only_fonts_remain_fixed_without_polluting_delegated_title_inventory() {
    let resource = font(b"not a font");
    let source = scene_text_mask_graph(&resource, "Masked title");
    let prepared = prepare_scene_frame(&source).unwrap();
    assert!(
        prepared
            .execution()
            .execution()
            .execution()
            .texts()
            .is_empty()
    );
    assert!(
        prepared
            .execution()
            .execution()
            .execution()
            .graph()
            .fonts
            .is_empty()
    );
    assert_eq!(prepared.mask_texts().len(), 1);
    assert_eq!(prepared.mask_texts()[0].font, resource);
    assert_eq!(prepared.mask_texts()[0].mask_id, "text-mask");
    assert_eq!(prepared.source_graph().fonts, source.fonts);
}
#[test]
fn scene_text_cache_hits_validate_complete_envelope_and_cancellation_without_cas() {
    let resource = font(b"not a font");
    let prepared = prepare_scene_frame(&scene_text_mask_graph(&resource, "Masked title")).unwrap();
    let missing = Path::new("/this-cas-must-not-be-opened-for-scene-cache-hits");
    let coverage = VerifiedClassicTextResources::new_scene_fonts_subset(
        missing,
        &prepared,
        &BTreeSet::new(),
        &FontImportPolicy::default(),
        || Ok(()),
    )
    .unwrap();
    assert!(coverage.is_empty());
    let policy = FontImportPolicy {
        max_text_bytes: 3,
        ..FontImportPolicy::default()
    };
    assert_eq!(
        VerifiedClassicTextResources::new_scene_fonts_subset(
            missing,
            &prepared,
            &BTreeSet::new(),
            &policy,
            || Ok(())
        )
        .unwrap_err()
        .code,
        "FONT_BUDGET_EXCEEDED"
    );
    let calls = Cell::new(0);
    let error = VerifiedClassicTextResources::new_scene_fonts_subset(
        missing,
        &prepared,
        &BTreeSet::new(),
        &FontImportPolicy::default(),
        || {
            calls.set(calls.get() + 1);
            if calls.get() > 1 {
                Err(EditorError::new("JOB_CANCELLED", "Cancelled"))
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "JOB_CANCELLED");
}
#[test]
fn scene_mask_cas_hash_does_not_substitute_for_actual_static_face_or_glyph_inspection() {
    let bytes = b"hash-correct but not an OpenType face";
    let resource = font(bytes);
    let prepared = prepare_scene_frame(&scene_text_mask_graph(&resource, "Masked title")).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("cas");
    let _gate = publish(&root, &resource, bytes);
    let result = VerifiedClassicTextResources::new_scene_frame(
        &root,
        &prepared,
        &FontImportPolicy::default(),
        || Ok(()),
    );
    assert!(result.is_err());
    let selected = BTreeSet::from(["unrelated-mask".into()]);
    assert_eq!(
        VerifiedClassicTextResources::new_scene_fonts_subset(
            Path::new("/absent"),
            &prepared,
            &selected,
            &FontImportPolicy::default(),
            || Ok(())
        )
        .unwrap_err()
        .code,
        "INVALID_VISUAL_PLAN"
    );
}
#[test]
#[ignore = "requires pinned licensed public fixture in OPENCUT_FONT_FIXTURE_DIR"]
fn licensed_scene_mask_font_proof_comes_from_actual_cas_bytes_and_preserves_text_owner() {
    let directory = PathBuf::from(
        std::env::var_os("OPENCUT_FONT_FIXTURE_DIR").expect("font fixture directory"),
    );
    let source = pinned_font_source("google-lato-regular-v1").unwrap();
    let bytes = std::fs::read(directory.join("lato-Lato-Regular.ttf")).unwrap();
    assert_eq!(version(&bytes).content_hash, source.content_hash);
    let imported = prepare_font(
        &StaticFontInspector,
        &bytes,
        &source.import_spec("mask-font".into(), "Masked title".into()),
        &FontImportPolicy::default(),
        &mut || Ok(()),
    )
    .unwrap();
    let frame =
        prepare_scene_frame(&scene_text_mask_graph(&imported.resource, "Masked title")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cas");
    let _gate = publish(&root, &imported.resource, &bytes);
    let resources = VerifiedClassicTextResources::new_scene_frame(
        &root,
        &frame,
        &FontImportPolicy::default(),
        || Ok(()),
    )
    .unwrap();
    assert_eq!(resources.font_handles().len(), 1);
    assert_eq!(
        resources.coverage(),
        &[NativeTextCoverage {
            element_id: "text-mask".into(),
            font: imported.resource,
            content: "Masked title".into()
        }]
    );
}

#[test]
#[ignore = "requires original approved color/mono sources in OPENCUT_FONT_CATALOG_CACHE"]
fn approved_color_and_bitmap_resources_share_output_preview_and_text_mask_native_admission() {
    use crate::font_inspection::CatalogFontInspector;
    use opencut_editor_application::font_catalog::native_source;
    let cache = PathBuf::from(
        std::env::var_os("OPENCUT_FONT_CATALOG_CACHE").expect("existing source cache"),
    );
    for (id, path) in [
        (
            "google-bungeetint-regular-color-v3",
            "ofl/bungeetint/BungeeTint-Regular.ttf",
        ),
        (
            "google-bungeespice-regular-color-v3",
            "ofl/bungeespice/BungeeSpice-Regular.ttf",
        ),
        (
            "google-anonymouspro-regular-color-v3",
            "ofl/anonymouspro/AnonymousPro-Regular.ttf",
        ),
    ] {
        let source = &native_source(id).unwrap().source;
        let bytes = std::fs::read(cache.join(path)).unwrap();
        let imported = prepare_font(
            &CatalogFontInspector,
            &bytes,
            &source.import_spec("font".into(), "AV".into()),
            &FontImportPolicy::default(),
            &mut || Ok(()),
        )
        .unwrap();
        assert_eq!(imported.resource.content.content_hash, source.content_hash);
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("cas");
        let _gate = publish(&root, &imported.resource, &bytes);
        let frame = prepare_caption_frame(&graph(&imported.resource, &["A", "V"])).unwrap();
        // PNG/video consume the complete caption execution resource path;
        // live preview consumes its font-only and incremental variants.
        let output = VerifiedClassicTextResources::new_caption_frame(
            &root,
            &frame,
            &FontImportPolicy::default(),
            || Ok(()),
        )
        .unwrap();
        let preview = VerifiedClassicTextResources::new_caption_fonts(
            &root,
            &frame,
            &FontImportPolicy::default(),
            || Ok(()),
        )
        .unwrap();
        assert_eq!(output.coverage(), preview.coverage(), "{id}");
        let incremental = VerifiedClassicTextResources::new_caption_fonts_subset(
            &root,
            &frame,
            &BTreeSet::from(["title-1".into()]),
            &FontImportPolicy::default(),
            || Ok(()),
        )
        .unwrap();
        assert_eq!(incremental, vec![preview.coverage()[1].clone()], "{id}");
        let mask = prepare_scene_frame(&scene_text_mask_graph(&imported.resource, "A")).unwrap();
        let resources = VerifiedClassicTextResources::new_scene_frame(
            &root,
            &mask,
            &FontImportPolicy::default(),
            || Ok(()),
        )
        .unwrap();
        assert_eq!(
            resources.coverage(),
            &[NativeTextCoverage {
                element_id: "text-mask".into(),
                font: imported.resource.clone(),
                content: "A".into()
            }],
            "{id}"
        );
        let denied = VerifiedClassicTextResources::new_caption_fonts(
            &root,
            &frame,
            &FontImportPolicy::default(),
            || Err(EditorError::new("NOT_AUTHORIZED", "revoked")),
        );
        assert_eq!(denied.err().unwrap().code, "NOT_AUTHORIZED", "{id}");
    }
}
