use super::*;
use opencut_contracts::{AssetVersion, catalog_resources::CatalogUploadedMedia};
fn request() -> ImportBrandResource {
    let hash = "a".repeat(64);
    ImportBrandResource {
        project_id: "p".into(),
        expected_revision: "r".into(),
        idempotency_key: "brand-original".into(),
        catalog_id: "simple-icons:16.35.0:dotnet".into(),
        asset_id: "brand".into(),
        uploaded: CatalogUploadedMedia {
            media_id: "upload".into(),
            version: AssetVersion {
                id: format!("av_{hash}"),
                content_hash: hash,
                size: 1024,
            },
        },
    }
}
fn observed(r: &ImportBrandResource) -> ProjectAsset {
    ProjectAsset {
        id: "uploaded".into(),
        name: "original.png".into(),
        kind: AssetKind::Image,
        version: r.uploaded.version.clone(),
        duration: None,
        origin: ResourceOrigin::LocalImport {
            original_name: "original.png".into(),
        },
        license: None,
        metadata: ProjectMediaMetadata {
            mime_type: Some("image/png".into()),
            intrinsic_size: Some(CanvasSize {
                width: 512,
                height: 512,
            }),
            frame_rate: None,
            rotation_degrees: None,
            audio: AudioPresence::Absent {},
        },
    }
}
#[test]
fn complete_collection_has_unique_pinned_ids_and_bounded_lossless_pagination() {
    let mut seen = std::collections::BTreeSet::new();
    let mut offset = 0;
    loop {
        let page = catalog(&ListBrandResources {
            query: None,
            offset,
            limit: 100,
        })
        .unwrap();
        assert_eq!(page.total, 3466);
        assert_eq!(page.matched, 3466);
        for e in page.entries {
            assert!(e.id.starts_with("simple-icons:16.35.0:"));
            assert!(seen.insert(e.id));
        }
        match page.next_offset {
            Some(next) => offset = next,
            None => break,
        }
    }
    assert_eq!(seen.len(), 3466);
    assert!(
        catalog(&ListBrandResources {
            query: None,
            offset: 0,
            limit: 101
        })
        .is_err()
    );
    assert!(
        catalog(&ListBrandResources {
            query: Some("a".repeat(257)),
            offset: 0,
            limit: 1
        })
        .is_err()
    );
    let search = catalog(&ListBrandResources {
        query: Some("dotnet".into()),
        offset: 0,
        limit: 40,
    })
    .unwrap();
    assert!(
        search
            .entries
            .iter()
            .any(|e| e.id == "simple-icons:16.35.0:dotnet")
    );
}
#[test]
fn missing_and_unreviewed_licenses_never_inherit_collection_cc0_or_expose_svg() {
    let unavailable: Vec<_> = records()
        .iter()
        .filter(|r| !r.entry.import_available)
        .collect();
    assert_eq!(unavailable.len(), 3386);
    for record in unavailable {
        assert!(record.entry.unavailable_reason.is_some());
        assert_eq!(
            source(&GetBrandSource {
                catalog_id: record.entry.id.clone()
            })
            .unwrap_err()
            .code,
            "RESOURCE_LICENSE_UNAVAILABLE"
        );
    }
    assert!(
        source(&GetBrandSource {
            catalog_id: "https://example.org/logo.svg".into()
        })
        .is_err()
    );
    assert!(
        source(&GetBrandSource {
            catalog_id: "simple-icons:latest:dotnet".into()
        })
        .is_err()
    );
}
#[test]
fn every_admitted_svg_retains_exact_source_hash_license_text_and_full_metadata_notice() {
    let admitted: Vec<_> = records()
        .iter()
        .filter(|r| r.entry.import_available)
        .collect();
    assert_eq!(admitted.len(), 80);
    for r in admitted {
        let value = source(&GetBrandSource {
            catalog_id: r.entry.id.clone(),
        })
        .unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(value.svg.as_bytes())),
            r.entry.svg_sha256
        );
        assert_eq!(value.license.identifier, r.entry.license_identifier);
        let notice = value.license.notice.unwrap();
        assert!(notice.contains(&r.entry.source_url));
        assert!(notice.contains(&r.entry.svg_sha256));
        assert!(notice.contains("No trademark rights"));
        assert!(notice.len() > 6000);
        if let Some(url) = &r.entry.guidelines_url {
            assert!(notice.contains(url));
        }
    }
}
#[test]
fn binder_preserves_authorized_png_and_attaches_only_the_exact_pinned_brand_provenance() {
    let r = request();
    let asset = observed(&r);
    let bound = bind_resource(&r, asset.clone()).unwrap();
    assert_eq!(bound.id, "brand");
    assert_eq!(bound.name, ".NET");
    assert_eq!(bound.version, asset.version);
    assert_eq!(bound.metadata, asset.metadata);
    assert_eq!(bound.license, Some(license(&r.catalog_id).unwrap()));
    assert!(
        matches!(&bound.origin,ResourceOrigin::Generated{provider,generation_id} if provider=="simple-icons-browser-raster-v1" && generation_id.contains(&entry(&r.catalog_id).unwrap().svg_sha256))
    );
    for change in 0..5 {
        let mut bad = asset.clone();
        match change {
            0 => bad.metadata.intrinsic_size.as_mut().unwrap().width = 511,
            1 => bad.metadata.mime_type = Some("image/svg+xml".into()),
            2 => bad.version.content_hash = "b".repeat(64),
            3 => bad.kind = AssetKind::Video,
            _ => bad.version.size = MAX_RASTER_BYTES + 1,
        };
        assert!(bind_resource(&r, bad).is_err());
    }
    let mut blocked = r;
    blocked.catalog_id = records()
        .iter()
        .find(|r| !r.entry.import_available)
        .unwrap()
        .entry
        .id
        .clone();
    assert!(validate_request(&blocked).is_err());
}
#[test]
fn sole_public_brand_request_rejects_markers_and_retains_original_receipt_identity() {
    let request = request();
    let current = serde_json::to_value(&request).unwrap();
    assert!(current.get("schema_version").is_none());
    let mut original = current;
    original
        .as_object_mut()
        .unwrap()
        .insert("schema_version".into(), 1.into());
    assert!(serde_json::from_value::<ImportBrandResource>(original.clone()).is_err());
    let expected =
        crate::identity::request_identity("brand_resources_import_v1", "p", &original).unwrap();
    let actual = crate::scene::SceneCommitIntent::BrandImport(&request)
        .identity()
        .unwrap();
    assert_eq!(actual.current_digest(), expected.current_digest());
    // Independently authored original struct order, including the immutable upload.
    let legacy_request = format!(
        r#"["brand_resources_import_v1","p",{{"schema_version":1,"project_id":"p","expected_revision":"r","idempotency_key":"brand-original","catalog_id":"simple-icons:16.35.0:dotnet","asset_id":"brand","uploaded":{{"media_id":"upload","version":{{"id":"av_{}","content_hash":"{}","size":1024}}}}}}]"#,
        "a".repeat(64),
        "a".repeat(64)
    );
    assert_eq!(
        actual.legacy_digest(),
        format!("{:x}", Sha256::digest(legacy_request.as_bytes()))
    );
}
