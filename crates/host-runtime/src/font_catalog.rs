//! Unified discovery and acquisition selected by the exact approved source.
//! Reuse original algorithms and one durable recovery without protocol fallback.
use super::*;
use opencut_contracts::{font_catalog::*, font_import::ImportFont};
use opencut_editor_application::{
    font_catalog as app,
    font_import::{FontImportPolicy, catalog::FontFetchPolicy},
    scene::{SceneCommitIntent, SceneRead},
};

fn policy() -> FontFetchPolicy {
    FontFetchPolicy {
        allow_public_font_download: std::env::var("OPENCUT_ALLOW_PUBLIC_FONT_DOWNLOAD").as_deref()
            == Ok("1"),
        max_bytes: app::MAX_CATALOG_FONT_BYTES,
        ..FontFetchPolicy::default()
    }
}
struct Availability {
    download: bool,
    synthesis_cache: Option<std::path::PathBuf>,
    outline: Result<
        opencut_native_services::font_derived::NativeConfiguredOutlineFontSource,
        EditorError,
    >,
    named:
        Result<opencut_native_services::font_named::NativeConfiguredNamedFontSource, EditorError>,
    color: Result<
        opencut_native_services::font_color_static::NativeConfiguredColorStaticFontSource,
        EditorError,
    >,
}
impl Availability {
    fn new(outline: bool, named: bool, color_static: bool) -> Self {
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
            outline: if outline {
                super::font_derived::configured().and_then(|runtime| {
                    runtime.availability()?;
                    Ok(runtime)
                })
            } else {
                Err(not_selected())
            },
            named: if named {
                super::font_named::configured().and_then(|runtime| {
                    runtime.availability()?;
                    Ok(runtime)
                })
            } else {
                Err(not_selected())
            },
            color: if color_static {
                super::font_color_static::configured().and_then(|runtime| {
                    runtime.availability()?;
                    Ok(runtime)
                })
            } else {
                Err(not_selected())
            },
        }
    }
    fn attach(&self, face: &mut FontFace) -> Result<(), EditorError> {
        if app::selection::select(&face.id)? == app::selection::FontAcquisition::RenamedOutline
            && opencut_editor_application::font_color_static::replaces(&face.id)
        {
            face.import_available = false;
            face.unavailable_reason = Some("This historical output retains variable color data. Select its complete static color source.".into());
            return Ok(());
        }
        let offline = || {
            "Public downloads are disabled and this exact output has no local cache candidate; cached bytes are verified again during import.".to_owned()
        };
        face.unavailable_reason = match app::selection::select(&face.id)? {
            app::selection::FontAcquisition::RepositoryStatic
            | app::selection::FontAcquisition::ApprovedStatic
            | app::selection::FontAcquisition::Distribution => {
                (!self.download).then(|| "Public font downloads are disabled by this host.".into())
            }
            app::selection::FontAcquisition::Outline => match &self.outline {
                Err(error) => Some(error.message.clone()),
                Ok(runtime) => (!self.download
                    && !runtime.cached_candidate(
                        &opencut_editor_application::font_derived::source(&face.id)?,
                    ))
                .then(offline),
            },
            app::selection::FontAcquisition::RenamedOutline => match &self.named {
                Err(error) => Some(error.message.clone()),
                Ok(runtime) => (!self.download
                    && !runtime.cached_candidate(&opencut_editor_application::font_named::source(
                        &face.id,
                    )?))
                .then(offline),
            },
            app::selection::FontAcquisition::CompleteColor => match &self.color {
                Err(error) => Some(error.message.clone()),
                Ok(runtime) => (!self.download
                    && !runtime.cached_candidate(
                        &opencut_editor_application::font_color_static::source(&face.id)?,
                    ))
                .then(offline),
            },
            app::selection::FontAcquisition::Synthesis => {
                use opencut_editor_application::font_synthesis::{
                    self as synthesis, SynthesisBaseSource,
                };
                let source = synthesis::source(&face.id)?;
                let base = synthesis::base_source(&face.id)?;
                if matches!(&base,SynthesisBaseSource::RenamedOutline(pin) if opencut_editor_application::font_color_static::replaces(&pin.source.face.id))
                {
                    Some("This historical synthesis base retains variable color data. Select its complete static color base.".into())
                } else if opencut_native_services::font_synthesis::cached_candidate(
                    &source,
                    self.synthesis_cache.as_deref(),
                ) {
                    None // Output-cache hints do not require a Python runtime.
                } else {
                    match base {
                    SynthesisBaseSource::RepositoryStatic(_)|SynthesisBaseSource::ApprovedStatic(_) => (!self.download)
                        .then(||"Public downloads are disabled and this exact normal source has no local cache candidate; import rechecks source bytes before recording explicit italic synthesis.".into()),
                    SynthesisBaseSource::Outline(_) => match &self.outline {Err(error)=>Some(error.message.clone()),Ok(_)=>self.synthesis_input_unavailable(&source)},
                    SynthesisBaseSource::RenamedOutline(_) => match &self.named {Err(error)=>Some(error.message.clone()),Ok(_)=>self.synthesis_input_unavailable(&source)},
                    SynthesisBaseSource::CompleteColor(_) => match &self.color {Err(error)=>Some(error.message.clone()),Ok(_)=>self.synthesis_input_unavailable(&source)},
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
        source: &opencut_contracts::font_synthesis::FontSynthesisSource,
    ) -> Option<String> {
        (!self.download && !opencut_native_services::font_synthesis::base_input_cached_candidate(
            source, self.synthesis_cache.as_deref()))
            .then(|| "Public downloads are disabled and this fixed base input has no local cache candidate; import verifies the source and derived output bytes.".into())
    }
}
pub(super) fn catalog() -> Result<FontCatalog, EditorError> {
    let availability = Availability::new(true, true, true);
    let allowed = availability.download;
    let metadata =
        opencut_editor_application::font_import::browser_catalog::classic_font_inventory();
    let mut fonts = app::catalog()?;
    for face in &mut fonts {
        availability.attach(face)?;
    }
    Ok(FontCatalog {
        families: metadata.families.clone(),
        pinned_face_count: fonts.len() as u32,
        download_available: allowed,
        unavailable_reason: (!allowed)
            .then(|| "Public font downloads are disabled by this host.".into()),
        source_commit: metadata.source_commit.clone(),
        classic_atlas_sha256: metadata.classic_atlas_sha256.clone(),
        classic_family_count: metadata.classic_family_count as u32,
        fonts,
    })
}
pub(super) fn source(request: &FontSourceRequest) -> Result<FontSource, EditorError> {
    let mut source = app::source(request)?;
    let cache = std::env::var_os("OPENCUT_FONT_DERIVE_CACHE")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from);
    let (outline, named, color) = source_runtime_selection(&source, cache.as_deref())?;
    Availability::new(outline, named, color).attach(&mut source.face)?;
    Ok(source)
}
fn source_runtime_selection(
    source: &FontSource,
    cache: Option<&std::path::Path>,
) -> Result<(bool, bool, bool), EditorError> {
    // A static synthesis or cached derivative never probes Python. A missing
    // derivative probes only the selected original base algorithm.
    let route = app::selection::select(&source.face.id)?;
    let (mut outline, mut named) = (
        route == app::selection::FontAcquisition::Outline,
        route == app::selection::FontAcquisition::RenamedOutline,
    );
    let mut color = route == app::selection::FontAcquisition::CompleteColor;
    if let FontSourceDetails::Synthesis(synthetic) = &source.details {
        use opencut_editor_application::font_synthesis::{self as synthesis, SynthesisBaseSource};
        if !opencut_native_services::font_synthesis::cached_candidate(synthetic, cache) {
            match synthesis::base_source(&synthetic.source.face.id)? {
                SynthesisBaseSource::Outline(_) => outline = true,
                SynthesisBaseSource::RenamedOutline(_) => named = true,
                SynthesisBaseSource::CompleteColor(_) => color = true,
                _ => {}
            }
        }
    }
    Ok((outline, named, color))
}
pub(super) fn import_font(
    host: &mut Automation,
    request: &ImportFont,
) -> Result<opencut_contracts::TransactionResult, EditorError> {
    use app::selection::{self, FontAcquisition};
    match selection::validate(request)? {
        FontAcquisition::RepositoryStatic => super::font_import::import(host, request),
        FontAcquisition::ApprovedStatic => import_selected(host, request, false),
        FontAcquisition::Distribution => import_selected(host, request, true),
        FontAcquisition::Outline => super::font_derived::import(host, request),
        FontAcquisition::RenamedOutline => super::font_named::import(host, request),
        FontAcquisition::CompleteColor => super::font_color_static::import(host, request),
        FontAcquisition::Synthesis => super::font_synthesis::import(host, request),
    }
}
fn import_selected(
    host: &mut Automation,
    request: &ImportFont,
    distribution: bool,
) -> Result<opencut_contracts::TransactionResult, EditorError> {
    if distribution {
        app::distribution::validate_request(request)?;
    } else {
        app::validate_import_request(request)?;
    }
    host.app
        .authorize_project(SceneCommitIntent::FontImport(request))?;
    let policy = policy();
    let mut authority = EditorApplication::open_managed(&host.database)?;
    authority.set_execution_context(host.app.execution_context().clone());
    let started = std::time::Instant::now();
    let mut checkpoint = || {
        authority.authorize_project(SceneCommitIntent::FontImport(request))?;
        if started.elapsed() > std::time::Duration::from_millis(policy.deadline_ms) {
            return Err(EditorError::new(
                "FONT_SOURCE_TIMEOUT",
                "Font import exceeded its host deadline",
            ));
        }
        Ok(())
    };
    let publisher =
        opencut_asset_store::font_import::ManagedFontPublisher::new(host.app.asset_store_root());
    if distribution {
        return app::distribution::import(
            &mut host.app,
            &opencut_native_services::font_catalog::GoogleFontsDistributionFetch,
            &opencut_native_services::font_inspection::CatalogFontInspector,
            &publisher,
            request,
            &policy,
            &FontImportPolicy {
                max_font_bytes: app::MAX_CATALOG_FONT_BYTES,
                ..FontImportPolicy::default()
            },
            &mut checkpoint,
        );
    }
    app::import_static_font(
        &mut host.app,
        &opencut_native_services::font_catalog::GoogleFontsFetch,
        &opencut_native_services::font_inspection::CatalogFontInspector,
        &publisher,
        request,
        &policy,
        &FontImportPolicy {
            max_font_bytes: app::MAX_CATALOG_FONT_BYTES,
            ..FontImportPolicy::default()
        },
        &mut checkpoint,
    )
}

#[cfg(test)]
mod synthesis_availability_tests {
    use super::*;

    #[test]
    fn synthesis_source_availability_uses_acquisition_policy_not_python_runtime() {
        let source = opencut_editor_application::font_synthesis::catalog()
            .unwrap()
            .remove(0);
        let mut face = app::source(&FontSourceRequest {
            catalog_id: source.source.face.id,
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
            outline: Err(unavailable()),
            named: Err(unavailable()),
            color: Err(unavailable()),
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
    use opencut_editor_application::font_synthesis::{self as synthesis, SynthesisBaseSource};
    fn derived(named: bool) -> opencut_contracts::font_synthesis::FontSynthesisSource {
        synthesis::catalog()
            .unwrap()
            .into_iter()
            .find(|s| {
                if named {
                    matches!(
                        synthesis::base_source(&s.source.face.id).unwrap(),
                        SynthesisBaseSource::RenamedOutline(_)
                    )
                } else {
                    matches!(
                        synthesis::base_source(&s.source.face.id).unwrap(),
                        SynthesisBaseSource::Outline(_)
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
            outline: Err(EditorError::new(
                "FONT_DERIVATION_UNAVAILABLE",
                "selected outline",
            )),
            named: Err(EditorError::new(
                "FONT_DERIVATION_UNAVAILABLE",
                "selected named",
            )),
            color: Err(EditorError::new(
                "FONT_DERIVATION_UNAVAILABLE",
                "selected color",
            )),
        };
        for named in [false, true] {
            let source = derived(named);
            let mut face = app::source(&FontSourceRequest {
                catalog_id: source.source.face.id,
            })
            .unwrap()
            .face;
            availability.attach(&mut face).unwrap();
            assert!(!face.import_available);
            assert_eq!(
                face.unavailable_reason.as_deref(),
                Some(if named {
                    "selected named"
                } else {
                    "selected outline"
                })
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
            outline: Err(EditorError::new("FONT_DERIVATION_UNAVAILABLE", "no Python")),
            named: Err(EditorError::new("FONT_DERIVATION_UNAVAILABLE", "no Python")),
            color: Err(EditorError::new("FONT_DERIVATION_UNAVAILABLE", "no Python")),
        };
        let mut face = app::source(&FontSourceRequest {
            catalog_id: source.source.face.id,
        })
        .unwrap()
        .face;
        availability.attach(&mut face).unwrap();
        assert!(face.import_available);
        // Availability is explicitly a metadata hint; native tests separately
        // reject these corrupt bytes before any runtime or publication.
    }

    fn raw_input_path(
        source: &opencut_contracts::font_synthesis::FontSynthesisSource,
        cache: &std::path::Path,
    ) -> (std::path::PathBuf, u64) {
        let (pin, named) = match synthesis::base_source(&source.source.face.id).unwrap() {
            SynthesisBaseSource::Outline(pin) => (pin, false),
            SynthesisBaseSource::RenamedOutline(pin) => (pin, true),
            _ => panic!("test requires a derived base"),
        };
        let proof: serde_json::Value = serde_json::from_str(&pin.source.proof_json).unwrap();
        let recipe = &proof["source"];
        let path = recipe["source_path"].as_str().unwrap();
        let path = if named
            && recipe["source_repository"].as_str() == Some("google/material-design-icons")
        {
            cache
                .join("official/google-material-design-icons")
                .join(pin.source.source_commit)
                .join(path)
        } else if named
            && pin.source.source_commit
                != opencut_editor_application::font_import::catalog::GOOGLE_FONTS_COMMIT
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
        for named in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let source = derived(named);
            let (path, size) = raw_input_path(&source, temp.path());
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let file = std::fs::File::create(&path).unwrap();
            file.set_len(size).unwrap();
            let mut result = app::source(&FontSourceRequest {
                catalog_id: source.source.face.id,
            })
            .unwrap();
            assert_eq!(
                source_runtime_selection(&result, Some(temp.path())).unwrap(),
                (!named, named, false)
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
                outline: Err(unavailable()),
                named: Err(unavailable()),
                color: Err(unavailable()),
            };
            availability.attach(&mut result.face).unwrap();
            assert!(
                !result.face.import_available,
                "Raw bytes cannot replace a derivation runtime"
            );
            // These pre-admitted runtime objects are never executed here. Their
            // nonexistent paths ensure attach does not rerun installation probes.
            if named {
                availability.named = Ok(
                    opencut_native_services::font_named::NativeConfiguredNamedFontSource::new(
                        "unused-python".into(),
                        "unused-script".into(),
                        Some(temp.path().into()),
                    ),
                );
            } else {
                availability.outline = Ok(
                    opencut_native_services::font_derived::NativeConfiguredOutlineFontSource::new(
                        "unused-python".into(),
                        "unused-script".into(),
                        Some(temp.path().into()),
                    ),
                );
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
        for named in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let source = derived(named);
            let result = app::source(&FontSourceRequest {
                catalog_id: source.source.face.id.clone(),
            })
            .unwrap();
            assert_eq!(
                source_runtime_selection(&result, Some(temp.path())).unwrap(),
                (!named, named, false)
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
                (false, false, false)
            );
        }
        let ordinary = app::source(&FontSourceRequest {
            catalog_id: "google-gudea-bold-synthetic-italic-v1".into(),
        })
        .unwrap();
        assert_eq!(
            source_runtime_selection(&ordinary, None).unwrap(),
            (false, false, false)
        );
    }
}
