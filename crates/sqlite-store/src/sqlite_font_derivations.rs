//! Immutable derivation records are committed with FontPut and retained across history/GC.
use super::*;
use opencut_contracts::{font_derived::*, scene::*, scene_ops::*};
use opencut_editor_application::{
    font_derived::proof::*,
    scene::{SceneCommitIntent, StoredProjectDocument},
};
pub(super) const MIGRATION: &str = "
CREATE TABLE font_derivations_v1(
 derivation_id TEXT PRIMARY KEY,
 source_json TEXT NOT NULL,
 proof_json TEXT NOT NULL,
 output_sha256 TEXT NOT NULL,
 output_size TEXT NOT NULL
);
CREATE TRIGGER font_derivations_v1_immutable_update BEFORE UPDATE ON font_derivations_v1
BEGIN SELECT RAISE(ABORT,'immutable font derivation'); END;
CREATE TRIGGER font_derivations_v1_immutable_delete BEFORE DELETE ON font_derivations_v1
BEGIN SELECT RAISE(ABORT,'retained font derivation'); END;
UPDATE metadata SET value='30' WHERE key='schema_version' AND value='29';
";
fn corrupt() -> EditorError {
    EditorError::new(
        "STORAGE_ERROR",
        "Retained derived font proof differs from fixed font identity",
    )
}
pub(super) fn require_derived_proposal(
    connection: &Connection,
    intent: SceneCommitIntent<'_>,
    snapshot: &EditorSnapshot,
) -> Result<(), EditorError> {
    let SceneCommitIntent::DerivedFontImport(request) = intent else {
        return Ok(());
    };
    let font =
        opencut_editor_application::font_derived::provenance::expected_import_resource(request)?;
    let stored = sqlite_scene::load_source(
        connection,
        &request.project_id,
        Some(&request.expected_revision),
    )?;
    let StoredProjectDocument::Editable(project) = stored.document else {
        return Err(corrupt());
    };
    let expected = opencut_editor_application::scene::plan(
        &EditorSnapshot {
            project: *project,
            revision: request.expected_revision.clone(),
        },
        &ApplySceneProject {
            project_id: request.project_id.clone(),
            expected_revision: request.expected_revision.clone(),
            idempotency_key: request.idempotency_key.clone(),
            label: format!("Import {}", font.family),
            operations: vec![ProjectOperation::FontPut { font }],
        },
    )?;
    if expected != snapshot.project {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Derived import proposal differs from original fixed FontPut",
        ));
    }
    Ok(())
}
pub(super) fn persist_proof(
    connection: &Connection,
    intent: SceneCommitIntent<'_>,
    snapshot: &EditorSnapshot,
) -> Result<(), EditorError> {
    let SceneCommitIntent::DerivedFontImport(request) = intent else {
        return Ok(());
    };
    let source =
        opencut_editor_application::font_derived::provenance::source(&request.catalog_id)?.source;
    let font = snapshot
        .project
        .fonts
        .iter()
        .find(|f| f.id == request.font_id)
        .ok_or_else(corrupt)?;
    persist_verified_source(connection, font, &source)
}
/// Shared package/import publication primitive. Caller holds the same transaction
/// as the referencing revision; an existing immutable ID must match every byte.
pub(super) fn persist_verified_source(
    connection: &Connection,
    font: &FontResource,
    source: &FontDerivedSource,
) -> Result<(), EditorError> {
    opencut_editor_application::font_derived::provenance::validate_resource_source(font, source)?;
    let raw = serde_json::to_string(source).map_err(|_| corrupt())?;
    let existing:Option<(String,String,String,String)>=connection.query_row("SELECT source_json,proof_json,output_sha256,output_size FROM font_derivations WHERE derivation_id=?1",[&source.face.derivation_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(storage_error)?;
    let expected = (
        raw,
        source.proof_json.clone(),
        source.face.content_hash.clone(),
        source.face.size_bytes.clone(),
    );
    if let Some(existing) = existing {
        if existing != expected {
            return Err(corrupt());
        }
    } else {
        connection.execute("INSERT INTO font_derivations(derivation_id,source_json,proof_json,output_sha256,output_size) VALUES(?1,?2,?3,?4,?5)",params![source.face.derivation_id,expected.0,expected.1,expected.2,expected.3]).map_err(storage_error)?;
    }
    Ok(())
}
/// Read a retained proof from the caller's fixed-revision transaction.
pub(super) fn read_source_for_font(
    connection: &Connection,
    font: &FontResource,
) -> Result<FontDerivedSource, EditorError> {
    let ResourceOrigin::Generated {
        provider,
        generation_id,
    } = &font.origin
    else {
        return Err(EditorError::new("NOT_FOUND", "Font has no derived proof"));
    };
    if !opencut_editor_application::font_derived::provenance::is_provider(provider) {
        return Err(EditorError::new(
            "NOT_FOUND",
            "Font has no supported derived proof",
        ));
    }
    let row:Option<(String,String,String,String)>=connection.query_row("SELECT source_json,proof_json,output_sha256,output_size FROM font_derivations WHERE derivation_id=?1",[generation_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(storage_error)?;
    let (raw, proof, hash, size) = row.ok_or_else(corrupt)?;
    let source: FontDerivedSource = decode_json(&raw, "Retained derived font source")?;
    if source.face.derivation_id != *generation_id
        || source.proof_json != proof
        || source.face.content_hash != hash
        || source.face.size_bytes != size
    {
        return Err(corrupt());
    }
    opencut_editor_application::font_derived::provenance::validate_resource_source(font, &source)
        .map_err(|_| corrupt())?;
    Ok(source)
}
impl DerivedFontProofRead for EditorApplication {
    fn read_derived_font_proof(
        &self,
        request: &ReadDerivedFontProof,
    ) -> Result<DerivedFontProof, EditorError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_access::require(
            &tx,
            &self.connection.1,
            &request.project_id,
            ProjectAction::Read,
        )?;
        let stored = sqlite_scene::load_source(&tx, &request.project_id, Some(&request.revision))?;
        let StoredProjectDocument::Editable(project) = stored.document else {
            return Err(EditorError::new(
                "UNSUPPORTED_PROJECT_VERSION",
                "Derived font proof requires a V4 revision",
            ));
        };
        let font = project
            .fonts
            .iter()
            .find(|f| f.id == request.font_id)
            .ok_or_else(|| {
                EditorError::new("NOT_FOUND", "Font is absent from this fixed revision")
            })?;
        let source = read_source_for_font(&tx, font)?;
        let result = DerivedFontProof {
            project_id: request.project_id.clone(),
            revision: request.revision.clone(),
            font_id: request.font_id.clone(),
            source,
        };
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
}
// Test modules share this name through `use super::*`; production code no longer needs it.
#[cfg(test)]
use opencut_editor_application::font_derived as derived;
#[cfg(test)]
#[path = "sqlite_font_named_tests.rs"]
mod renamed_tests;
#[cfg(test)]
#[path = "sqlite_font_synthesis_tests.rs"]
mod synthesis_tests;
#[cfg(test)]
#[path = "sqlite_font_derivations_tests.rs"]
mod tests;
