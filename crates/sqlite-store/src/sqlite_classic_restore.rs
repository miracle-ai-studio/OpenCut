//! Direct Classic restoration from authorized immutable revision resources.
use super::*;
use opencut_api_contracts::classic_restore::*;
use opencut_contracts::scene::*;
use opencut_editor_application::{
    classic_restore::{self, ClassicResourceRead, ClassicResources, ResolvedClassicFont},
    scene::StoredProjectDocument,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

// Immutable request-family identity is private storage metadata,not an API selector.
const FAMILY: &str = "import_classic_v5";
const RESULT: &str = "classic_restore";
mod receipts;
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
pub(super) fn resolve_resources(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    input: &AnalyzeClassic,
) -> Result<ClassicResources, EditorError> {
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
        let source = sqlite_scene::load_source(
            connection,
            &mapping.source_project_id,
            Some(&mapping.source_revision),
        )?;
        let asset = match source.document {
            StoredProjectDocument::Editable(document) => document
                .assets
                .iter()
                .find(|a| a.id == mapping.asset_id)
                .cloned(),
            other => {
                let assets = match &other {
                    StoredProjectDocument::Legacy(d) => &d.assets,
                    StoredProjectDocument::HistoricalVisual(d) => &d.base.assets,
                    _ => unreachable!(),
                };
                assets
                    .iter()
                    .find(|a| a.id == mapping.asset_id)
                    .and_then(|a| {
                        sqlite_scene::resources::known_version(a)
                            .and_then(|v| sqlite_scene::resources::canonical_asset(a, &v))
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
        let source = sqlite_scene::load_source(
            connection,
            &mapping.source_project_id,
            Some(&mapping.source_revision),
        )?;
        let StoredProjectDocument::Editable(document) = source.document else {
            return Err(invalid("Classic font mapping requires a fixed V4 revision"));
        };
        let font = document
            .fonts
            .iter()
            .find(|f| f.id == mapping.font_id)
            .cloned()
            .ok_or_else(|| invalid("Mapped font is absent from fixed revision"))?;
        opencut_editor_application::classic_fonts::validate_font_mapping(
            &ClassicFontRequest {
                family: mapping.classic_family.clone(),
                font_weight: mapping.font_weight,
                font_style: mapping.font_style,
            },
            &font,
        )?;
        if let ResourceOrigin::Generated { provider, .. } = &font.origin
            && opencut_editor_application::font_derived::provenance::is_provider(provider)
        {
            sqlite_font_derivations::read_source_for_font(connection, &font)?;
        }
        // Original import established inspector admission. This preserves the complete
        // provenance/license record; it does not assert glyph coverage for new text.
        fonts.push(ResolvedClassicFont {
            request: ClassicFontRequest {
                family: mapping.classic_family.clone(),
                font_weight: mapping.font_weight,
                font_style: mapping.font_style,
            },
            font,
        });
    }
    Ok(ClassicResources { media, fonts })
}
impl ClassicResourceRead for EditorApplication {
    fn resolve_classic_resources(
        &self,
        input: &AnalyzeClassic,
    ) -> Result<ClassicResources, EditorError> {
        resolve_resources(&self.connection, &self.connection.1, input)
    }
}
fn identity(input: &ImportClassic) -> Result<RequestIdentity, EditorError> {
    receipts::identity(input, FAMILY)
}
impl EditorApplication {
    pub fn classic_restore_analyze(
        &self,
        input: &AnalyzeClassic,
    ) -> Result<ClassicReview, EditorError> {
        let resources = self.resolve_classic_resources(input)?;
        Ok(classic_restore::analyze(input, &resources)?.analysis)
    }

    pub fn classic_restore_import(
        &mut self,
        input: &ImportClassic,
    ) -> Result<ClassicRestoreReceipt, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        require_idempotency_key(&input.idempotency_key)?;
        let identity = identity(input)?;
        if let Some(receipt) = receipts::replay(&self.connection, &self.connection.1, input)? {
            return Ok(receipt);
        }
        let resources = resolve_resources(&self.connection, &self.connection.1, &input.source)?;
        let prepared = classic_restore::analyze(&input.source, &resources)?;
        let analysis = prepared.analysis;
        let document = prepared
            .document
            .ok_or_else(|| invalid("Classic restoration conversion has unresolved blockers"))?;
        if !analysis.blockers.is_empty()
            || analysis.source_content_id != input.expected_source_content_id
            || document.id != input.source.target_project_id
        {
            return Err(invalid(
                "Classic restoration conversion identity or blockers differ",
            ));
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
        opencut_editor_core::scene::validate_project_scene(&document)?;
        let revision = format!(
            "rev_{:x}",
            Sha256::digest(encode_json(&document, "Classic restoration project")?.as_bytes())
        );
        let receipt = ClassicRestoreReceipt {
            target_project_id: document.id.clone(),
            revision: revision.clone(),
            source_content_id: analysis.source_content_id,
            asset_count: document.assets.len(),
            font_count: document.fonts.len(),
            unresolved_fonts: analysis.unresolved_fonts,
            losses: analysis.losses,
        };
        let _cas =
            sqlite_scene::resources::verify_document(&self.asset_store_root, &document, None)?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_access::owner(&context)?;
        if let Some(receipt) = receipts::replay(&tx, &context, input)? {
            return Ok(receipt);
        }
        let latest = resolve_resources(&tx, &context, &input.source)?;
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
        {
            for font in &document.fonts {
                if let ResourceOrigin::Generated { provider, .. } = &font.origin
                    && opencut_editor_application::font_derived::provenance::is_provider(provider)
                {
                    let proof = sqlite_font_derivations::read_source_for_font(&tx, font)?;
                    sqlite_font_derivations::persist_verified_source(&tx, font, &proof)?;
                }
            }
        }
        let receipt_json = encode_json(&receipt, "Classic import receipt")?;
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
                event_type: "project.classic_imported",
                event_payload: &payload,
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(receipt)
    }
}

#[cfg(test)]
#[path = "sqlite_classic_restore/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "sqlite_classic_restore/font_proofs.rs"]
mod font_proofs;
