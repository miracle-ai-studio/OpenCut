use super::*;
use opencut_contracts::{AssetKind, AssetVersion, FrameRate};
mod fixture;
use fixture::Fixture;

fn version() -> AssetVersion {
    AssetVersion {
        id: format!("av_{}", "0".repeat(64)),
        content_hash: "0".repeat(64),
        size: 1,
    }
}
fn snapshot() -> ProjectSnapshotV4 {
    let mut project = super::super::new_document(&CreateProject {
        id: "p".into(),
        name: "P".into(),
        sequence_id: "s".into(),
        width: 320,
        height: 180,
        fps: FrameRate {
            numerator: 24,
            denominator: 1,
        },
    })
    .unwrap();
    let origin = ResourceOriginV4::LegacyLocator {
        uri: "/Users/private/source/image.png".into(),
    };
    let license = Some(ResourceLicenseV4 {
        identifier: Some("Example-License".into()),
        source_url: Some("file:///Users/private/license".into()),
        notice: Some("Retain this notice".into()),
        attribution: Some("Example author".into()),
    });
    project.assets.push(AssetV4 {
        id: "asset".into(),
        name: "Image".into(),
        kind: AssetKind::Image,
        version: version(),
        duration: None,
        origin: origin.clone(),
        metadata: MediaMetadataV4 {
            mime_type: Some("image/png".into()),
            intrinsic_size: Some(CanvasSizeV4 {
                width: 320,
                height: 180,
            }),
            frame_rate: None,
            rotation_degrees: Some(0.0),
            audio: AudioPresenceV4::Absent {},
        },
        license: license.clone(),
    });
    project.fonts.push(FontResourceV4 {
        id: "font".into(),
        family: "Example".into(),
        weight: 400,
        style: FontStyleV4::Normal,
        face_index: 0,
        content: version(),
        origin,
        license,
    });
    project.sequences[0]
        .bookmarks
        .push(opencut_contracts::scene_bookmarks::SceneBookmarkV1 {
            id: "b".into(),
            sequence_id: "s".into(),
            time: opencut_contracts::Time {
                ticks: "1".into(),
                timebase: 24,
            },
            duration: None,
            color: None,
            note: Some("Marker".into()),
        });
    validate_project_v4(&project).unwrap();
    ProjectSnapshotV4 {
        project,
        revision: "fixed".into(),
    }
}
fn request(
    snapshot: &ProjectSnapshotV4,
    operations: Vec<PublicProjectOperationV4>,
) -> ApplyPublicV4Project {
    ApplyPublicV4Project {
        project_id: "p".into(),
        expected_revision: snapshot.revision.clone(),
        idempotency_key: "public".into(),
        label: "Edit".into(),
        operations,
    }
}
fn bindings() -> Vec<PublicProjectOperationV4> {
    vec![
        PublicProjectOperationV4::AssetBind {
            id: "alias".into(),
            name: "Renamed".into(),
            source: PublicAssetSelectorV4::ProjectAsset {
                revision: "fixed".into(),
                asset_id: "asset".into(),
            },
        },
        PublicProjectOperationV4::FontBind {
            id: "font-alias".into(),
            source: PublicFontSelectorV4::ProjectFont {
                revision: "fixed".into(),
                font_id: "font".into(),
            },
        },
    ]
}

#[test]
fn complete_public_roundtrip_keeps_scene_semantics_and_hides_private_resource_locators() {
    let original = snapshot();
    let public = project(&original);
    let json = serde_json::to_string(&public).unwrap();
    assert!(!json.contains("/Users/private"));
    assert!(!json.contains("legacy_locator"));
    let roundtrip: PublicProjectSnapshotV4 = serde_json::from_str(&json).unwrap();
    assert_eq!(roundtrip, public);
    assert_eq!(public.project.settings, original.project.settings);
    assert_eq!(public.project.sequences, original.project.sequences);
    assert_eq!(
        public.project.assets[0].metadata,
        original.project.assets[0].metadata
    );
    assert_eq!(
        public.project.assets[0]
            .license
            .as_ref()
            .unwrap()
            .notice
            .as_deref(),
        Some("Retain this notice")
    );
    assert!(
        public.project.assets[0]
            .license
            .as_ref()
            .unwrap()
            .source_url
            .is_none()
    );
    assert!(matches!(
        public.project.assets[0].origin,
        PublicResourceOriginV4::Legacy {}
    ));
    assert!(matches!(
        original.project.assets[0].origin,
        ResourceOriginV4::LegacyLocator { .. }
    ));
}

#[test]
fn provenance_url_projection_avoids_credential_and_ambiguous_urls_without_rewriting() {
    for (url, allowed) in [
        ("https://example.com/license", true),
        ("http://example.com:8080/source", true),
        ("file:///private/source", false),
        ("https://user:pass@example.com/source", false),
        ("https://example.com/source?token=secret", false),
        ("https://example.com/source#secret", false),
        ("https://example.com\\@other/source", false),
        ("https:///private/source", false),
        ("https://example.com/%2Fprivate", false),
        ("https://example.com/source\n", false),
    ] {
        let mut value = snapshot();
        value.project.fonts[0].origin = ResourceOriginV4::Online {
            provider: "example".into(),
            source_url: url.into(),
        };
        value.project.fonts[0].license.as_mut().unwrap().source_url = Some(url.into());
        let public = project(&value);
        let PublicResourceOriginV4::Online { source_url, .. } = &public.project.fonts[0].origin
        else {
            panic!("online")
        };
        assert_eq!(source_url.as_deref(), allowed.then_some(url));
        assert_eq!(
            public.project.fonts[0]
                .license
                .as_ref()
                .unwrap()
                .source_url
                .as_deref(),
            allowed.then_some(url)
        );
    }
    let mut value = snapshot();
    value.project.assets[0].origin = ResourceOriginV4::LocalImport {
        original_name: "C:\\private\\image.png".into(),
    };
    assert_eq!(
        project(&value).project.assets[0].origin,
        PublicResourceOriginV4::LocalImport {
            original_name: Some("image.png".into())
        }
    );
}

#[test]
fn fixed_bindings_preserve_private_records_and_original_public_request_identity() {
    let source = snapshot();
    let mut store = Fixture::new(source.clone());
    store.current.revision = "head".into();
    store.current.project.assets[0].name = "Head name".into();
    let input = request(&store.current, bindings());
    let candidate = plan_public(&store, &input).unwrap();
    assert_eq!(store.commits, 0);
    assert!(store.receipt.is_none());
    assert_eq!(candidate.assets[1].name, "Renamed");
    assert!(
        !serde_json::to_string(&candidate)
            .unwrap()
            .contains("/Users/private")
    );
    let result = apply_public(&mut store, &input).unwrap();
    let mut expected = source.project.assets[0].clone();
    expected.id = "alias".into();
    expected.name = "Renamed".into();
    assert_eq!(store.current.project.assets[1], expected);
    let mut expected_font = source.project.fonts[0].clone();
    expected_font.id = "font-alias".into();
    assert_eq!(store.current.project.fonts[1], expected_font);
    assert_eq!(
        serde_json::from_str::<ApplyPublicV4Project>(&store.journal).unwrap(),
        input
    );
    assert!(!store.journal.contains("/Users/private"));
    assert!(!store.journal.contains("legacy_locator"));
    let reads = store.selection_reads.get();
    assert_eq!(apply_public(&mut store, &input).unwrap(), result);
    assert_eq!(store.selection_reads.get(), reads);
    assert_eq!(store.commits, 1);
    let mut changed = input.clone();
    changed.label = "Different".into();
    assert_eq!(
        apply_public(&mut store, &changed).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
}

#[test]
fn unscoped_media_hash_and_wrong_fixed_revision_cannot_bind_or_publish() {
    let source = snapshot();
    let mut store = Fixture::new(source.clone());
    store.media = Some(source.project.assets[0].clone());
    let input = request(
        &source,
        vec![PublicProjectOperationV4::AssetBind {
            id: "alias".into(),
            name: "A".into(),
            source: PublicAssetSelectorV4::ProjectMedia {
                media_id: "unscoped".into(),
                version: version(),
            },
        }],
    );
    assert_eq!(
        apply_public(&mut store, &input).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(store.commits, 0);
    let mut allowed = input.clone();
    if let PublicProjectOperationV4::AssetBind {
        source: PublicAssetSelectorV4::ProjectMedia { media_id, .. },
        ..
    } = &mut allowed.operations[0]
    {
        *media_id = "available".into();
    }
    assert!(plan_public(&store, &allowed).is_ok());
    assert_eq!(store.commits, 0);
    store.fixed.get_mut("fixed").unwrap().revision = "another".into();
    assert_eq!(
        apply_public(&mut store, &request(&source, bindings()))
            .unwrap_err()
            .code,
        "STORAGE_ERROR"
    );
    assert_eq!(store.commits, 0);
    store.deny = true;
    let reads = store.selection_reads.get();
    assert_eq!(
        plan_public(&store, &allowed).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(store.selection_reads.get(), reads);
}

#[test]
fn public_writes_reject_internal_resource_payloads_and_selector_extensions() {
    let value = serde_json::json!({"type":"asset_put","asset":snapshot().project.assets[0]});
    assert!(serde_json::from_value::<PublicProjectOperationV4>(value).is_err());
    let value = serde_json::json!({"type":"asset_bind","id":"alias","name":"A","source":{"kind":"project_asset","revision":"fixed","asset_id":"asset","uri":"/private/host"}});
    assert!(serde_json::from_value::<PublicProjectOperationV4>(value).is_err());
}
