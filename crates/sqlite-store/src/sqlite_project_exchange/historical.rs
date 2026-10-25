//! Exact original request families and receipts remain private replay facts.
//! Normalization reads the published fixed revision, never today's main head.
use super::*;
use opencut_api_contracts::{
    historical_snapshot_package as snapshot, historical_visual_package as visual,
    project_archive as history, project_package as media,
};

pub(super) fn family(
    connection: &Connection,
    input: &ImportPackage,
) -> Result<Option<String>, EditorError> {
    let mut query=connection.prepare("SELECT operation_family,request_hash FROM idempotency WHERE scope='workspace' AND idempotency_key=?1 AND operation_family IN ('import_package','import_package_v3','import_package_v4','project_history_package_v1_import') ORDER BY operation_family").map_err(storage_error)?;
    let values = query
        .query_map([&input.idempotency_key], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(storage_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage_error)?;
    let mut matched = None;
    for (family, hash) in &values {
        let identity = request_hash(
            family,
            "workspace",
            &json!({"target_project_id":input.target_project_id,"manifest_sha256":input.expected_manifest_sha256}),
        )?;
        if identity.matches(hash, None) {
            if matched.is_some() {
                return Err(EditorError::new(
                    "IDEMPOTENCY_KEY_REUSED",
                    "The exact request matches multiple historical imports",
                ));
            }
            matched = Some(family.clone());
        }
    }
    if matched.is_none() && !values.is_empty() {
        return Err(EditorError::new(
            "IDEMPOTENCY_KEY_REUSED",
            "The key belongs to a different historical package request",
        ));
    }
    Ok(matched)
}
fn media_input(i: &ImportPackage) -> media::ImportPackage {
    media::ImportPackage {
        package_path: i.package_path.clone(),
        target_project_id: i.target_project_id.clone(),
        expected_manifest_sha256: i.expected_manifest_sha256.clone(),
        idempotency_key: i.idempotency_key.clone(),
    }
}
fn visual_input(i: &ImportPackage) -> visual::HistoricalVisualImport {
    visual::HistoricalVisualImport {
        package_path: i.package_path.clone(),
        target_project_id: i.target_project_id.clone(),
        expected_manifest_sha256: i.expected_manifest_sha256.clone(),
        idempotency_key: i.idempotency_key.clone(),
    }
}
fn snapshot_input(i: &ImportPackage) -> snapshot::HistoricalSnapshotImport {
    snapshot::HistoricalSnapshotImport {
        package_path: i.package_path.clone(),
        target_project_id: i.target_project_id.clone(),
        expected_manifest_sha256: i.expected_manifest_sha256.clone(),
        idempotency_key: i.idempotency_key.clone(),
    }
}
fn history_input(i: &ImportPackage) -> history::HistoricalArchiveImport {
    history::HistoricalArchiveImport {
        package_path: i.package_path.clone(),
        target_project_id: i.target_project_id.clone(),
        expected_manifest_sha256: i.expected_manifest_sha256.clone(),
        idempotency_key: i.idempotency_key.clone(),
    }
}

pub(super) fn replay(
    app: &EditorApplication,
    input: &ImportPackage,
) -> Result<Option<ImportReceipt>, EditorError> {
    let Some(kind) = family(&app.connection, input)? else {
        return Ok(None);
    };
    // Each original function checks its exact family, request digest, result
    // kind and stored bytes; none guesses a suffix or retries another decoder.
    let missing = || package::invalid("Historical import receipt disappeared during replay");
    let receipt = match kind.as_str() {
        "import_package" => from_media(
            app,
            app.package_import_replay(&media_input(input))?
                .ok_or_else(missing)?,
        )?,
        "import_package_v3" => from_visual(
            app,
            app.historical_visual_replay(&visual_input(input))?
                .ok_or_else(missing)?,
        )?,
        "import_package_v4" => from_snapshot(
            app,
            app.historical_snapshot_replay(&snapshot_input(input))?
                .ok_or_else(missing)?,
        )?,
        "project_history_package_v1_import" => from_history(
            app,
            app.historical_archive_replay(&history_input(input))?
                .ok_or_else(missing)?,
        )?,
        _ => return Err(package::invalid("Unknown historical import family")),
    };
    if receipt.target_project_id != input.target_project_id
        || receipt.package.manifest_sha256 != input.expected_manifest_sha256
    {
        return Err(package::invalid("Historical receipt identity differs"));
    }
    Ok(Some(receipt))
}
pub(super) fn commit(
    app: &mut EditorApplication,
    input: &ImportPackage,
    p: &PreparedImport,
) -> Result<ImportReceipt, EditorError> {
    let package = package::inspect_prepared(p)?;
    if package.manifest_sha256 != input.expected_manifest_sha256 {
        return Err(package::invalid(
            "Prepared historical package differs from original request",
        ));
    }
    match p {
        PreparedImport::Media(p) => {
            let ready = opencut_editor_application::project_package::prepare_import(
                p,
                &input.target_project_id,
            )?;
            let versions = versions(&ready.document.assets)?;
            publish_single(
                app,
                input,
                &ready.document,
                &ready.revision,
                package,
                vec![],
                &versions,
            )
        }
        PreparedImport::Visual(p) => {
            let document = opencut_editor_application::historical_visual_package::prepare_import(
                p,
                &input.target_project_id,
            )?;
            let revision = sqlite_historical_visual::revision_id(&document, None, None)?;
            let versions = versions(&document.base.assets)?;
            let mut plan = p.plugin_package_install_plan.clone();
            plan.extend(
                opencut_editor_application::historical_visual_package::plugin_grant_restore_plan(
                    &p.manifest.plugin_grants,
                    &input.target_project_id,
                    &p.manifest_sha256,
                )?,
            );
            publish_single(app, input, &document, &revision, package, plan, &versions)
        }
        PreparedImport::Snapshot(p) => {
            let document = opencut_editor_application::historical_snapshot_package::prepare_import(
                p,
                &input.target_project_id,
            )?;
            let revision = format!(
                "rev_{}",
                package::digest(encode_json(&document, "Restored package document")?.as_bytes())
            );
            let created_at = app
                .connection
                .query_row("SELECT CURRENT_TIMESTAMP", [], |r| r.get(0))
                .map_err(storage_error)?;
            let archive = ProjectArchive {
                schema_version: 1,
                source_project_id: input.target_project_id.clone(),
                main_head_revision: revision.clone(),
                revisions: vec![ArchivedRevision {
                    revision: revision.clone(),
                    parent_revision: None,
                    transaction_id: None,
                    document: document.into(),
                    created_at,
                }],
                transactions: vec![],
                branches: vec![],
                merges: vec![],
            };
            let receipt = ImportReceipt {
                target_project_id: input.target_project_id.clone(),
                revision,
                package,
                plugin_restore_plan: vec![],
            };
            publish_archive_import(app, input, &archive, &p.manifest.derived_fonts, receipt)
        }
        PreparedImport::History(p) => {
            let archive = package::prepare_historical_archive(p, &input.target_project_id)?;
            let receipt = ImportReceipt {
                target_project_id: input.target_project_id.clone(),
                revision: archive.main_head_revision.clone(),
                package,
                plugin_restore_plan: vec![],
            };
            publish_archive_import(app, input, &archive, &p.manifest.derived_fonts, receipt)
        }
        PreparedImport::Current(_) => Err(package::invalid(
            "Current package reached historical preparation",
        )),
    }
}
fn versions(
    assets: &[opencut_contracts::Asset],
) -> Result<Vec<opencut_contracts::AssetVersion>, EditorError> {
    assets
        .iter()
        .map(|a| {
            a.version
                .clone()
                .ok_or_else(|| package::invalid("Historical package resource is not pinned"))
        })
        .collect()
}
fn publish_single<T: sqlite_new_project::PublishedDocument>(
    app: &mut EditorApplication,
    input: &ImportPackage,
    document: &T,
    revision: &str,
    package: PackageInspection,
    plan: Vec<PluginGrantRestoreAction>,
    versions: &[opencut_contracts::AssetVersion],
) -> Result<ImportReceipt, EditorError> {
    let id = identity(input)?;
    let cas = opencut_asset_store::AssetStore::read_only(app.asset_store_root())?;
    for version in versions {
        cas.open_version_verified(version)?;
    }
    let context = app.connection.1.clone();
    let tx = app
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage_error)?;
    sqlite_access::owner(&context)?;
    if let Some(receipt) = super::replay(&tx, input, &id)? {
        return Ok(receipt);
    }
    if family(&tx, input)?.is_some() {
        return Err(EditorError::new(
            "IDEMPOTENCY_KEY_REUSED",
            "A historical request committed concurrently;retry the original request",
        ));
    }
    let receipt = ImportReceipt {
        target_project_id: input.target_project_id.clone(),
        revision: revision.into(),
        package,
        plugin_restore_plan: plan,
    };
    let raw = encode_json(&receipt, "Package import receipt")?;
    let payload = serde_json::to_value(&receipt)
        .map_err(|_| package::invalid("Cannot encode package publication"))?;
    sqlite_new_project::publish(
        &tx,
        &context,
        sqlite_new_project::NewProjectPublication {
            document,
            revision,
            operation_family: FAMILY,
            idempotency_key: &input.idempotency_key,
            request_digest: id.current_digest(),
            result_kind: RESULT,
            result_json: &raw,
            event_type: "project.package_imported",
            event_payload: &payload,
        },
    )?;
    tx.commit().map_err(storage_error)?;
    drop(cas);
    Ok(receipt)
}

fn fixed_inspection(
    app: &EditorApplication,
    project: &str,
    revision: &str,
    hash: &str,
    source: &str,
    source_revision: &str,
    mode: PackageMode,
) -> Result<PackageInspection, EditorError> {
    let raw: String = app
        .connection
        .query_row(
            "SELECT document_json FROM revisions WHERE project_id=?1 AND revision=?2",
            params![project, revision],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage_error)?
        .ok_or_else(|| {
            package::invalid("Original published revision is unavailable for receipt normalization")
        })?;
    use sqlite_project_document::ProjectStorageDocument as D;
    let (name, assets, fonts, proofs, editable) =
        match sqlite_project_document::decode_stored_project(&raw)? {
            D::Legacy(d) => (d.name, d.assets.len(), 0, 0, false),
            D::HistoricalVisual(d) => (d.base.name, d.base.assets.len(), 0, 0, false),
            D::Editable(d) => {
                let proofs = d
                    .fonts
                    .iter()
                    .filter_map(|f| match &f.origin {
                        ResourceOrigin::Generated {
                            provider,
                            generation_id,
                        } if opencut_editor_application::font_derived::provenance::is_provider(
                            provider,
                        ) =>
                        {
                            Some(generation_id)
                        }
                        _ => None,
                    })
                    .collect::<std::collections::BTreeSet<_>>()
                    .len();
                (d.name, d.assets.len(), d.fonts.len(), proofs, true)
            }
            D::ReadOnly(_) => {
                return Err(package::invalid(
                    "Historical package receipt points to preserved unsupported content",
                ));
            }
        };
    Ok(PackageInspection {
        manifest_sha256: hash.into(),
        source_project_id: source.into(),
        source_revision: source_revision.into(),
        project_name: name,
        authoring_available: editable,
        mode,
        scope: PackageScope::SavedRevision,
        revision_count: 1,
        transaction_count: 0,
        branch_count: 0,
        merge_count: 0,
        asset_version_count: assets,
        font_version_count: fonts,
        derived_font_count: proofs,
        plugin_bundle_count: 0,
        external_font_families: vec![],
        fonts_missing: vec![],
        tool_locks: vec![],
        tool_environment_matches: false,
        tool_lock_mismatches: vec!["ffmpeg".into(), "ffprobe".into()],
    })
}
fn mode(self_contained: bool) -> PackageMode {
    if self_contained {
        PackageMode::SelfContained
    } else {
        PackageMode::Linked
    }
}
fn from_media(
    app: &EditorApplication,
    r: media::ImportReceipt,
) -> Result<ImportReceipt, EditorError> {
    let mut package = fixed_inspection(
        app,
        &r.target_project_id,
        &r.revision,
        &r.manifest_sha256,
        &r.source_project_id,
        &r.source_revision,
        mode(r.self_contained_media),
    )?;
    package.external_font_families = r.dependencies.fonts_not_bundled;
    Ok(ImportReceipt {
        target_project_id: r.target_project_id,
        revision: r.revision,
        package,
        plugin_restore_plan: vec![],
    })
}
fn from_visual(
    app: &EditorApplication,
    r: visual::ImportReceipt,
) -> Result<ImportReceipt, EditorError> {
    let mut package = fixed_inspection(
        app,
        &r.target_project_id,
        &r.revision,
        &r.manifest_sha256,
        &r.source_project_id,
        &r.source_revision,
        mode(r.self_contained_media),
    )?;
    package.external_font_families = r.dependencies.fonts_not_bundled;
    package.fonts_missing = r.fonts_missing;
    package.tool_locks = r.tool_locks;
    package.tool_environment_matches = r.tool_environment_matches;
    package.tool_lock_mismatches = r.tool_lock_mismatches;
    package.plugin_bundle_count = r
        .plugin_grant_restore_plan
        .iter()
        .filter(|p| matches!(p, PluginGrantRestoreAction::Install(_)))
        .count();
    Ok(ImportReceipt {
        target_project_id: r.target_project_id,
        revision: r.revision,
        package,
        plugin_restore_plan: r.plugin_grant_restore_plan,
    })
}
fn from_snapshot(
    app: &EditorApplication,
    r: snapshot::HistoricalSnapshotImportReceipt,
) -> Result<ImportReceipt, EditorError> {
    let package = fixed_inspection(
        app,
        &r.target_project_id,
        &r.revision,
        &r.manifest_sha256,
        &r.source_project_id,
        &r.source_revision,
        PackageMode::SelfContained,
    )?;
    if (
        package.asset_version_count,
        package.font_version_count,
        package.derived_font_count,
    ) != (r.asset_count, r.font_count, r.derived_font_count)
    {
        return Err(package::invalid(
            "Historical snapshot receipt differs from original published content",
        ));
    }
    Ok(ImportReceipt {
        target_project_id: r.target_project_id,
        revision: r.revision,
        package,
        plugin_restore_plan: vec![],
    })
}
fn from_history(
    app: &EditorApplication,
    r: history::HistoricalArchiveImportReceipt,
) -> Result<ImportReceipt, EditorError> {
    let m = r.package;
    let mut package = fixed_inspection(
        app,
        &r.target_project_id,
        &r.revision,
        &m.manifest_sha256,
        &m.source_project_id,
        &m.source_revision,
        PackageMode::SelfContained,
    )?;
    if package.project_name != m.project_name {
        return Err(package::invalid(
            "Historical archive receipt differs from original main head",
        ));
    }
    package.scope = PackageScope::History;
    package.revision_count = m.revision_count;
    package.transaction_count = m.transaction_count;
    package.branch_count = m.branch_count;
    package.merge_count = m.merge_count;
    package.asset_version_count = m.asset_version_count;
    package.font_version_count = m.font_version_count;
    package.derived_font_count = m.derived_font_count;
    Ok(ImportReceipt {
        target_project_id: r.target_project_id,
        revision: r.revision,
        package,
        plugin_restore_plan: vec![],
    })
}
