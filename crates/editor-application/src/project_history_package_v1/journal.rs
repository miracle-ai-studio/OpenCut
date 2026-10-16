use super::*;
use opencut_contracts::{
    brand_resources_v1::ImportBrandResourceV1,
    catalog_resources_v1::{GenerateCatalogSoundV1, ImportCatalogResourceV1},
    font_import_v1::ImportFontV1,
    project_v4_history::CausalJournalV4,
    project_v4_ops::{ApplyV4Project, RestoreV4Revision},
    project_v4_public::{
        ApplyPublicV4Project, PublicAssetSelectorV4, PublicFontSelectorV4, PublicProjectOperationV4,
    },
    subtitle_v4::{GenerateCaptionsV4, SubtitleImportV4},
};

/// The archive envelope identifies a known original request shape. Storage is
/// restored to that original shape; no arbitrary JSON is rewritten or executed.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum HistoryPackageJournalV1 {
    Apply(ApplyV4Project),
    ApplyPublic(ApplyPublicV4Project),
    Restore(RestoreV4Revision),
    Causal(CausalJournalV4),
    /// Static and derived imports have exactly the same frozen request shape.
    FontImport(ImportFontV1),
    CaptionGenerate(GenerateCaptionsV4),
    CaptionImport(SubtitleImportV4),
    CatalogImport(ImportCatalogResourceV1),
    CatalogGenerate(GenerateCatalogSoundV1),
    BrandImport(ImportBrandResourceV1),
}
impl HistoryPackageJournalV1 {
    pub fn from_storage(raw: &str) -> Result<Self, EditorError> {
        if raw.len() > MAX_DOCUMENT_BYTES {
            return Err(invalid("Historical journal exceeds document limit"));
        }
        let value: serde_json::Value =
            serde_json::from_str(raw).map_err(|_| invalid("Malformed historical journal"))?;
        macro_rules! parse {
            ($ty:ty,$variant:ident) => {
                if let Ok(request) = serde_json::from_value::<$ty>(value.clone()) {
                    return Ok(Self::$variant(request));
                }
            };
        }
        parse!(CausalJournalV4, Causal);
        parse!(ApplyV4Project, Apply);
        parse!(ApplyPublicV4Project, ApplyPublic);
        parse!(RestoreV4Revision, Restore);
        parse!(ImportFontV1, FontImport);
        parse!(GenerateCaptionsV4, CaptionGenerate);
        parse!(SubtitleImportV4, CaptionImport);
        parse!(ImportCatalogResourceV1, CatalogImport);
        parse!(GenerateCatalogSoundV1, CatalogGenerate);
        parse!(ImportBrandResourceV1, BrandImport);
        Err(unsupported(
            "History package requires known V4 transaction journals; no unknown journal is omitted",
        ))
    }
    pub fn storage_json(&self) -> Result<String, EditorError> {
        macro_rules! encode {
            ($r:expr) => {
                serde_json::to_string($r)
                    .map_err(|_| invalid("Cannot serialize original historical request"))
            };
        }
        match self {
            Self::Apply(r) => encode!(r),
            Self::ApplyPublic(r) => encode!(r),
            Self::Restore(r) => encode!(r),
            Self::Causal(r) => encode!(r),
            Self::FontImport(r) => encode!(r),
            Self::CaptionGenerate(r) => encode!(r),
            Self::CaptionImport(r) => encode!(r),
            Self::CatalogImport(r) => encode!(r),
            Self::CatalogGenerate(r) => encode!(r),
            Self::BrandImport(r) => encode!(r),
        }
    }
    fn scope(&self) -> (&str, &str, &str) {
        macro_rules! scope {
            ($r:expr) => {
                (&$r.project_id, &$r.expected_revision, &$r.idempotency_key)
            };
        }
        match self {
            Self::Apply(r) => scope!(r),
            Self::ApplyPublic(r) => scope!(r),
            Self::Restore(r) => scope!(r),
            Self::Causal(CausalJournalV4::Undo { request, .. }) => scope!(request),
            Self::Causal(CausalJournalV4::Redo { request, .. }) => scope!(request),
            Self::FontImport(r) => scope!(r),
            Self::CaptionGenerate(r) => scope!(r),
            Self::CaptionImport(r) => scope!(r),
            Self::CatalogImport(r) => scope!(r),
            Self::CatalogGenerate(r) => scope!(r),
            Self::BrandImport(r) => scope!(r),
        }
    }
    pub(super) fn transaction_references(&self) -> Vec<&str> {
        match self {
            Self::Causal(CausalJournalV4::Undo { request, .. }) => {
                vec![&request.target_transaction_id]
            }
            Self::Causal(CausalJournalV4::Redo {
                request,
                target_transaction_id,
                ..
            }) => vec![&request.undo_transaction_id, target_transaction_id],
            _ => vec![],
        }
    }
    pub(super) fn revision_references(&self) -> Vec<&str> {
        match self {
            Self::Restore(r) => vec![r.target_revision.as_str()],
            Self::ApplyPublic(r) => r
                .operations
                .iter()
                .filter_map(|op| match op {
                    PublicProjectOperationV4::AssetBind {
                        source: PublicAssetSelectorV4::ProjectAsset { revision, .. },
                        ..
                    } => Some(revision.as_str()),
                    PublicProjectOperationV4::FontBind {
                        source: PublicFontSelectorV4::ProjectFont { revision, .. },
                        ..
                    } => Some(revision.as_str()),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        }
    }
    pub(super) fn validate(
        &self,
        project: &str,
        base: &str,
        revisions: &BTreeMap<&str, &HistoryRevisionV1>,
        transactions: &BTreeMap<&str, &HistoryTransactionV1>,
        traversal_left: &mut usize,
    ) -> Result<(), EditorError> {
        let (p, b, key) = self.scope();
        if p != project || b != base || !valid_id(key) {
            return Err(invalid(
                "Historical request differs from its scoped transaction",
            ));
        }
        let require_revision = |id: &str| -> Result<(), EditorError> {
            if revisions.contains_key(id) {
                Ok(())
            } else {
                Err(invalid(
                    "Historical request references a revision outside the package",
                ))
            }
        };
        match self {
            Self::BrandImport(r) => crate::brand_resources_v1::validate_request(r)?,
            Self::Restore(r) => {
                require_revision(&r.target_revision)?;
                if !r.asset_versions.is_empty() {
                    return Err(unsupported(
                        "V4-only restore journals must not contain legacy asset overrides",
                    ));
                }
            }
            Self::ApplyPublic(r) => {
                for op in &r.operations {
                    match op {
                        PublicProjectOperationV4::AssetBind {
                            source: PublicAssetSelectorV4::ProjectAsset { revision, .. },
                            ..
                        } => require_revision(revision)?,
                        PublicProjectOperationV4::FontBind {
                            source: PublicFontSelectorV4::ProjectFont { revision, .. },
                            ..
                        } => require_revision(revision)?,
                        _ => {}
                    }
                }
            }
            Self::Causal(j) => {
                let (schema, original) = match j {
                    CausalJournalV4::Undo {
                        schema_version,
                        request,
                        ..
                    } => (*schema_version, request.target_transaction_id.as_str()),
                    CausalJournalV4::Redo {
                        schema_version,
                        request,
                        target_transaction_id,
                        ..
                    } => {
                        let undo = transactions
                            .get(request.undo_transaction_id.as_str())
                            .ok_or_else(|| invalid("Redo undo transaction is missing"))?;
                        if !matches!(&undo.journal,Self::Causal(CausalJournalV4::Undo{request,..}) if request.target_transaction_id==*target_transaction_id)
                        {
                            return Err(invalid("Redo does not reference its original undo"));
                        }
                        (*schema_version, target_transaction_id.as_str())
                    }
                };
                if schema != 4
                    || transactions
                        .get(original)
                        .is_none_or(|t| matches!(&t.journal, Self::Causal(_)))
                {
                    return Err(invalid(
                        "Causal journal has no ordinary original transaction",
                    ));
                }
                // Walk newest first to find the active causal state for this
                // original action. The global graph budget bounds this walk.
                let mut cursor = Some(base);
                let mut active = None;
                while let Some(id) = cursor {
                    *traversal_left = traversal_left.checked_sub(1).ok_or_else(|| {
                        EditorError::new(
                            "RESOURCE_TOO_LARGE",
                            "Causal history exceeds bounded traversal budget",
                        )
                    })?;
                    let revision = revisions[id];
                    if let Some(id) = &revision.transaction_id {
                        match &transactions[id.as_str()].journal {
                            Self::Causal(CausalJournalV4::Undo { request, .. })
                                if request.target_transaction_id == original =>
                            {
                                active = Some(id.as_str());
                                break;
                            }
                            Self::Causal(CausalJournalV4::Redo {
                                target_transaction_id,
                                ..
                            }) if target_transaction_id == original => break,
                            _ => {}
                        }
                    }
                    cursor = revision.parent_revision.as_deref();
                }
                match j {
                    CausalJournalV4::Undo { .. } if active.is_some() => {
                        return Err(invalid("Original transaction is already undone"));
                    }
                    CausalJournalV4::Redo { request, .. }
                        if active != Some(request.undo_transaction_id.as_str()) =>
                    {
                        return Err(invalid("Redo does not match the active undo"));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn remap(
        &mut self,
        project: &str,
        revisions: &BTreeMap<String, String>,
        transactions: &BTreeMap<String, String>,
    ) -> Result<(), EditorError> {
        let revision = |id: &mut String| -> Result<(), EditorError> {
            *id = revisions
                .get(id)
                .ok_or_else(|| invalid("Cannot map historical revision"))?
                .clone();
            Ok(())
        };
        let transaction = |id: &mut String| -> Result<(), EditorError> {
            *id = transactions
                .get(id)
                .ok_or_else(|| invalid("Cannot map historical transaction"))?
                .clone();
            Ok(())
        };
        macro_rules! scope {
            ($r:expr) => {{
                $r.project_id = project.into();
                revision(&mut $r.expected_revision)?;
            }};
        }
        match self {
            Self::Apply(r) => scope!(r),
            Self::ApplyPublic(r) => {
                scope!(r);
                for op in &mut r.operations {
                    match op {
                        PublicProjectOperationV4::AssetBind {
                            source: PublicAssetSelectorV4::ProjectAsset { revision: id, .. },
                            ..
                        } => revision(id)?,
                        PublicProjectOperationV4::FontBind {
                            source: PublicFontSelectorV4::ProjectFont { revision: id, .. },
                            ..
                        } => revision(id)?,
                        _ => {}
                    }
                }
            }
            Self::Restore(r) => {
                scope!(r);
                revision(&mut r.target_revision)?;
            }
            Self::Causal(CausalJournalV4::Undo { request, .. }) => {
                scope!(request);
                transaction(&mut request.target_transaction_id)?;
            }
            Self::Causal(CausalJournalV4::Redo {
                request,
                target_transaction_id,
                ..
            }) => {
                scope!(request);
                transaction(&mut request.undo_transaction_id)?;
                transaction(target_transaction_id)?;
            }
            Self::FontImport(r) => scope!(r),
            Self::CaptionGenerate(r) => scope!(r),
            Self::CaptionImport(r) => scope!(r),
            Self::CatalogImport(r) => scope!(r),
            Self::CatalogGenerate(r) => scope!(r),
            Self::BrandImport(r) => scope!(r),
        }
        Ok(())
    }
}
