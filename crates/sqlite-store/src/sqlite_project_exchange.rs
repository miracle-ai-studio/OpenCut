//! Consistent capture and atomic publication for the sole package entry point.
use super::*;
use opencut_api_contracts::project_exchange::*;
use opencut_contracts::scene::ResourceOrigin;
use opencut_editor_application::{
    project_archive::{ArchiveSource, ArchivedRevision, ProjectArchive},
    project_exchange::{self as package, PackageSource, PreparedImport, PreparedPackage},
};
use std::collections::BTreeMap;
mod historical;

const FAMILY: &str = "project_package_import";
const RESULT: &str = "project_package_import";
/// Existing branch integration cases share one package/publication assertion.
/// No native package files or media are produced by this fixture adapter.
#[cfg(test)]
pub(super) fn assert_current_archive_roundtrip(
    app: &mut EditorApplication,
    project: &str,
    revision: &str,
    target: &str,
) -> opencut_editor_application::project_archive::ProjectArchive {
    let source = app
        .project_package_source(&ExportPackage {
            project_id: project.into(),
            revision: revision.into(),
            destination: "/fixture-only/package".into(),
            mode: PackageMode::SelfContained,
            scope: PackageScope::History,
        })
        .unwrap();
    assert!(
        package::blob_versions(&source.content.archive)
            .unwrap()
            .is_empty()
    );
    assert!(
        source
            .content
            .archive
            .transactions
            .iter()
            .all(|t| t.source_order.is_some())
    );
    let mut missing = source.clone();
    missing.content.archive.transactions[0].source_order = None;
    assert!(
        package::validate_source(&missing, PackageScope::History, &PackageMode::SelfContained)
            .is_err()
    );
    let manifest = package::make_manifest(
        &source,
        PackageScope::History,
        PackageMode::SelfContained,
        vec![],
        vec![],
        vec![],
    )
    .unwrap();
    let manifest_sha256 = package::digest(
        &package::canonical_bytes(
            &manifest,
            opencut_editor_application::project_archive::MAX_MANIFEST_BYTES,
        )
        .unwrap(),
    );
    let bytes = package::archive_bytes(&source.content.archive).unwrap();
    let prepared = PreparedPackage {
        manifest_sha256: manifest_sha256.clone(),
        manifest,
        archive: package::decode_archive(&bytes).unwrap(),
        tool_lock_mismatches: vec![],
        fonts_missing: vec![],
        install_plan: vec![],
    };
    let mapped = package::prepare_import(&prepared, target).unwrap();
    let request = ImportPackage {
        package_path: "/fixture-only/package".into(),
        target_project_id: target.into(),
        expected_manifest_sha256: manifest_sha256,
        idempotency_key: format!("roundtrip-{target}"),
    };
    let prepared = PreparedImport::Current(prepared);
    let receipt = app.project_package_commit(&request, &prepared).unwrap();
    assert_eq!(
        serde_json::to_value(app.project_package_commit(&request, &prepared).unwrap()).unwrap(),
        serde_json::to_value(&receipt).unwrap()
    );
    assert_eq!(receipt.revision, mapped.main_head_revision);
    let mut expected: Vec<_> = mapped.transactions.iter().collect();
    expected.sort_by_key(|t| t.source_order);
    let expected: Vec<_> = expected
        .into_iter()
        .map(|t| t.transaction_id.clone())
        .collect();
    let mut statement = app
        .connection
        .prepare(
            "SELECT transaction_id FROM editor_transactions WHERE project_id=?1 ORDER BY rowid",
        )
        .unwrap();
    let actual: Vec<String> = statement
        .query_map([target], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(actual, expected);
    let raw: String = app
        .connection
        .query_row(
            "SELECT document_json FROM revisions WHERE project_id=?1 AND revision=?2",
            params![target, receipt.revision],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        sqlite_project_archive::archived_document(&raw).unwrap(),
        mapped
            .revisions
            .iter()
            .find(|r| r.revision == receipt.revision)
            .unwrap()
            .document
    );
    assert_eq!(
        sqlite_project_document::project_head_revision(&app.connection, project).unwrap(),
        revision
    );
    mapped
}
fn identity(input: &ImportPackage) -> Result<RequestIdentity, EditorError> {
    require_idempotency_key(&input.idempotency_key)?;
    if !opencut_editor_application::project_archive::valid_id(&input.target_project_id)
        || input.target_project_id.len() > 128
        || !package::valid_hash(&input.expected_manifest_sha256)
    {
        return Err(package::invalid("Invalid package import identity"));
    }
    request_hash(
        FAMILY,
        "workspace",
        &json!({"target_project_id":input.target_project_id,"manifest_sha256":input.expected_manifest_sha256}),
    )
}
fn replay(
    connection: &Connection,
    input: &ImportPackage,
    identity: &RequestIdentity,
) -> Result<Option<ImportReceipt>, EditorError> {
    load_idempotent(
        connection,
        FAMILY,
        "workspace",
        &input.idempotency_key,
        identity,
        RESULT,
    )?
    .map(|s| decode_json(&s, "Package import receipt"))
    .transpose()
}
fn policies(connection: &Connection, project: &str) -> Result<Vec<PluginPolicy>, EditorError> {
    let mut out = Vec::new();
    for (table, wasm) in [
        ("project_plugin_grants", true),
        ("project_preset_grants", false),
    ] {
        let mut statement = connection
            .prepare(&format!(
                "SELECT grant_json FROM {table} WHERE project_id=?1 ORDER BY plugin_id,version"
            ))
            .map_err(storage_error)?;
        let mut rows = statement.query([project]).map_err(storage_error)?;
        while let Some(row) = rows.next().map_err(storage_error)? {
            if out.len() >= 128 {
                return Err(package::invalid(
                    "Package contains too many plugin policies",
                ));
            }
            let raw: String = row.get(0).map_err(storage_error)?;
            if raw.len() > opencut_editor_application::project_archive::MAX_DOCUMENT_BYTES {
                return Err(package::invalid("Plugin policy exceeds JSON budget"));
            }
            out.push(if wasm {
                PluginPolicy::Wasm(decode_json(&raw, "Project plugin policy")?)
            } else {
                PluginPolicy::Presets(decode_json(&raw, "Project preset policy")?)
            });
        }
    }
    out.sort_by(|a, b| {
        let (ak, ai, av, _, _) = package::policy_identity(a);
        let (bk, bi, bv, _, _) = package::policy_identity(b);
        (ak, ai, av).cmp(&(bk, bi, bv))
    });
    Ok(out)
}
impl EditorApplication {
    pub fn project_package_source(
        &self,
        input: &ExportPackage,
    ) -> Result<PackageSource, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_project_library::require_live(&tx, &input.project_id)?;
        let content = match input.scope {
            PackageScope::History => {
                let head = sqlite_project_document::project_head_revision(&tx, &input.project_id)?;
                if head != input.revision {
                    return Err(revision_conflict(&input.revision, &head));
                }
                sqlite_project_archive::capture_history(
                    &tx,
                    &input.project_id,
                    &input.revision,
                    input.mode == PackageMode::SelfContained,
                )?
            }
            PackageScope::SavedRevision => {
                let (raw, created_at): (String, String) = tx.query_row(
                    "SELECT document_json,created_at FROM revisions WHERE project_id=?1 AND revision=?2",
                    params![input.project_id, input.revision],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                ).optional().map_err(storage_error)?.ok_or_else(|| EditorError::new("NOT_FOUND", "Selected project revision does not exist"))?;
                if raw.len() > opencut_editor_application::project_archive::MAX_DOCUMENT_BYTES {
                    return Err(package::invalid(
                        "Selected document exceeds package JSON budget",
                    ));
                }
                let document = sqlite_project_archive::archived_document(&raw)?;
                if document.project_id() != input.project_id {
                    return Err(package::invalid(
                        "Selected document differs from project scope",
                    ));
                }
                let mut proofs = BTreeMap::new();
                for font in document.fonts() {
                    if let ResourceOrigin::Generated {
                        provider,
                        generation_id,
                    } = &font.origin
                        && opencut_editor_application::font_derived::provenance::is_provider(
                            provider,
                        )
                    {
                        proofs.insert(
                            generation_id.clone(),
                            sqlite_font_derivations::read_source_for_font(&tx, font)?,
                        );
                    }
                }
                ArchiveSource {
                    archive: ProjectArchive {
                        schema_version: 1,
                        source_project_id: input.project_id.clone(),
                        main_head_revision: input.revision.clone(),
                        revisions: vec![ArchivedRevision {
                            revision: input.revision.clone(),
                            parent_revision: None,
                            transaction_id: None,
                            document,
                            created_at,
                        }],
                        transactions: vec![],
                        branches: vec![],
                        merges: vec![],
                    },
                    derived_fonts: proofs.into_values().collect(),
                }
            }
        };
        let source = PackageSource {
            content,
            plugin_policies: policies(&tx, &input.project_id)?,
        };
        package::validate_source(&source, input.scope, &input.mode)?;
        tx.commit().map_err(storage_error)?;
        Ok(source)
    }
    /// Original committed requests are resolved before any path or media access.
    pub fn project_package_replay(
        &self,
        input: &ImportPackage,
    ) -> Result<Option<ImportReceipt>, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        let id = identity(input)?;
        if let Some(receipt) = replay(&self.connection, input, &id)? {
            return Ok(Some(receipt));
        }
        historical::replay(self, input)
    }
    pub fn project_package_commit(
        &mut self,
        input: &ImportPackage,
        prepared: &PreparedImport,
    ) -> Result<ImportReceipt, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        if let Some(receipt) = self.project_package_replay(input)? {
            return Ok(receipt);
        }
        match prepared {
            PreparedImport::Current(p) => self.commit_current_package(input, p),
            _ => historical::commit(self, input, prepared),
        }
    }
    fn commit_current_package(
        &mut self,
        input: &ImportPackage,
        p: &PreparedPackage,
    ) -> Result<ImportReceipt, EditorError> {
        if p.manifest_sha256 != input.expected_manifest_sha256 {
            return Err(package::invalid(
                "Prepared package differs from original request",
            ));
        }
        let archive = package::prepare_import(p, &input.target_project_id)?;
        let mut plugin_restore_plan = p.install_plan.clone();
        plugin_restore_plan.extend(package::grant_plan(
            &p.manifest.plugin_policies,
            &input.target_project_id,
            &p.manifest_sha256,
        )?);
        let receipt = ImportReceipt {
            target_project_id: input.target_project_id.clone(),
            revision: archive.main_head_revision.clone(),
            package: package::inspection(p)?,
            plugin_restore_plan,
        };
        publish_archive_import(self, input, &archive, &p.manifest.derived_fonts, receipt)
    }
}

pub(super) fn publish_archive_import(
    app: &mut EditorApplication,
    input: &ImportPackage,
    archive: &ProjectArchive,
    proofs: &[opencut_contracts::font_derived::FontDerivedSource],
    receipt: ImportReceipt,
) -> Result<ImportReceipt, EditorError> {
    let id = identity(input)?;
    sqlite_project_archive::validate_journals(archive)?;
    let cas = opencut_asset_store::AssetStore::read_only(app.asset_store_root())?;
    for version in package::blob_versions(archive)?.values() {
        cas.open_version_verified(version)?;
    }
    let context = app.connection.1.clone();
    let tx = app
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    sqlite_access::owner(&context)?;
    if let Some(receipt) = replay(&tx, input, &id)? {
        return Ok(receipt);
    }
    if historical::family(&tx, input)?.is_some() {
        return Err(EditorError::new(
            "IDEMPOTENCY_KEY_REUSED",
            "A historical import committed concurrently;retry the original request",
        ));
    }
    sqlite_project_archive::publish_archive(&tx, archive, proofs)?;
    let raw = encode_json(&receipt, "Package import receipt")?;
    tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json) VALUES(?1,'workspace',?2,?3,?4,?5)",params![FAMILY,input.idempotency_key,id.current_digest(),RESULT,raw]).map_err(idempotency_insert_error)?;
    tx.execute("INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json) VALUES(?1,?2,?3,'project.package_imported',?4)",params![format!("evt_{}",Uuid::new_v4().simple()),input.target_project_id,archive.main_head_revision,raw]).map_err(storage_error)?;
    sqlite_audit::insert(
        &tx,
        &context,
        "project.package_imported",
        sqlite_audit::Links {
            project_id: Some(&input.target_project_id),
            revision: Some(&archive.main_head_revision),
            ..Default::default()
        },
    )?;
    tx.commit().map_err(storage_error)?;
    drop(cas);
    Ok(receipt)
}
