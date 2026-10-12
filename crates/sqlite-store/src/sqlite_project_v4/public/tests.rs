use super::*;
use opencut_contracts::{Asset, AssetKind, FrameRate};
use opencut_editor_application::access::{Caller, PrincipalPolicy, ProjectGrant};
use std::collections::BTreeMap;
mod source_audio;

#[test]
fn packet_observation_preserves_header_and_scoped_raw_rate_through_binding() {
    use opencut_media_contracts::{CLASSIC_PACKET_RATE_METADATA_KEY_V1, ClassicPacketRateV1};
    for rate in [Some(60.0), None] {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("workspace.sqlite");
        let mut app = EditorApplication::open(&database).unwrap();
        let initial = create(&mut app, "p");
        create(&mut app, "other");
        let (version, _) = media(&app, directory.path(), "p");
        let raw: String = app
            .connection
            .query_row(
                "SELECT asset_json FROM project_media WHERE media_id='media-fixture'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let mut asset: Asset = serde_json::from_str(&raw).unwrap();
        asset.kind = AssetKind::Video;
        asset.uri = "opencut-media://media-fixture".into();
        asset
            .metadata
            .insert("frame_rate".into(), json!({"numerator":24,"denominator":1}));
        let observation = ClassicPacketRateV1 {
            schema_version: 1,
            packet_count: 100,
            average_packet_rate: rate,
        };
        asset.metadata.insert(
            CLASSIC_PACKET_RATE_METADATA_KEY_V1.into(),
            json!(observation),
        );
        let raw = serde_json::to_string(&asset).unwrap();
        app.connection
            .execute(
                "UPDATE project_media SET asset_json=?1 WHERE media_id='media-fixture'",
                [&raw],
            )
            .unwrap();
        restricted(&mut app, "p");
        assert_eq!(
            app.authorized_project_media_packet_rate_v4("p", "media-fixture", &version)
                .unwrap(),
            Some(observation.clone())
        );
        assert!(
            app.authorized_project_media_packet_rate_v4("other", "media-fixture", &version)
                .is_err()
        );
        let wrong = AssetVersion {
            size: version.size + 1,
            ..version.clone()
        };
        assert!(
            app.authorized_project_media_packet_rate_v4("p", "media-fixture", &wrong)
                .is_err()
        );
        let projected = app
            .authorized_project_media_v4("p", "media-fixture", &version)
            .unwrap();
        assert_eq!(
            projected.metadata.frame_rate,
            rate.map(|_| FrameRate {
                numerator: 60,
                denominator: 1
            })
        );
        let receipt = app
            .apply_public_v4_project(&input(&initial, "media-fixture", &version))
            .unwrap();
        assert_eq!(
            app.project_asset_packet_rate_v4("p", &receipt.revision, "alias")
                .unwrap(),
            Some(observation.clone())
        );
        assert!(
            app.project_asset_packet_rate_v4("p", &initial.revision, "alias")
                .is_err()
        );
        let persisted: String = app
            .connection
            .query_row(
                "SELECT asset_json FROM project_media WHERE media_id='media-fixture'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(persisted, raw);
        drop(app);
        let app = EditorApplication::open(&database).unwrap();
        assert_eq!(
            app.project_asset_packet_rate_v4("p", &receipt.revision, "alias")
                .unwrap(),
            Some(observation)
        );
        asset.metadata.insert(
            CLASSIC_PACKET_RATE_METADATA_KEY_V1.into(),
            json!({"schema_version":2,"packet_count":100,"average_packet_rate":60}),
        );
        assert!(resources::canonical_asset(&asset, &version).is_none());
    }
}

fn create(app: &mut EditorApplication, id: &str) -> ProjectSnapshotV4 {
    app.create_v4_project(&CreateV4Project {
        project: CreateProject {
            id: id.into(),
            name: id.into(),
            sequence_id: "main".into(),
            width: 64,
            height: 48,
            fps: FrameRate {
                numerator: 25,
                denominator: 1,
            },
        },
        idempotency_key: format!("create-{id}"),
    })
    .unwrap()
}
fn input(
    snapshot: &ProjectSnapshotV4,
    media_id: &str,
    version: &AssetVersion,
) -> ApplyPublicV4Project {
    ApplyPublicV4Project {
        project_id: snapshot.project.id.clone(),
        expected_revision: snapshot.revision.clone(),
        idempotency_key: "public-bind".into(),
        label: "Bind scoped media".into(),
        operations: vec![PublicProjectOperationV4::AssetBind {
            id: "alias".into(),
            name: "Client name".into(),
            source: PublicAssetSelectorV4::ProjectMedia {
                media_id: media_id.into(),
                version: version.clone(),
            },
        }],
    }
}
/// Seed the same immutable observation rows used by native upload publication.
/// CAS contains real tiny bytes; the probe observation remains unversioned, as
/// production project_media commonly does, with authoritative hash/size fields.
fn media(
    app: &EditorApplication,
    directory: &std::path::Path,
    project_id: &str,
) -> (AssetVersion, String) {
    let path = directory.join("private-source.bin");
    std::fs::write(&path, b"tiny public binding CAS fixture").unwrap();
    let mut asset = Asset {
        id: "media-fixture".into(),
        name: "Observation name".into(),
        kind: AssetKind::Image,
        uri: path.to_string_lossy().into(),
        duration: None,
        version: None,
        metadata: BTreeMap::new(),
    };
    let pinned = AssetStore::open(&app.asset_store_root)
        .unwrap()
        .pin_asset(&asset)
        .unwrap();
    let version = pinned.version.unwrap();
    asset.metadata = BTreeMap::from([
        ("content_hash".into(), json!(version.content_hash)),
        ("size".into(), json!(version.size)),
        ("mime_type".into(), json!("application/octet-stream")),
        ("has_audio".into(), json!(false)),
    ]);
    app.connection.execute("INSERT INTO jobs(job_id,state,attempt,kind,input_json,cancel_requested) VALUES ('producer','succeeded',1,'media.import','{}',0)",[]).unwrap();
    app.connection.execute("INSERT INTO upload_sessions(upload_id,project_id,idempotency_key,request_hash,name,size_bytes,content_hash,state,job_id) VALUES ('upload',?1,'upload','fixture-hash','Observation',?2,?3,'sealed','producer')",params![project_id,version.size,version.content_hash]).unwrap();
    app.connection.execute("INSERT INTO project_media(media_id,project_id,upload_id,job_id,asset_json) VALUES ('media-fixture',?1,'upload','producer',?2)",params![project_id,serde_json::to_string(&asset).unwrap()]).unwrap();
    (version, asset.uri)
}
fn restricted(app: &mut EditorApplication, project_id: &str) {
    app.set_principal_policy(PrincipalPolicy {
        principal_id: "public-agent".into(),
        expected_revision: "0".into(),
        enabled: true,
        grants: vec![ProjectGrant {
            project_id: project_id.into(),
            actions: vec![ProjectAction::Read, ProjectAction::Edit],
        }],
    })
    .unwrap();
    let issued = app.issue_credential("public-agent").unwrap();
    let caller = app.authenticate(&issued.secret).unwrap();
    app.set_caller(caller);
}
fn counts(app: &EditorApplication) -> Vec<i64> {
    [
        "projects",
        "revisions",
        "editor_transactions",
        "idempotency",
        "outbox",
        "audit_records",
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
struct Capture<'a> {
    app: &'a mut EditorApplication,
    prepared: Option<PreparedV4Commit>,
}
impl ProjectV4Read for Capture<'_> {
    fn authorize_v4(&self, intent: V4Intent<'_>) -> Result<(), EditorError> {
        self.app.authorize_v4(intent)
    }
    fn head_v4_source(&self, id: &str) -> Result<StoredProjectRevisionV4, EditorError> {
        self.app.head_v4_source(id)
    }
    fn revision_v4_source(
        &self,
        id: &str,
        revision: &str,
    ) -> Result<StoredProjectRevisionV4, EditorError> {
        self.app.revision_v4_source(id, revision)
    }
    fn replay_v4_create(
        &self,
        request: &CreateV4Project,
    ) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        self.app.replay_v4_create(request)
    }
    fn replay_v4_copy(
        &self,
        request: &CopyV4Project,
    ) -> Result<Option<ProjectSnapshotV4>, EditorError> {
        self.app.replay_v4_copy(request)
    }
    fn replay_v4_edit(
        &self,
        intent: V4Intent<'_>,
    ) -> Result<Option<TransactionResult>, EditorError> {
        self.app.replay_v4_edit(intent)
    }
}
impl ProjectV4PublicResources for Capture<'_> {
    fn authorized_project_media_v4(
        &self,
        project_id: &str,
        media_id: &str,
        version: &AssetVersion,
    ) -> Result<AssetV4, EditorError> {
        self.app
            .authorized_project_media_v4(project_id, media_id, version)
    }
}
impl ProjectV4Store for Capture<'_> {
    fn create_v4_atomic(
        &mut self,
        request: &CreateV4Project,
        prepared: PreparedV4Create,
    ) -> Result<ProjectSnapshotV4, EditorError> {
        self.app.create_v4_atomic(request, prepared)
    }
    fn copy_v4_atomic(
        &mut self,
        request: &CopyV4Project,
        prepared: PreparedV4Create,
    ) -> Result<ProjectSnapshotV4, EditorError> {
        self.app.copy_v4_atomic(request, prepared)
    }
    fn commit_v4_atomic(
        &mut self,
        _: V4Intent<'_>,
        prepared: PreparedV4Commit,
    ) -> Result<TransactionResult, EditorError> {
        self.prepared = Some(prepared);
        Err(EditorError::new("CAPTURED", "Before native publication"))
    }
}

#[test]
fn observed_media_binds_and_public_receipt_identity_never_contains_private_provenance() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(directory.path().join("workspace.sqlite")).unwrap();
    let initial = create(&mut app, "p");
    let (version, uri) = media(&app, directory.path(), "p");
    restricted(&mut app, "p");
    let request = input(&initial, "media-fixture", &version);
    let candidate = app.plan_public_v4_project(&request).unwrap();
    assert!(matches!(
        candidate.assets[0].origin,
        PublicResourceOriginV4::Legacy {}
    ));
    let result = app.apply_public_v4_project(&request).unwrap();
    let snapshot = app
        .inspect_v4_project(&InspectV4Project {
            project_id: "p".into(),
            revision: Some(result.revision.clone()),
        })
        .unwrap();
    let bound = &snapshot.project.assets[0];
    assert_eq!(bound.id, "alias");
    assert_eq!(bound.name, "Client name");
    assert_eq!(bound.version, version);
    assert_eq!(
        bound.origin,
        ResourceOriginV4::LegacyLocator { uri: uri.clone() }
    );
    assert_eq!(bound.metadata.audio, AudioPresenceV4::Absent {});
    let projected = public::project(&snapshot);
    assert!(!serde_json::to_string(&projected).unwrap().contains(&uri));
    let journal: String = app
        .connection
        .query_row(
            "SELECT request_json FROM editor_transactions WHERE transaction_id=?1",
            [&result.transaction_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<ApplyPublicV4Project>(&journal).unwrap(),
        request
    );
    assert!(!journal.contains(&uri));
    assert!(!journal.contains("legacy_locator"));
    let digest:String=app.connection.query_row("SELECT request_hash FROM idempotency WHERE operation_family='project_v4_public_apply' AND scope='p' AND idempotency_key='public-bind'",[],|row|row.get(0)).unwrap();
    assert!(
        V4Intent::PublicApply(&request)
            .identity()
            .unwrap()
            .matches(&digest, None)
    );
    let before = counts(&app);
    assert_eq!(app.apply_public_v4_project(&request).unwrap(), result);
    assert_eq!(counts(&app), before);
    let mut changed = request.clone();
    changed.label = "Another original request".into();
    assert_eq!(
        app.apply_public_v4_project(&changed).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    assert_eq!(counts(&app), before);
}

#[test]
fn identical_cas_hash_without_project_media_membership_is_not_authority() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = EditorApplication::open(directory.path().join("workspace.sqlite")).unwrap();
    let a = create(&mut app, "a");
    create(&mut app, "b");
    let (version, _) = media(&app, directory.path(), "b");
    restricted(&mut app, "a");
    let before = counts(&app);
    let request = input(&a, "media-fixture", &version);
    assert_eq!(
        app.apply_public_v4_project(&request).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(counts(&app), before);
    // The bytes exist in the same CAS, and even the exact version is supplied;
    // the selector still needs the target project's scoped observation row.
    assert_eq!(
        app.plan_public_v4_project(&request).unwrap_err().code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(counts(&app), before);
}

#[test]
fn removed_media_membership_after_prepare_rejects_atomic_publication() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workspace.sqlite");
    let mut app = EditorApplication::open(&path).unwrap();
    let target = create(&mut app, "target");
    create(&mut app, "producer-project");
    let (version, _) = media(&app, directory.path(), "producer-project");
    app.connection.execute("INSERT INTO project_media_bindings(project_id,media_id) VALUES ('target','media-fixture')",[]).unwrap();
    restricted(&mut app, "target");
    let request = input(&target, "media-fixture", &version);
    let payload = {
        let mut capture = Capture {
            app: &mut app,
            prepared: None,
        };
        assert_eq!(
            public::apply_public(&mut capture, &request)
                .unwrap_err()
                .code,
            "CAPTURED"
        );
        capture.prepared.take().unwrap()
    };
    let owner = EditorApplication::open(&path).unwrap();
    owner.connection.execute("DELETE FROM project_media_bindings WHERE project_id='target' AND media_id='media-fixture'",[]).unwrap();
    let before = counts(&app);
    assert_eq!(
        app.commit_v4_atomic(V4Intent::PublicApply(&request), payload)
            .unwrap_err()
            .code,
        "NOT_AUTHORIZED"
    );
    assert_eq!(counts(&app), before);
    app.set_caller(Caller::LocalOwner);
    assert_eq!(
        app.inspect_v4_project(&InspectV4Project {
            project_id: "target".into(),
            revision: None
        })
        .unwrap(),
        target
    );
}

#[test]
fn real_v4_media_plan_submits_unchanged_and_reopens_one_atomic_revision() {
    use opencut_contracts::media_import_v4::*;
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("workspace.sqlite");
    let mut app = EditorApplication::open(&database).unwrap();
    let initial = create(&mut app, "p");
    let (version, private_locator) = media(&app, directory.path(), "p");
    restricted(&mut app, "p");
    let caller = app.connection.1.caller().clone();
    let request = MediaImportRequestV4 {
        project_id: "p".into(),
        expected_revision: initial.revision.clone(),
        sequence_id: "main".into(),
        idempotency_key: "media-plan-submit".into(),
        label: "Import and place".into(),
        ripple: false,
        bindings: vec![MediaBindingV4 {
            media_id: "media-fixture".into(),
            version,
            asset_id: "alias".into(),
            name: "Image".into(),
            mode: MediaBindingModeV4::Bind,
        }],
        placements: vec![MediaPlacementV4 {
            element_id: "placed".into(),
            name: "Image".into(),
            track_id: initial.project.sequences[0].main_track_id.clone(),
            asset_id: "alias".into(),
            start: opencut_contracts::Time {
                ticks: "3".into(),
                timebase: 1,
            },
            content: MediaPlacementContentV4::Image {
                duration: opencut_contracts::Time {
                    ticks: "1".into(),
                    timebase: 1,
                },
            },
        }],
    };
    let before = counts(&app);
    let plan = app.plan_v4_media_import(&request).unwrap();
    assert_eq!(counts(&app), before);
    assert!(
        !serde_json::to_string(&plan)
            .unwrap()
            .contains(&private_locator)
    );
    let result = app.apply_public_v4_project(&plan.transaction).unwrap();
    let after = counts(&app);
    assert_eq!(
        app.apply_public_v4_project(&plan.transaction).unwrap(),
        result
    );
    assert_eq!(counts(&app), after);
    assert_eq!(
        app.plan_v4_media_import(&request).unwrap_err().code,
        "REVISION_CONFLICT"
    );
    drop(app);
    let mut app = EditorApplication::open(&database).unwrap();
    app.set_caller(caller);
    let snapshot = app
        .inspect_v4_project(&InspectV4Project {
            project_id: "p".into(),
            revision: None,
        })
        .unwrap();
    assert_eq!(snapshot.revision, result.revision);
    assert_eq!(snapshot.project.assets[0].id, "alias");
    let element = &snapshot.project.sequences[0].tracks[0].elements[0];
    assert_eq!(element.id, "placed");
    assert_eq!(
        element.timeline_range.start,
        opencut_contracts::Time {
            ticks: "0".into(),
            timebase: 1
        }
    );
    assert_eq!(
        element.timeline_range.duration,
        opencut_contracts::Time {
            ticks: "1".into(),
            timebase: 1
        }
    );
}
