use super::*;
impl opencut_editor_application::plugin_v3::PresetV3ProjectRead for EditorApplication {
    fn preset_v3_snapshot(
        &self,
        project: &str,
        revision: &str,
    ) -> Result<opencut_contracts::project_v3::ProjectSnapshotV3, EditorError> {
        self.inspect_v3_project(&opencut_contracts::project_v3_ops::InspectV3Project {
            project_id: project.into(),
            revision: Some(revision.into()),
        })
    }
}
