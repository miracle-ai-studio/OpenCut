use super::*;
use opencut_contracts::{
    AssetKind, AssetVersion, brand_resources::ImportBrandResource,
    catalog_resources::CatalogUploadedMedia,
};
use std::cell::Cell;
impl public::ProjectPublicResources for MemoryStore {
    fn authorized_project_media_scene(
        &self,
        project: &str,
        media: &str,
        version: &AssetVersion,
    ) -> Result<ProjectAsset, EditorError> {
        self.reads.set(self.reads.get() + 1);
        if project != "p"
            || media != "brand-upload"
            || version.content_hash != "a".repeat(64)
            || version.size != 1024
        {
            return Err(invalid("Denied upload selector"));
        }
        Ok(ProjectAsset {
            id: "observed-upload".into(),
            name: "source.png".into(),
            kind: AssetKind::Image,
            version: version.clone(),
            duration: None,
            origin: ResourceOrigin::LocalImport {
                original_name: "source.png".into(),
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
        })
    }
}
fn fixture() -> (MemoryStore, ImportBrandResource) {
    let mut store = MemoryStore::default();
    let source = create(&mut store, &input()).unwrap();
    let hash = "a".repeat(64);
    let request = ImportBrandResource {
        project_id: "p".into(),
        expected_revision: source.revision,
        idempotency_key: "brand-original".into(),
        catalog_id: "simple-icons:16.35.0:dotnet".into(),
        asset_id: "brand".into(),
        uploaded: CatalogUploadedMedia {
            media_id: "brand-upload".into(),
            version: AssetVersion {
                id: format!("av_{hash}"),
                content_hash: hash,
                size: 1024,
            },
        },
    };
    (store, request)
}
#[test]
fn brand_import_is_one_original_transaction_and_replay_does_not_read_upload_again() {
    let (mut store, request) = fixture();
    let first = brand_resources::import(&mut store, &request, &mut || Ok(())).unwrap();
    assert_eq!(first.created_ids, vec!["brand"]);
    assert_eq!(store.publications, 2);
    let StoredProjectDocument::Editable(project) = &store.heads["p"].document else {
        unreachable!()
    };
    assert_eq!(project.assets.len(), 1);
    assert_eq!(
        project.assets[0]
            .license
            .as_ref()
            .unwrap()
            .identifier
            .as_deref(),
        Some("CC0-1.0")
    );
    let reads = store.reads.get();
    store.heads.clear();
    assert_eq!(
        brand_resources::import(&mut store, &request, &mut || panic!(
            "replay must not reacquire"
        ))
        .unwrap(),
        first
    );
    assert_eq!(store.reads.get(), reads);
    assert_eq!(store.publications, 2);
    let mut changed = request;
    changed.asset_id = "different".into();
    assert!(brand_resources::import(&mut store, &changed, &mut || Ok(())).is_err());
}
#[test]
fn brand_import_rechecks_commit_authority_and_cancellation_without_publishing() {
    let (mut store, request) = fixture();
    let before = store.heads["p"].clone();
    store.deny_commit = true;
    assert!(brand_resources::import(&mut store, &request, &mut || Ok(())).is_err());
    assert_eq!(store.heads["p"], before);
    assert_eq!(store.publications, 1);
    store.deny_commit = false;
    let checks = Cell::new(0);
    assert!(
        brand_resources::import(&mut store, &request, &mut || {
            checks.set(checks.get() + 1);
            if checks.get() == 2 {
                Err(invalid("Cancelled"))
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    assert_eq!(store.heads["p"], before);
    assert_eq!(store.publications, 1);
}
#[test]
fn stale_revision_and_unavailable_license_do_not_acquire_or_publish() {
    let (mut store, mut request) = fixture();
    request.expected_revision = "stale".into();
    assert_eq!(
        brand_resources::import(&mut store, &request, &mut || panic!(
            "stale head must not acquire"
        ))
        .unwrap_err()
        .code,
        "REVISION_CONFLICT"
    );
    assert_eq!(store.publications, 1);
    request.catalog_id = "simple-icons:16.35.0:apple".into();
    assert_eq!(
        brand_resources::import(&mut store, &request, &mut || panic!(
            "unavailable source must not acquire"
        ))
        .unwrap_err()
        .code,
        "RESOURCE_LICENSE_UNAVAILABLE"
    );
}
