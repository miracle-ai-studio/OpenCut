//! Unified discovery plus strictly selected static V3 import. Derived imports
//! keep their versioned runtime/proof operations and the same durable recovery.
use super::*;
use opencut_contracts::{font_catalog_v3::*, font_import_v1::ImportFontV1};
use opencut_editor_application::{
    font_catalog_v3 as app,
    font_import_v1::{FontImportPolicyV1, catalog::FontFetchPolicyV1},
    project_v4::{ProjectV4Read, V4Intent},
};

fn policy() -> FontFetchPolicyV1 {
    FontFetchPolicyV1 {
        allow_public_font_download: std::env::var("OPENCUT_ALLOW_PUBLIC_FONT_DOWNLOAD").as_deref()
            == Ok("1"),
        max_bytes: app::MAX_CATALOG_FONT_BYTES_V3,
        ..FontFetchPolicyV1::default()
    }
}
struct Availability {
    download: bool,
    synthesis_cache: Option<std::path::PathBuf>,
    v1: Result<
        opencut_native_services::font_derived_v1::NativeConfiguredDerivedFontSourceV1,
        EditorError,
    >,
    v2: Result<
        opencut_native_services::font_derived_v2::NativeConfiguredDerivedFontSourceV2,
        EditorError,
    >,
}
impl Availability {
    fn new(derived_v1: bool, derived_v2: bool) -> Self {
        let not_selected = || {
            EditorError::new(
                "FONT_DERIVATION_UNAVAILABLE",
                "This source does not select a derivation runtime",
            )
        };
        Self {
            download: policy().allow_public_font_download,
            synthesis_cache: std::env::var_os("OPENCUT_FONT_DERIVE_CACHE")
                .filter(|value| !value.is_empty())
                .map(Into::into),
            v1: if derived_v1 {
                super::font_derived_v1::configured().and_then(|runtime| {
                    runtime.availability()?;
                    Ok(runtime)
                })
            } else {
                Err(not_selected())
            },
            v2: if derived_v2 {
                super::font_derived_v2::configured().and_then(|runtime| {
                    runtime.availability()?;
                    Ok(runtime)
                })
            } else {
                Err(not_selected())
            },
        }
    }
    fn attach(&self, face: &mut FontCatalogFaceV3) -> Result<(), EditorError> {
        let offline = || {
            "Public downloads are disabled and this exact output has no local cache candidate; cached bytes are verified again during import.".to_owned()
        };
        face.unavailable_reason = match face.kind {
            FontCatalogKindV3::StaticV1 | FontCatalogKindV3::StaticV3 => {
                (!self.download).then(|| "Public font downloads are disabled by this host.".into())
            }
            FontCatalogKindV3::DerivedV1 => match &self.v1 {
                Err(error) => Some(error.message.clone()),
                Ok(runtime) => (!self.download
                    && !runtime.cached_candidate(
                        &opencut_editor_application::font_derived_v1::source(&face.id)?,
                    ))
                .then(offline),
            },
            FontCatalogKindV3::DerivedV2 => match &self.v2 {
                Err(error) => Some(error.message.clone()),
                Ok(runtime) => (!self.download
                    && !runtime.cached_candidate(
                        &opencut_editor_application::font_derived_v2::source(&face.id)?,
                    ))
                .then(offline),
            },
            FontCatalogKindV3::SynthesisV1 => {
                use opencut_editor_application::font_synthesis_v1::{
                    self as synthesis, SynthesisBaseSourceV1,
                };
                let source = synthesis::source(&face.id)?;
                if opencut_native_services::font_synthesis_v1::cached_candidate(
                    &source,
                    self.synthesis_cache.as_deref(),
                ) {
                    None // Output-cache hints do not require a Python runtime.
                } else {
                    match synthesis::base_source(&face.id)? {
                    SynthesisBaseSourceV1::StaticV1(_)|SynthesisBaseSourceV1::StaticV3(_) => (!self.download)
                        .then(||"Public downloads are disabled and this exact normal source has no local cache candidate; import rechecks source bytes before recording explicit italic synthesis.".into()),
                    SynthesisBaseSourceV1::DerivedV1(_) => match &self.v1 {Err(error)=>Some(error.message.clone()),Ok(_)=>self.synthesis_input_unavailable(&source)},
                    SynthesisBaseSourceV1::DerivedV2(_) => match &self.v2 {Err(error)=>Some(error.message.clone()),Ok(_)=>self.synthesis_input_unavailable(&source)},
                }
                }
            }
        };
        face.import_available = face.unavailable_reason.is_none();
        Ok(())
    }
    // Reached only after the selected original runtime passed availability.
    // A raw source hint cannot replace that runtime or become an output hint.
    fn synthesis_input_unavailable(
        &self,
        source: &opencut_contracts::font_synthesis_v1::FontSynthesisSourceV1,
    ) -> Option<String> {
        (!self.download && !opencut_native_services::font_synthesis_v1::base_input_cached_candidate(
            source, self.synthesis_cache.as_deref()))
            .then(|| "Public downloads are disabled and this fixed base input has no local cache candidate; import verifies the source and derived output bytes.".into())
    }
}
pub(super) fn catalog() -> Result<FontCatalogV3, EditorError> {
    let availability = Availability::new(true, true);
    let allowed = availability.download;
    let metadata =
        opencut_editor_application::font_import_v1::browser_catalog::font_browser_catalog_v2();
    let mut fonts = app::catalog()?;
    for face in &mut fonts {
        availability.attach(face)?;
    }
    Ok(FontCatalogV3 {
        schema_version: 3,
        download_available: allowed,
        unavailable_reason: (!allowed)
            .then(|| "Public font downloads are disabled by this host.".into()),
        source_commit: metadata.source_commit.clone(),
        classic_atlas_sha256: metadata.classic_atlas_sha256.clone(),
        classic_family_count: metadata.classic_family_count as u32,
        fonts,
    })
}
pub(super) fn source(request: &FontSourceRequestV3) -> Result<FontSourceV3, EditorError> {
    let mut source = app::source(request)?;
    let cache = std::env::var_os("OPENCUT_FONT_DERIVE_CACHE")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from);
    let (v1, v2) = source_runtime_selection(&source, cache.as_deref())?;
    Availability::new(v1, v2).attach(&mut source.face)?;
    Ok(source)
}
fn source_runtime_selection(
    source: &FontSourceV3,
    cache: Option<&std::path::Path>,
) -> Result<(bool, bool), EditorError> {
    // A static synthesis or cached derivative never probes Python. A missing
    // derivative probes only the selected original base version.
    let (mut v1, mut v2) = (
        source.face.kind == FontCatalogKindV3::DerivedV1,
        source.face.kind == FontCatalogKindV3::DerivedV2,
    );
    if let FontSourceDetailsV3::SynthesisV1(synthetic) = &source.details {
        use opencut_editor_application::font_synthesis_v1::{
            self as synthesis, SynthesisBaseSourceV1,
        };
        if !opencut_native_services::font_synthesis_v1::cached_candidate(synthetic, cache) {
            match synthesis::base_source(&synthetic.source.face.id)? {
                SynthesisBaseSourceV1::DerivedV1(_) => v1 = true,
                SynthesisBaseSourceV1::DerivedV2(_) => v2 = true,
                _ => {}
            }
        }
    }
    Ok((v1, v2))
}
pub(super) fn import(
    host: &mut Automation,
    request: &ImportFontV1,
) -> Result<opencut_contracts::TransactionResult, EditorError> {
    app::validate_import_request_v3(request)?;
    host.app.authorize_v4(V4Intent::FontImport(request))?;
    let policy = policy();
    let mut authority = EditorApplication::open_managed(&host.database)?;
    authority.set_execution_context(host.app.execution_context().clone());
    let started = std::time::Instant::now();
    let mut checkpoint = || {
        authority.authorize_v4(V4Intent::FontImport(request))?;
        if started.elapsed() > std::time::Duration::from_millis(policy.deadline_ms) {
            return Err(EditorError::new(
                "FONT_SOURCE_TIMEOUT",
                "Font import exceeded its host deadline",
            ));
        }
        Ok(())
    };
    let publisher = opencut_asset_store::font_import_v1::ManagedFontPublisherV1::new(
        host.app.asset_store_root(),
    );
    app::import_static_font_v3(
        &mut host.app,
        &opencut_native_services::font_catalog_v3::GoogleFontsFetchV3,
        &opencut_native_services::font_import_v2::CatalogFontInspectorV2,
        &publisher,
        request,
        &policy,
        &FontImportPolicyV1 {
            max_font_bytes: app::MAX_CATALOG_FONT_BYTES_V3,
            ..FontImportPolicyV1::default()
        },
        &mut checkpoint,
    )
}

#[cfg(test)]
mod synthesis_availability_tests {
    use super::*;

    #[test]
    fn synthesis_source_availability_uses_acquisition_policy_not_python_runtime() {
        let source = opencut_editor_application::font_synthesis_v1::catalog()
            .unwrap()
            .remove(0);
        let mut face = app::source(&FontSourceRequestV3 {
            catalog_id: source.source.face.id,
            kind: FontCatalogKindV3::SynthesisV1,
        })
        .unwrap()
        .face;
        let unavailable = || {
            EditorError::new(
                "FONT_DERIVATION_UNAVAILABLE",
                "Python must not govern a synthetic face",
            )
        };
        let mut availability = Availability {
            download: true,
            synthesis_cache: None,
            v1: Err(unavailable()),
            v2: Err(unavailable()),
        };
        availability.attach(&mut face).unwrap();
        assert!(face.import_available);
        assert!(face.unavailable_reason.is_none());
        availability.download = false;
        availability.attach(&mut face).unwrap();
        assert!(!face.import_available);
        assert!(
            face.unavailable_reason
                .unwrap()
                .contains("exact normal source")
        );
    }
}

#[cfg(test)]
mod routed_synthesis_availability_tests {
    use super::*;
    use opencut_editor_application::font_synthesis_v1::{self as synthesis, SynthesisBaseSourceV1};
    fn derived(v2: bool) -> opencut_contracts::font_synthesis_v1::FontSynthesisSourceV1 {
        synthesis::catalog()
            .unwrap()
            .into_iter()
            .find(|s| {
                if v2 {
                    matches!(
                        synthesis::base_source(&s.source.face.id).unwrap(),
                        SynthesisBaseSourceV1::DerivedV2(_)
                    )
                } else {
                    matches!(
                        synthesis::base_source(&s.source.face.id).unwrap(),
                        SynthesisBaseSourceV1::DerivedV1(_)
                    )
                }
            })
            .unwrap()
    }
    #[test]
    fn derived_synthesis_availability_follows_its_selected_version() {
        let availability = Availability {
            download: true,
            synthesis_cache: None,
            v1: Err(EditorError::new(
                "FONT_DERIVATION_UNAVAILABLE",
                "selected v1",
            )),
            v2: Err(EditorError::new(
                "FONT_DERIVATION_UNAVAILABLE",
                "selected v2",
            )),
        };
        for v2 in [false, true] {
            let source = derived(v2);
            let mut face = app::source(&FontSourceRequestV3 {
                catalog_id: source.source.face.id,
                kind: FontCatalogKindV3::SynthesisV1,
            })
            .unwrap()
            .face;
            availability.attach(&mut face).unwrap();
            assert!(!face.import_available);
            assert_eq!(
                face.unavailable_reason.as_deref(),
                Some(if v2 { "selected v2" } else { "selected v1" })
            );
        }
    }
    #[test]
    fn cached_derived_candidate_does_not_require_runtime_configuration() {
        let temp = tempfile::tempdir().unwrap();
        let source = derived(true);
        std::fs::create_dir_all(temp.path().join("derived")).unwrap();
        std::fs::write(
            temp.path()
                .join("derived")
                .join(&source.binding.actual.content_hash),
            vec![0; source.binding.actual.size_bytes.parse::<usize>().unwrap()],
        )
        .unwrap();
        let availability = Availability {
            download: false,
            synthesis_cache: Some(temp.path().into()),
            v1: Err(EditorError::new("FONT_DERIVATION_UNAVAILABLE", "no Python")),
            v2: Err(EditorError::new("FONT_DERIVATION_UNAVAILABLE", "no Python")),
        };
        let mut face = app::source(&FontSourceRequestV3 {
            catalog_id: source.source.face.id,
            kind: FontCatalogKindV3::SynthesisV1,
        })
        .unwrap()
        .face;
        availability.attach(&mut face).unwrap();
        assert!(face.import_available);
        // Availability is explicitly a metadata hint; native tests separately
        // reject these corrupt bytes before any runtime or publication.
    }

    fn raw_input_path(
        source: &opencut_contracts::font_synthesis_v1::FontSynthesisSourceV1,
        cache: &std::path::Path,
    ) -> (std::path::PathBuf, u64) {
        let (pin, v2) = match synthesis::base_source(&source.source.face.id).unwrap() {
            SynthesisBaseSourceV1::DerivedV1(pin) => (pin, false),
            SynthesisBaseSourceV1::DerivedV2(pin) => (pin, true),
            _ => panic!("test requires a derived base"),
        };
        let proof: serde_json::Value = serde_json::from_str(&pin.source.proof_json).unwrap();
        let recipe = &proof["source"];
        let path = recipe["source_path"].as_str().unwrap();
        let path =
            if v2 && recipe["source_repository"].as_str() == Some("google/material-design-icons") {
                cache
                    .join("official/google-material-design-icons")
                    .join(pin.source.source_commit)
                    .join(path)
            } else if v2
                && pin.source.source_commit
                    != opencut_editor_application::font_import_v1::catalog::GOOGLE_FONTS_COMMIT_V1
            {
                cache
                    .join("fixed")
                    .join(pin.source.source_commit)
                    .join(path)
            } else {
                cache.join(path)
            };
        (path, recipe["source_size"].as_u64().unwrap())
    }

    #[test]
    fn raw_input_cache_allows_offline_synthesis_only_with_its_selected_runtime() {
        for v2 in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let source = derived(v2);
            let (path, size) = raw_input_path(&source, temp.path());
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let file = std::fs::File::create(&path).unwrap();
            file.set_len(size).unwrap();
            let mut result = app::source(&FontSourceRequestV3 {
                catalog_id: source.source.face.id,
                kind: FontCatalogKindV3::SynthesisV1,
            })
            .unwrap();
            assert_eq!(
                source_runtime_selection(&result, Some(temp.path())).unwrap(),
                (!v2, v2)
            );
            let unavailable = || {
                EditorError::new(
                    "FONT_DERIVATION_UNAVAILABLE",
                    "selected runtime unavailable",
                )
            };
            let mut availability = Availability {
                download: false,
                synthesis_cache: Some(temp.path().into()),
                v1: Err(unavailable()),
                v2: Err(unavailable()),
            };
            availability.attach(&mut result.face).unwrap();
            assert!(
                !result.face.import_available,
                "Raw bytes cannot replace a derivation runtime"
            );
            // These pre-admitted runtime objects are never executed here. Their
            // nonexistent paths ensure attach does not rerun installation probes.
            if v2 {
                availability.v2 = Ok(opencut_native_services::font_derived_v2::NativeConfiguredDerivedFontSourceV2::new("unused-python".into(),"unused-script".into(),Some(temp.path().into())));
            } else {
                availability.v1 = Ok(opencut_native_services::font_derived_v1::NativeConfiguredDerivedFontSourceV1::new("unused-python".into(),"unused-script".into(),Some(temp.path().into())));
            }
            availability.attach(&mut result.face).unwrap();
            assert!(result.face.import_available);
            file.set_len(size - 1).unwrap();
            availability.attach(&mut result.face).unwrap();
            assert!(
                !result.face.import_available,
                "Wrong input size does not advertise offline acquisition"
            );
            availability.download = true;
            availability.attach(&mut result.face).unwrap();
            assert!(result.face.import_available);
        }
    }

    #[test]
    fn cached_output_selects_no_runtime_before_single_source_availability_assembly() {
        for v2 in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let source = derived(v2);
            let result = app::source(&FontSourceRequestV3 {
                catalog_id: source.source.face.id.clone(),
                kind: FontCatalogKindV3::SynthesisV1,
            })
            .unwrap();
            assert_eq!(
                source_runtime_selection(&result, Some(temp.path())).unwrap(),
                (!v2, v2)
            );
            let output = temp
                .path()
                .join("derived")
                .join(source.binding.actual.content_hash);
            std::fs::create_dir_all(output.parent().unwrap()).unwrap();
            std::fs::File::create(output)
                .unwrap()
                .set_len(source.binding.actual.size_bytes.parse().unwrap())
                .unwrap();
            assert_eq!(
                source_runtime_selection(&result, Some(temp.path())).unwrap(),
                (false, false)
            );
        }
        let ordinary = app::source(&FontSourceRequestV3 {
            catalog_id: "google-gudea-bold-synthetic-italic-v1".into(),
            kind: FontCatalogKindV3::SynthesisV1,
        })
        .unwrap();
        assert_eq!(
            source_runtime_selection(&ordinary, None).unwrap(),
            (false, false)
        );
    }
}
