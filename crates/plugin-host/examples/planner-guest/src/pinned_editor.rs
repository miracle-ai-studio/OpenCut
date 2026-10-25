//! Only the published dependency's function signature and metadata shim differ.
wit_bindgen::generate!({path:"../../wit",world:"planner-v4-v1"});
struct PinnedEditorPlanner;
impl Guest for PinnedEditorPlanner {
    fn plan_scene_v1(snapshot_json: String, parameters_json: String) -> String {
        let snapshot: serde_json::Value = serde_json::from_str(&snapshot_json).unwrap();
        assert_eq!(snapshot["project"]["schema_version"], 4);
        crate::proposal::plan(snapshot_json, parameters_json)
    }
}
export!(PinnedEditorPlanner);
