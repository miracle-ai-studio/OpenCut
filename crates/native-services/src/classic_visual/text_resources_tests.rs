use super::*;
use opencut_asset_store::{AssetStore, font_import_v1::ManagedFontPublisherV1};
use opencut_composition::{CompositionScope, project_v4::*};
use opencut_contracts::{AssetVersion, FrameRate, Time, project_v4::*};
use opencut_editor_application::{
    classic_visual::VisualResourceKind,
    font_import_v1::{
        FontCasPublishV1, PreparedFontV1, catalog::pinned_font_source_v1, prepare_font_v1,
    },
};
use std::{cell::Cell, path::PathBuf, rc::Rc};

fn color() -> ColorV4 {
    ColorV4 {
        encoding: ColorEncodingV4::Srgb,
        red: 1.0,
        green: 1.0,
        blue: 1.0,
        alpha: 1.0,
    }
}
fn graph(font: &FontResourceV4, contents: &[&str]) -> CompositionV4 {
    let layers = contents
        .iter()
        .enumerate()
        .map(|(index, content)| CompiledLayerV4 {
            element_id: format!("title-{index}"),
            track_id: "text-track".into(),
            track_order: 0,
            element_order: index,
            local_time: Time {
                ticks: "0".into(),
                timebase: 1,
            },
            content: SampledContentV4::Text {
                text: TextV4 {
                    content: (*content).into(),
                    font: FontBindingV4::Locked {
                        font_id: font.id.clone(),
                    },
                    font_weight: if font.weight == 700 {
                        FontWeightV4::Bold
                    } else {
                        FontWeightV4::Normal
                    },
                    font_style: font.style,
                    decoration: TextDecorationV4::None,
                    align: TextAlignV4::Center,
                    font_size: FontSizeV4::ClassicHeight90 { value: 15.0 },
                    color: color(),
                    letter_spacing_px: 0.0,
                    line_height_multiplier: 1.2,
                    background: TextBackgroundV4 {
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
            visual: Some(SampledVisualV4 {
                profile: VisualProfileV4::ClassicCanvasV31,
                transform: TransformV4 {
                    position_x_px: 0.0,
                    position_y_px: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation_degrees: 0.0,
                },
                element_opacity: 1.0,
                track_opacity: 1.0,
                effective_opacity: 1.0,
                opacity_policy: LayerOpacityPolicyV4::MultiplyTrack,
                blend_mode: BlendModeV4::Normal,
                effects: vec![],
                masks: vec![],
            }),
        })
        .collect();
    CompositionV4 {
        project_id: "project".into(),
        revision: "fixed-revision".into(),
        sequence_id: "scene".into(),
        scope: CompositionScope::Visual,
        time: Time {
            ticks: "0".into(),
            timebase: 1,
        },
        format: CompiledFormatV4 {
            fps: FrameRate {
                numerator: 24,
                denominator: 1,
            },
            canvas_size: CanvasSizeV4 {
                width: 320,
                height: 180,
            },
            profile: FormatProfileV4::ClassicGlobal,
        },
        tracks: vec![],
        layers,
        audio: vec![],
        transitions: vec![],
        background: CompiledBackgroundV4::Color { color: color() },
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
fn font(bytes: &[u8]) -> FontResourceV4 {
    FontResourceV4 {
        id: "same".into(),
        family: "Test".into(),
        weight: 400,
        style: FontStyleV4::Normal,
        face_index: 0,
        content: version(bytes),
        origin: ResourceOriginV4::LocalImport {
            original_name: "test.ttf".into(),
        },
        license: None,
    }
}
fn publish(root: &Path, resource: &FontResourceV4, bytes: &[u8]) -> AssetStore {
    // This CAS port verifies bytes only. Deliberately malformed test bytes must
    // still be rejected by the actual native inspector before any coverage.
    ManagedFontPublisherV1::new(root)
        .publish(
            &PreparedFontV1 {
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
    let prepared = prepare_text_frame_v2(&graph(&font, &["ABCD", "EFGH"])).unwrap();
    let policy = FontImportPolicyV1 {
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
    let policy = FontImportPolicyV1 {
        max_font_bytes: 1,
        ..Default::default()
    };
    let error = VerifiedClassicTextResources::new(&absent, &prepared, &policy, || Ok(()))
        .err()
        .unwrap();
    assert_eq!(error.code, "FONT_BUDGET_EXCEEDED");
    let error = VerifiedClassicTextResources::new(
        &absent,
        &prepared,
        &FontImportPolicyV1::default(),
        || Err(EditorError::new("JOB_CANCELLED", "Stopped")),
    )
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
    let prepared = prepare_text_frame_v2(&graph(&resource, &["AV To"])).unwrap();
    let error =
        VerifiedClassicTextResources::new(&root, &prepared, &FontImportPolicyV1::default(), || {
            Ok(())
        })
        .err()
        .unwrap();
    assert_eq!(error.code, "UNSUPPORTED_FONT");
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
        let source = pinned_font_source_v1(catalog).unwrap();
        let bytes = std::fs::read(fixture.join(file)).unwrap();
        assert_eq!(version(&bytes).content_hash, source.content_hash);
        assert_eq!(bytes.len() as u64, source.size_bytes);
        let spec = source.import_spec("font".into(), contents.join(""));
        let font = prepare_font_v1(
            &StaticFontInspectorV1,
            &bytes,
            &spec,
            &FontImportPolicyV1::default(),
            &mut || Ok(()),
        )
        .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("cas");
        let _gate = ManagedFontPublisherV1::new(&root)
            .publish(&font, &bytes, &mut || Ok(()))
            .unwrap();
        let prepared = prepare_text_frame_v2(&graph(&font.resource, &contents)).unwrap();
        let cancelled = Rc::new(Cell::new(false));
        let flag = cancelled.clone();
        let mut resources = VerifiedClassicTextResources::new(
            &root,
            &prepared,
            &FontImportPolicyV1::default(),
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
        let missing = prepare_text_frame_v2(&graph(&font.resource, &["\u{10ffff}"])).unwrap();
        assert_eq!(
            VerifiedClassicTextResources::new(
                &root,
                &missing,
                &FontImportPolicyV1::default(),
                || Ok(())
            )
            .err()
            .unwrap()
            .code,
            "FONT_GLYPH_MISSING"
        );
        let mut wrong = font.resource.clone();
        wrong.family = "Different family".into();
        let wrong = prepare_text_frame_v2(&graph(&wrong, &contents)).unwrap();
        assert_eq!(
            VerifiedClassicTextResources::new(
                &root,
                &wrong,
                &FontImportPolicyV1::default(),
                || Ok(())
            )
            .err()
            .unwrap()
            .code,
            "FONT_FACE_MISMATCH"
        );
    }
}
