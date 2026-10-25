//! Resolve public resource selectors from scoped immutable authority. A hash is
//! never a grant; bindings are checked before preparation and under SQL lock.
use super::*;
use opencut_contracts::AssetVersion;
use opencut_contracts::project_public::*;
use opencut_editor_application::scene::public::{self, ProjectPublicResources};
#[cfg(test)]
mod tests;

fn media_observation(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
    media_id: &str,
    version: &AssetVersion,
) -> Result<opencut_contracts::Asset, EditorError> {
    sqlite_project_library::require_live(connection, project)?;
    sqlite_access::require(connection, context, project, ProjectAction::Read)?;
    if media_id.is_empty() || media_id.len() > 1024 {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Media selector identity is invalid",
        ));
    }
    let raw: Option<String> = connection.query_row(
        "SELECT m.asset_json FROM available_project_media a JOIN project_media m USING(media_id)
         WHERE a.project_id=?1 AND a.media_id=?2",
        params![project, media_id], |row| row.get(0),
    ).optional().map_err(storage_error)?;
    let raw = raw.ok_or_else(access::denied)?;
    if raw.len() > opencut_editor_application::gc::MAX_REFERENCE_DOCUMENT_BYTES {
        return Err(EditorError::new(
            "RESOURCE_TOO_LARGE",
            "Media observation exceeds the reference budget",
        ));
    }
    let asset: opencut_contracts::Asset = decode_json(&raw, "Scoped media observation")?;
    if resources::known_version(&asset).as_ref() != Some(version) {
        return Err(access::denied());
    }
    Ok(asset)
}

pub(super) fn media(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    project: &str,
    media_id: &str,
    version: &AssetVersion,
) -> Result<ProjectAsset, EditorError> {
    let asset = media_observation(connection, context, project, media_id, version)?;
    resources::canonical_asset(&asset, version).ok_or_else(|| {
        EditorError::new(
            "CONVERSION_REJECTED",
            "Scoped media observation cannot be represented by V4",
        )
    })
}

pub(super) fn authorize_bindings(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    input: &ApplyProject,
) -> Result<(), EditorError> {
    for operation in &input.operations {
        match operation {
            ProjectEditOperation::AssetBind {
                source: AssetSelector::ProjectMedia { media_id, version },
                ..
            } => {
                media(connection, context, &input.project_id, media_id, version)?;
            }
            ProjectEditOperation::AssetBind {
                source: AssetSelector::ProjectAsset { revision, asset_id },
                ..
            } => {
                let source = load_source(connection, &input.project_id, Some(revision))?;
                let StoredProjectDocument::Editable(project) = source.document else {
                    return Err(EditorError::new(
                        "UNSUPPORTED_PROJECT_VERSION",
                        "Resource selector requires a V4 revision",
                    ));
                };
                if !project.assets.iter().any(|a| &a.id == asset_id) {
                    return Err(access::denied());
                }
            }
            ProjectEditOperation::FontBind {
                source: FontSelector::ProjectFont { revision, font_id },
                ..
            } => {
                let source = load_source(connection, &input.project_id, Some(revision))?;
                let StoredProjectDocument::Editable(project) = source.document else {
                    return Err(EditorError::new(
                        "UNSUPPORTED_PROJECT_VERSION",
                        "Font selector requires a V4 revision",
                    ));
                };
                if !project.fonts.iter().any(|f| &f.id == font_id) {
                    return Err(access::denied());
                }
            }
            _ => {}
        }
    }
    Ok(())
}

impl ProjectPublicResources for EditorApplication {
    fn authorized_project_media_packet_rate_scene(
        &self,
        project: &str,
        media_id: &str,
        version: &AssetVersion,
    ) -> Result<Option<opencut_media_contracts::ClassicPacketRate>, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let asset = media_observation(&tx, &self.connection.1, project, media_id, version)?;
        let observation = resources::packet_rate(&asset)?;
        tx.commit().map_err(storage_error)?;
        Ok(observation)
    }

    fn project_asset_packet_rate_scene(
        &self,
        project: &str,
        revision: &str,
        asset_id: &str,
    ) -> Result<Option<opencut_media_contracts::ClassicPacketRate>, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_access::require(&tx, &self.connection.1, project, ProjectAction::Read)?;
        let source = load_source(&tx, project, Some(revision))?;
        let StoredProjectDocument::Editable(document) = source.document else {
            return Err(EditorError::new(
                "UNSUPPORTED_PROJECT_VERSION",
                "Packet observation requires a V4 revision",
            ));
        };
        let asset = document
            .assets
            .iter()
            .find(|asset| asset.id == asset_id)
            .ok_or_else(access::denied)?;
        let observation = if let ResourceOrigin::LegacyLocator { uri } = &asset.origin
            && let Some(media_id) = uri.strip_prefix("opencut-media://")
        {
            let native =
                media_observation(&tx, &self.connection.1, project, media_id, &asset.version)?;
            resources::packet_rate(&native)?
        } else {
            None
        };
        tx.commit().map_err(storage_error)?;
        Ok(observation)
    }

    fn authorized_project_media_scene(
        &self,
        project: &str,
        media_id: &str,
        version: &AssetVersion,
    ) -> Result<ProjectAsset, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let asset = media(&tx, &self.connection.1, project, media_id, version)?;
        tx.commit().map_err(storage_error)?;
        Ok(asset)
    }
}

impl EditorApplication {
    pub fn apply_editor_edit(
        &mut self,
        input: &ApplyProject,
    ) -> Result<TransactionResult, EditorError> {
        public::apply_public(self, input)
    }
    pub fn plan_editor_edit(
        &self,
        input: &ApplyProject,
    ) -> Result<PublicProjectDocument, EditorError> {
        public::plan_public(self, input)
    }
}
