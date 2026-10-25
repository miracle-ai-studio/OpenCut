use super::*;
use opencut_editor_application::font_import::{FontImportPolicy, FontInspector, ParsedFontFace};

const ID: &str = "google-gudea-bold-synthetic-italic-v1";
fn pin() -> PinnedDerivedFont {
    application::pinned_source(ID).unwrap()
}
fn cache_file(cache: &Path) -> PathBuf {
    cache_path(application::actual_source(&pin()).unwrap(), cache).unwrap()
}

#[test]
fn synthesis_has_no_runtime_dependency_and_offline_miss_never_fetches() {
    let temp = tempfile::tempdir().unwrap();
    let source = application::source(ID).unwrap();
    assert!(!cached_candidate(&source, Some(temp.path())));
    let native = NativeSynthesisFontSource::new(Some(temp.path().into()));
    let error = native
        .obtain(&pin(), &FontFetchPolicy::default(), &mut || Ok(()))
        .unwrap_err();
    assert_eq!(error.code, "FONT_NETWORK_DENIED");
}

#[test]
fn candidate_size_does_not_authorize_corrupt_cached_bytes_or_forged_binding() {
    let temp = tempfile::tempdir().unwrap();
    let path = cache_file(temp.path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        vec![0; application::actual_source(&pin()).unwrap().size_bytes as usize],
    )
    .unwrap();
    let mut source = application::source(ID).unwrap();
    assert!(cached_candidate(&source, Some(temp.path())));
    source.binding.requested.weight = 400;
    assert!(!cached_candidate(&source, Some(temp.path())));
    let native = NativeSynthesisFontSource::new(Some(temp.path().into()));
    assert_eq!(
        native
            .obtain(&pin(), &FontFetchPolicy::default(), &mut || Ok(()))
            .unwrap_err()
            .code,
        "FONT_SOURCE_INTEGRITY_ERROR"
    );
}

#[cfg(unix)]
#[test]
fn source_cache_symlink_is_not_an_available_font_or_a_read_target() {
    let temp = tempfile::tempdir().unwrap();
    let path = cache_file(temp.path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let target = temp.path().join("target.ttf");
    fs::write(
        &target,
        vec![0; application::actual_source(&pin()).unwrap().size_bytes as usize],
    )
    .unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    assert!(!cached_candidate(
        &application::source(ID).unwrap(),
        Some(temp.path())
    ));
    assert_eq!(
        NativeSynthesisFontSource::new(Some(temp.path().into()))
            .obtain(&pin(), &FontFetchPolicy::default(), &mut || Ok(()))
            .unwrap_err()
            .code,
        "FONT_SOURCE_INTEGRITY_ERROR"
    );
}

#[test]
fn historical_static_cache_reuses_original_location_only_as_an_exact_candidate() {
    let source = application::catalog().unwrap().into_iter().find(|source| {
        matches!(application::base_source(&source.source.face.id).unwrap(),
            SynthesisBaseSource::ApprovedStatic(actual)
                if actual.source.source_url.contains("/google/fonts/")
                && actual.source.source_commit != opencut_editor_application::font_import::catalog::GOOGLE_FONTS_COMMIT)
    }).expect("An approved historical normal face is present");
    let pin = application::pinned_source(&source.source.face.id).unwrap();
    let actual = application::actual_source(&pin).unwrap();
    let cache = tempfile::tempdir().unwrap();
    let primary = cache_path(actual, cache.path()).unwrap();
    let original = original_historical_cache_path(actual, cache.path())
        .unwrap()
        .unwrap();
    assert_ne!(primary, original);
    fs::create_dir_all(original.parent().unwrap()).unwrap();
    fs::write(&original, vec![0; actual.size_bytes as usize]).unwrap();
    assert!(cached_candidate(&source, Some(cache.path())));
    let native = NativeSynthesisFontSource::new(Some(cache.path().into()));
    // A different commit at the old path is not an authorized output, and is
    // never overwritten. Offline acquisition still refuses a public fetch.
    assert_eq!(
        native
            .obtain(&pin, &FontFetchPolicy::default(), &mut || Ok(()))
            .unwrap_err()
            .code,
        "FONT_NETWORK_DENIED"
    );
    assert!(!primary.exists());
    assert_eq!(
        fs::read(&original).unwrap(),
        vec![0; actual.size_bytes as usize]
    );
    let checks = std::cell::Cell::new(0);
    assert_eq!(
        native
            .obtain(&pin, &FontFetchPolicy::default(), &mut || {
                checks.set(checks.get() + 1);
                if checks.get() >= 3 {
                    Err(EditorError::new(
                        "CANCELLED",
                        "Historical cache read cancelled",
                    ))
                } else {
                    Ok(())
                }
            })
            .unwrap_err()
            .code,
        "CANCELLED"
    );
    fs::create_dir_all(primary.parent().unwrap()).unwrap();
    fs::File::create(&primary)
        .unwrap()
        .set_len(actual.size_bytes - 1)
        .unwrap();
    assert!(
        !cached_candidate(&source, Some(cache.path())),
        "Known malformed primary cannot be hidden by the historical fallback"
    );
    assert_eq!(
        native
            .obtain(&pin, &FontFetchPolicy::default(), &mut || Ok(()))
            .unwrap_err()
            .code,
        "FONT_SOURCE_INTEGRITY_ERROR"
    );
}

#[test]
fn synthesis_native_enforces_actual_budget_and_cancellation_before_io() {
    let native = NativeSynthesisFontSource::new(None);
    let policy = FontFetchPolicy {
        max_bytes: 1,
        ..FontFetchPolicy::default()
    };
    assert_eq!(
        native
            .obtain(&pin(), &policy, &mut || Ok(()))
            .unwrap_err()
            .code,
        "FONT_BUDGET_EXCEEDED"
    );
    assert_eq!(
        native
            .obtain(&pin(), &FontFetchPolicy::default(), &mut || Err(
                EditorError::new("CANCELLED", "cancelled")
            ))
            .unwrap_err()
            .code,
        "CANCELLED"
    );
    let mut forged = pin();
    forged.source.source_url = "https://example.invalid/font.ttf".into();
    assert_eq!(
        native
            .obtain(&forged, &FontFetchPolicy::default(), &mut || Ok(()))
            .unwrap_err()
            .code,
        "FONT_SYNTHESIS_INVALID"
    );
}

#[test]
#[ignore = "requires the authorized fixed Google source cache, with no downloads"]
fn fixed_gudea_source_import_keeps_real_normal_metadata_and_explicit_italic_request() {
    let cache =
        std::env::var_os("OPENCUT_FONT_DERIVE_CACHE").expect("fixed source cache is required");
    let native = NativeSynthesisFontSource::new(Some(cache.into()));
    let request = opencut_contracts::font_derived::ImportFont {
        project_id: "p".into(),
        expected_revision: "r".into(),
        idempotency_key: "synthesis-fixture".into(),
        catalog_id: ID.into(),
        font_id: "font".into(),
        required_text: "AV café".into(),
    };
    let prepared = application::prepare_synthesis_font(
        &native,
        &crate::font_inspection::CatalogFontInspector,
        &request,
        &FontFetchPolicy::default(),
        &FontImportPolicy::default(),
        &mut || Ok(()),
    )
    .unwrap();
    let binding = application::binding_for_resource(&prepared.font.resource)
        .unwrap()
        .unwrap();
    assert_eq!(
        prepared.font.resource.style,
        opencut_contracts::scene::FontStyle::Normal
    );
    assert_eq!(
        binding.requested.style,
        opencut_contracts::scene::FontStyle::Italic
    );
    assert_eq!(
        binding.actual.content_hash,
        prepared.font.resource.content.content_hash
    );
    assert_eq!(
        prepared.bytes.len() as u64,
        prepared.font.resource.content.size
    );
    assert!(prepared.font.checked_codepoints > 0);

    struct RetainedBytes(Vec<u8>);
    impl DerivedFontSource for RetainedBytes {
        fn obtain(
            &self,
            _: &PinnedDerivedFont,
            _: &FontFetchPolicy,
            checkpoint: &mut dyn FontImportCheckpoint,
        ) -> Result<Vec<u8>, EditorError> {
            checkpoint.check()?;
            Ok(self.0.clone())
        }
    }
    let bytes = RetainedBytes(prepared.bytes);
    let mut missing = request.clone();
    missing.required_text = "\u{10ffff}".into();
    assert_eq!(
        application::prepare_synthesis_font(
            &bytes,
            &crate::font_inspection::CatalogFontInspector,
            &missing,
            &FontFetchPolicy::default(),
            &FontImportPolicy::default(),
            &mut || Ok(())
        )
        .err()
        .unwrap()
        .code,
        "FONT_GLYPH_MISSING"
    );

    struct WrongStyle;
    impl FontInspector for WrongStyle {
        fn inspect(
            &self,
            bytes: &[u8],
            text: &str,
            policy: &FontImportPolicy,
            checkpoint: &mut dyn FontImportCheckpoint,
        ) -> Result<ParsedFontFace, EditorError> {
            let mut actual = crate::font_inspection::CatalogFontInspector
                .inspect(bytes, text, policy, checkpoint)?;
            actual.style = opencut_contracts::scene::FontStyle::Italic;
            Ok(actual)
        }
    }
    assert_eq!(
        application::prepare_synthesis_font(
            &bytes,
            &WrongStyle,
            &request,
            &FontFetchPolicy::default(),
            &FontImportPolicy::default(),
            &mut || Ok(())
        )
        .err()
        .unwrap()
        .code,
        "FONT_FACE_MISMATCH"
    );

    struct RevokeAfterInspect<'a>(&'a std::cell::Cell<bool>);
    impl FontInspector for RevokeAfterInspect<'_> {
        fn inspect(
            &self,
            bytes: &[u8],
            text: &str,
            policy: &FontImportPolicy,
            checkpoint: &mut dyn FontImportCheckpoint,
        ) -> Result<ParsedFontFace, EditorError> {
            let actual = crate::font_inspection::CatalogFontInspector
                .inspect(bytes, text, policy, checkpoint)?;
            self.0.set(true);
            Ok(actual)
        }
    }
    let revoked = std::cell::Cell::new(false);
    assert_eq!(
        application::prepare_synthesis_font(
            &bytes,
            &RevokeAfterInspect(&revoked),
            &request,
            &FontFetchPolicy::default(),
            &FontImportPolicy::default(),
            &mut || if revoked.get() {
                Err(EditorError::new("CANCELLED", "revoked after inspection"))
            } else {
                Ok(())
            }
        )
        .err()
        .unwrap()
        .code,
        "CANCELLED"
    );
}

fn derived_source(v2: bool) -> FontSynthesisSource {
    application::catalog()
        .unwrap()
        .into_iter()
        .find(|source| {
            if v2 {
                matches!(
                    application::base_source(&source.source.face.id).unwrap(),
                    SynthesisBaseSource::RenamedOutline(_)
                )
            } else {
                matches!(
                    application::base_source(&source.source.face.id).unwrap(),
                    SynthesisBaseSource::Outline(_)
                )
            }
        })
        .unwrap()
}

#[test]
fn raw_base_hints_follow_compiled_acquisition_locations_without_becoming_outputs() {
    let mut cases = std::collections::BTreeMap::new();
    for source in application::catalog().unwrap() {
        let (pin, v2) = match application::base_source(&source.source.face.id).unwrap() {
            SynthesisBaseSource::Outline(pin) => (pin, false),
            SynthesisBaseSource::RenamedOutline(pin) => (pin, true),
            _ => continue,
        };
        let proof: serde_json::Value = serde_json::from_str(&pin.source.proof_json).unwrap();
        let repository = proof["source"]["source_repository"]
            .as_str()
            .unwrap_or("google/fonts")
            .to_owned();
        let historical = pin.source.source_commit
            != opencut_editor_application::font_import::catalog::GOOGLE_FONTS_COMMIT;
        let large = pin.source.source_size_bytes.parse::<u64>().unwrap() > MAX_FONT_BYTES;
        cases
            .entry((v2, historical, repository, large))
            .or_insert(source);
    }
    assert!(cases.keys().any(|key| !key.0));
    assert!(cases.keys().any(|key| key.0));
    // One representative of every compiled path form, including historical
    // Google and the official repository when those recipes are present.
    for source in cases.into_values() {
        let cache = tempfile::tempdir().unwrap();
        let (paths, size) = base_input_cache_paths(&source, cache.path())
            .unwrap()
            .unwrap();
        assert!(!base_input_cached_candidate(&source, Some(cache.path())));
        for path in paths {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let file = fs::File::create(&path).unwrap();
            file.set_len(size).unwrap();
            assert!(base_input_cached_candidate(&source, Some(cache.path())));
            assert!(
                !cached_candidate(&source, Some(cache.path())),
                "Raw input is never the pinned completed output"
            );
            let mut forged = source.clone();
            forged.source.proof_json.push(' ');
            assert!(!base_input_cached_candidate(&forged, Some(cache.path())));
            file.set_len(size - 1).unwrap();
            assert!(!base_input_cached_candidate(&source, Some(cache.path())));
            drop(file);
            fs::remove_file(&path).unwrap();
            #[cfg(unix)]
            {
                let target = cache.path().join("raw-target");
                fs::File::create(&target).unwrap().set_len(size).unwrap();
                std::os::unix::fs::symlink(&target, &path).unwrap();
                assert!(!base_input_cached_candidate(&source, Some(cache.path())));
                fs::remove_file(&path).unwrap();
                fs::remove_file(target).unwrap();
            }
        }
        let output = cache
            .path()
            .join("derived")
            .join(&source.binding.actual.content_hash);
        fs::create_dir_all(output.parent().unwrap()).unwrap();
        fs::File::create(output)
            .unwrap()
            .set_len(source.binding.actual.size_bytes.parse().unwrap())
            .unwrap();
        assert!(cached_candidate(&source, Some(cache.path())));
        assert!(
            !base_input_cached_candidate(&source, Some(cache.path())),
            "Output cache is not an input-cache claim"
        );
    }
    assert!(!base_input_cached_candidate(
        &application::source(ID).unwrap(),
        Some(Path::new("/unused"))
    ));
}
struct SelectedBase {
    expected: PinnedDerivedFont,
    calls: std::cell::Cell<usize>,
    response: Option<Vec<u8>>,
}
impl DerivedFontSource for SelectedBase {
    fn obtain(
        &self,
        pin: &PinnedDerivedFont,
        _: &FontFetchPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        assert_eq!(pin.source, self.expected.source);
        self.calls.set(self.calls.get() + 1);
        self.response.clone().ok_or_else(|| {
            EditorError::new("SELECTED_BASE", "Selected original derivation was called")
        })
    }
}
struct NoRuntime;
impl DerivedFontSource for NoRuntime {
    fn obtain(
        &self,
        _: &PinnedDerivedFont,
        _: &FontFetchPolicy,
        _: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError> {
        panic!("Unselected or cached route must not initialize a runtime")
    }
}
#[test]
fn routed_synthesis_passes_original_pin_only_to_selected_lazy_base_port() {
    let forbidden = NoRuntime;
    for v2 in [false, true] {
        let source = derived_source(v2);
        let base = match application::base_source(&source.source.face.id).unwrap() {
            SynthesisBaseSource::Outline(pin) | SynthesisBaseSource::RenamedOutline(pin) => pin,
            _ => panic!(),
        };
        let selected = SelectedBase {
            expected: base,
            calls: std::cell::Cell::new(0),
            response: None,
        };
        let native = if v2 {
            RoutedSynthesisFontSource::new(None, Some(&forbidden), Some(&selected))
        } else {
            RoutedSynthesisFontSource::new(None, Some(&selected), Some(&forbidden))
        };
        let limits = application::acquisition_limits(&source.source.face.id).unwrap();
        let policy = FontFetchPolicy {
            max_bytes: limits.max_source_bytes,
            deadline_ms: 90_000,
            ..FontFetchPolicy::default()
        };
        assert_eq!(
            native
                .obtain(
                    &PinnedDerivedFont {
                        source: source.source
                    },
                    &policy,
                    &mut || Ok(())
                )
                .unwrap_err()
                .code,
            "SELECTED_BASE"
        );
        assert_eq!(selected.calls.get(), 1);
    }
}
#[test]
fn routed_cache_is_a_hint_until_bytes_are_verified_and_corruption_never_calls_python() {
    let temp = tempfile::tempdir().unwrap();
    let source = derived_source(true);
    let path = temp
        .path()
        .join("derived")
        .join(&source.binding.actual.content_hash);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        vec![0; source.binding.actual.size_bytes.parse::<usize>().unwrap()],
    )
    .unwrap();
    assert!(cached_candidate(&source, Some(temp.path())));
    let forbidden = NoRuntime;
    let native = RoutedSynthesisFontSource::new(
        Some(temp.path().into()),
        Some(&forbidden),
        Some(&forbidden),
    );
    let pin = PinnedDerivedFont {
        source: source.source,
    };
    assert_eq!(
        native
            .obtain(&pin, &FontFetchPolicy::default(), &mut || Ok(()))
            .unwrap_err()
            .code,
        "FONT_SOURCE_INTEGRITY_ERROR"
    );
    assert_eq!(
        native
            .obtain(&pin, &FontFetchPolicy::default(), &mut || Err(
                EditorError::new("CANCELLED", "cancelled")
            ))
            .unwrap_err()
            .code,
        "CANCELLED"
    );
}
#[test]
fn original_base_port_cannot_return_variable_input_or_arbitrary_output_bytes() {
    let source = derived_source(true);
    let base = match application::base_source(&source.source.face.id).unwrap() {
        SynthesisBaseSource::RenamedOutline(pin) => pin,
        _ => panic!(),
    };
    let selected = SelectedBase {
        expected: base,
        calls: std::cell::Cell::new(0),
        response: Some(b"variable-source-is-not-the-pinned-static-output".to_vec()),
    };
    let native = RoutedSynthesisFontSource::new(None, None, Some(&selected));
    assert_eq!(
        native
            .obtain(
                &PinnedDerivedFont {
                    source: source.source
                },
                &FontFetchPolicy::default(),
                &mut || Ok(())
            )
            .unwrap_err()
            .code,
        "FONT_SOURCE_INTEGRITY_ERROR"
    );
    assert_eq!(selected.calls.get(), 1);
}
#[test]
fn static_routes_keep_old_policy_and_do_not_call_either_derivation_port() {
    let forbidden = NoRuntime;
    let native = RoutedSynthesisFontSource::new(None, Some(&forbidden), Some(&forbidden));
    assert_eq!(
        native
            .obtain(&pin(), &FontFetchPolicy::default(), &mut || Ok(()))
            .unwrap_err()
            .code,
        "FONT_NETWORK_DENIED"
    );
    for policy in [
        FontFetchPolicy {
            deadline_ms: 30_001,
            ..FontFetchPolicy::default()
        },
        FontFetchPolicy {
            max_bytes: 64 * 1024 * 1024,
            ..FontFetchPolicy::default()
        },
    ] {
        assert_eq!(
            native
                .obtain(&pin(), &policy, &mut || Ok(()))
                .unwrap_err()
                .code,
            "FONT_SYNTHESIS_INVALID"
        );
    }
}
#[test]
#[ignore = "requires existing OPENCUT_FONT_DERIVE_CACHE with exact V1 and V2 output objects; no derivation or downloads"]
fn exact_cached_original_outputs_support_synthesis_without_any_python_configuration() {
    let cache =
        PathBuf::from(std::env::var_os("OPENCUT_FONT_DERIVE_CACHE").expect("fixed output cache"));
    let forbidden = NoRuntime;
    let native =
        RoutedSynthesisFontSource::new(Some(cache.clone()), Some(&forbidden), Some(&forbidden));
    for v2 in [false, true] {
        let source = application::catalog()
            .unwrap()
            .into_iter()
            .find(|source| {
                let selected = if v2 {
                    matches!(
                        application::base_source(&source.source.face.id).unwrap(),
                        SynthesisBaseSource::RenamedOutline(_)
                    )
                } else {
                    matches!(
                        application::base_source(&source.source.face.id).unwrap(),
                        SynthesisBaseSource::Outline(_)
                    )
                };
                selected && cached_candidate(source, Some(&cache))
            })
            .expect("at least one original output object for each version");
        let request = opencut_contracts::font_derived::ImportFont {
            project_id: "p".into(),
            expected_revision: "r".into(),
            idempotency_key: "cached-route".into(),
            catalog_id: source.source.face.id.clone(),
            font_id: "font".into(),
            required_text: String::new(),
        };
        let result = application::prepare_synthesis_font(
            &native,
            &crate::font_inspection::CatalogFontInspector,
            &request,
            &FontFetchPolicy::default(),
            &FontImportPolicy::default(),
            &mut || Ok(()),
        )
        .unwrap();
        assert_eq!(
            result.font.resource.content.content_hash,
            source.binding.actual.content_hash
        );
        assert_eq!(
            result.font.resource.style,
            opencut_contracts::scene::FontStyle::Normal
        );
        assert_eq!(
            application::binding_for_resource(&result.font.resource)
                .unwrap()
                .unwrap()
                .requested
                .style,
            opencut_contracts::scene::FontStyle::Italic
        );
    }
}
