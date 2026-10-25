//! Owner-local V4 exchange uses existing CAS fencing and atomic first-revision
//! publication. Retained font proofs commit with the project, never ahead of it.
use super::*;
use opencut_api_contracts::historical_snapshot_package::{
    HistoricalSnapshotImport, HistoricalSnapshotImportReceipt,
};
use opencut_contracts::scene::{EditorSnapshot, ResourceOrigin};
use opencut_editor_application::{
    historical_snapshot_package::{
        self as package, HistoricalSnapshotSource, PreparedHistoricalSnapshot,
    },
    scene::StoredProjectDocument,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
const FAMILY: &str = "import_package_v4";
const RESULT: &str = "package_import_v4";
fn invalid(message: &str) -> EditorError {
    EditorError::new("PACKAGE_INVALID", message)
}
fn identity(input: &HistoricalSnapshotImport) -> Result<RequestIdentity, EditorError> {
    require_idempotency_key(&input.idempotency_key)?;
    if input.target_project_id.trim().is_empty()
        || input.target_project_id.len() > 128
        || input.target_project_id.chars().any(char::is_control)
        || input.expected_manifest_sha256.len() != 64
        || !input
            .expected_manifest_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid(
            "Invalid V4 package target or expected manifest digest",
        ));
    }
    // Relocating the exact same package must not change logical replay identity.
    request_hash(
        FAMILY,
        "workspace",
        &json!({"target_project_id":input.target_project_id,"manifest_sha256":input.expected_manifest_sha256}),
    )
}
fn replay(
    connection: &Connection,
    input: &HistoricalSnapshotImport,
    identity: &RequestIdentity,
) -> Result<Option<HistoricalSnapshotImportReceipt>, EditorError> {
    load_idempotent(
        connection,
        FAMILY,
        "workspace",
        &input.idempotency_key,
        identity,
        RESULT,
    )?
    .map(|raw| decode_json(&raw, "V4 package import receipt"))
    .transpose()
}
fn no_plugin_grants(connection: &Connection, project: &str) -> Result<(), EditorError> {
    let has: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM project_plugin_grants WHERE project_id=?1) OR EXISTS(SELECT 1 FROM project_preset_grants WHERE project_id=?1)",
        [project], |row| row.get(0),
    ).map_err(storage_error)?;
    if has {
        return Err(EditorError::new(
            "PACKAGE_DEPENDENCY_UNSUPPORTED",
            "This V4 package scope cannot omit existing project plugin grants",
        ));
    }
    Ok(())
}
impl EditorApplication {
    #[cfg(test)]
    pub fn historical_snapshot_source(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<HistoricalSnapshotSource, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        no_plugin_grants(&tx, project_id)?;
        let selected = sqlite_scene::load_source(&tx, project_id, Some(revision))?;
        let StoredProjectDocument::Editable(document) = selected.document else {
            return Err(EditorError::new(
                "UNSUPPORTED_PROJECT_VERSION",
                "V4 package requires a fixed V4 source revision",
            ));
        };
        let mut derived = BTreeMap::new();
        for font in &document.fonts {
            if let ResourceOrigin::Generated {
                provider,
                generation_id,
            } = &font.origin
                && opencut_editor_application::font_derived::provenance::is_provider(provider)
            {
                let proof = sqlite_font_derivations::read_source_for_font(&tx, font)?;
                derived.insert(generation_id.clone(), proof);
            }
        }
        let source = HistoricalSnapshotSource {
            snapshot: EditorSnapshot {
                project: *document,
                revision: selected.revision,
            },
            derived_fonts: derived.into_values().collect(),
        };
        package::validate_export_source(&source)?;
        tx.commit().map_err(storage_error)?;
        Ok(source)
    }
    pub fn historical_snapshot_replay(
        &self,
        input: &HistoricalSnapshotImport,
    ) -> Result<Option<HistoricalSnapshotImportReceipt>, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        replay(&self.connection, input, &identity(input)?)
    }
    #[cfg(test)]
    pub fn historical_snapshot_fixture_commit(
        &mut self,
        input: &HistoricalSnapshotImport,
        prepared: &PreparedHistoricalSnapshot,
    ) -> Result<HistoricalSnapshotImportReceipt, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        let identity = identity(input)?;
        if let Some(receipt) = replay(&self.connection, input, &identity)? {
            return Ok(receipt);
        }
        if input.expected_manifest_sha256 != prepared.manifest_sha256 {
            return Err(invalid(
                "Prepared V4 manifest differs from the original request",
            ));
        }
        // This recomputes both canonical manifest and document hashes; private
        // prepared structs are typed data, not a substitute for verification.
        let document = package::prepare_import(prepared, &input.target_project_id)?;
        let revision = format!(
            "rev_{:x}",
            Sha256::digest(encode_json(&document, "Restored V4 package")?.as_bytes())
        );
        // Recheck all captured CAS bytes and retain the shared collection gate
        // through publication, including every font and unused media resource.
        let _cas =
            sqlite_scene::resources::verify_document(self.asset_store_root(), &document, None)?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_access::owner(&context)?;
        if let Some(receipt) = replay(&tx, input, &identity)? {
            return Ok(receipt);
        }
        for source in &prepared.manifest.derived_fonts {
            let font = document.fonts.iter().find(|font| matches!(&font.origin,
                ResourceOrigin::Generated { provider, generation_id }
                if opencut_editor_application::font_derived::provenance::is_provider(provider) && generation_id == &source.face.derivation_id
            )).ok_or_else(|| invalid("Derived proof has no restored font"))?;
            sqlite_font_derivations::persist_verified_source(&tx, font, source)?;
        }
        let receipt = HistoricalSnapshotImportReceipt {
            target_project_id: document.id.clone(),
            revision: revision.clone(),
            manifest_sha256: prepared.manifest_sha256.clone(),
            source_project_id: prepared.manifest.source_project_id.clone(),
            source_revision: prepared.manifest.source_revision.clone(),
            asset_count: document.assets.len(),
            font_count: document.fonts.len(),
            derived_font_count: prepared.manifest.derived_fonts.len(),
            self_contained_resources: true,
            history_included: false,
            fully_reproducible: false,
        };
        let receipt_json = encode_json(&receipt, "V4 package receipt")?;
        let payload = json!({"project_id":document.id,"revision":revision,"source_project_id":prepared.manifest.source_project_id,"source_revision":prepared.manifest.source_revision,"manifest_sha256":prepared.manifest_sha256});
        sqlite_new_project::publish(
            &tx,
            &context,
            sqlite_new_project::NewProjectPublication {
                document: &document,
                revision: &revision,
                operation_family: FAMILY,
                idempotency_key: &input.idempotency_key,
                request_digest: identity.current_digest(),
                result_kind: RESULT,
                result_json: &receipt_json,
                event_type: "project.package_v4_imported",
                event_payload: &payload,
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(receipt)
    }
}
#[cfg(test)]
#[path = "sqlite_historical_snapshot_package/tests.rs"]
mod tests;
