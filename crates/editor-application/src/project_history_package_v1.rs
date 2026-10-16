//! Pure schema 7 closure validation and deterministic identity translation.
//! Native paths, authority grants, jobs and import receipts are not archive data.
use opencut_api_contracts::project_history_package_v1::*;
use opencut_contracts::{
    EditorError, TransactionResult, font_derived_v1::FontDerivedSourceV1, project_v4::*,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

mod journal;
pub use journal::HistoryPackageJournalV1;
#[cfg(test)]
mod tests;

pub const MAX_HISTORY_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_HISTORY_ROWS: usize = 100_000;
pub const MAX_MANIFEST_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RESOURCE_BYTES: u64 = 64 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryRevisionV1 {
    pub revision: String,
    pub parent_revision: Option<String>,
    pub transaction_id: Option<String>,
    pub document: ProjectDocumentV4,
    pub created_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryTransactionV1 {
    pub transaction_id: String,
    pub base_revision: String,
    pub revision: String,
    pub journal: HistoryPackageJournalV1,
    pub result: TransactionResult,
    pub created_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryBranchV1 {
    pub name: String,
    pub head_revision: String,
    pub created_from_revision: String,
    pub created_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryMergeV1 {
    pub revision: String,
    pub source_revision: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryArchiveV1 {
    pub schema_version: u32,
    pub source_project_id: String,
    pub main_head_revision: String,
    /// Lexicographic identity order, not timestamp or insertion order.
    pub revisions: Vec<HistoryRevisionV1>,
    pub transactions: Vec<HistoryTransactionV1>,
    pub branches: Vec<HistoryBranchV1>,
    pub merges: Vec<HistoryMergeV1>,
}
#[derive(Clone, Debug)]
pub struct HistoryPackageSourceV1 {
    pub archive: HistoryArchiveV1,
    pub derived_fonts: Vec<FontDerivedSourceV1>,
}
#[derive(Clone, Debug)]
pub struct PreparedHistoryImportV1 {
    pub manifest_sha256: String,
    pub manifest: HistoryPackageManifestV1,
    pub archive: HistoryArchiveV1,
}

pub fn invalid(message: &str) -> EditorError {
    EditorError::new("PACKAGE_INVALID", message)
}
pub fn unsupported(message: &str) -> EditorError {
    EditorError::new("PACKAGE_HISTORY_UNSUPPORTED", message)
}
pub fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
pub fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn canonical_bytes<T: Serialize>(value: &T, max: usize) -> Result<Vec<u8>, EditorError> {
    let bytes =
        serde_json::to_vec(value).map_err(|_| invalid("Cannot serialize history package"))?;
    if bytes.len() > max {
        return Err(invalid("History package exceeds its bounded JSON envelope"));
    }
    Ok(bytes)
}
fn sorted<'a>(values: impl IntoIterator<Item = &'a str>) -> Result<(), EditorError> {
    let mut previous = None;
    for value in values {
        if !valid_id(value) || previous.is_some_and(|p| p >= value) {
            return Err(invalid("History identities must be unique and sorted"));
        }
        previous = Some(value);
    }
    Ok(())
}
fn stamp(value: &str) -> Result<(), EditorError> {
    if !valid_id(value) || value.len() > 128 {
        return Err(invalid("Invalid historical timestamp"));
    }
    Ok(())
}
fn inventory(
    archive: &HistoryArchiveV1,
) -> Result<(Vec<HistoryPackageBlobV1>, usize, usize), EditorError> {
    let mut blobs = BTreeMap::new();
    let mut assets = BTreeSet::new();
    let mut fonts = BTreeSet::new();
    for revision in &archive.revisions {
        for (font, id, version) in revision
            .document
            .assets
            .iter()
            .map(|a| (false, &a.id, &a.version))
            .chain(
                revision
                    .document
                    .fonts
                    .iter()
                    .map(|f| (true, &f.id, &f.content)),
            )
        {
            if !valid_hash(&version.content_hash)
                || version.size == 0
                || version.size
                    > if font {
                        crate::font_import_v1::MAX_FONT_BYTES_V1
                    } else {
                        opencut_api_contracts::uploads::MAX_UPLOAD_BYTES
                    }
            {
                return Err(invalid("Invalid historical resource version"));
            }
            if blobs
                .insert(version.content_hash.clone(), version.size)
                .is_some_and(|old| old != version.size)
            {
                return Err(invalid("Conflicting size for one historical resource hash"));
            }
            if font {
                fonts.insert((id.clone(), version.content_hash.clone()));
            } else {
                assets.insert((id.clone(), version.content_hash.clone()));
            }
        }
    }
    let total = blobs.values().try_fold(0u64, |sum, n| sum.checked_add(*n));
    if total.is_none_or(|sum| sum > MAX_RESOURCE_BYTES) {
        return Err(invalid("Historical resource bytes exceed package budget"));
    }
    Ok((
        blobs
            .into_iter()
            .map(|(content_hash, size_bytes)| HistoryPackageBlobV1 {
                path: format!("blobs/{content_hash}"),
                content_hash,
                size_bytes,
            })
            .collect(),
        assets.len(),
        fonts.len(),
    ))
}

/// Returns an insertion order after validating every parent and merge edge.
pub fn revision_order(archive: &HistoryArchiveV1) -> Result<Vec<usize>, EditorError> {
    let indexes: BTreeMap<_, _> = archive
        .revisions
        .iter()
        .enumerate()
        .map(|(i, r)| (r.revision.as_str(), i))
        .collect();
    let merges: BTreeMap<_, _> = archive
        .merges
        .iter()
        .map(|m| (m.revision.as_str(), m.source_revision.as_str()))
        .collect();
    let transactions: BTreeMap<_, _> = archive
        .transactions
        .iter()
        .map(|t| (t.transaction_id.as_str(), t))
        .collect();
    let mut pending = vec![0usize; archive.revisions.len()];
    let mut successors: Vec<Vec<usize>> = vec![Vec::new(); archive.revisions.len()];
    for (i, r) in archive.revisions.iter().enumerate() {
        let references = r
            .transaction_id
            .as_deref()
            .and_then(|id| transactions.get(id))
            .map(|t| t.journal.revision_references())
            .unwrap_or_default();
        for parent in r
            .parent_revision
            .as_deref()
            .into_iter()
            .chain(merges.get(r.revision.as_str()).copied())
            .chain(references)
        {
            let p = *indexes
                .get(parent)
                .ok_or_else(|| invalid("Historical edge leaves the project closure"))?;
            pending[i] += 1;
            successors[p].push(i);
        }
    }
    let mut ready: BTreeSet<_> = pending
        .iter()
        .enumerate()
        .filter_map(|(i, n)| (*n == 0).then_some(i))
        .collect();
    let mut order = Vec::with_capacity(pending.len());
    while let Some(i) = ready.pop_first() {
        order.push(i);
        for next in &successors[i] {
            pending[*next] -= 1;
            if pending[*next] == 0 {
                ready.insert(*next);
            }
        }
    }
    if order.len() != archive.revisions.len() {
        return Err(invalid("Historical revision graph contains a cycle"));
    }
    Ok(order)
}

pub fn validate_source(source: &HistoryPackageSourceV1) -> Result<(), EditorError> {
    let a = &source.archive;
    if a.schema_version != 1
        || !valid_id(&a.source_project_id)
        || a.revisions.is_empty()
        || a.revisions
            .len()
            .saturating_add(a.transactions.len())
            .saturating_add(a.branches.len())
            .saturating_add(a.merges.len())
            > MAX_HISTORY_ROWS
    {
        return Err(invalid("Unsupported or oversized V4 history closure"));
    }
    canonical_bytes(a, MAX_HISTORY_BYTES)?;
    sorted(a.revisions.iter().map(|r| r.revision.as_str()))?;
    sorted(a.transactions.iter().map(|t| t.transaction_id.as_str()))?;
    sorted(a.branches.iter().map(|b| b.name.as_str()))?;
    sorted(a.merges.iter().map(|m| m.revision.as_str()))?;
    sorted(
        source
            .derived_fonts
            .iter()
            .map(|f| f.face.derivation_id.as_str()),
    )?;
    let revisions: BTreeMap<_, _> = a
        .revisions
        .iter()
        .map(|r| (r.revision.as_str(), r))
        .collect();
    let transactions: BTreeMap<_, _> = a
        .transactions
        .iter()
        .map(|t| (t.transaction_id.as_str(), t))
        .collect();
    if !revisions.contains_key(a.main_head_revision.as_str()) {
        return Err(invalid("Main head is outside history closure"));
    }
    let mut roots = 0;
    let mut proofs = BTreeSet::new();
    for r in &a.revisions {
        stamp(&r.created_at)?;
        if r.document.id != a.source_project_id {
            return Err(invalid("Historical document belongs to another project"));
        }
        let needed: BTreeSet<_> = r
            .document
            .fonts
            .iter()
            .filter_map(|f| match &f.origin {
                ResourceOriginV4::Generated {
                    provider,
                    generation_id,
                } if crate::font_derived_v2::compat::is_provider(provider) => {
                    Some(generation_id.as_str())
                }
                _ => None,
            })
            .collect();
        proofs.extend(needed.iter().copied());
        // Reuse schema 6 pure document/fixed-font/proof validation, not its package
        // format or import projection. Each revision must be independently valid.
        crate::project_package_v4::validate_export_source(
            &crate::project_package_v4::PackageExportSourceV4 {
                snapshot: ProjectSnapshotV4 {
                    project: r.document.clone(),
                    revision: r.revision.clone(),
                },
                derived_fonts: source
                    .derived_fonts
                    .iter()
                    .filter(|f| needed.contains(f.face.derivation_id.as_str()))
                    .cloned()
                    .collect(),
            },
        )?;
        match (&r.parent_revision, &r.transaction_id) {
            (None, None) => roots += 1,
            (Some(parent), Some(transaction)) => {
                if !revisions.contains_key(parent.as_str()) {
                    return Err(invalid("Historical parent is missing"));
                }
                let t = transactions
                    .get(transaction.as_str())
                    .ok_or_else(|| invalid("Historical transaction is missing"))?;
                if t.base_revision != *parent || t.revision != r.revision {
                    return Err(invalid("Historical transaction edges differ"));
                }
            }
            _ => return Err(invalid("Only the initial revision may omit a transaction")),
        }
    }
    if roots != 1 || proofs.len() != source.derived_fonts.len() {
        return Err(invalid(
            "History requires one root and an exact derived-font proof inventory",
        ));
    }
    revision_order(a)?;
    let mut traversal_left = MAX_HISTORY_ROWS;
    for t in &a.transactions {
        stamp(&t.created_at)?;
        let r = revisions
            .get(t.revision.as_str())
            .ok_or_else(|| invalid("Transaction revision is missing"))?;
        if r.transaction_id.as_deref() != Some(t.transaction_id.as_str())
            || r.parent_revision.as_deref() != Some(t.base_revision.as_str())
            || t.result.transaction_id != t.transaction_id
            || t.result.base_revision != t.base_revision
            || t.result.revision != t.revision
        {
            return Err(invalid(
                "Original transaction result differs from its revision edges",
            ));
        }
        t.journal.validate(
            &a.source_project_id,
            &t.base_revision,
            &revisions,
            &transactions,
            &mut traversal_left,
        )?;
        for target in t.journal.transaction_references() {
            let target = transactions
                .get(target)
                .ok_or_else(|| invalid("Causal transaction is missing"))?;
            // Causal navigation is first-parent scoped, even when merge edges exist.
            let mut cursor = Some(t.base_revision.as_str());
            let mut found = false;
            while let Some(current) = cursor {
                traversal_left = traversal_left.checked_sub(1).ok_or_else(|| {
                    EditorError::new(
                        "RESOURCE_TOO_LARGE",
                        "Causal history exceeds bounded traversal budget",
                    )
                })?;
                if current == target.revision {
                    found = true;
                    break;
                }
                cursor = revisions[current].parent_revision.as_deref();
            }
            if !found {
                return Err(invalid(
                    "Causal transaction is outside first-parent ancestry",
                ));
            }
        }
    }
    for b in &a.branches {
        stamp(&b.created_at)?;
        if b.name == "main"
            || b.name.len() > 64
            || !b
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || !revisions.contains_key(b.head_revision.as_str())
            || !revisions.contains_key(b.created_from_revision.as_str())
        {
            return Err(invalid("Invalid or unclosed branch reference"));
        }
    }
    for m in &a.merges {
        let r = revisions
            .get(m.revision.as_str())
            .ok_or_else(|| invalid("Merge revision is missing"))?;
        if r.parent_revision.is_none() || !revisions.contains_key(m.source_revision.as_str()) {
            return Err(invalid("Invalid merge parent"));
        }
    }
    inventory(a)?;
    Ok(())
}
pub fn make_manifest(
    source: &HistoryPackageSourceV1,
) -> Result<HistoryPackageManifestV1, EditorError> {
    validate_source(source)?;
    let a = &source.archive;
    let (blobs, asset_version_count, font_version_count) = inventory(a)?;
    let m = HistoryPackageManifestV1 {
        schema_version: 7,
        project_schema_version: 4,
        source_project_id: a.source_project_id.clone(),
        source_revision: a.main_head_revision.clone(),
        history_path: "history.json".into(),
        history_sha256: digest(&canonical_bytes(a, MAX_HISTORY_BYTES)?),
        revision_count: a.revisions.len(),
        transaction_count: a.transactions.len(),
        branch_count: a.branches.len(),
        merge_count: a.merges.len(),
        asset_version_count,
        font_version_count,
        blobs,
        derived_fonts: source.derived_fonts.clone(),
        self_contained_resources: true,
        history_included: true,
        fully_reproducible: false,
    };
    validate_manifest(&m)?;
    Ok(m)
}
pub fn validate_manifest(m: &HistoryPackageManifestV1) -> Result<(), EditorError> {
    if m.schema_version != 7
        || m.project_schema_version != 4
        || !m.self_contained_resources
        || !m.history_included
        || m.fully_reproducible
        || !valid_id(&m.source_project_id)
        || !valid_id(&m.source_revision)
        || m.history_path != "history.json"
        || !valid_hash(&m.history_sha256)
        || m.revision_count == 0
        || m.revision_count
            .saturating_add(m.transaction_count)
            .saturating_add(m.branch_count)
            .saturating_add(m.merge_count)
            > MAX_HISTORY_ROWS
    {
        return Err(invalid("Unsupported history package scope"));
    }
    canonical_bytes(m, MAX_MANIFEST_BYTES)?;
    sorted(m.blobs.iter().map(|b| b.content_hash.as_str()))?;
    let mut total = 0u64;
    for b in &m.blobs {
        if !valid_hash(&b.content_hash)
            || b.path != format!("blobs/{}", b.content_hash)
            || b.size_bytes == 0
            || b.size_bytes > opencut_api_contracts::uploads::MAX_UPLOAD_BYTES
        {
            return Err(invalid("Invalid history package blob"));
        }
        total = total
            .checked_add(b.size_bytes)
            .ok_or_else(|| invalid("Package size overflow"))?;
    }
    if total > MAX_RESOURCE_BYTES {
        return Err(invalid("Package exceeds total resource limit"));
    }
    Ok(())
}
pub fn validate_prepared(p: &PreparedHistoryImportV1) -> Result<(), EditorError> {
    if !valid_hash(&p.manifest_sha256)
        || digest(&canonical_bytes(&p.manifest, MAX_MANIFEST_BYTES)?) != p.manifest_sha256
    {
        return Err(invalid("History manifest identity differs"));
    }
    let expected = make_manifest(&HistoryPackageSourceV1 {
        archive: p.archive.clone(),
        derived_fonts: p.manifest.derived_fonts.clone(),
    })?;
    if canonical_bytes(&expected, MAX_MANIFEST_BYTES)?
        != canonical_bytes(&p.manifest, MAX_MANIFEST_BYTES)?
    {
        return Err(invalid(
            "Manifest does not exactly describe complete history and resources",
        ));
    }
    Ok(())
}
pub fn inspection(p: &PreparedHistoryImportV1) -> HistoryPackageInspectionV1 {
    let m = &p.manifest;
    HistoryPackageInspectionV1 {
        manifest_sha256: p.manifest_sha256.clone(),
        source_project_id: m.source_project_id.clone(),
        source_revision: m.source_revision.clone(),
        project_name: p
            .archive
            .revisions
            .iter()
            .find(|r| r.revision == m.source_revision)
            .expect("validated main head")
            .document
            .name
            .clone(),
        revision_count: m.revision_count,
        transaction_count: m.transaction_count,
        branch_count: m.branch_count,
        merge_count: m.merge_count,
        asset_version_count: m.asset_version_count,
        font_version_count: m.font_version_count,
        derived_font_count: m.derived_fonts.len(),
        self_contained_resources: true,
        history_included: true,
        fully_reproducible: false,
    }
}
pub fn prepare_import(
    p: &PreparedHistoryImportV1,
    target: &str,
) -> Result<HistoryArchiveV1, EditorError> {
    if !valid_id(target) || target.len() > 128 {
        return Err(invalid("Invalid restored project ID"));
    }
    validate_prepared(p)?;
    let mapped = |kind: &str, old: &str| -> String {
        format!(
            "{kind}_{}",
            digest(
                &serde_json::to_vec(&("history-package-v1", &p.manifest_sha256, target, kind, old))
                    .expect("identity tuple")
            )
        )
    };
    let revisions: BTreeMap<_, _> = p
        .archive
        .revisions
        .iter()
        .map(|r| (r.revision.clone(), mapped("rev", &r.revision)))
        .collect();
    let transactions: BTreeMap<_, _> = p
        .archive
        .transactions
        .iter()
        .map(|t| (t.transaction_id.clone(), mapped("txn", &t.transaction_id)))
        .collect();
    let mut a = p.archive.clone();
    a.source_project_id = target.into();
    a.main_head_revision = revisions[&a.main_head_revision].clone();
    for r in &mut a.revisions {
        r.revision = revisions[&r.revision].clone();
        r.parent_revision = r.parent_revision.as_ref().map(|v| revisions[v].clone());
        r.transaction_id = r.transaction_id.as_ref().map(|v| transactions[v].clone());
        r.document.id = target.into();
    }
    for t in &mut a.transactions {
        t.transaction_id = transactions[&t.transaction_id].clone();
        t.base_revision = revisions[&t.base_revision].clone();
        t.revision = revisions[&t.revision].clone();
        t.journal.remap(target, &revisions, &transactions)?;
        t.result.transaction_id = t.transaction_id.clone();
        t.result.base_revision = t.base_revision.clone();
        t.result.revision = t.revision.clone();
        for id in t
            .result
            .created_ids
            .iter_mut()
            .chain(&mut t.result.changed_ids)
            .chain(&mut t.result.deleted_ids)
        {
            if id.as_str() == p.archive.source_project_id {
                *id = target.into();
            }
        }
    }
    for b in &mut a.branches {
        b.head_revision = revisions[&b.head_revision].clone();
        b.created_from_revision = revisions[&b.created_from_revision].clone();
    }
    for m in &mut a.merges {
        m.revision = revisions[&m.revision].clone();
        m.source_revision = revisions[&m.source_revision].clone();
    }
    a.revisions.sort_by(|a, b| a.revision.cmp(&b.revision));
    a.transactions
        .sort_by(|a, b| a.transaction_id.cmp(&b.transaction_id));
    a.merges.sort_by(|a, b| a.revision.cmp(&b.revision));
    validate_source(&HistoryPackageSourceV1 {
        archive: a.clone(),
        derived_fonts: p.manifest.derived_fonts.clone(),
    })?;
    Ok(a)
}
