use super::*;
impl opencut_editor_application::render_v3::ProjectExecutionReadV3 for EditorApplication {
    fn execution_snapshot_v3(
        &self,
        project: &str,
        revision: &str,
    ) -> Result<opencut_editor_application::project_v3::AuthorizedSnapshotV3, EditorError> {
        EditorApplication::execution_snapshot_v3(self, project, revision)
    }
}
