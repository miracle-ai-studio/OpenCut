use super::*;

fn source() -> Value {
    json!({
        "version": 31,
        "metadata": {"id":"classic-source","name":"Original title"},
        "settings": {"fps":{"numerator":30,"denominator":1},
            "canvasSize":{"width":320,"height":180},
            "background":{"type":"color","color":"#000000"}},
        "scenes":[{"id":"scene","name":"Main","isMain":true,"bookmarks":[],
            "tracks":{"main":{"id":"main","name":"Main","type":"video",
                "elements":[],"muted":false,"hidden":false},
                "overlay":[{"id":"titles","name":"Titles","type":"text",
                    "hidden":false,"elements":[{"id":"title","name":"Title",
                        "type":"text","startTime":0,"duration":120000,
                        "trimStart":12000,"trimEnd":24000,"sourceDuration":156000,
                        "params":{"content":"OpenCut\n中文","fontFamily":"Lato",
                            "fontSize":12,"color":"#ffffff","textAlign":"center"}}]}],
                "audio":[]}}]
    })
}

#[test]
fn classic_scene_public_domain_import_preserves_independent_text_and_exact_replay() {
    let directory = tempfile::tempdir().unwrap();
    let mut automation = Automation::open(directory.path().join("workspace.sqlite")).unwrap();
    let input = json!({"content":source().to_string(),"target_project_id":"recovered",
        "media_mappings":[],"font_mappings":[]});
    let analysis = data(automation.call(
        "opencut_project",
        json!({
            "action":"classic.analyze","input":input
        }),
    ));
    assert_eq!(analysis["blockers"], json!([]));
    assert_eq!(
        analysis["unresolved_fonts"],
        json!([
            {"family":"Lato","font_weight":"normal","font_style":"normal"}
        ])
    );
    let project = &analysis["proposed_project"];
    assert!(project.get("schema_version").is_none());
    let scene = &project["sequences"][0];
    let text = scene["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|track| track["id"] == "titles")
        .unwrap()["elements"][0]
        .clone();
    assert_eq!(text["content"]["kind"], "text");
    assert_eq!(text["content"]["text"]["content"], "OpenCut\n中文");
    assert_eq!(
        text["content"]["text"]["font_size"],
        json!({"unit":"classic_height90","value":12.0})
    );
    assert_eq!(text["source_span"]["profile"], "classic_trimmed");
    assert!(scene["captions"].as_array().unwrap().is_empty());
    let accepted: Vec<_> = analysis["losses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|finding| finding["code"].clone())
        .collect();
    let request = json!({"source":input,"expected_source_content_id":analysis["source_content_id"],
        "idempotency_key":"direct-classic-import","accepted_loss_codes":accepted});
    let imported = data(automation.call("opencut_classic_import", request.clone()));
    assert_eq!(
        imported,
        data(automation.call(
            "opencut_project",
            json!({
                "action":"classic.import","input":request
            })
        ))
    );
    let saved = data(automation.call(
        "opencut_project_inspect",
        json!({
            "project_id":"recovered","revision":imported["revision"]
        }),
    ));
    assert_eq!(saved["kind"], "editor");
    let saved = saved["snapshot"].clone();
    assert_eq!(saved["project"], *project);
    let mut different = request;
    different["source"]["content"] = json!(source().to_string() + " ");
    error(
        automation.call("opencut_classic_import", different),
        "INVALID_ARGUMENT",
    );
}

#[test]
fn classic_scene_denies_restricted_callers_and_client_prepared_documents() {
    let directory = tempfile::tempdir().unwrap();
    let mut automation = Automation::open(directory.path().join("workspace.sqlite")).unwrap();
    create(&mut automation);
    policy(
        &mut automation,
        "0",
        vec![ProjectAction::Read, ProjectAction::Edit],
    );
    let issued = automation.app.issue_credential("agent").unwrap();
    automation.caller = automation.app.authenticate(&issued.secret).unwrap();
    // Authorization must precede parsing and even the first resource lookup.
    for operation in ["opencut_classic_analyze", "opencut_classic_import"] {
        error(automation.call(operation, json!({})), "NOT_AUTHORIZED");
    }
    automation.caller = Caller::LocalOwner;
    error(
        automation.call(
            "opencut_classic_analyze",
            json!({
                "content":source().to_string(),"target_project_id":"recovered",
                "media_mappings":[],"font_mappings":[],"proposed_project":{}
            }),
        ),
        "INVALID_ARGUMENT",
    );
}
