wit_bindgen::generate!({path:"../../wit",world:"planner"});
struct ExamplePlanner;
impl Guest for ExamplePlanner {
    fn plan(snapshot_json: String, parameters_json: String) -> String {
        let snapshot: serde_json::Value = serde_json::from_str(&snapshot_json).unwrap();
        assert!(snapshot["project"].get("schema_version").is_none());
        crate::proposal::plan(snapshot_json, parameters_json)
    }
}
export!(ExamplePlanner);
