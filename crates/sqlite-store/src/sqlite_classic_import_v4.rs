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
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ImportVersion {
    V4,
    V5,
}
impl ImportVersion {
    fn family(self) -> &'static str {
        match self {
            Self::V4 => FAMILY,
            Self::V5 => "import_classic_v5",
        }
    }
    fn result(self) -> &'static str {
        match self {
            Self::V4 => RESULT,
            Self::V5 => "classic_import_v5",
        }
    }
    fn event(self) -> &'static str {
        match self {
            Self::V4 => "project.classic_imported_v4",
            Self::V5 => "project.classic_imported_v5",
        }
    }
    fn schema(self) -> u32 {
        match self {
            Self::V4 => 4,
            Self::V5 => 5,
        }
    }
    fn analyze(
        self,
        input: &ClassicAnalyzeV4,
        resources: &ClassicImportResourcesV4,
    ) -> Result<classic_import_v4::PreparedClassicImportV4, EditorError> {
        match self {
            Self::V4 => classic_import_v4::analyze(input, resources),
            Self::V5 => opencut_editor_application::classic_import_v5::analyze(input, resources),
        }
    }
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
pub(super) fn resolve_version(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    input: &ClassicAnalyzeV4,
    version: ImportVersion,
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
        if version == ImportVersion::V5 {
            opencut_editor_application::classic_import_v5::validate_font_mapping(
                &ClassicFontRequestV4 {
                    family: mapping.classic_family.clone(),
                    font_weight: mapping.font_weight,
                    font_style: mapping.font_style,
                },
                &font,
            )?;
            // Generated identity is not sufficient. Its retained immutable proof
            // must exist and match this authorized fixed-revision resource.
            if let ResourceOriginV4::Generated { provider, .. } = &font.origin
                && opencut_editor_application::font_derived_v2::compat::is_provider(provider)
            {
                sqlite_font_derived_v1::read_source_for_font(connection, &font)?;
            }
        } else if font.family != mapping.classic_family
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
        resolve_version(
            &self.connection,
            &self.connection.1,
            input,
            ImportVersion::V4,
        )
    }
}
#[cfg(test)]
fn identity(input: &ClassicImportV4) -> Result<RequestIdentity, EditorError> {
    identity_version(input, ImportVersion::V4)
}
fn identity_version(
    input: &ClassicImportV4,
    version: ImportVersion,
) -> Result<RequestIdentity, EditorError> {
    if opencut_editor_application::classic_import::source_content_id(&input.source.content)
        != input.expected_source_content_id
    {
        return Err(invalid("Classic source content ID mismatch"));
    }
    request_hash(
        version.family(),
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
        self.classic_import_version(input, ImportVersion::V4)
    }
    pub(super) fn classic_import_version(
        &mut self,
        input: &ClassicImportV4,
        version: ImportVersion,
    ) -> Result<ClassicImportReceiptV4, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        require_idempotency_key(&input.idempotency_key)?;
        let identity = identity_version(input, version)?;
        if let Some(raw) = load_idempotent(
            &self.connection,
            version.family(),
            "workspace",
            &input.idempotency_key,
            &identity,
            version.result(),
        )? {
            return decode_json(&raw, "Classic V4 receipt");
        }
        let resources =
            resolve_version(&self.connection, &self.connection.1, &input.source, version)?;
        let prepared = version.analyze(&input.source, &resources)?;
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
            schema_version: version.schema(),
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
            version.family(),
            "workspace",
            &input.idempotency_key,
            &identity,
            version.result(),
        )? {
            return decode_json(&raw, "Classic V4 receipt");
        }
        let latest = resolve_version(&tx, &context, &input.source, version)?;
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
        if version == ImportVersion::V5 {
            for font in &document.fonts {
                if let ResourceOriginV4::Generated { provider, .. } = &font.origin
                    && opencut_editor_application::font_derived_v2::compat::is_provider(provider)
                {
                    let proof = sqlite_font_derived_v1::read_source_for_font(&tx, font)?;
                    sqlite_font_derived_v1::persist_verified_source(&tx, font, &proof)?;
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
                operation_family: version.family(),
                idempotency_key: &input.idempotency_key,
                request_digest: identity.current_digest(),
                result_kind: version.result(),
                result_json: &receipt_json,
                event_type: version.event(),
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
