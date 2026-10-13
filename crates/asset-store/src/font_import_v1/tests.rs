use super::*;
use opencut_contracts::project_v4::{FontResourceV4, FontStyleV4, ResourceOriginV4};

fn prepared(bytes: &[u8]) -> PreparedFontV1 {
    let hash = format!("{:x}", Sha256::digest(bytes));
    PreparedFontV1 {
        resource: FontResourceV4 {
            id: "font".into(),
            family: "Fixture".into(),
            weight: 400,
            style: FontStyleV4::Normal,
            face_index: 0,
            content: AssetVersion {
                id: format!("av_{hash}"),
                content_hash: hash,
                size: bytes.len() as u64,
            },
            origin: ResourceOriginV4::LocalImport {
                original_name: "Synthetic publisher fixture".into(),
            },
            license: None,
        },
        glyph_count: 1,
        checked_codepoints: 1,
    }
}

#[test]
fn publishes_exact_immutable_bytes_and_reuses_without_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let publisher = ManagedFontPublisherV1::new(directory.path().join("cas"));
    let bytes = b"publisher fixture bytes, not an actual font";
    let input = prepared(bytes);
    let lease = publisher.publish(&input, bytes, &mut || Ok(())).unwrap();
    let (mut file, size) = lease
        .open_version_verified(&input.resource.content)
        .unwrap();
    assert_eq!(size, bytes.len() as u64);
    let mut actual = Vec::new();
    file.read_to_end(&mut actual).unwrap();
    assert_eq!(actual, bytes);
    assert!(file.metadata().unwrap().permissions().readonly());
    #[cfg(unix)]
    assert_eq!(
        gate::Gate::exclusive(&publisher.root).unwrap_err().code,
        "STORAGE_BUSY"
    );
    let second = publisher.publish(&input, bytes, &mut || Ok(())).unwrap();
    assert!(
        second
            .open_version_verified(&input.resource.content)
            .is_ok()
    );
    assert!(!fs::read_dir(&publisher.root).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".capture-")
    }));
    drop(second);
    drop(lease);
    #[cfg(unix)]
    assert!(gate::Gate::exclusive(&publisher.root).is_ok());
}

#[test]
fn refuses_changed_bytes_and_corrupt_existing_object_without_overwriting() {
    let directory = tempfile::tempdir().unwrap();
    let publisher = ManagedFontPublisherV1::new(directory.path().join("cas"));
    let bytes = b"abc";
    let input = prepared(bytes);
    assert_eq!(
        publisher
            .publish(&input, b"abd", &mut || Ok(()))
            .err()
            .unwrap()
            .code,
        "ASSET_INTEGRITY_ERROR"
    );
    let store = AssetStore::open_managed(&publisher.root).unwrap();
    let path = store
        .object_path(&input.resource.content.content_hash)
        .unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"bad").unwrap();
    assert_eq!(
        publisher
            .publish(&input, bytes, &mut || Ok(()))
            .err()
            .unwrap()
            .code,
        "ASSET_INTEGRITY_ERROR"
    );
    assert_eq!(fs::read(path).unwrap(), b"bad");
}

#[test]
fn revocation_during_copy_cleans_partial_file_and_never_publishes() {
    let directory = tempfile::tempdir().unwrap();
    let publisher = ManagedFontPublisherV1::new(directory.path().join("cas"));
    let bytes = vec![7; 65536 * 3];
    let input = prepared(&bytes);
    let mut checks = 0;
    let result = publisher.publish(&input, &bytes, &mut || {
        checks += 1;
        // Initial check + three hash chunks + first written chunk succeed.
        if checks == 6 {
            Err(error("NOT_AUTHORIZED", "revoked"))
        } else {
            Ok(())
        }
    });
    assert_eq!(result.err().unwrap().code, "NOT_AUTHORIZED");
    let store = AssetStore::read_only(&publisher.root).unwrap();
    assert!(
        !store
            .object_path(&input.resource.content.content_hash)
            .unwrap()
            .exists()
    );
    assert!(!fs::read_dir(&publisher.root).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".capture-")
    }));
}
