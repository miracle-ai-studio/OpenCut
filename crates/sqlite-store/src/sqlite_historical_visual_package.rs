//! Owner-only fixed schema 3 package source and atomic target publication.
use super::*;
use opencut_api_contracts::historical_visual_package::{
    HistoricalVisualImport, ImportReceipt, PackagePluginGrant, PackageToolLock,
    PreparedHistoricalVisual,
};
use opencut_editor_application::historical_visual_package::{self, HistoricalVisualSource};

fn decode_import_receipt(raw: &str) -> Result<ImportReceipt, EditorError> {
    let mut value: serde_json::Value = decode_json(raw, "schema 3 package import receipt")?;
    // Only genuinely older base receipts lack these fields. A malformed current
    // receipt must never downgrade to a projection that discards restore actions.
    let historical_base = value.as_object().is_some_and(|object| {
        [
            "font_environment_checked",
            "fonts_missing",
            "tool_locks",
            "tool_environment_matches",
            "tool_lock_mismatches",
        ]
        .iter()
        .all(|key| !object.contains_key(*key))
    });
    if let Some(object) = value.as_object_mut() {
        // The public receipt always includes this field; only rows written before schema 4 omit it.
        object
            .entry("plugin_grant_restore_plan")
            .or_insert_with(|| json!([]));
    }
    if let Some(actions) = value
        .get_mut("plugin_grant_restore_plan")
        .and_then(serde_json::Value::as_array_mut)
    {
        for action in actions {
            let tool = action.get("tool").and_then(serde_json::Value::as_str);
            let input = action
                .get("input")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let retained = match tool {
                Some("opencut_plugin_install_pinned") => Some(
                    serde_json::from_value::<
                        opencut_api_contracts::plugin_packages::PinnedPluginPackage,
                    >(input)
                    .map_err(|_| {
                        EditorError::new(
                            "STORAGE_ERROR",
                            "Invalid retained pinned WASM restore action",
                        )
                    })?,
                ),
                Some("opencut_plugin_v2_install_pinned") => {
                    let old: opencut_api_contracts::presets::InstallPinnedInput =
                        serde_json::from_value(input).map_err(|_| {
                            EditorError::new(
                                "STORAGE_ERROR",
                                "Invalid retained pinned preset restore action",
                            )
                        })?;
                    Some(
                        opencut_api_contracts::plugin_packages::PinnedPluginPackage {
                            package_root: old.package_root,
                            granted_permissions: vec![],
                            expected_package_sha256: old.expected_package_sha256,
                        },
                    )
                }
                _ => None,
            };
            if let Some(input) = retained {
                // Normalize only the returned projection. The original raw
                // receipt/journal remains untouched and the required pin stays.
                action["tool"] = json!("opencut_plugin_install");
                action["input"] = serde_json::to_value(input).map_err(|_| {
                    EditorError::new("STORAGE_ERROR", "Invalid retained restore action")
                })?;
            }
        }
    }
    if let Ok(receipt) = serde_json::from_value::<ImportReceipt>(value) {
        return Ok(receipt);
    }
    if !historical_base {
        return Err(EditorError::new(
            "STORAGE_ERROR",
            "Invalid retained package import receipt",
        ));
    }
    let legacy: opencut_api_contracts::project_package::ImportReceipt =
        decode_json(raw, "legacy schema 3 package import receipt")?;
    Ok(ImportReceipt {
        target_project_id: legacy.target_project_id,
        revision: legacy.revision,
        manifest_sha256: legacy.manifest_sha256,
        source_project_id: legacy.source_project_id,
        source_revision: legacy.source_revision,
        font_environment_checked: false,
        fonts_missing: Vec::new(),
        tool_lock_mismatches: legacy.dependencies.tools_not_locked.clone(),
        dependencies: legacy.dependencies,
        tool_locks: Vec::<PackageToolLock>::new(),
        tool_environment_matches: false,
        self_contained_media: legacy.self_contained_media,
        fully_reproducible: legacy.fully_reproducible,
        plugin_grant_restore_plan: Vec::new(),
    })
}

impl EditorApplication {
    #[cfg(test)]
    pub fn historical_visual_source(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<HistoricalVisualSource, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(storage_error)?;
        let snapshot = sqlite_historical_visual::load_historical_visual_snapshot(
            &tx,
            project_id,
            Some(revision),
        )?;
        let mut statement = tx.prepare(
            "SELECT grant_json FROM project_plugin_grants WHERE project_id=?1 ORDER BY plugin_id,version",
        ).map_err(storage_error)?;
        let planner_grants = statement
            .query_map([project_id], |row| row.get::<_, String>(0))
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        drop(statement);
        let mut plugin_grants = planner_grants
            .iter()
            .map(|raw| {
                let grant: opencut_api_contracts::project_plugins::Grant =
                    decode_json(raw, "project plugin grant")?;
                Ok(PackagePluginGrant::PlannerV1 {
                    id: grant.target.id,
                    version: grant.target.version,
                    package_sha256: grant.package_sha256,
                    operations: grant.operations,
                    enabled: grant.enabled,
                })
            })
            .collect::<Result<Vec<_>, EditorError>>()?;
        let mut v2_statement = tx.prepare(
            "SELECT grant_json FROM project_preset_grants WHERE project_id=?1 ORDER BY plugin_id,version",
        ).map_err(storage_error)?;
        let v2 = v2_statement
            .query_map([project_id], |row| row.get::<_, String>(0))
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?;
        for raw in v2 {
            let grant: opencut_api_contracts::presets::Grant = decode_json(&raw, "preset grant")?;
            plugin_grants.push(PackagePluginGrant::PresetV2 {
                id: grant.target.id,
                version: grant.target.version,
                package_sha256: grant.package_sha256,
                preset_names: grant.preset_names,
                enabled: grant.enabled,
            });
        }
        drop(v2_statement);
        plugin_grants.sort_by_key(|grant| match grant {
            PackagePluginGrant::PlannerV1 { id, version, .. } => {
                format!("v1:{id}@{version}")
            }
            PackagePluginGrant::PresetV2 { id, version, .. } => {
                format!("v2:{id}@{version}")
            }
        });
        tx.commit().map_err(storage_error)?;
        Ok(HistoricalVisualSource {
            snapshot,
            plugin_grants,
        })
    }

    pub fn historical_visual_replay(
        &self,
        input: &HistoricalVisualImport,
    ) -> Result<Option<ImportReceipt>, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
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
        let identity = request_hash("import_package_v3", "workspace", &identity_value)?;
        load_idempotent(
            &self.connection,
            "import_package_v3",
            "workspace",
            &input.idempotency_key,
            &identity,
            "package_import_v3",
        )?
        .map(|stored| decode_import_receipt(&stored))
        .transpose()
    }

    /// CAS capture happens before this transaction. Failed publication can only
    /// leave collectable unreferenced bytes, never a partial project or receipt.
    #[cfg(test)]
    pub fn historical_visual_fixture_commit(
        &mut self,
        input: &HistoricalVisualImport,
        prepared: &PreparedHistoricalVisual,
    ) -> Result<ImportReceipt, EditorError> {
        sqlite_access::owner(&self.connection.1)?;
        require_idempotency_key(&input.idempotency_key)?;
        if input.expected_manifest_sha256 != prepared.manifest_sha256 {
            return Err(EditorError::new(
                "PACKAGE_INVALID",
                "Prepared manifest identity differs from request",
            ));
        }
        historical_visual_package::validate_manifest(&prepared.manifest)?;
        let document =
            historical_visual_package::prepare_import(prepared, &input.target_project_id)?;
        let revision = sqlite_historical_visual::revision_id(&document, None, None)?;
        let identity_value = json!({"target_project_id":input.target_project_id,
            "manifest_sha256":prepared.manifest_sha256});
        let identity = request_hash("import_package_v3", "workspace", &identity_value)?;
        let context = self.connection.1.clone();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        sqlite_access::owner(&context)?;
        if let Some(stored) = load_idempotent(
            &tx,
            "import_package_v3",
            "workspace",
            &input.idempotency_key,
            &identity,
            "package_import_v3",
        )? {
            return decode_import_receipt(&stored);
        }
        let receipt = ImportReceipt {
            target_project_id: input.target_project_id.clone(),
            revision: revision.clone(),
            manifest_sha256: prepared.manifest_sha256.clone(),
            source_project_id: prepared.manifest.source_project_id.clone(),
            source_revision: prepared.manifest.source_revision.clone(),
            dependencies: prepared.manifest.dependencies.clone(),
            font_environment_checked: prepared.font_environment_checked,
            fonts_missing: prepared.fonts_missing.clone(),
            tool_locks: prepared.manifest.tool_locks.clone(),
            tool_environment_matches: prepared.tool_environment_matches,
            tool_lock_mismatches: prepared.tool_lock_mismatches.clone(),
            plugin_grant_restore_plan: if prepared.manifest.schema_version == 5 {
                let mut plan = prepared.plugin_package_install_plan.clone();
                plan.extend(historical_visual_package::plugin_grant_restore_plan(
                    &prepared.manifest.plugin_grants,
                    &input.target_project_id,
                    &prepared.manifest_sha256,
                )?);
                plan
            } else if prepared.manifest.schema_version == 4 {
                historical_visual_package::plugin_grant_restore_plan(
                    &prepared.manifest.plugin_grants,
                    &input.target_project_id,
                    &prepared.manifest_sha256,
                )?
            } else {
                Vec::new()
            },
            self_contained_media: prepared.manifest.self_contained_media,
            fully_reproducible: prepared.manifest.fully_reproducible,
        };
        let receipt_json = encode_json(&receipt, "schema 3 package import receipt")?;
        let payload = json!({"project_id":input.target_project_id,"revision":revision,
            "source_project_id":prepared.manifest.source_project_id,
            "source_revision":prepared.manifest.source_revision,
            "manifest_sha256":prepared.manifest_sha256});
        sqlite_new_project::publish(
            &tx,
            &context,
            sqlite_new_project::NewProjectPublication {
                document: &document,
                revision: &revision,
                operation_family: "import_package_v3",
                idempotency_key: &input.idempotency_key,
                request_digest: identity.current_digest(),
                result_kind: "package_import_v3",
                result_json: &receipt_json,
                event_type: "project.package_v3_imported",
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
    use opencut_api_contracts::project_package::PackageDependencies;

    #[test]
    fn old_persisted_schema3_receipt_replays_as_explicitly_unlocked() {
        let legacy = opencut_api_contracts::project_package::ImportReceipt {
            target_project_id: "restored".into(),
            revision: "rev_new".into(),
            manifest_sha256: "a".repeat(64),
            source_project_id: "source".into(),
            source_revision: "rev_source".into(),
            dependencies: PackageDependencies {
                fonts_not_bundled: Vec::new(),
                plugins_not_bundled: Vec::new(),
                tools_not_locked: vec!["ffmpeg".into(), "ffprobe".into()],
                unsupported: Vec::new(),
            },
            self_contained_media: true,
            fully_reproducible: false,
        };
        let raw = serde_json::to_string(&legacy).unwrap();
        let restored = decode_import_receipt(&raw).unwrap();
        assert!(!restored.font_environment_checked);
        assert!(restored.fonts_missing.is_empty());
        assert!(!restored.tool_environment_matches);
        assert_eq!(
            restored.tool_lock_mismatches,
            vec!["ffmpeg".to_string(), "ffprobe".to_string()]
        );
        assert!(restored.tool_locks.is_empty());
    }

    #[test]
    fn intermediate_schema3_receipt_replays_without_a_grant_restore_plan() {
        let receipt = ImportReceipt {
            target_project_id: "restored".into(),
            revision: "rev_new".into(),
            manifest_sha256: "a".repeat(64),
            source_project_id: "source".into(),
            source_revision: "rev_source".into(),
            dependencies: PackageDependencies {
                fonts_not_bundled: vec!["Arial".into()],
                plugins_not_bundled: vec!["org.example.plugin@1.0.0".into()],
                tools_not_locked: Vec::new(),
                unsupported: Vec::new(),
            },
            font_environment_checked: true,
            fonts_missing: Vec::new(),
            tool_locks: Vec::new(),
            tool_environment_matches: true,
            tool_lock_mismatches: Vec::new(),
            plugin_grant_restore_plan: Vec::new(),
            self_contained_media: true,
            fully_reproducible: false,
        };
        let mut stored = serde_json::to_value(receipt).unwrap();
        stored
            .as_object_mut()
            .unwrap()
            .remove("plugin_grant_restore_plan");
        let restored = decode_import_receipt(&stored.to_string()).unwrap();
        assert!(restored.plugin_grant_restore_plan.is_empty());
        assert_eq!(
            restored.dependencies.plugins_not_bundled,
            ["org.example.plugin@1.0.0"]
        );
        assert!(restored.font_environment_checked);
        for tool in [
            "opencut_plugin_install_pinned",
            "opencut_plugin_v2_install_pinned",
        ] {
            let mut historical = stored.clone();
            historical["plugin_grant_restore_plan"] = json!([{ "tool":tool,"input":{
              "package_root":"/synthetic/retained-bundle", "expected_package_sha256":"b".repeat(64)
            }}]);
            if tool == "opencut_plugin_install_pinned" {
                historical["plugin_grant_restore_plan"][0]["input"]["granted_permissions"] =
                    json!([]);
            }
            let raw = historical.to_string();
            let decoded = decode_import_receipt(&raw).unwrap();
            let opencut_api_contracts::historical_visual_package::PluginGrantRestoreAction::Install(
                input,
            ) = &decoded.plugin_grant_restore_plan[0]
            else {
                panic!("expected one current install action")
            };
            assert_eq!(input.expected_package_sha256, "b".repeat(64));
            assert!(input.granted_permissions.is_empty());
            let mut missing_pin = historical.clone();
            missing_pin["plugin_grant_restore_plan"][0]["input"]
                .as_object_mut()
                .unwrap()
                .remove("expected_package_sha256");
            assert_eq!(
                decode_import_receipt(&missing_pin.to_string())
                    .unwrap_err()
                    .code,
                "STORAGE_ERROR"
            );
            // Current actions also cannot be silently downgraded to old empty plans.
            missing_pin["plugin_grant_restore_plan"][0]["tool"] = json!("opencut_plugin_install");
            assert_eq!(
                decode_import_receipt(&missing_pin.to_string())
                    .unwrap_err()
                    .code,
                "STORAGE_ERROR"
            );
        }
    }
}
