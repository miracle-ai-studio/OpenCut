//! Exact original receipt readers. No historical publisher or format selector.
use super::*;
use opencut_api_contracts::classic_restore::CompatibilityFinding;
use serde::Deserialize;

pub(super) fn identity(
    input: &ImportClassic,
    family: &str,
) -> Result<RequestIdentity, EditorError> {
    if opencut_editor_application::classic_restore::source_content_id(&input.source.content)
        != input.expected_source_content_id
    {
        return Err(invalid("Classic source content ID mismatch"));
    }
    request_hash(
        family,
        "workspace",
        &serde_json::to_value(input).map_err(|_| {
            EditorError::new(
                "INTERNAL_ERROR",
                "Classic restoration request cannot be serialized",
            )
        })?,
    )
}

fn original_identity(
    input: &ImportClassic,
    family: &str,
) -> Result<Option<RequestIdentity>, EditorError> {
    if matches!(family, "import_classic_v4" | "import_classic_v5") {
        return identity(input, family).map(Some);
    }
    if !input.source.font_mappings.is_empty() {
        return Ok(None);
    }
    // These two old publishers hashed a sorted mapping projection rather than
    // the complete request. Keep that exact original identity algorithm.
    let mut mappings = input.source.media_mappings.clone();
    mappings.sort_by(|a, b| a.classic_media_id.cmp(&b.classic_media_id));
    request_hash(
        family,
        "workspace",
        &json!({
            "source_content_id":input.expected_source_content_id,
            "target_project_id":input.source.target_project_id,
            "media_mappings":mappings,
            "accepted_loss_codes":input.accepted_loss_codes
        }),
    )
    .map(Some)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoricalFontReceipt {
    schema_version: u32,
    target_project_id: String,
    revision: String,
    source_content_id: String,
    asset_count: usize,
    font_count: usize,
    unresolved_fonts: Vec<ClassicFontRequest>,
    losses: Vec<CompatibilityFinding>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoricalLibraryReceipt {
    target_project_id: String,
    revision: String,
    source_content_id: String,
    asset_count: usize,
    losses: Vec<CompatibilityFinding>,
}

fn decode(family: &str, kind: &str, raw: &str) -> Result<ClassicRestoreReceipt, EditorError> {
    match (family, kind) {
        (FAMILY, RESULT) => decode_json(raw, "Classic restoration receipt"),
        ("import_classic_v4", "classic_import_v4") | ("import_classic_v5", "classic_import_v5") => {
            let stored: HistoricalFontReceipt = decode_json(raw, "Original Classic font receipt")?;
            let expected = if family == "import_classic_v4" { 4 } else { 5 };
            if stored.schema_version != expected {
                return Err(EditorError::new(
                    "STORAGE_ERROR",
                    "Original Classic receipt format differs from its recorded kind",
                ));
            }
            Ok(ClassicRestoreReceipt {
                target_project_id: stored.target_project_id,
                revision: stored.revision,
                source_content_id: stored.source_content_id,
                asset_count: stored.asset_count,
                font_count: stored.font_count,
                unresolved_fonts: stored.unresolved_fonts,
                losses: stored.losses,
            })
        }
        ("import_classic", "classic_import")
        | ("import_classic_recovery_v1", "classic_recovery_v1") => {
            let stored: HistoricalLibraryReceipt =
                decode_json(raw, "Original Classic library receipt")?;
            Ok(ClassicRestoreReceipt {
                target_project_id: stored.target_project_id,
                revision: stored.revision,
                source_content_id: stored.source_content_id,
                asset_count: stored.asset_count,
                font_count: 0,
                unresolved_fonts: vec![],
                losses: stored.losses,
            })
        }
        _ => Err(EditorError::new(
            "STORAGE_ERROR",
            "Classic receipt kind differs from its original operation",
        )),
    }
}

pub(super) fn replay(
    connection: &Connection,
    context: &opencut_editor_application::audit::ExecutionContext,
    input: &ImportClassic,
) -> Result<Option<ClassicRestoreReceipt>, EditorError> {
    sqlite_access::owner(context)?;
    require_idempotency_key(&input.idempotency_key)?;
    identity(input, FAMILY)?;
    let collision: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM idempotency WHERE scope='workspace' AND idempotency_key=?1 AND (operation_family GLOB 'project_v4_*' OR operation_family GLOB 'project_v5_*'))",
        [&input.idempotency_key], |row| row.get(0),
    ).map_err(storage_error)?;
    if collision {
        return Err(EditorError::new(
            "IDEMPOTENCY_KEY_REUSED",
            "Classic key was already used for a different project operation",
        ));
    }
    let mut statement = connection.prepare(
        "SELECT operation_family,request_hash,result_kind,result_json FROM idempotency WHERE scope='workspace' AND idempotency_key=?1 AND operation_family IN ('import_classic','import_classic_recovery_v1','import_classic_v4','import_classic_v5')",
    ).map_err(storage_error)?;
    let rows = statement
        .query_map([&input.idempotency_key], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(storage_error)?;
    let mut found = None;
    let mut any = false;
    for row in rows {
        let (family, hash, kind, raw) = row.map_err(storage_error)?;
        any = true;
        let Some(identity) = original_identity(input, &family)? else {
            continue;
        };
        if !identity.matches(&hash, None) {
            continue;
        }
        let receipt = decode(&family, &kind, &raw)?;
        if receipt.target_project_id != input.source.target_project_id
            || receipt.source_content_id != input.expected_source_content_id
            || receipt.revision.trim().is_empty()
            || found.is_some()
        {
            return Err(EditorError::new(
                "STORAGE_ERROR",
                "Classic receipt scope or immutable identity is inconsistent",
            ));
        }
        found = Some(receipt);
    }
    if any && found.is_none() {
        return Err(EditorError::new(
            "IDEMPOTENCY_KEY_REUSED",
            "Classic key was already used for a different request",
        ));
    }
    Ok(found)
}
