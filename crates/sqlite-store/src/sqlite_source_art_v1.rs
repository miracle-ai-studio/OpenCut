//! Source-art selection uses the same fixed-revision execution authorization
//! as audio jobs. A cache receipt never replaces this live permission check.
use super::sqlite_project_v4_jobs::{authority, context, fixed, require_scope};
use super::*;
use opencut_editor_application::source_art_v1::{
    AuthorizedSourceArtV1, SourceArtPolicyV1, SourceArtReadV1,
};
use opencut_job_execution::project_v4_audio::TrustedJobAuthorityV1;

impl SourceArtReadV1 for EditorApplication {
    fn fixed_source_art_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<AuthorizedSourceArtV1, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        require_scope(&tx, &self.connection.1, project_id)?;
        let selected = authority(&self.connection.1);
        let scope = serde_json::to_string(&(project_id, &selected)).map_err(|_| {
            EditorError::new(
                "SERIALIZATION_ERROR",
                "Could not capture source-art authority",
            )
        })?;
        Ok(AuthorizedSourceArtV1 {
            snapshot: fixed(&tx, project_id, revision)?,
            authority: selected,
            scope,
            policy: SourceArtPolicyV1 {
                max_encoded_bytes: 32 * 1024 * 1024,
                max_pcm_bytes: 256 * 1024 * 1024,
                max_rss_bytes: 2 * 1024 * 1024 * 1024,
                deadline_ms: 60_000,
            },
        })
    }

    fn revalidate_source_art(
        &self,
        project_id: &str,
        revision: &str,
        selected: &TrustedJobAuthorityV1,
    ) -> Result<(), EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        require_scope(&tx, &context(selected), project_id)?;
        let present: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM revisions WHERE project_id=?1 AND revision=?2)",
                params![project_id, revision],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if !present {
            return Err(EditorError::new(
                "NOT_FOUND",
                "Fixed source-art revision is unavailable",
            ));
        }
        Ok(())
    }
}
