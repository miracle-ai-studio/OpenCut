//! Owner-local Classic import with exact replay and one atomic first revision.
use super::*;
use opencut_api_contracts::classic_import::{ClassicAnalysis, ClassicImport, ClassicImportReceipt};
use opencut_editor_application::classic_import;
use std::collections::BTreeSet;

fn identity(input: &ClassicImport) -> Result<RequestIdentity, EditorError> {
    let source_id = classic_import::source_content_id(&input.source.content);
    if source_id != input.expected_source_content_id {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Classic source content ID mismatch",
        ));
    }
    let mut mappings = input.source.media_mappings.clone();
    mappings.sort_by(|a, b| a.classic_media_id.cmp(&b.classic_media_id));
    let value = json!({"source_content_id":source_id,
        "target_project_id":input.source.target_project_id,
        "media_mappings":mappings,"accepted_loss_codes":input.accepted_loss_codes});
    request_hash("import_classic", "workspace", &value)
}

impl EditorApplication {
    /// Check exact retry before resolving source media references.
    pub fn classic_import_replay(
        &self,
        input: &ClassicImport,
    ) -> Result<Option<ClassicImportReceipt>, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        require_idempotency_key(&input.idempotency_key)?;
        let identity = identity(input)?;
        load_idempotent(
            &self.connection,
            "import_classic",
            "workspace",
            &input.idempotency_key,
            &identity,
            "classic_import",
        )?
        .map(|stored| decode_json(&stored, "Classic import receipt"))
        .transpose()
    }

    /// `analysis` must have been generated from host-verified fixed media mappings.
    pub fn classic_import_commit(
        &mut self,
        input: &ClassicImport,
        analysis: &ClassicAnalysis,
    ) -> Result<ClassicImportReceipt, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        require_idempotency_key(&input.idempotency_key)?;
        let identity = identity(input)?;
        if analysis.source_content_id != input.expected_source_content_id
            || analysis.target_project_id != input.source.target_project_id
            || analysis.source_version != Some(31)
        {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Classic analysis does not match request",
            ));
        }
        if !analysis.blockers.is_empty() || analysis.proposed_project.is_none() {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Classic project has unresolved compatibility blockers",
            )
            .with_details(json!({"blockers":analysis.blockers})));
        }
        let losses: BTreeSet<_> = analysis
            .losses
            .iter()
            .map(|finding| finding.code.as_str())
            .collect();
        let accepted: BTreeSet<_> = input
            .accepted_loss_codes
            .iter()
            .map(String::as_str)
            .collect();
        if losses != accepted || accepted.len() != input.accepted_loss_codes.len() {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Classic migration loss codes need exact acknowledgement",
            )
            .with_details(json!({"required_loss_codes":losses})));
        }
        let document = analysis.proposed_project.as_ref().unwrap();
        if document.id != input.source.target_project_id {
            return Err(EditorError::new(
                "INVALID_ARGUMENT",
                "Classic target identity differs",
            ));
        }
        opencut_editor_core::validate_project(document)?;
        let revision = classic_import::document_revision(document)?;
        let receipt = ClassicImportReceipt {
            target_project_id: document.id.clone(),
            revision: revision.clone(),
            source_content_id: analysis.source_content_id.clone(),
            asset_count: document.assets.len(),
            losses: analysis.losses.clone(),
        };
        let receipt_json = encode_json(&receipt, "Classic import receipt")?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_access::owner(&context)?;
        if let Some(stored) = load_idempotent(
            &tx,
            "import_classic",
            "workspace",
            &input.idempotency_key,
            &identity,
            "classic_import",
        )? {
            return decode_json(&stored, "Classic import receipt");
        }
        let payload = json!({"project_id":document.id,"revision":revision,
            "source_format":"classic-indexeddb-serialized-project","source_version":31,
            "source_content_id":analysis.source_content_id,"losses":analysis.losses});
        sqlite_new_project::publish(
            &tx,
            &context,
            sqlite_new_project::NewProjectPublication {
                document,
                revision: &revision,
                operation_family: "import_classic",
                idempotency_key: &input.idempotency_key,
                request_digest: identity.current_digest(),
                result_kind: "classic_import",
                result_json: &receipt_json,
                event_type: "project.classic_imported",
                event_payload: &payload,
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_api_contracts::classic_import::ClassicAnalyze;
    use std::collections::BTreeMap;

    #[test]
    fn classic_import_audit_failure_rolls_back_project_and_replay() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = EditorApplication::open(directory.path().join("workspace.sqlite")).unwrap();
        let source = ClassicAnalyze {
            target_project_id: "imported".into(),
            media_mappings: vec![],
            content: json!({"version":31,"metadata":{"id":"classic","name":"Classic"},
                "settings":{"fps":{"numerator":24,"denominator":1},
                    "canvasSize":{"width":64,"height":48},
                    "background":{"type":"color","color":"#000000"}},
                "scenes":[{"id":"main","name":"Main","isMain":true,"bookmarks":[],
                    "tracks":{"main":{"id":"video","name":"Video","type":"video",
                        "elements":[]},"overlay":[],"audio":[]}}]})
            .to_string(),
        };
        let analysis = classic_import::analyze(&source, &BTreeMap::new());
        assert!(analysis.blockers.is_empty(), "{:?}", analysis.blockers);
        let request = ClassicImport {
            expected_source_content_id: analysis.source_content_id.clone(),
            idempotency_key: "classic-once".into(),
            accepted_loss_codes: vec!["BACKGROUND_DEFAULT".into()],
            source,
        };
        app.connection
            .execute_batch(
                "CREATE TRIGGER fail_classic_audit BEFORE INSERT ON audit_records
            WHEN NEW.action='project.classic_imported' BEGIN SELECT RAISE(ABORT,'forced'); END;",
            )
            .unwrap();
        assert!(app.classic_import_commit(&request, &analysis).is_err());
        assert_eq!(app.snapshot("imported").unwrap_err().code, "NOT_FOUND");
        assert!(app.classic_import_replay(&request).unwrap().is_none());
        app.connection
            .execute_batch("DROP TRIGGER fail_classic_audit")
            .unwrap();
        let receipt = app.classic_import_commit(&request, &analysis).unwrap();
        assert_eq!(
            app.classic_import_replay(&request)
                .unwrap()
                .unwrap()
                .revision,
            receipt.revision
        );
        assert_eq!(app.snapshot("imported").unwrap().revision, receipt.revision);
    }
}
