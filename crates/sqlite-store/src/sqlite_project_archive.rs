//! Full V4 history publication is one transaction; historical requests are facts,
//! never replay grants. Original import replay precedes all package file access.
use super::*;
use opencut_api_contracts::project_archive::*;
use opencut_contracts::scene::ResourceOrigin;
use opencut_editor_application::project_archive::{self as package, *};
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;
const FAMILY: &str = "project_history_package_v1_import";
const RESULT: &str = "project_history_package_v1_import";
fn identity(input: &HistoricalArchiveImport) -> Result<RequestIdentity, EditorError> {
    require_idempotency_key(&input.idempotency_key)?;
    if !package::valid_id(&input.target_project_id)
        || input.target_project_id.len() > 128
        || !package::valid_hash(&input.expected_manifest_sha256)
    {
        return Err(package::invalid("Invalid history import identity"));
    }
    request_hash(
        FAMILY,
        "workspace",
        &json!({"target_project_id":input.target_project_id,"manifest_sha256":input.expected_manifest_sha256}),
    )
}
fn replay(
    connection: &Connection,
    input: &HistoricalArchiveImport,
    identity: &RequestIdentity,
) -> Result<Option<HistoricalArchiveImportReceipt>, EditorError> {
    load_idempotent(
        connection,
        FAMILY,
        "workspace",
        &input.idempotency_key,
        identity,
        RESULT,
    )?
    .map(|raw| decode_json(&raw, "History import receipt"))
    .transpose()
}
struct ScanBudget {
    bytes: usize,
    rows: usize,
    start: std::time::Instant,
}
impl ScanBudget {
    fn new() -> Self {
        Self {
            bytes: 0,
            rows: 0,
            start: std::time::Instant::now(),
        }
    }
    fn observe(&mut self, raw: &[&str]) -> Result<(), EditorError> {
        self.rows = self.rows.saturating_add(1);
        for value in raw {
            self.bytes = self.bytes.saturating_add(value.len());
            if value.len() > MAX_DOCUMENT_BYTES {
                return Err(package::invalid("History row exceeds document budget"));
            }
        }
        if self.bytes > MAX_HISTORY_BYTES
            || self.rows > MAX_HISTORY_ROWS
            || self.start.elapsed()
                > Duration::from_millis(opencut_editor_application::gc::MAX_REFERENCE_MS)
        {
            return Err(EditorError::new(
                "RESOURCE_TOO_LARGE",
                "History capture exceeds bounded traversal budget",
            ));
        }
        Ok(())
    }
}
pub(super) fn validate_journals(archive: &ProjectArchive) -> Result<(), EditorError> {
    let docs: BTreeMap<_, _> = archive
        .revisions
        .iter()
        .map(|r| (r.revision.as_str(), &r.document))
        .collect();
    for t in &archive.transactions {
        sqlite_project_history::journal::entry(
            &archive.source_project_id,
            &t.transaction_id,
            &t.base_revision,
            &t.revision,
            &t.journal.storage_json()?,
            &encode_json(docs[&t.base_revision.as_str()], "History base")?,
            &encode_json(docs[&t.revision.as_str()], "History result")?,
        )?;
    }
    Ok(())
}
pub(super) fn archived_document(raw: &str) -> Result<ArchivedDocument, EditorError> {
    use sqlite_project_document::ProjectStorageDocument as D;
    Ok(match sqlite_project_document::decode_stored_project(raw)? {
        D::Legacy(p) => ArchivedDocument::Legacy(p),
        D::HistoricalVisual(p) => ArchivedDocument::HistoricalVisual(p),
        D::Editable(p) => ArchivedDocument::Editable(p),
        D::ReadOnly(p) => ArchivedDocument::ReadOnly(p),
    })
}
pub(super) fn capture_history(
    connection: &Connection,
    project: &str,
    expected_head: &str,
    require_bundled_fonts: bool,
) -> Result<ArchiveSource, EditorError> {
    let mut budget = ScanBudget::new();
    let mut archive = ProjectArchive {
        schema_version: 1,
        source_project_id: project.into(),
        main_head_revision: expected_head.into(),
        revisions: vec![],
        transactions: vec![],
        branches: vec![],
        merges: vec![],
    };
    let mut proofs = BTreeMap::new();
    {
        let mut query=connection.prepare("SELECT revision,parent_revision,transaction_id,document_json,created_at FROM revisions WHERE project_id=?1 ORDER BY revision").map_err(storage_error)?;
        let mut rows = query.query([project]).map_err(storage_error)?;
        while let Some(row) = rows.next().map_err(storage_error)? {
            let raw: String = row.get(3).map_err(storage_error)?;
            budget.observe(&[&raw])?;
            let document = archived_document(&raw)?;
            for font in document.fonts() {
                if let ResourceOrigin::Generated {
                    provider,
                    generation_id,
                } = &font.origin
                    && opencut_editor_application::font_derived::provenance::is_provider(provider)
                {
                    let proof = sqlite_font_derivations::read_source_for_font(connection, font)?;
                    if let Some(previous) = proofs.insert(generation_id.clone(), proof.clone())
                        && serde_json::to_value(previous).ok() != serde_json::to_value(&proof).ok()
                    {
                        return Err(package::invalid(
                            "Historical derived proof identity changed",
                        ));
                    }
                }
            }
            archive.revisions.push(ArchivedRevision {
                revision: row.get(0).map_err(storage_error)?,
                parent_revision: row.get(1).map_err(storage_error)?,
                transaction_id: row.get(2).map_err(storage_error)?,
                document,
                created_at: row.get(4).map_err(storage_error)?,
            });
        }
    }
    {
        let mut query=connection.prepare("SELECT transaction_id,base_revision,revision,request_json,result_json,created_at,rowid FROM editor_transactions WHERE project_id=?1 ORDER BY transaction_id").map_err(storage_error)?;
        let mut rows = query.query([project]).map_err(storage_error)?;
        while let Some(row) = rows.next().map_err(storage_error)? {
            let request: String = row.get(3).map_err(storage_error)?;
            let result: String = row.get(4).map_err(storage_error)?;
            budget.observe(&[&request, &result])?;
            archive.transactions.push(ArchivedTransaction {
                transaction_id: row.get(0).map_err(storage_error)?,
                base_revision: row.get(1).map_err(storage_error)?,
                revision: row.get(2).map_err(storage_error)?,
                journal: ArchivedJournal::from_storage(&request)?,
                result: decode_json(&result, "Original history transaction result")?,
                created_at: row.get(5).map_err(storage_error)?,
                source_order: Some(row.get(6).map_err(storage_error)?),
            });
        }
    }
    {
        let mut query=connection.prepare("SELECT branch_name,head_revision,created_from_revision,created_at FROM project_branches WHERE project_id=?1 ORDER BY branch_name").map_err(storage_error)?;
        let mut rows = query.query([project]).map_err(storage_error)?;
        while let Some(row) = rows.next().map_err(storage_error)? {
            let name: String = row.get(0).map_err(storage_error)?;
            budget.observe(&[&name])?;
            archive.branches.push(ArchivedBranch {
                name,
                head_revision: row.get(1).map_err(storage_error)?,
                created_from_revision: row.get(2).map_err(storage_error)?,
                created_at: row.get(3).map_err(storage_error)?,
            });
        }
    }
    {
        let mut query=connection.prepare("SELECT revision,source_revision FROM project_merge_parents WHERE project_id=?1 ORDER BY revision").map_err(storage_error)?;
        let mut rows = query.query([project]).map_err(storage_error)?;
        while let Some(row) = rows.next().map_err(storage_error)? {
            let revision: String = row.get(0).map_err(storage_error)?;
            budget.observe(&[&revision])?;
            archive.merges.push(ArchivedMerge {
                revision,
                source_revision: row.get(1).map_err(storage_error)?,
            });
        }
    }
    let source = ArchiveSource {
        archive,
        derived_fonts: proofs.into_values().collect(),
    };
    package::validate_content_for_exchange(&source, require_bundled_fonts)?;
    validate_journals(&source.archive)?;
    budget.observe(&[])?;
    Ok(source)
}
pub(super) fn publish_archive(
    tx: &rusqlite::Transaction<'_>,
    archive: &ProjectArchive,
    proofs: &[opencut_contracts::font_derived::FontDerivedSource],
) -> Result<(), EditorError> {
    if tx
        .query_row(
            "SELECT 1 FROM projects WHERE project_id=?1",
            [&archive.source_project_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(storage_error)?
        .is_some()
    {
        return Err(EditorError::new(
            "ALREADY_EXISTS",
            "Target project already exists",
        ));
    }
    let main = archive
        .revisions
        .iter()
        .find(|r| r.revision == archive.main_head_revision)
        .ok_or_else(|| package::invalid("Restored main head is missing"))?;
    tx.execute(
        "INSERT INTO projects(project_id,name,head_revision) VALUES(?1,?2,?3)",
        params![
            archive.source_project_id,
            main.document.name(),
            archive.main_head_revision
        ],
    )
    .map_err(constraint_or_storage)?;
    for index in package::revision_order(archive)? {
        let r = &archive.revisions[index];
        tx.execute("INSERT INTO revisions(revision,project_id,parent_revision,transaction_id,document_json,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![r.revision,archive.source_project_id,r.parent_revision,r.transaction_id,encode_json(&r.document,"Restored historical document")?,r.created_at]).map_err(storage_error)?;
    }
    for index in package::transaction_order(archive)? {
        let t = &archive.transactions[index];
        tx.execute("INSERT INTO editor_transactions(transaction_id,project_id,base_revision,revision,request_json,result_json,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![t.transaction_id,archive.source_project_id,t.base_revision,t.revision,t.journal.storage_json()?,encode_json(&t.result,"Restored historical transaction result")?,t.created_at]).map_err(storage_error)?;
    }
    for b in &archive.branches {
        tx.execute("INSERT INTO project_branches(project_id,branch_name,head_revision,created_from_revision,created_at) VALUES(?1,?2,?3,?4,?5)",params![archive.source_project_id,b.name,b.head_revision,b.created_from_revision,b.created_at]).map_err(storage_error)?;
    }
    for m in &archive.merges {
        tx.execute("INSERT INTO project_merge_parents(revision,project_id,source_revision) VALUES(?1,?2,?3)",params![m.revision,archive.source_project_id,m.source_revision]).map_err(storage_error)?;
    }
    for proof in proofs {
        let font=archive.revisions.iter().flat_map(|r|r.document.fonts()).find(|font|matches!(&font.origin,ResourceOrigin::Generated{provider,generation_id} if opencut_editor_application::font_derived::provenance::is_provider(provider) && generation_id==&proof.face.derivation_id)).ok_or_else(||package::invalid("Restored derived proof is unreferenced"))?;
        sqlite_font_derivations::persist_verified_source(&tx, font, proof)?;
    }
    Ok(())
}
impl EditorApplication {
    #[cfg(test)]
    pub fn historical_archive_source(
        &self,
        project: &str,
        expected_head: &str,
    ) -> Result<ArchiveSource, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        sqlite_project_library::require_live(&tx, project)?;
        let head = sqlite_project_document::project_head_revision(&tx, project)?;
        if head != expected_head {
            return Err(revision_conflict(expected_head, &head));
        }
        let plugins:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM project_plugin_grants WHERE project_id=?1) OR EXISTS(SELECT 1 FROM project_preset_grants WHERE project_id=?1)",[project],|r|r.get(0)).map_err(storage_error)?;
        if plugins {
            return Err(EditorError::new(
                "PACKAGE_DEPENDENCY_UNSUPPORTED",
                "History package cannot omit project plugin or preset dependencies",
            ));
        }
        let mut source = capture_history(&tx, project, expected_head, true)?;
        for transaction in &mut source.archive.transactions {
            transaction.source_order = None;
        }
        // This fixture adapter represents the frozen original package family;
        // the sole current package entry uses capture_history directly.
        package::validate_source(&source)?;
        tx.commit().map_err(storage_error)?;
        Ok(source)
    }
    pub fn historical_archive_replay(
        &self,
        input: &HistoricalArchiveImport,
    ) -> Result<Option<HistoricalArchiveImportReceipt>, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        replay(&self.connection, input, &identity(input)?)
    }
    #[cfg(test)]
    pub fn historical_archive_fixture_commit(
        &mut self,
        input: &HistoricalArchiveImport,
        prepared: &PreparedHistoricalArchive,
    ) -> Result<HistoricalArchiveImportReceipt, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        let identity = identity(input)?;
        if let Some(receipt) = replay(&self.connection, input, &identity)? {
            return Ok(receipt);
        }
        if prepared.manifest_sha256 != input.expected_manifest_sha256 {
            return Err(package::invalid(
                "Prepared history differs from the original import request",
            ));
        }
        let archive = package::prepare_import(prepared, &input.target_project_id)?;
        validate_journals(&archive)?;
        // Pure validation above proves exact resource closure for EVERY revision.
        // One collection gate protects the union, including unused/deleted
        // resources. Verify each immutable version once, avoiding O(revisions)
        // open directory descriptors and repeated hashing of shared media.
        let cas = opencut_asset_store::AssetStore::read_only(self.asset_store_root())?;
        for blob in &prepared.manifest.blobs {
            cas.open_version_verified(&opencut_contracts::AssetVersion {
                id: format!("av_{}", blob.content_hash),
                content_hash: blob.content_hash.clone(),
                size: blob.size_bytes,
            })?;
        }
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_access::owner(&context)?;
        if let Some(receipt) = replay(&tx, input, &identity)? {
            return Ok(receipt);
        }
        publish_archive(&tx, &archive, &prepared.manifest.derived_fonts)?;
        let receipt = HistoricalArchiveImportReceipt {
            target_project_id: input.target_project_id.clone(),
            revision: archive.main_head_revision.clone(),
            package: package::inspection(prepared),
        };
        let raw = encode_json(&receipt, "History package import receipt")?;
        tx.execute("INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json) VALUES(?1,'workspace',?2,?3,?4,?5)",params![FAMILY,input.idempotency_key,identity.current_digest(),RESULT,raw]).map_err(idempotency_insert_error)?;
        tx.execute("INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json) VALUES(?1,?2,?3,'project.history_package_v1_imported',?4)",params![format!("evt_{}",Uuid::new_v4().simple()),input.target_project_id,archive.main_head_revision,raw]).map_err(storage_error)?;
        sqlite_audit::insert(
            &tx,
            &context,
            "project.history_package_v1_imported",
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
}
