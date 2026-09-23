wit_bindgen::generate!({ path: "../../wit", world: "planner" });

struct ExamplePlanner;

impl Guest for ExamplePlanner {
    fn plan(snapshot_json: String, parameters_json: String) -> String {
        if parameters_json.contains("\"loop\":true") {
            loop { std::hint::black_box(()); }
        }
        let snapshot: serde_json::Value = serde_json::from_str(&snapshot_json).unwrap();
        let parameters: serde_json::Value = serde_json::from_str(&parameters_json).unwrap();
        // Fault modes used by the public Host conformance journey.
        if parameters.get("fault").and_then(|v| v.as_str()) == Some("invalid_json") {
            return "this is not JSON".into();
        }
        if parameters.get("fault").and_then(|v| v.as_str()) == Some("invalid_operation") {
            return r#"[{"type":"not.an.operation"}]"#.into();
        }
        let sequence_id = if parameters.get("fault").and_then(|v| v.as_str()) == Some("missing_sequence") {
            "missing-sequence"
        } else {
            snapshot["project"]["default_sequence_id"].as_str().unwrap()
        };
        let track_id = parameters.get("track_id").and_then(|value| value.as_str()).unwrap_or("plugin-track");
        serde_json::json!([{
            "type": "track.create",
            "sequence_id": sequence_id,
            "track": {
                "id": track_id,
                "name": "Plugin Track",
                "kind": "video",
                "enabled": true,
                "muted": false,
                "locked": false,
                "clips": [],
                "params": {}
            }
        }]).to_string()
    }
}

export!(ExamplePlanner);
