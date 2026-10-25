use super::*;
fn request() -> ImportClassic {
    let source = AnalyzeClassic { target_project_id: "direct-v4".into(), media_mappings: vec![], font_mappings: vec![], content: json!({"version":31,"metadata":{"id":"classic","name":"Classic"},"settings":{"fps":{"numerator":24,"denominator":1},"canvasSize":{"width":64,"height":48},"background":{"type":"color","color":"#000000"}},"scenes":[{"id":"main","name":"Main","isMain":true,"bookmarks":[],"tracks":{"main":{"id":"video","name":"Video","type":"video","elements":[]},"overlay":[],"audio":[]}}]}).to_string() };
    ImportClassic {
        expected_source_content_id: opencut_editor_application::classic_restore::source_content_id(
            &source.content,
        ),
        source,
        idempotency_key: "direct-key".into(),
        accepted_loss_codes: vec![],
    }
}
#[test]
fn direct_public_view_and_exact_replay_conflict() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let input = request();
    let analysis = app.classic_restore_analyze(&input.source).unwrap();
    assert!(analysis.blockers.is_empty(), "{:?}", analysis.blockers);
    let public = serde_json::to_string(&analysis).unwrap();
    assert!(!public.contains("asset_store_root"));
    assert!(!public.contains("license_notice"));
    let first = app.classic_restore_import(&input).unwrap();
    assert_eq!(
        app.classic_restore_import(&input).unwrap().revision,
        first.revision
    );
    let mut conflict = input.clone();
    conflict.source.target_project_id = "other".into();
    assert_eq!(
        app.classic_restore_import(&conflict).unwrap_err().code,
        "IDEMPOTENCY_KEY_REUSED"
    );
    let raw: String = app
        .connection
        .query_row(
            "SELECT document_json FROM revisions WHERE revision=?1",
            [&first.revision],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&raw).unwrap()["schema_version"],
        4
    );
}
#[test]
fn audit_failure_rolls_back_all_publication() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let input = request();
    app.connection.execute_batch("CREATE TRIGGER fail_direct BEFORE INSERT ON audit_records WHEN NEW.action='project.classic_imported' BEGIN SELECT RAISE(ABORT,'forced'); END;").unwrap();
    assert!(app.classic_restore_import(&input).is_err());
    for table in ["projects", "revisions", "outbox", "idempotency"] {
        let count: i64 = app
            .connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
    app.connection
        .execute_batch("DROP TRIGGER fail_direct")
        .unwrap();
    assert!(app.classic_restore_import(&input).is_ok());
}
#[test]
fn fixed_source_missing_and_duplicate_are_not_accepted() {
    let app = EditorApplication::open(":memory:").unwrap();
    let mut input = request().source;
    input.font_mappings.push(ClassicFontMapping {
        classic_family: "Lato".into(),
        font_weight: FontWeight::Bold,
        font_style: FontStyle::Italic,
        source_project_id: "missing".into(),
        source_revision: "fixed".into(),
        font_id: "font".into(),
    });
    assert!(app.classic_restore_analyze(&input).is_err());
    input.font_mappings.clear();
    let mut resources = ClassicResources {
        media: BTreeMap::new(),
        fonts: vec![],
    };
    // A fixed font record is reused without fresh cmap promises; missing text
    // glyphs remain a renderer capability refusal, never system fallback.
    assert!(
        classic_restore::analyze(&input, &resources)
            .unwrap()
            .analysis
            .blockers
            .is_empty()
    );
    resources.media.clear();
}

#[test]
fn cross_source_font_uses_exact_record_and_replay_precedes_resolution() {
    let mut app = EditorApplication::open(":memory:").unwrap();
    let mut source = request().source;
    source.target_project_id = "font-source".into();
    let resources = ClassicResources {
        media: BTreeMap::new(),
        fonts: vec![],
    };
    let mut document = classic_restore::analyze(&source, &resources)
        .unwrap()
        .document
        .unwrap();
    let font = FontResource {
        id: "face".into(),
        family: "Lato".into(),
        weight: 700,
        style: FontStyle::Italic,
        face_index: 0,
        content: opencut_contracts::AssetVersion {
            id: format!("av_{}", "a".repeat(64)),
            content_hash: "a".repeat(64),
            size: 3,
        },
        origin: ResourceOrigin::LocalImport {
            original_name: "licensed.ttf".into(),
        },
        license: Some(ResourceLicense {
            identifier: Some("OFL-1.1".into()),
            source_url: None,
            notice: Some("complete original notice".into()),
            attribution: None,
        }),
    };
    document.fonts.push(font.clone());
    app.connection.execute("INSERT INTO projects(project_id,name,head_revision) VALUES ('font-source','Fonts','fixed')",[]).unwrap();
    app.connection.execute("INSERT INTO revisions(revision,project_id,document_json) VALUES ('fixed','font-source',?1)",[encode_json(&document,"fixture").unwrap()]).unwrap();
    let mut input = request();
    input.source.font_mappings.push(ClassicFontMapping {
        classic_family: "Lato".into(),
        font_weight: FontWeight::Bold,
        font_style: FontStyle::Italic,
        source_project_id: "font-source".into(),
        source_revision: "fixed".into(),
        font_id: "face".into(),
    });
    let resolved = app.resolve_classic_resources(&input.source).unwrap();
    assert_eq!(resolved.fonts[0].font, font);
    let mut mismatch = input.source.clone();
    mismatch.font_mappings[0].font_weight = FontWeight::Normal;
    assert!(app.resolve_classic_resources(&mismatch).is_err());
    let mut duplicate = input.source.clone();
    duplicate
        .font_mappings
        .push(duplicate.font_mappings[0].clone());
    assert!(app.resolve_classic_resources(&duplicate).is_err());
    // CAS absence refuses publication despite the authorized source record.
    assert!(app.classic_restore_import(&input).is_err());
    assert_eq!(
        app.connection
            .query_row(
                "SELECT COUNT(*) FROM projects WHERE project_id='direct-v4'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    // Persisted exact replay remains readable after its source disappears.
    let receipt = ClassicRestoreReceipt {
        target_project_id: "direct-v4".into(),
        revision: "already-published".into(),
        source_content_id: input.expected_source_content_id.clone(),
        asset_count: 0,
        font_count: 1,
        unresolved_fonts: vec![],
        losses: vec![],
    };
    let identity = identity(&input).unwrap();
    app.connection.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json) VALUES (?1,'workspace',?2,?3,?4,?5)",params![FAMILY,input.idempotency_key,identity.current_digest(),RESULT,encode_json(&receipt,"fixture receipt").unwrap()]).unwrap();
    app.connection
        .execute("DELETE FROM revisions WHERE project_id='font-source'", [])
        .unwrap();
    assert_eq!(
        app.classic_restore_import(&input).unwrap().revision,
        "already-published"
    );
}
