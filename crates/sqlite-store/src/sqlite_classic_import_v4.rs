//! Direct Classic V4 publication from authorized immutable revision resources.
use super::*;
use opencut_api_contracts::classic_import_v4::*;
use opencut_contracts::project_v4::*;
use opencut_editor_application::{
    classic_import_v4::{
        self, ClassicImportResourcesV4, ClassicImportV4Resources, ResolvedClassicFontV4,
    },
    project_v4::StoredProjectDocumentV4,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const FAMILY: &str = "import_classic_v4";
const RESULT: &str = "classic_import_v4";
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn resolve(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    input: &ClassicAnalyzeV4,
) -> Result<ClassicImportResourcesV4, EditorError> {
    sqlite_access::owner(context)?;
    if input.content.len() > 1024 * 1024
        || input.media_mappings.len() > 256
        || input.font_mappings.len() > 256
    {
        return Err(invalid("Classic resource request exceeds bounded envelope"));
    }
    let mut media = BTreeMap::new();
    for mapping in &input.media_mappings {
        sqlite_access::require(
            connection,
            context,
            &mapping.source_project_id,
            ProjectAction::Read,
        )?;
        let source = sqlite_project_v4::load_source(
            connection,
            &mapping.source_project_id,
            Some(&mapping.source_revision),
        )?;
        let asset = match source.document {
            StoredProjectDocumentV4::V4(document) => document
                .assets
                .iter()
                .find(|a| a.id == mapping.asset_id)
                .cloned(),
            other => {
                let assets = match &other {
                    StoredProjectDocumentV4::Legacy(d) => &d.assets,
                    StoredProjectDocumentV4::V3(d) => &d.base.assets,
                    _ => unreachable!(),
                };
                assets
                    .iter()
                    .find(|a| a.id == mapping.asset_id)
                    .and_then(|a| {
                        sqlite_project_v4::resources::known_version(a)
                            .and_then(|v| sqlite_project_v4::resources::canonical_asset(a, &v))
                    })
            }
        }
        .ok_or_else(|| {
            invalid("Classic mapped media is absent or not pinned in its fixed revision")
        })?;
        if media
            .insert(mapping.classic_media_id.clone(), asset)
            .is_some()
        {
            return Err(invalid("Duplicate Classic media mapping"));
        }
    }
    let mut fonts = Vec::new();
    let mut requests = BTreeSet::new();
    for mapping in &input.font_mappings {
        let key = encode_json(
            &(
                &mapping.classic_family,
                &mapping.font_weight,
                &mapping.font_style,
            ),
            "Classic font selector",
        )?;
        if !requests.insert(key) {
            return Err(invalid("Duplicate Classic font mapping"));
        }
        sqlite_access::require(
            connection,
            context,
            &mapping.source_project_id,
            ProjectAction::Read,
        )?;
        let source = sqlite_project_v4::load_source(
            connection,
            &mapping.source_project_id,
            Some(&mapping.source_revision),
        )?;
        let StoredProjectDocumentV4::V4(document) = source.document else {
            return Err(invalid("Classic font mapping requires a fixed V4 revision"));
        };
        let font = document
            .fonts
            .iter()
            .find(|f| f.id == mapping.font_id)
            .cloned()
            .ok_or_else(|| invalid("Mapped font is absent from fixed revision"))?;
        let weight = match mapping.font_weight {
            FontWeightV4::Normal => 400,
            FontWeightV4::Bold => 700,
        };
        if font.family != mapping.classic_family
            || font.weight != weight
            || font.style != mapping.font_style
        {
            return Err(invalid(
                "Mapped font face differs from exact Classic selector",
            ));
        }
        // Original import established inspector admission. This preserves the complete
        // provenance/license record; it does not assert glyph coverage for new text.
        fonts.push(ResolvedClassicFontV4 {
            request: ClassicFontRequestV4 {
                family: mapping.classic_family.clone(),
                font_weight: mapping.font_weight,
                font_style: mapping.font_style,
            },
            font,
        });
    }
    Ok(ClassicImportResourcesV4 { media, fonts })
}
impl ClassicImportV4Resources for EditorApplication {
    fn resolve_classic_resources_v4(
        &self,
        input: &ClassicAnalyzeV4,
    ) -> Result<ClassicImportResourcesV4, EditorError> {
        resolve(&self.connection, &self.connection.1, input)
    }
}
fn identity(input: &ClassicImportV4) -> Result<RequestIdentity, EditorError> {
    if opencut_editor_application::classic_import::source_content_id(&input.source.content)
        != input.expected_source_content_id
    {
        return Err(invalid("Classic source content ID mismatch"));
    }
    request_hash(
        FAMILY,
        "workspace",
        &serde_json::to_value(input).map_err(|_| invalid("Invalid Classic import request"))?,
    )
}
impl EditorApplication {
    pub fn classic_v4_analyze(
        &self,
        input: &ClassicAnalyzeV4,
    ) -> Result<ClassicAnalysisV4, EditorError> {
        let resources = self.resolve_classic_resources_v4(input)?;
        Ok(classic_import_v4::analyze(input, &resources)?.analysis)
    }
    pub fn classic_v4_import(
        &mut self,
        input: &ClassicImportV4,
    ) -> Result<ClassicImportReceiptV4, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        require_idempotency_key(&input.idempotency_key)?;
        let identity = identity(input)?;
        if let Some(raw) = load_idempotent(
            &self.connection,
            FAMILY,
            "workspace",
            &input.idempotency_key,
            &identity,
            RESULT,
        )? {
            return decode_json(&raw, "Classic V4 receipt");
        }
        let resources = self.resolve_classic_resources_v4(&input.source)?;
        let prepared = classic_import_v4::analyze(&input.source, &resources)?;
        let analysis = prepared.analysis;
        let document = prepared
            .document
            .ok_or_else(|| invalid("Classic V4 conversion has unresolved blockers"))?;
        if !analysis.blockers.is_empty()
            || analysis.source_content_id != input.expected_source_content_id
            || document.id != input.source.target_project_id
        {
            return Err(invalid("Classic V4 conversion identity or blockers differ"));
        }
        let losses: BTreeSet<_> = analysis.losses.iter().map(|f| f.code.as_str()).collect();
        let accepted: BTreeSet<_> = input
            .accepted_loss_codes
            .iter()
            .map(String::as_str)
            .collect();
        if losses != accepted || accepted.len() != input.accepted_loss_codes.len() {
            return Err(invalid(
                "Classic migration losses require exact acknowledgement",
            ));
        }
        opencut_editor_core::project_v4::validate_project_v4(&document)?;
        let revision = format!(
            "rev_{:x}",
            Sha256::digest(encode_json(&document, "Classic V4 project")?.as_bytes())
        );
        let receipt = ClassicImportReceiptV4 {
            schema_version: 4,
            target_project_id: document.id.clone(),
            revision: revision.clone(),
            source_content_id: analysis.source_content_id,
            asset_count: document.assets.len(),
            font_count: document.fonts.len(),
            unresolved_fonts: analysis.unresolved_fonts,
            losses: analysis.losses,
        };
        let _cas =
            sqlite_project_v4::resources::verify_document(&self.asset_store_root, &document, None)?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_access::owner(&context)?;
        if let Some(raw) = load_idempotent(
            &tx,
            FAMILY,
            "workspace",
            &input.idempotency_key,
            &identity,
            RESULT,
        )? {
            return decode_json(&raw, "Classic V4 receipt");
        }
        let latest = resolve(&tx, &context, &input.source)?;
        if encode_json(&latest.media, "Classic media authority")?
            != encode_json(&resources.media, "Classic media authority")?
            || encode_json(
                &latest
                    .fonts
                    .iter()
                    .map(|f| (&f.request, &f.font))
                    .collect::<Vec<_>>(),
                "Classic font authority",
            )? != encode_json(
                &resources
                    .fonts
                    .iter()
                    .map(|f| (&f.request, &f.font))
                    .collect::<Vec<_>>(),
                "Classic font authority",
            )?
        {
            return Err(invalid("Classic fixed resource authority changed"));
        }
        let receipt_json = encode_json(&receipt, "Classic V4 receipt")?;
        let payload = json!({"project_id":document.id,"revision":revision,"source_content_id":receipt.source_content_id});
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
                event_type: "project.classic_imported_v4",
                event_payload: &payload,
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(receipt)
    }
}

#[cfg(test)]
#[path = "sqlite_classic_import_v4/tests.rs"]
mod tests;
