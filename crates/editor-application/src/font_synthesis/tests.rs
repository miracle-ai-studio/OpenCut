use super::*;
use std::cell::Cell;

#[test]
fn legacy_snapshot_consumers_reject_unreferenced_synthesis_before_sampling() {
    let mut input = crate::media_video_jobs::tests::input();
    input
        .snapshot
        .project
        .fonts
        .push(expected_import_resource(&request()).unwrap());
    opencut_editor_core::scene::validate_project_scene(&input.snapshot.project).unwrap();
    let snapshot = &input.snapshot;
    let sequence = &input.sequence_id;
    let at = opencut_contracts::Time {
        ticks: "0".into(),
        timebase: 1,
    };
    let errors = [
        crate::media_frame_jobs::prepare(snapshot, sequence, &at).unwrap_err(),
        crate::scene_text_frame_jobs::prepare(snapshot, sequence, &at).unwrap_err(),
        crate::scene_caption_frame_jobs::prepare(snapshot, sequence, &at).unwrap_err(),
        crate::scene_graphic_frame_jobs::prepare(snapshot, sequence, &at).unwrap_err(),
        crate::scene_blend_frame_jobs::prepare(snapshot, sequence, &at).unwrap_err(),
        crate::scene_mask_frame_jobs::prepare(snapshot, sequence, &at).unwrap_err(),
        crate::media_video_jobs::prepare_frame(snapshot, sequence, &at).unwrap_err(),
    ];
    for error in errors {
        assert_eq!(error.code, "UNSUPPORTED_V4");
        assert!(error.message.contains("Explicit font synthesis"));
    }
    use crate::media_video_jobs::{VideoFrameProfile, prepare_with_audio_selection};
    for profile in [
        VideoFrameProfile::TextV2,
        VideoFrameProfile::CaptionV3,
        VideoFrameProfile::GraphicV1,
        VideoFrameProfile::BlendV1,
        VideoFrameProfile::MaskV1,
    ] {
        let error = prepare_with_audio_selection(
            snapshot,
            sequence,
            &input.processing_policy,
            profile,
            false,
        )
        .err()
        .unwrap();
        assert_eq!(error.code, "UNSUPPORTED_V4");
        assert!(error.message.contains("Explicit font synthesis"));
    }
}

const ID: &str = "google-gudea-bold-synthetic-italic-v1";
fn request() -> ImportFont {
    ImportFont {
        project_id: "p".into(),
        expected_revision: "r".into(),
        idempotency_key: "synthesis-import".into(),
        catalog_id: ID.into(),
        font_id: "font".into(),
        required_text: "AV".into(),
    }
}

#[test]
fn eleven_bindings_preserve_actual_source_bytes_metadata_and_complete_license() {
    let pins = catalog()
        .unwrap()
        .into_iter()
        .filter(|pin| ACTUAL_SOURCE_IDS.contains(&pin.binding.actual.id.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(pins.len(), 11);
    let mut ids = std::collections::BTreeSet::new();
    for pin in pins {
        assert!(ids.insert(pin.binding.binding_id.clone()));
        let actual = pinned_font_source(&pin.binding.actual.id).unwrap();
        assert_ne!(pin.source.face.id, pin.binding.actual.id);
        assert!(pin.source.face.id.ends_with("-synthetic-italic-v1"));
        assert_eq!(pin.source.face.style, FontStyle::Normal);
        assert_eq!(pin.binding.actual.style, FontStyle::Normal);
        assert_eq!(pin.binding.requested.style, FontStyle::Italic);
        assert_eq!(pin.binding.requested.weight, actual.weight);
        assert_eq!(pin.source.face.weight, actual.weight);
        assert_eq!(pin.source.face.family, actual.family);
        assert_eq!(pin.source.face.content_hash, actual.content_hash);
        assert_eq!(pin.source.face.content_hash, pin.source.source_sha256);
        assert_eq!(pin.source.face.size_bytes, actual.size_bytes.to_string());
        assert_eq!(pin.source.face.size_bytes, pin.source.source_size_bytes);
        assert_eq!(pin.source.license_notice, actual.license_notice);
        assert!(pin.source.axes.is_empty());
        let mut proof: Value = serde_json::from_str(&pin.source.proof_json).unwrap();
        assert_eq!(canonical(&proof).unwrap(), pin.source.proof_json);
        assert_eq!(proof["schema_version"], 3);
        assert_eq!(proof["source_bytes_unchanged"], true);
        assert_eq!(proof["license_sha256"], actual.license_sha256);
        assert_eq!(proof["attribution"], actual.attribution);
        proof.as_object_mut().unwrap().remove("derivation_id");
        assert_eq!(
            pin.binding.binding_id,
            format!(
                "{FONT_SYNTHESIS_BINDING_ID_PREFIX}{:x}",
                Sha256::digest(canonical(&proof).unwrap().as_bytes())
            )
        );
        assert_eq!(source(&pin.source.face.id).unwrap(), pin);
        let opaque = PinnedDerivedFont {
            source: pin.source.clone(),
        };
        validate_pinned_source(&opaque).unwrap();
        assert!(crate::font_derived::validate_pinned_source(&opaque).is_err());
        assert!(crate::font_named::validate_pinned_source(&opaque).is_err());
    }
}

#[test]
fn a_canonical_but_unapproved_request_cannot_reseal_its_own_binding() {
    let mut pin = pinned_source(ID).unwrap();
    let mut proof: Value = serde_json::from_str(&pin.source.proof_json).unwrap();
    proof.as_object_mut().unwrap().remove("derivation_id");
    proof["requested"]["weight"] = json!(400);
    let id = format!(
        "{FONT_SYNTHESIS_BINDING_ID_PREFIX}{:x}",
        Sha256::digest(canonical(&proof).unwrap().as_bytes())
    );
    proof["derivation_id"] = json!(id);
    pin.source.face.derivation_id = id;
    pin.source.proof_json = canonical(&proof).unwrap();
    assert_eq!(
        validate_pinned_source(&pin).unwrap_err().code,
        "FONT_SYNTHESIS_INVALID"
    );
    assert!(source("google-gudea-regular-synthetic-italic-v1").is_err());
    assert!(source("google-gudea-bold-v1").is_err());
}

#[test]
fn resource_binding_rejects_native_face_hash_license_provider_and_generation_forgery() {
    let original = expected_import_resource(&request()).unwrap();
    let pin = source(ID).unwrap();
    assert_eq!(
        binding_for_resource(&original).unwrap(),
        Some(pin.binding.clone())
    );
    for mutation in 0..8 {
        let mut changed = original.clone();
        match mutation {
            0 => changed.style = FontStyle::Italic,
            1 => changed.weight = 400,
            2 => changed.family.push_str(" forged"),
            3 => changed.content.content_hash = "0".repeat(64),
            4 => changed.content.size += 1,
            5 => changed.license.as_mut().unwrap().attribution = None,
            6 => changed.face_index = 1,
            7 => changed.content.id = "different".into(),
            _ => unreachable!(),
        }
        assert!(
            binding_for_resource(&changed).is_err(),
            "mutation {mutation}"
        );
        assert!(validate_resource_source(&changed, &pin.source).is_err());
    }
    let mut changed = original.clone();
    if let ResourceOrigin::Generated { generation_id, .. } = &mut changed.origin {
        generation_id.push('0');
    }
    assert!(binding_for_resource(&changed).is_err());
    if let ResourceOrigin::Generated { provider, .. } = &mut changed.origin {
        *provider = "other-provider".into();
    }
    assert_eq!(binding_for_resource(&changed).unwrap(), None);
    assert!(validate_resource_source(&changed, &pin.source).is_err());
    let mut alias = original;
    alias.id = "font-copy".into();
    assert_eq!(binding_for_resource(&alias).unwrap(), Some(pin.binding));
}

#[test]
fn source_envelope_cannot_relabel_real_bytes_or_drop_any_proof_or_license_field() {
    for mutation in 0..7 {
        let mut pin = pinned_source(ID).unwrap();
        match mutation {
            0 => pin.source.face.style = FontStyle::Italic,
            1 => pin.source.source_sha256 = "0".repeat(64),
            2 => pin.source.source_url.push_str("?changed"),
            3 => pin.source.license_notice.clear(),
            4 => pin.source.face.size_bytes.insert(0, '0'),
            5 => pin.source.axes.push(FontDerivedAxis {
                tag: "ital".into(),
                coordinate: "1".into(),
            }),
            6 => pin.source.proof_json.push(' '),
            _ => unreachable!(),
        }
        assert!(validate_pinned_source(&pin).is_err(), "mutation {mutation}");
    }
}

struct BytesSource {
    calls: Cell<usize>,
}
impl DerivedFontSource for BytesSource {
    fn obtain(
        &self,
        _: &PinnedDerivedFont,
        _: &FontFetchPolicy,
        _: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError> {
        self.calls.set(self.calls.get() + 1);
        Ok(vec![0; 100])
    }
}
struct NoInspector;
impl FontInspector for NoInspector {
    fn inspect(
        &self,
        _: &[u8],
        _: &str,
        _: &FontImportPolicy,
        _: &mut dyn FontImportCheckpoint,
    ) -> Result<ParsedFontFace, EditorError> {
        panic!("invalid source or budget must be rejected before parsing")
    }
}
fn prepare_error(
    provider: &BytesSource,
    input: &ImportFont,
    fetch: &FontFetchPolicy,
    inspection: &FontImportPolicy,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> EditorError {
    prepare_synthesis_font(provider, &NoInspector, input, fetch, inspection, checkpoint)
        .err()
        .expect("must fail")
}

#[test]
fn preparation_checks_full_budget_before_source_and_hash_before_inspector() {
    let provider = BytesSource {
        calls: Cell::new(0),
    };
    let fetch = FontFetchPolicy::default();
    let inspection = FontImportPolicy::default();
    let too_small = FontImportPolicy {
        max_font_bytes: 1,
        ..inspection
    };
    assert_eq!(
        prepare_error(&provider, &request(), &fetch, &too_small, &mut || Ok(())).code,
        "FONT_BUDGET_EXCEEDED"
    );
    assert_eq!(provider.calls.get(), 0);
    let fetch_small = FontFetchPolicy {
        max_bytes: 1,
        ..fetch
    };
    assert_eq!(
        prepare_error(
            &provider,
            &request(),
            &fetch_small,
            &inspection,
            &mut || Ok(())
        )
        .code,
        "FONT_BUDGET_EXCEEDED"
    );
    let mut input = request();
    input.required_text = "a".repeat(MAX_FONT_TEXT_BYTES + 1);
    assert_eq!(
        prepare_error(&provider, &input, &fetch, &inspection, &mut || Ok(())).code,
        "FONT_BUDGET_EXCEEDED"
    );
    assert_eq!(provider.calls.get(), 0);
    assert_eq!(
        prepare_error(&provider, &request(), &fetch, &inspection, &mut || Ok(())).code,
        "FONT_SOURCE_INTEGRITY_ERROR"
    );
    assert_eq!(provider.calls.get(), 1);
}

#[test]
fn synthesis_preserves_cancellation_before_and_after_source_acquisition() {
    let provider = BytesSource {
        calls: Cell::new(0),
    };
    let cancelled = || EditorError::new("CANCELLED", "scope revoked");
    assert_eq!(
        prepare_error(
            &provider,
            &request(),
            &FontFetchPolicy::default(),
            &FontImportPolicy::default(),
            &mut || Err(cancelled())
        )
        .code,
        "CANCELLED"
    );
    assert_eq!(provider.calls.get(), 0);
    let checks = Cell::new(0);
    let mut checkpoint = || {
        checks.set(checks.get() + 1);
        if checks.get() == 2 {
            Err(cancelled())
        } else {
            Ok(())
        }
    };
    assert_eq!(
        prepare_error(
            &provider,
            &request(),
            &FontFetchPolicy::default(),
            &FontImportPolicy::default(),
            &mut checkpoint
        )
        .code,
        "CANCELLED"
    );
    assert_eq!(provider.calls.get(), 1);
}

#[test]
fn invalid_fixed_requests_and_unbounded_fetch_policies_never_acquire_bytes() {
    let provider = BytesSource {
        calls: Cell::new(0),
    };
    for mutation in 0..4 {
        let mut input = request();
        match mutation {
            0 => input.expected_revision.clear(),
            1 => input.font_id.push('\n'),
            2 => input.catalog_id = "google-gudea-bold-v1".into(),
            3 => input.required_text.push('\0'),
            _ => unreachable!(),
        }
        assert!(
            prepare_synthesis_font(
                &provider,
                &NoInspector,
                &input,
                &FontFetchPolicy::default(),
                &FontImportPolicy::default(),
                &mut || Ok(())
            )
            .is_err()
        );
    }
    for policy in [
        FontFetchPolicy {
            max_bytes: MAX_FONT_BYTES + 1,
            ..FontFetchPolicy::default()
        },
        FontFetchPolicy {
            deadline_ms: 30_001,
            ..FontFetchPolicy::default()
        },
        FontFetchPolicy {
            deadline_ms: 0,
            ..FontFetchPolicy::default()
        },
    ] {
        assert_eq!(
            prepare_error(
                &provider,
                &request(),
                &policy,
                &FontImportPolicy::default(),
                &mut || Ok(())
            )
            .code,
            "FONT_SYNTHESIS_INVALID"
        );
    }
    assert_eq!(provider.calls.get(), 0);
}

struct ProofStore(DerivedFontProof);
impl crate::font_derived::proof::DerivedFontProofRead for ProofStore {
    fn read_derived_font_proof(
        &self,
        _: &ReadDerivedFontProof,
    ) -> Result<DerivedFontProof, EditorError> {
        Ok(self.0.clone())
    }
}

#[test]
fn typed_proof_read_requires_exact_fixed_scope_and_compiled_binding() {
    let input = ReadDerivedFontProof {
        project_id: "p".into(),
        revision: "r".into(),
        font_id: "font".into(),
    };
    let stored = DerivedFontProof {
        project_id: "p".into(),
        revision: "r".into(),
        font_id: "font".into(),
        source: pinned_source(ID).unwrap().source,
    };
    let result = proof::read(&ProofStore(stored.clone()), &input).unwrap();
    assert_eq!(result.source.binding.actual.style, FontStyle::Normal);
    assert_eq!(result.source.binding.requested.style, FontStyle::Italic);
    for mutation in 0..5 {
        let mut changed = stored.clone();
        match mutation {
            0 => {
                let mut raw: serde_json::Value =
                    serde_json::from_str(&changed.source.proof_json).unwrap();
                raw["schema_version"] = serde_json::json!(2);
                changed.source.proof_json = serde_json::to_string(&raw).unwrap();
            }
            1 => changed.project_id = "other".into(),
            2 => changed.revision = "new-head".into(),
            3 => changed.font_id = "other-font".into(),
            4 => changed.source.proof_json.clear(),
            _ => unreachable!(),
        }
        assert!(
            proof::read(&ProofStore(changed), &input).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn every_approved_normal_output_has_one_explicit_base_route_with_complete_provenance() {
    let normal = |weight, style| matches!(weight, 400 | 700) && style == FontStyle::Normal;
    let expected = crate::font_import::catalog::static_font_inventory()
        .iter()
        .filter(|s| normal(s.weight, s.style))
        .count()
        + crate::font_catalog::static_sources()
            .filter(|s| normal(s.approved.source.weight, s.approved.source.style))
            .count()
        + crate::font_derived::catalog()
            .unwrap()
            .iter()
            .filter(|s| normal(s.source.face.weight, s.source.face.style))
            .count()
        + crate::font_named::compiled_sources()
            .unwrap()
            .iter()
            .filter(|s| normal(s.source.face.weight, s.source.face.style))
            .count()
        + crate::font_color_static::compiled_sources()
            .unwrap()
            .iter()
            .filter(|s| normal(s.source.face.weight, s.source.face.style))
            .count();
    let sources = catalog().unwrap();
    assert_eq!(sources.len(), expected);
    let mut routes = std::collections::BTreeSet::new();
    let mut bindings = std::collections::BTreeSet::new();
    for source in sources {
        assert!(bindings.insert(source.binding.binding_id.clone()));
        let base = base_source(&source.source.face.id).unwrap();
        let mut input = request();
        input.catalog_id = source.source.face.id.clone();
        let font = expected_import_resource(&input).unwrap();
        assert_eq!(font.style, FontStyle::Normal);
        assert_eq!(font.weight, source.binding.requested.weight);
        assert_eq!(
            font.content.content_hash,
            source.binding.actual.content_hash
        );
        assert_eq!(
            binding_for_resource(&font).unwrap().unwrap(),
            source.binding
        );
        let json: Value = serde_json::from_str(&source.source.proof_json).unwrap();
        assert_eq!(canonical(&json).unwrap(), source.source.proof_json);
        assert_eq!(
            source.source.source_sha256,
            source.binding.actual.content_hash
        );
        assert_eq!(
            source.source.source_size_bytes,
            source.binding.actual.size_bytes
        );
        let (route, id) = match &base {
            SynthesisBaseSource::RepositoryStatic(pin) => ("static_v1", pin.catalog_id),
            SynthesisBaseSource::ApprovedStatic(pin) => ("static_v3", pin.source.catalog_id),
            SynthesisBaseSource::Outline(pin) => ("derived_v1", pin.source.face.id.as_str()),
            SynthesisBaseSource::RenamedOutline(pin) => ("derived_v2", pin.source.face.id.as_str()),
            SynthesisBaseSource::CompleteColor(pin) => {
                ("color_static_v1", pin.source.face.id.as_str())
            }
        };
        routes.insert(route);
        assert_eq!(source.binding.actual.id, id);
        if ACTUAL_SOURCE_IDS.contains(&id) && route == "static_v1" {
            assert!(json.get("base").is_none());
            assert_eq!(
                source,
                from_actual(pinned_font_source(id).unwrap()).unwrap()
            );
        } else {
            assert_eq!(json["base"]["route"], route);
            assert_eq!(json["base"]["catalog_id"], id);
            assert_eq!(
                json["base"]["display_family"],
                display_family(&source.source.face.id).unwrap()
            );
            assert_eq!(
                json["base"]["render_profile"],
                serde_json::to_value(render_profile(&source.source.face.id).unwrap()).unwrap()
            );
            if let SynthesisBaseSource::Outline(pin)
            | SynthesisBaseSource::RenamedOutline(pin)
            | SynthesisBaseSource::CompleteColor(pin) = &base
            {
                assert_eq!(
                    json["base"]["source"],
                    serde_json::to_value(&pin.source).unwrap()
                );
                assert!(json["attribution"].is_null());
                assert_eq!(font.license.as_ref().unwrap().attribution, None);
                assert!(
                    actual_source(&PinnedDerivedFont {
                        source: source.source.clone()
                    })
                    .is_err()
                );
            }
        }
    }
    assert_eq!(
        routes,
        std::collections::BTreeSet::from([
            "static_v1",
            "static_v3",
            "derived_v1",
            "derived_v2",
            "color_static_v1"
        ])
    );
}

#[test]
fn base_route_and_nested_recipe_cannot_be_resealed_into_an_approved_binding() {
    let source = catalog()
        .unwrap()
        .into_iter()
        .find(|s| {
            matches!(
                base_source(&s.source.face.id).unwrap(),
                SynthesisBaseSource::RenamedOutline(_)
            )
        })
        .unwrap();
    let original = source.source;
    for field in 0..4 {
        let mut pin = PinnedDerivedFont {
            source: original.clone(),
        };
        let mut proof: Value = serde_json::from_str(&pin.source.proof_json).unwrap();
        proof.as_object_mut().unwrap().remove("derivation_id");
        match field {
            0 => proof["base"]["route"] = json!("static_v1"),
            1 => proof["base"]["display_family"] = json!("Arbitrary"),
            2 => proof["base"]["source"]["proof_json"] = json!("{}"),
            _ => proof["base"]["render_profile"] = json!("static_outline_fallback"),
        };
        let id = format!(
            "{FONT_SYNTHESIS_BINDING_ID_PREFIX}{:x}",
            Sha256::digest(canonical(&proof).unwrap().as_bytes())
        );
        proof["derivation_id"] = json!(id);
        pin.source.face.derivation_id = id;
        pin.source.proof_json = canonical(&proof).unwrap();
        assert!(validate_pinned_source(&pin).is_err());
    }
}

#[test]
fn base_limits_keep_static_thirty_seconds_and_derived_source_output_caps_separate() {
    for source in catalog().unwrap() {
        let limits = acquisition_limits(&source.source.face.id).unwrap();
        assert_eq!(limits.max_output_bytes, MAX_FONT_BYTES);
        match base_source(&source.source.face.id).unwrap() {
            SynthesisBaseSource::RepositoryStatic(_) | SynthesisBaseSource::ApprovedStatic(_) => {
                assert_eq!(limits.max_deadline_ms, 30_000);
                assert_eq!(limits.max_source_bytes, MAX_FONT_BYTES);
            }
            SynthesisBaseSource::Outline(_) => {
                assert_eq!(limits.max_deadline_ms, 120_000);
                assert_eq!(limits.max_source_bytes, MAX_FONT_BYTES);
            }
            SynthesisBaseSource::RenamedOutline(_) | SynthesisBaseSource::CompleteColor(_) => {
                assert_eq!(limits.max_deadline_ms, 120_000);
                assert_eq!(limits.max_source_bytes, 64 * 1024 * 1024);
            }
        }
    }
}
