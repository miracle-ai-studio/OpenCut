//! Immutable derivation records are committed with FontPut and retained across history/GC.
use super::*;
use opencut_contracts::{font_derived_v1::*, project_v4::*, project_v4_ops::*};
use opencut_editor_application::{
    font_derived_v1::{self as derived, proof::*},
    project_v4::{StoredProjectDocumentV4, V4Intent},
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
    intent: V4Intent<'_>,
    snapshot: &ProjectSnapshotV4,
) -> Result<(), EditorError> {
    let V4Intent::DerivedFontImport(request) = intent else {
        return Ok(());
    };
    let font = derived::expected_import_resource_v1(request)?;
    let stored = sqlite_project_v4::load_source(
        connection,
        &request.project_id,
        Some(&request.expected_revision),
    )?;
    let StoredProjectDocumentV4::V4(project) = stored.document else {
        return Err(corrupt());
    };
    let expected = opencut_editor_application::project_v4::plan(
        &ProjectSnapshotV4 {
            project: *project,
            revision: request.expected_revision.clone(),
        },
        &ApplyV4Project {
            project_id: request.project_id.clone(),
            expected_revision: request.expected_revision.clone(),
            idempotency_key: request.idempotency_key.clone(),
            label: format!("Import {}", font.family),
            operations: vec![ProjectOperationV4::FontPut { font }],
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
    intent: V4Intent<'_>,
    snapshot: &ProjectSnapshotV4,
) -> Result<(), EditorError> {
    let V4Intent::DerivedFontImport(request) = intent else {
        return Ok(());
    };
    let source = derived::source(&request.catalog_id)?.source;
    let font = snapshot
        .project
        .fonts
        .iter()
        .find(|f| f.id == request.font_id)
        .ok_or_else(corrupt)?;
    validate_resource_source(font, &source)?;
    let raw = serde_json::to_string(&source).map_err(|_| corrupt())?;
    let existing:Option<(String,String,String,String)>=connection.query_row("SELECT source_json,proof_json,output_sha256,output_size FROM font_derivations_v1 WHERE derivation_id=?1",[&source.face.derivation_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(storage_error)?;
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
        connection.execute("INSERT INTO font_derivations_v1(derivation_id,source_json,proof_json,output_sha256,output_size) VALUES(?1,?2,?3,?4,?5)",params![source.face.derivation_id,expected.0,expected.1,expected.2,expected.3]).map_err(storage_error)?;
    }
    Ok(())
}
impl DerivedFontProofReadV1 for EditorApplication {
    fn read_derived_font_proof_v1(
        &self,
        request: &ReadDerivedFontProofV1,
    ) -> Result<DerivedFontProofV1, EditorError> {
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
        let stored =
            sqlite_project_v4::load_source(&tx, &request.project_id, Some(&request.revision))?;
        let StoredProjectDocumentV4::V4(project) = stored.document else {
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
        let ResourceOriginV4::Generated {
            provider,
            generation_id,
        } = &font.origin
        else {
            return Err(EditorError::new("NOT_FOUND", "Font has no derived proof"));
        };
        if provider != derived::DERIVED_FONT_PROVIDER_V1 {
            return Err(EditorError::new(
                "NOT_FOUND",
                "Font has no supported derived proof",
            ));
        }
        let row:Option<(String,String,String,String)>=tx.query_row("SELECT source_json,proof_json,output_sha256,output_size FROM font_derivations_v1 WHERE derivation_id=?1",[generation_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(storage_error)?;
        let (raw, proof, hash, size) = row.ok_or_else(corrupt)?;
        let source: FontDerivedSourceV1 = decode_json(&raw, "Retained derived font source")?;
        if source.face.derivation_id != *generation_id
            || source.proof_json != proof
            || source.face.content_hash != hash
            || source.face.size_bytes != size
        {
            return Err(corrupt());
        }
        validate_resource_source(font, &source).map_err(|_| corrupt())?;
        let result = DerivedFontProofV1 {
            schema_version: 1,
            project_id: request.project_id.clone(),
            revision: request.revision.clone(),
            font_id: request.font_id.clone(),
            source,
        };
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
}
#[cfg(test)]
#[path = "sqlite_font_derived_v1_tests.rs"]
mod tests;
