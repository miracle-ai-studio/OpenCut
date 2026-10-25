//! The one public snapshot and operation contract,shared by dependency fixtures.
pub fn plan(snapshot_json: String, parameters_json: String) -> String {
    let snapshot: serde_json::Value = serde_json::from_str(&snapshot_json).unwrap();
    let p: serde_json::Value = serde_json::from_str(&parameters_json).unwrap();
    if p["loop"] == true {
        loop {
            std::hint::black_box(());
        }
    }
    match p["fault"].as_str() {
            Some("loop") => loop { std::hint::black_box(()); },
            Some("allocate") => { std::hint::black_box(vec![17u8; 128 * 1024 * 1024]); }
            Some("invalid_json") => return "invalid json".into(),
            Some("invalid_operation") => return r#"[{"type":"not.an.operation"}]"#.into(),
            Some("legacy_operation") => return r#"[{"type":"track.create","sequence_id":"main","track":{}}]"#.into(),
            Some("private_resource") => return r#"[{"type":"asset_put","asset":{"origin":{"kind":"legacy_locator","uri":"/private/not-a-capability"}}}]"#.into(),
            Some("empty") => return "[]".into(),
            Some("too_many") => return serde_json::Value::Array((0..33).map(|_| serde_json::json!({"type":"rename","name":"too many"})).collect()).to_string(),
            _ => {}
        }
    // The typed public snapshot must not contain private resource origins.
    for resource in snapshot["project"]["assets"]
        .as_array()
        .unwrap()
        .iter()
        .chain(snapshot["project"]["fonts"].as_array().unwrap())
    {
        assert!(resource["origin"].get("uri").is_none());
        assert!(resource["origin"].get("path").is_none());
    }
    if p["fault"] == "invalid_sequence" || p["fault"] == "missing_sequence" {
        return r#"[{"type":"track_delete","sequence_id":"missing","track_id":"missing"}]"#.into();
    }
    if let Some(track_id) = p["track_id"].as_str() {
        return serde_json::json!([{"type":"track_put","sequence_id":snapshot["project"]["default_sequence_id"],"track":{
                "id":track_id,"name":"Plugin Track","kind":"video","enabled":true,"muted":false,"locked":false,
                "opacity":1.0,"gain":{"unit":"linear_legacy","value":1.0},"elements":[],"channels":[]
            }}]).to_string();
    }
    serde_json::json!([{"type":"rename","name":p["name"].as_str().unwrap_or("Plugin planned edit")}]).to_string()
}
