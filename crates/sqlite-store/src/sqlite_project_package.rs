//! Owner-only package source reads and atomic target project publication.
use super::*;
use opencut_api_contracts::project_package::{ImportPackage, ImportReceipt, PreparedImport};
use opencut_editor_application::project_package::{self, PackageExportSource};

impl EditorApplication {
    /// Call before native path access when accepting an import request.
    pub fn package_require_owner(&self) -> Result<(), EditorError> {
        sqlite_access::owner(&self.connection.1)
    }

    /// Exact retry before touching the source path. The caller-supplied
    /// manifest digest and target identity must match the committed receipt.
    pub fn package_import_replay(
        &self,
        input: &ImportPackage,
    ) -> Result<Option<ImportReceipt>, EditorError> {
        self.package_require_owner()?;
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
        let identity = request_hash("import_package", "workspace", &identity_value)?;
        load_idempotent(
            &self.connection,
            "import_package",
            "workspace",
            &input.idempotency_key,
            &identity,
            "package_import",
        )?
        .map(|stored| decode_json(&stored, "package import receipt"))
        .transpose()
    }

    /// Read one fixed project revision and every declared project plugin grant.
    /// The native adapter then packages exact CAS content for this snapshot.
    pub fn package_export_source(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<PackageExportSource, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        let snapshot = self.snapshot_at(project_id, revision)?;
        let mut statement = self.connection.prepare(
            "SELECT plugin_id,version FROM project_plugin_grants WHERE project_id=?1 ORDER BY plugin_id,version",
        ).map_err(storage_error)?;
        let plugin_grants = statement
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
        Ok(PackageExportSource {
            snapshot,
            plugin_grants,
        })
    }

    /// Publish a verified package as one new project. CAS capture occurs before
    /// this transaction; failed SQL may leave unreferenced CAS bytes for GC,
    /// never a visible partial project or success receipt.
    pub fn package_import_commit(
        &mut self,
        input: &ImportPackage,
        prepared: &PreparedImport,
    ) -> Result<ImportReceipt, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        require_idempotency_key(&input.idempotency_key)?;
        if input.expected_manifest_sha256 != prepared.manifest_sha256 {
            return Err(EditorError::new(
                "PACKAGE_INVALID",
                "Prepared manifest identity differs from request",
            ));
        }
        let ready = project_package::prepare_import(prepared, &input.target_project_id)?;
        let identity_value = json!({"target_project_id":input.target_project_id,
            "manifest_sha256":prepared.manifest_sha256});
        let identity = request_hash("import_package", "workspace", &identity_value)?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_access::owner(&context)?;
        if let Some(stored) = load_idempotent(
            &tx,
            "import_package",
            "workspace",
            &input.idempotency_key,
            &identity,
            "package_import",
        )? {
            return decode_json(&stored, "package import receipt");
        }
        let receipt = ImportReceipt {
            target_project_id: input.target_project_id.clone(),
            revision: ready.revision.clone(),
            manifest_sha256: prepared.manifest_sha256.clone(),
            source_project_id: prepared.manifest.source_project_id.clone(),
            source_revision: prepared.manifest.source_revision.clone(),
            dependencies: prepared.manifest.dependencies.clone(),
            self_contained_media: prepared.manifest.self_contained_media,
            fully_reproducible: prepared.manifest.fully_reproducible,
        };
        let receipt_json = encode_json(&receipt, "package import receipt")?;
        let payload = json!({"project_id":input.target_project_id,
            "revision":ready.revision,"source_project_id":prepared.manifest.source_project_id,
            "source_revision":prepared.manifest.source_revision,
            "manifest_sha256":prepared.manifest_sha256});
        sqlite_new_project::publish(
            &tx,
            &context,
            sqlite_new_project::NewProjectPublication {
                document: &ready.document,
                revision: &ready.revision,
                operation_family: "import_package",
                idempotency_key: &input.idempotency_key,
                request_digest: identity.current_digest(),
                result_kind: "package_import",
                result_json: &receipt_json,
                event_type: "project.package_imported",
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
    use opencut_api_contracts::project_package::{
        PackageDependencies, PackageManifest, PackageMode,
    };
    use opencut_contracts::{CreateProject, FrameRate};

    #[test]
    fn audit_failure_rolls_back_import_head_revision_and_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = EditorApplication::open(directory.path().join("workspace.sqlite")).unwrap();
        let document = opencut_editor_core::create_project(&CreateProject {
            id: "source".into(),
            name: "Source".into(),
            sequence_id: "main".into(),
            width: 64,
            height: 48,
            fps: FrameRate {
                numerator: 24,
                denominator: 1,
            },
        })
        .unwrap();
        let dependencies = PackageDependencies {
            fonts_not_bundled: vec![],
            plugins_not_bundled: vec![],
            tools_not_locked: vec!["ffmpeg".into()],
            unsupported: vec![],
        };
        let prepared = PreparedImport {
            manifest_sha256: "a".repeat(64),
            manifest: PackageManifest {
                schema_version: 1,
                mode: PackageMode::Linked,
                source_project_id: "source".into(),
                source_revision: "rev-source".into(),
                project_path: "project.json".into(),
                project_sha256: "b".repeat(64),
                assets: vec![],
                dependencies,
                self_contained_media: false,
                fully_reproducible: false,
            },
            document,
        };
        let input = ImportPackage {
            package_path: "/unused".into(),
            target_project_id: "restored".into(),
            expected_manifest_sha256: "a".repeat(64),
            idempotency_key: "import-once".into(),
        };
        app.connection
            .execute_batch(
                "CREATE TRIGGER fail_package_audit BEFORE INSERT ON audit_records
            WHEN NEW.action='project.package_imported' BEGIN SELECT RAISE(ABORT,'forced'); END;",
            )
            .unwrap();
        assert!(app.package_import_commit(&input, &prepared).is_err());
        assert_eq!(app.snapshot("restored").unwrap_err().code, "NOT_FOUND");
        let receipts: i64 = app
            .connection
            .query_row(
                "SELECT COUNT(*) FROM idempotency WHERE operation_family='import_package'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(receipts, 0);
        app.connection
            .execute_batch("DROP TRIGGER fail_package_audit")
            .unwrap();
        let receipt = app.package_import_commit(&input, &prepared).unwrap();
        assert_eq!(
            app.package_import_commit(&input, &prepared)
                .unwrap()
                .revision,
            receipt.revision
        );
        assert_eq!(app.snapshot("restored").unwrap().revision, receipt.revision);
    }
}
