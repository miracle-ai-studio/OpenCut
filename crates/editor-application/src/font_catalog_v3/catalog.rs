use super::*;
use crate::font_import_v1::catalog::font_catalog_v1;
use opencut_contracts::{font_catalog_v3::*, font_import_v1::FontCatalogEntryV1};

fn static_face(
    source: &PinnedFontSourceV1,
    display: &str,
    kind: FontCatalogKindV3,
    profile: FontRenderProfileV3,
    alias: Option<&str>,
) -> FontCatalogFaceV3 {
    FontCatalogFaceV3 {
        id: source.catalog_id.into(),
        display_family: display.into(),
        family: source.family.into(),
        weight: source.weight,
        style: source.style,
        requested_style: None,
        kind,
        content_hash: source.content_hash.into(),
        size_bytes: source.size_bytes.to_string(),
        render_profile: profile,
        alias_reason: alias.map(Into::into),
        import_available: false,
        unavailable_reason: Some("Host source availability has not been attached.".into()),
    }
}
fn static_details(source: &PinnedFontSourceV1) -> FontCatalogEntryV1 {
    FontCatalogEntryV1 {
        id: source.catalog_id.into(),
        family: source.family.into(),
        weight: source.weight,
        style: source.style,
        source_url: source.source_url.into(),
        source_commit: source.source_commit.into(),
        content_hash: source.content_hash.into(),
        size_bytes: source.size_bytes.to_string(),
        license_id: source.license_identifier.into(),
        license_notice: source.license_notice.into(),
    }
}
fn derived_face(
    source: &opencut_contracts::font_derived_v1::FontDerivedSourceV1,
    kind: FontCatalogKindV3,
) -> Result<FontCatalogFaceV3, EditorError> {
    let (display, profile) = if kind == FontCatalogKindV3::DerivedV2 {
        let display = crate::font_derived_v2::display_family(&source.face.id)?;
        let profile: FontRenderProfileV3 = serde_json::from_value(serde_json::Value::String(
            crate::font_derived_v2::render_profile(&source.face.id)?,
        ))
        .map_err(|_| {
            EditorError::new(
                "FONT_SOURCE_INTEGRITY_ERROR",
                "Unknown pinned derived render profile",
            )
        })?;
        (display, profile)
    } else {
        (
            source.face.family.clone(),
            FontRenderProfileV3::StaticOutline,
        )
    };
    let alias = (display != source.face.family).then(||
        "Original Classic family is shown; the imported derived face uses its licensed, deterministic native name.".into());
    Ok(FontCatalogFaceV3 {
        id: source.face.id.clone(),
        display_family: display,
        family: source.face.family.clone(),
        weight: source.face.weight,
        style: source.face.style,
        requested_style: None,
        kind,
        content_hash: source.face.content_hash.clone(),
        size_bytes: source.face.size_bytes.clone(),
        render_profile: profile,
        alias_reason: alias,
        import_available: false,
        unavailable_reason: Some("Host source availability has not been attached.".into()),
    })
}

fn synthesis_face(
    source: &opencut_contracts::font_synthesis_v1::FontSynthesisSourceV1,
) -> Result<FontCatalogFaceV3, EditorError> {
    let actual = &source.binding.actual;
    // The source module owns the fixed recipe set. Discovery must not silently
    // turn a requested italic into native italic or add synthetic weight.
    if source.source.face.family != actual.family
        || source.source.face.weight != actual.weight
        || source.source.face.style != actual.style
        || source.source.face.content_hash != actual.content_hash
        || source.source.face.size_bytes != actual.size_bytes
        || source.binding.requested.weight != actual.weight
        || actual.style != opencut_contracts::project_v4::FontStyleV4::Normal
        || source.binding.requested.style != opencut_contracts::project_v4::FontStyleV4::Italic
    {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Synthetic request differs from its fixed native face",
        ));
    }
    if crate::font_synthesis_v1::source(&source.source.face.id)? != *source {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Synthesis source differs from the compiled base route",
        ));
    }
    let display = crate::font_synthesis_v1::display_family(&source.source.face.id)?;
    let profile = crate::font_synthesis_v1::render_profile(&source.source.face.id)?;
    Ok(FontCatalogFaceV3 {
        id:source.source.face.id.clone(),display_family:display.into(),family:actual.family.clone(),
        weight:actual.weight,style:actual.style,requested_style:Some(source.binding.requested.style),
        kind:FontCatalogKindV3::SynthesisV1,content_hash:actual.content_hash.clone(),size_bytes:actual.size_bytes.clone(),
        render_profile:profile,
        alias_reason:Some("Explicit Chromium synthetic italic; the imported font bytes and native face remain normal.".into()),
        import_available:false,unavailable_reason:Some("Host source availability has not been attached.".into()),
    })
}

/// Entries preserve every pinned route. No family/name normalization chooses an
/// import and no unpinned declaration becomes a downloadable source.
pub fn catalog() -> Result<Vec<FontCatalogFaceV3>, EditorError> {
    let mut faces: Vec<_> = font_catalog_v1()
        .iter()
        .map(|source| {
            static_face(
                source,
                source.family,
                FontCatalogKindV3::StaticV1,
                FontRenderProfileV3::StaticOutline,
                None,
            )
        })
        .collect();
    for pin in crate::font_derived_v1::catalog()? {
        faces.push(derived_face(&pin.source, FontCatalogKindV3::DerivedV1)?);
    }
    for pin in crate::font_derived_v2::catalog()? {
        faces.push(derived_face(&pin.source, FontCatalogKindV3::DerivedV2)?);
    }
    faces.extend(static_sources_v3().map(|entry| {
        static_face(
            &entry.approved.source,
            entry.display_family,
            FontCatalogKindV3::StaticV3,
            entry.approved.render_profile,
            entry.alias_reason,
        )
    }));
    for source in crate::font_synthesis_v1::catalog()? {
        faces.push(synthesis_face(&source)?);
    }
    let preference = |face: &FontCatalogFaceV3| {
        if face.kind == FontCatalogKindV3::SynthesisV1 {
            crate::font_synthesis_v1::base_priority(&face.id).expect("compiled synthesis source")
        } else {
            0
        }
    };
    faces.sort_by(|a, b| {
        (
            &a.display_family,
            a.weight,
            format!("{:?}", a.requested_style.unwrap_or(a.style)),
            preference(a),
            &a.id,
        )
            .cmp(&(
                &b.display_family,
                b.weight,
                format!("{:?}", b.requested_style.unwrap_or(b.style)),
                preference(b),
                &b.id,
            ))
    });
    let mut ids = std::collections::BTreeSet::new();
    if faces.iter().any(|face| !ids.insert(&face.id)) {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Compiled font catalog has duplicate identities",
        ));
    }
    Ok(faces)
}
pub fn source(request: &FontSourceRequestV3) -> Result<FontSourceV3, EditorError> {
    let (face, details) = match request.kind {
        FontCatalogKindV3::StaticV1 => {
            let pin = crate::font_import_v1::catalog::pinned_font_source_v1(&request.catalog_id)?;
            (
                static_face(
                    pin,
                    pin.family,
                    request.kind,
                    FontRenderProfileV3::StaticOutline,
                    None,
                ),
                FontSourceDetailsV3::StaticV1(static_details(pin)),
            )
        }
        FontCatalogKindV3::StaticV3 => {
            let entry = static_sources_v3()
                .find(|x| x.approved.source.catalog_id == request.catalog_id)
                .ok_or_else(|| EditorError::new("NOT_FOUND", "Unknown static V3 source"))?;
            (
                static_face(
                    &entry.approved.source,
                    entry.display_family,
                    request.kind,
                    entry.approved.render_profile,
                    entry.alias_reason,
                ),
                FontSourceDetailsV3::StaticV3(static_details(&entry.approved.source)),
            )
        }
        FontCatalogKindV3::DerivedV1 => {
            let pin = crate::font_derived_v1::source(&request.catalog_id)?;
            (
                derived_face(&pin.source, request.kind)?,
                FontSourceDetailsV3::DerivedV1(pin.source),
            )
        }
        FontCatalogKindV3::DerivedV2 => {
            let pin = crate::font_derived_v2::source(&request.catalog_id)?;
            (
                derived_face(&pin.source, request.kind)?,
                FontSourceDetailsV3::DerivedV2(pin.source),
            )
        }
        FontCatalogKindV3::SynthesisV1 => {
            let source = crate::font_synthesis_v1::source(&request.catalog_id)?;
            (
                synthesis_face(&source)?,
                FontSourceDetailsV3::SynthesisV1(source),
            )
        }
    };
    let (license_url, license_sha256) = match &details {
        FontSourceDetailsV3::StaticV1(source) => {
            let pin = crate::font_import_v1::catalog::pinned_font_source_v1(&source.id)?;
            (pin.license_url.into(), pin.license_sha256.into())
        }
        FontSourceDetailsV3::StaticV3(source) => {
            let pin = &native_source_v3(&source.id)?.source;
            (pin.license_url.into(), pin.license_sha256.into())
        }
        FontSourceDetailsV3::DerivedV1(source) | FontSourceDetailsV3::DerivedV2(source) => {
            use sha2::{Digest, Sha256};
            (
                source.license_url.clone(),
                format!("{:x}", Sha256::digest(source.license_notice.as_bytes())),
            )
        }
        FontSourceDetailsV3::SynthesisV1(source) => {
            use sha2::{Digest, Sha256};
            (
                source.source.license_url.clone(),
                format!(
                    "{:x}",
                    Sha256::digest(source.source.license_notice.as_bytes())
                ),
            )
        }
    };
    Ok(FontSourceV3 {
        face,
        details,
        license_url,
        license_sha256,
    })
}

#[cfg(test)]
mod synthesis_discovery_tests {
    use super::*;
    use opencut_contracts::project_v4::FontStyleV4;

    #[test]
    fn explicit_synthetic_italic_preserves_actual_bytes_and_normal_descriptor() {
        let sources = crate::font_synthesis_v1::catalog().unwrap();
        assert!(
            sources.len() > 11,
            "All approved normal source routes participate"
        );
        for expected in sources {
            let response = source(&FontSourceRequestV3 {
                catalog_id: expected.source.face.id.clone(),
                kind: FontCatalogKindV3::SynthesisV1,
            })
            .unwrap();
            assert_eq!(response.face.style, FontStyleV4::Normal);
            assert_eq!(response.face.requested_style, Some(FontStyleV4::Italic));
            assert_eq!(response.face.weight, expected.binding.actual.weight);
            assert_eq!(response.face.family, expected.binding.actual.family);
            assert_eq!(
                response.face.content_hash,
                expected.binding.actual.content_hash
            );
            assert_eq!(response.face.size_bytes, expected.binding.actual.size_bytes);
            assert_ne!(response.face.id, expected.binding.actual.id);
            assert!(
                !response.face.import_available,
                "Host availability must be attached separately"
            );
            let FontSourceDetailsV3::SynthesisV1(details) = response.details else {
                panic!("wrong route")
            };
            assert_eq!(details.binding.binding_id, expected.binding.binding_id);
            assert_eq!(response.license_url, expected.source.license_url);
            assert!(
                source(&FontSourceRequestV3 {
                    catalog_id: response.face.id,
                    kind: FontCatalogKindV3::StaticV1,
                })
                .is_err()
            );
        }
    }

    #[test]
    fn ordinary_catalog_faces_do_not_request_synthesis() {
        let faces = catalog().unwrap();
        assert!(
            faces
                .iter()
                .any(|face| face.kind == FontCatalogKindV3::SynthesisV1)
        );
        for face in faces {
            assert_eq!(
                face.requested_style,
                if face.kind == FontCatalogKindV3::SynthesisV1 {
                    Some(FontStyleV4::Italic)
                } else {
                    None
                }
            );
        }
    }

    #[test]
    fn synthesis_discovery_inherits_base_display_profile_and_stable_route_preference() {
        let faces = catalog().unwrap();
        let mut last = std::collections::BTreeMap::new();
        let mut renamed = false;
        let mut colored = false;
        for face in faces
            .iter()
            .filter(|face| face.kind == FontCatalogKindV3::SynthesisV1)
        {
            let source = crate::font_synthesis_v1::source(&face.id).unwrap();
            assert_eq!(
                face.display_family,
                crate::font_synthesis_v1::display_family(&face.id).unwrap()
            );
            assert_eq!(
                face.render_profile,
                crate::font_synthesis_v1::render_profile(&face.id).unwrap()
            );
            assert_eq!(face.family, source.binding.actual.family);
            assert_eq!(face.content_hash, source.binding.actual.content_hash);
            assert_eq!(face.style, FontStyleV4::Normal);
            assert_eq!(face.requested_style, Some(FontStyleV4::Italic));
            renamed |= face.family != face.display_family;
            colored |= face.render_profile != FontRenderProfileV3::StaticOutline;
            let priority = crate::font_synthesis_v1::base_priority(&face.id).unwrap();
            if let Some(previous) =
                last.insert((face.display_family.clone(), face.weight), priority)
            {
                assert!(
                    priority >= previous,
                    "Discovery must not change the selected normal-base preference"
                );
            }
        }
        assert!(
            renamed,
            "Renamed OCDF and explicit aliases retain their display family"
        );
        assert!(
            colored,
            "Synthesis cannot relabel an approved color or bitmap base as outline"
        );
    }

    #[test]
    fn discovery_rejects_a_synthesis_binding_that_relabels_native_style_or_weight() {
        let original = crate::font_synthesis_v1::catalog().unwrap().remove(0);
        let mut changed = original.clone();
        changed.source.face.style = FontStyleV4::Italic;
        assert!(synthesis_face(&changed).is_err());
        let mut changed = original;
        changed.binding.requested.weight = 400;
        assert!(synthesis_face(&changed).is_err());
    }
}
