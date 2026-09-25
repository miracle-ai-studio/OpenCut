//! Owner-only fixed schema 3 package source and atomic target publication.
use super::*;
use opencut_api_contracts::project_package_v3::{
    ImportPackageV3, ImportReceipt, PackageToolLock, PreparedImportV3,
};
use opencut_editor_application::project_package_v3::{self, PackageExportSourceV3};

fn decode_import_receipt(raw: &str) -> Result<ImportReceipt, EditorError> {
    if let Ok(receipt) = serde_json::from_str::<ImportReceipt>(raw) {
        return Ok(receipt);
    }
    let legacy: opencut_api_contracts::project_package::ImportReceipt =
        decode_json(raw, "legacy schema 3 package import receipt")?;
    Ok(ImportReceipt {
        target_project_id: legacy.target_project_id,
        revision: legacy.revision,
        manifest_sha256: legacy.manifest_sha256,
        source_project_id: legacy.source_project_id,
        source_revision: legacy.source_revision,
        tool_lock_mismatches: legacy.dependencies.tools_not_locked.clone(),
        dependencies: legacy.dependencies,
        tool_locks: Vec::<PackageToolLock>::new(),
        tool_environment_matches: false,
        self_contained_media: legacy.self_contained_media,
        fully_reproducible: legacy.fully_reproducible,
    })
}

impl EditorApplication {
    pub fn package_v3_export_source(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<PackageExportSourceV3, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let snapshot = sqlite_project_v3::load_v3_snapshot(&tx, project_id, Some(revision))?;
        let mut statement = tx.prepare(
            "SELECT plugin_id,version FROM project_plugin_grants WHERE project_id=?1 ORDER BY plugin_id,version",
        ).map_err(storage_error)?;
        let mut plugin_grants = statement
            .query_map([project_id], |row| {
                Ok(format!(
                    "{}@{}",
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?
                ))
            })
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        drop(statement);
        let mut v2_statement = tx.prepare(
            "SELECT grant_json FROM project_preset_grants WHERE project_id=?1 ORDER BY plugin_id,version",
        ).map_err(storage_error)?;
        let v2 = v2_statement
            .query_map([project_id], |row| row.get::<_, String>(0))
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        for raw in v2 {
            let grant: opencut_api_contracts::plugin_v2::Grant = decode_json(&raw, "preset grant")?;
            if grant.enabled {
                plugin_grants.push(format!(
                    "v2:{}@{}#{}",
                    grant.target.id, grant.target.version, grant.package_sha256
                ));
            }
        }
        drop(v2_statement);
        tx.commit().map_err(storage_error)?;
        Ok(PackageExportSourceV3 {
            snapshot,
            plugin_grants,
        })
    }

    pub fn package_v3_import_replay(
        &self,
        input: &ImportPackageV3,
    ) -> Result<Option<ImportReceipt>, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        require_idempotency_key(&input.idempotency_key)?;
        if input.expected_manifest_sha256.len() != 64
            || !input
                .expected_manifest_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(EditorError::new(
                "PACKAGE_INVALID",
                "Invalid expected manifest SHA-256",
            ));
        }
        let identity_value = json!({"target_project_id":input.target_project_id,
            "manifest_sha256":input.expected_manifest_sha256});
        let identity = request_hash("import_package_v3", "workspace", &identity_value)?;
        load_idempotent(
            &self.connection,
            "import_package_v3",
            "workspace",
            &input.idempotency_key,
            &identity,
            "package_import_v3",
        )?
        .map(|stored| decode_import_receipt(&stored))
        .transpose()
    }

    /// CAS capture happens before this transaction. Failed publication can only
    /// leave collectable unreferenced bytes, never a partial project or receipt.
    pub fn package_v3_import_commit(
        &mut self,
        input: &ImportPackageV3,
        prepared: &PreparedImportV3,
    ) -> Result<ImportReceipt, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        require_idempotency_key(&input.idempotency_key)?;
        if input.expected_manifest_sha256 != prepared.manifest_sha256 {
            return Err(EditorError::new(
                "PACKAGE_INVALID",
                "Prepared manifest identity differs from request",
            ));
        }
        project_package_v3::validate_manifest(&prepared.manifest)?;
        let document = project_package_v3::prepare_import(prepared, &input.target_project_id)?;
        let revision = sqlite_project_v3::revision_id(&document, None, None)?;
        let identity_value = json!({"target_project_id":input.target_project_id,
            "manifest_sha256":prepared.manifest_sha256});
        let identity = request_hash("import_package_v3", "workspace", &identity_value)?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_access::owner(&context)?;
        if let Some(stored) = load_idempotent(
            &tx,
            "import_package_v3",
            "workspace",
            &input.idempotency_key,
            &identity,
            "package_import_v3",
        )? {
            return decode_import_receipt(&stored);
        }
        let receipt = ImportReceipt {
            target_project_id: input.target_project_id.clone(),
            revision: revision.clone(),
            manifest_sha256: prepared.manifest_sha256.clone(),
            source_project_id: prepared.manifest.source_project_id.clone(),
            source_revision: prepared.manifest.source_revision.clone(),
            dependencies: prepared.manifest.dependencies.clone(),
            tool_locks: prepared.manifest.tool_locks.clone(),
            tool_environment_matches: prepared.tool_environment_matches,
            tool_lock_mismatches: prepared.tool_lock_mismatches.clone(),
            self_contained_media: prepared.manifest.self_contained_media,
            fully_reproducible: prepared.manifest.fully_reproducible,
        };
        let receipt_json = encode_json(&receipt, "schema 3 package import receipt")?;
        let payload = json!({"project_id":input.target_project_id,"revision":revision,
            "source_project_id":prepared.manifest.source_project_id,
            "source_revision":prepared.manifest.source_revision,
            "manifest_sha256":prepared.manifest_sha256});
        sqlite_new_project::publish(
            &tx,
            &context,
            sqlite_new_project::NewProjectPublication {
                document: &document,
                revision: &revision,
                operation_family: "import_package_v3",
                idempotency_key: &input.idempotency_key,
                request_digest: identity.current_digest(),
                result_kind: "package_import_v3",
                result_json: &receipt_json,
                event_type: "project.package_v3_imported",
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
    use opencut_api_contracts::project_package::PackageDependencies;

    #[test]
    fn old_persisted_schema3_receipt_replays_as_explicitly_unlocked() {
        let legacy = opencut_api_contracts::project_package::ImportReceipt {
            target_project_id: "restored".into(),
            revision: "rev_new".into(),
            manifest_sha256: "a".repeat(64),
            source_project_id: "source".into(),
            source_revision: "rev_source".into(),
            dependencies: PackageDependencies {
                fonts_not_bundled: Vec::new(),
                plugins_not_bundled: Vec::new(),
                tools_not_locked: vec!["ffmpeg".into(), "ffprobe".into()],
                unsupported: Vec::new(),
            },
            self_contained_media: true,
            fully_reproducible: false,
        };
        let raw = serde_json::to_string(&legacy).unwrap();
        let restored = decode_import_receipt(&raw).unwrap();
        assert!(!restored.tool_environment_matches);
        assert_eq!(
            restored.tool_lock_mismatches,
            vec!["ffmpeg".to_string(), "ffprobe".to_string()]
        );
        assert!(restored.tool_locks.is_empty());
    }
}
