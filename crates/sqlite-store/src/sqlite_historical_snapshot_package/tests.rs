//! Synthetic CAS/proof fixtures exercise atomic storage and identity, not font
//! parsing, media decoding or reproduction of the recorded derivation recipe.
use super::*;
use opencut_api_contracts::historical_snapshot_package::{
    HistoricalSnapshotManifest, HistoricalSnapshotResource,
};
use opencut_asset_store::AssetStore;
use opencut_contracts::{
    Asset, AssetKind, AssetVersion, CreateProject, FrameRate, font_derived::*, scene::*,
    scene_ops::*,
};
use opencut_editor_application::{
    access::Caller,
    font_derived::{self as derived, proof::DerivedFontProofRead},
};
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|key| (key.clone(), canonical(&map[key])))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        _ => value.clone(),
    }
}
fn synthetic_source(version: &AssetVersion) -> FontDerivedSource {
    let mut source = derived::catalog()
        .unwrap()
        .into_iter()
        .find(|pin| pin.source.face.weight == 400 && pin.source.face.style == FontStyle::Normal)
        .unwrap()
        .source;
    let mut proof: Value = serde_json::from_str(&source.proof_json).unwrap();
    proof.as_object_mut().unwrap().remove("derivation_id");
    proof["output_sha256"] = json!(version.content_hash);
    proof["output_size"] = json!(version.size);
    let id = format!(
        "font-derived_{}",
        sha(serde_json::to_string(&canonical(&proof))
            .unwrap()
            .as_bytes())
    );
    proof["derivation_id"] = json!(id);
    source.face.derivation_id = id;
    source.face.content_hash = version.content_hash.clone();
    source.face.size_bytes = version.size.to_string();
    source.proof_json = serde_json::to_string(&canonical(&proof)).unwrap();
    derived::validate_pinned_source(&derived::PinnedDerivedFont {
        source: source.clone(),
    })
    .unwrap();
    source
}
struct Fixture {
    directory: tempfile::TempDir,
    app: EditorApplication,
    prepared: PreparedHistoricalSnapshot,
    input: HistoricalSnapshotImport,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let app = EditorApplication::open(directory.path().join("workspace.sqlite")).unwrap();
        let capture = |id: &str, bytes: &[u8]| {
            let path = directory.path().join(id);
            std::fs::write(&path, bytes).unwrap();
            AssetStore::open(app.asset_store_root())
                .unwrap()
                .pin_asset(&Asset {
                    id: id.into(),
                    name: id.into(),
                    kind: AssetKind::Image,
                    uri: path.to_string_lossy().into_owned(),
                    duration: None,
                    version: None,
                    metadata: Default::default(),
                })
                .unwrap()
                .version
                .unwrap()
        };
        let media = capture("source-media", b"package-fixed-media");
        let font_version = capture("source-font", b"package-fixed-font");
        let source = synthetic_source(&font_version);
        let mut document = opencut_editor_application::scene::new_document(&CreateProject {
            id: "source".into(),
            name: "Portable".into(),
            sequence_id: "scene".into(),
            width: 64,
            height: 48,
            fps: FrameRate {
                numerator: 24,
                denominator: 1,
            },
        })
        .unwrap();
        document.assets.push(ProjectAsset {
            id: "media".into(),
            name: "Unused but retained".into(),
            kind: AssetKind::Image,
            version: media.clone(),
            duration: None,
            origin: ResourceOrigin::LegacyLocator {
                uri: "/unavailable-original/image".into(),
            },
            metadata: ProjectMediaMetadata {
                mime_type: Some("image/png".into()),
                intrinsic_size: Some(CanvasSize {
                    width: 1,
                    height: 1,
                }),
                frame_rate: None,
                rotation_degrees: None,
                audio: AudioPresence::Absent {},
            },
            license: None,
        });
        document.fonts.push(FontResource {
            id: "font".into(),
            family: source.face.family.clone(),
            weight: source.face.weight,
            style: source.face.style,
            face_index: 0,
            content: font_version.clone(),
            origin: ResourceOrigin::Generated {
                provider: derived::OUTLINE_FONT_PROVIDER.into(),
                generation_id: source.face.derivation_id.clone(),
            },
            license: Some(ResourceLicense {
                identifier: Some(source.license_id.clone()),
                source_url: Some(source.license_url.clone()),
                notice: Some(source.license_notice.clone()),
                attribution: None,
            }),
        });
        let manifest = HistoricalSnapshotManifest {
            schema_version: 6,
            project_schema_version: 4,
            source_project_id: "source".into(),
            source_revision: "source-fixed".into(),
            project_path: "project.json".into(),
            project_sha256: sha(&serde_json::to_vec(&document).unwrap()),
            assets: vec![HistoricalSnapshotResource {
                resource_id: "media".into(),
                content_hash: media.content_hash.clone(),
                size_bytes: media.size,
                path: format!("media/{}", media.content_hash),
            }],
            fonts: vec![HistoricalSnapshotResource {
                resource_id: "font".into(),
                content_hash: font_version.content_hash.clone(),
                size_bytes: font_version.size,
                path: format!("fonts/{}", font_version.content_hash),
            }],
            derived_fonts: vec![source],
            self_contained_resources: true,
            history_included: false,
            fully_reproducible: false,
        };
        let digest = sha(&serde_json::to_vec(&manifest).unwrap());
        let input = HistoricalSnapshotImport {
            package_path: "/not-consumed-by-sqlite".into(),
            target_project_id: "restored".into(),
            expected_manifest_sha256: digest.clone(),
            idempotency_key: "restore".into(),
        };
        let prepared = PreparedHistoricalSnapshot {
            manifest_sha256: digest,
            manifest,
            document,
        };
        Self {
            directory,
            app,
            prepared,
            input,
        }
    }
    fn import(&mut self) -> HistoricalSnapshotImportReceipt {
        self.app
            .historical_snapshot_fixture_commit(&self.input, &self.prepared)
            .unwrap()
    }
    fn reseal(&mut self) {
        self.prepared.manifest.project_sha256 =
            sha(&serde_json::to_vec(&self.prepared.document).unwrap());
        self.prepared.manifest_sha256 = sha(&serde_json::to_vec(&self.prepared.manifest).unwrap());
        self.input.expected_manifest_sha256 = self.prepared.manifest_sha256.clone();
    }
}
fn counts(app: &EditorApplication) -> Vec<i64> {
    [
        "projects",
        "revisions",
        "outbox",
        "idempotency",
        "audit_records",
        "font_derivations",
    ]
    .into_iter()
    .map(|table| {
        app.connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    })
    .collect()
}
#[test]
fn package_restore_publishes_resources_and_proof_once_then_reopens_and_remains_editable() {
    let mut f = Fixture::new();
    let receipt = f.import();
    assert_eq!(
        (
            receipt.asset_count,
            receipt.font_count,
            receipt.derived_font_count
        ),
        (1, 1, 1)
    );
    let before = counts(&f.app);
    f.input.package_path = "/moved-package".into();
    assert_eq!(
        f.app
            .historical_snapshot_replay(&f.input)
            .unwrap()
            .unwrap()
            .revision,
        receipt.revision
    );
    assert_eq!(f.import().revision, receipt.revision);
    assert_eq!(counts(&f.app), before);
    let mut reopened =
        EditorApplication::open(f.directory.path().join("workspace.sqlite")).unwrap();
    let source = reopened
        .historical_snapshot_source("restored", &receipt.revision)
        .unwrap();
    let mut expected = f.prepared.document.clone();
    expected.id = "restored".into();
    assert_eq!(source.snapshot.project, expected);
    assert_eq!(
        source.derived_fonts[0].proof_json,
        f.prepared.manifest.derived_fonts[0].proof_json
    );
    let proof = reopened
        .read_derived_font_proof(&ReadDerivedFontProof {
            project_id: "restored".into(),
            revision: receipt.revision.clone(),
            font_id: "font".into(),
        })
        .unwrap();
    assert_eq!(proof.source.proof_json, source.derived_fonts[0].proof_json);
    let changed = reopened
        .apply_scene_project(&ApplySceneProject {
            project_id: "restored".into(),
            expected_revision: receipt.revision.clone(),
            idempotency_key: "edit-after-restore".into(),
            label: "Rename restored project".into(),
            operations: vec![ProjectOperation::Rename {
                name: "Edited".into(),
            }],
        })
        .unwrap();
    assert_ne!(changed.revision, receipt.revision);
    assert_eq!(
        reopened
            .historical_snapshot_source("restored", &receipt.revision)
            .unwrap()
            .snapshot
            .project
            .name,
        "Portable"
    );
}
#[test]
fn package_audit_failure_rolls_back_project_receipt_and_new_proof_together() {
    let mut f = Fixture::new();
    let before = counts(&f.app);
    f.app.connection.execute_batch("CREATE TRIGGER reject_package BEFORE INSERT ON audit_records WHEN NEW.action='project.package_v4_imported' BEGIN SELECT RAISE(ABORT,'forced audit failure'); END;").unwrap();
    assert!(
        f.app
            .historical_snapshot_fixture_commit(&f.input, &f.prepared)
            .is_err()
    );
    assert_eq!(counts(&f.app), before);
    f.app
        .connection
        .execute_batch("DROP TRIGGER reject_package")
        .unwrap();
    f.import();
}
#[test]
fn package_commit_rechecks_actual_cas_and_both_prepared_digests_before_publication() {
    for case in 0..4 {
        let mut f = Fixture::new();
        let before = counts(&f.app);
        match case {
            0 => f.prepared.document.name = "forged after parsing".into(),
            1 => {
                f.prepared.manifest.history_included = true;
            }
            2 => {
                f.prepared.document.assets[0].version.content_hash = "a".repeat(64);
                f.prepared.document.assets[0].version.id = format!("av_{}", "a".repeat(64));
                f.prepared.manifest.assets[0].content_hash = "a".repeat(64);
                f.prepared.manifest.assets[0].path = format!("media/{}", "a".repeat(64));
                f.reseal();
            }
            _ => {
                f.prepared.manifest.derived_fonts.clear();
                f.reseal();
            }
        }
        assert!(
            f.app
                .historical_snapshot_fixture_commit(&f.input, &f.prepared)
                .is_err(),
            "case {case}"
        );
        assert_eq!(counts(&f.app), before);
    }
}
#[test]
fn package_restore_never_overwrites_target_or_conflicting_immutable_proof() {
    let mut f = Fixture::new();
    let first = f.import();
    let before = counts(&f.app);
    f.input.idempotency_key = "different-key".into();
    assert_eq!(
        f.app
            .historical_snapshot_fixture_commit(&f.input, &f.prepared)
            .unwrap_err()
            .code,
        "ALREADY_EXISTS"
    );
    assert_eq!(counts(&f.app), before);
    f.input.target_project_id = "second".into();
    let source = &mut f.prepared.manifest.derived_fonts[0];
    source.face.id = "different-public-alias".into();
    f.reseal();
    assert_eq!(
        f.app
            .historical_snapshot_fixture_commit(&f.input, &f.prepared)
            .unwrap_err()
            .code,
        "STORAGE_ERROR"
    );
    assert_eq!(counts(&f.app), before);
    assert!(
        f.app
            .historical_snapshot_source("restored", &first.revision)
            .is_ok()
    );
}
#[test]
fn package_owner_gate_replay_conflict_and_plugin_dependency_refusal_remain_explicit() {
    let mut f = Fixture::new();
    let receipt = f.import();
    let before = counts(&f.app);
    f.input.target_project_id = "other".into();
    assert_eq!(
        f.app.historical_snapshot_replay(&f.input).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    f.app.set_caller(Caller::Credential(
        opencut_editor_application::access::VerifiedCredential::from_verified(
            "reader".into(),
            "verified-fixture".into(),
        ),
    ));
    assert_eq!(
        f.app.historical_snapshot_replay(&f.input).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        f.app
            .historical_snapshot_source("restored", &receipt.revision)
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(counts(&f.app), before);
    f.app.set_caller(Caller::LocalOwner);
    for table in ["project_plugin_grants", "project_preset_grants"] {
        f.app.connection.execute(&format!("INSERT INTO {table}(project_id,plugin_id,version,revision,grant_json) VALUES('restored','example','1',1,'{{}}')"), []).unwrap();
        assert_eq!(
            f.app
                .historical_snapshot_source("restored", &receipt.revision)
                .unwrap_err()
                .code,
            "PACKAGE_DEPENDENCY_UNSUPPORTED"
        );
        f.app
            .connection
            .execute(&format!("DELETE FROM {table}"), [])
            .unwrap();
    }
}
