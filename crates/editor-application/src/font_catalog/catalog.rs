use super::*;
use crate::font_import::catalog::static_font_inventory;
use opencut_contracts::{font_catalog::*, font_import::FontCatalogEntry};

fn static_face(
    source: &PinnedFontSource,
    display: &str,
    kind: FontKind,
    profile: FontRenderProfile,
    alias: Option<&str>,
) -> FontFace {
    FontFace {
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
fn static_details(source: &PinnedFontSource) -> FontCatalogEntry {
    FontCatalogEntry {
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
    source: &opencut_contracts::font_derived::FontDerivedSource,
    route: super::selection::FontAcquisition,
) -> Result<FontFace, EditorError> {
    let (display, profile) = if route == super::selection::FontAcquisition::RenamedOutline {
        let display = crate::font_named::display_family(&source.face.id)?;
        let profile: FontRenderProfile = serde_json::from_value(serde_json::Value::String(
            crate::font_named::render_profile(&source.face.id)?,
        ))
        .map_err(|_| {
            EditorError::new(
                "FONT_SOURCE_INTEGRITY_ERROR",
                "Unknown pinned derived render profile",
            )
        })?;
        (display, profile)
    } else {
        (source.face.family.clone(), FontRenderProfile::StaticOutline)
    };
    let alias = (display != source.face.family).then(||
        "Original Classic family is shown; the imported derived face uses its licensed, deterministic native name.".into());
    Ok(FontFace {
        id: source.face.id.clone(),
        display_family: display,
        family: source.face.family.clone(),
        weight: source.face.weight,
        style: source.face.style,
        requested_style: None,
        kind: FontKind::Derived,
        content_hash: source.face.content_hash.clone(),
        size_bytes: source.face.size_bytes.clone(),
        render_profile: profile,
        alias_reason: alias,
        import_available: false,
        unavailable_reason: Some("Host source availability has not been attached.".into()),
    })
}

fn synthesis_face(
    source: &opencut_contracts::font_synthesis::FontSynthesisSource,
) -> Result<FontFace, EditorError> {
    let actual = &source.binding.actual;
    // The source module owns the fixed recipe set. Discovery must not silently
    // turn a requested italic into native italic or add synthetic weight.
    if source.source.face.family != actual.family
        || source.source.face.weight != actual.weight
        || source.source.face.style != actual.style
        || source.source.face.content_hash != actual.content_hash
        || source.source.face.size_bytes != actual.size_bytes
        || source.binding.requested.weight != actual.weight
        || actual.style != opencut_contracts::scene::FontStyle::Normal
        || source.binding.requested.style != opencut_contracts::scene::FontStyle::Italic
    {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Synthetic request differs from its fixed native face",
        ));
    }
    if crate::font_synthesis::source(&source.source.face.id)? != *source {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Synthesis source differs from the compiled base route",
        ));
    }
    let display = crate::font_synthesis::display_family(&source.source.face.id)?;
    let profile = crate::font_synthesis::render_profile(&source.source.face.id)?;
    Ok(FontFace {
        id:source.source.face.id.clone(),display_family:display.into(),family:actual.family.clone(),
        weight:actual.weight,style:actual.style,requested_style:Some(source.binding.requested.style),
        kind:FontKind::Synthesis,content_hash:actual.content_hash.clone(),size_bytes:actual.size_bytes.clone(),
        render_profile:profile,
        alias_reason:Some("Explicit Chromium synthetic italic; the imported font bytes and native face remain normal.".into()),
        import_available:false,unavailable_reason:Some("Host source availability has not been attached.".into()),
    })
}

fn color_face(
    source: &opencut_contracts::font_derived::FontDerivedSource,
) -> Result<FontFace, EditorError> {
    crate::font_color_static::face(source)
}
/// One inventory retains every approved source and original native identity.
pub fn catalog() -> Result<Vec<FontFace>, EditorError> {
    use super::selection::FontAcquisition;
    let mut faces: Vec<_> = static_font_inventory()
        .iter()
        .map(|pin| {
            static_face(
                pin,
                pin.family,
                FontKind::Static,
                FontRenderProfile::StaticOutline,
                None,
            )
        })
        .collect();
    for pin in crate::font_derived::catalog()? {
        faces.push(derived_face(&pin.source, FontAcquisition::Outline)?);
    }
    for pin in crate::font_named::catalog()? {
        faces.push(derived_face(&pin.source, FontAcquisition::RenamedOutline)?);
    }
    for pin in crate::font_color_static::compiled_sources()? {
        faces.push(color_face(&pin.source)?);
    }
    faces.extend(static_sources().map(|entry| {
        static_face(
            &entry.approved.source,
            entry.display_family,
            FontKind::Static,
            entry.approved.render_profile,
            entry.alias_reason,
        )
    }));
    for source in crate::font_synthesis::catalog()? {
        faces.push(synthesis_face(&source)?);
    }
    faces.extend(super::distribution::catalog());
    let mut seen = std::collections::BTreeSet::new();
    for face in &faces {
        if !seen.insert(&face.id) {
            return Err(EditorError::new(
                "FONT_SOURCE_INTEGRITY_ERROR",
                "Duplicate approved font source identity",
            ));
        }
    }
    let preference = |face: &FontFace| {
        if face.kind == FontKind::Synthesis {
            crate::font_synthesis::base_priority(&face.id).expect("approved synthesis source")
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
    Ok(faces)
}
pub fn source(request: &FontSourceRequest) -> Result<FontSource, EditorError> {
    use super::selection::{self, FontAcquisition};
    let (face, details, license_url, license_sha256) = match selection::select(&request.catalog_id)?
    {
        FontAcquisition::RepositoryStatic => {
            let pin = crate::font_import::catalog::pinned_font_source(&request.catalog_id)?;
            (
                static_face(
                    pin,
                    pin.family,
                    FontKind::Static,
                    FontRenderProfile::StaticOutline,
                    None,
                ),
                FontSourceDetails::Static(static_details(pin)),
                pin.license_url.into(),
                pin.license_sha256.into(),
            )
        }
        FontAcquisition::ApprovedStatic => {
            let entry = static_sources()
                .find(|entry| entry.approved.source.catalog_id == request.catalog_id)
                .ok_or_else(|| EditorError::new("NOT_FOUND", "Unknown approved static font"))?;
            let pin = &entry.approved.source;
            (
                static_face(
                    pin,
                    entry.display_family,
                    FontKind::Static,
                    entry.approved.render_profile,
                    entry.alias_reason,
                ),
                FontSourceDetails::Static(static_details(pin)),
                pin.license_url.into(),
                pin.license_sha256.into(),
            )
        }
        route @ (FontAcquisition::Outline
        | FontAcquisition::RenamedOutline
        | FontAcquisition::CompleteColor) => {
            let source = match route {
                FontAcquisition::Outline => {
                    crate::font_derived::source(&request.catalog_id)?.source
                }
                FontAcquisition::RenamedOutline => {
                    crate::font_named::source(&request.catalog_id)?.source
                }
                _ => crate::font_color_static::source(&request.catalog_id)?.source,
            };
            let face = if route == FontAcquisition::CompleteColor {
                color_face(&source)?
            } else {
                derived_face(&source, route)?
            };
            use sha2::{Digest, Sha256};
            let hash = format!("{:x}", Sha256::digest(source.license_notice.as_bytes()));
            let url = source.license_url.clone();
            (face, FontSourceDetails::Derived(source), url, hash)
        }
        FontAcquisition::Synthesis => {
            let source = crate::font_synthesis::source(&request.catalog_id)?;
            let face = synthesis_face(&source)?;
            use sha2::{Digest, Sha256};
            let hash = format!(
                "{:x}",
                Sha256::digest(source.source.license_notice.as_bytes())
            );
            let url = source.source.license_url.clone();
            (face, FontSourceDetails::Synthesis(source), url, hash)
        }
        FontAcquisition::Distribution => {
            let source = super::distribution::source(&FontSourceRequest {
                catalog_id: request.catalog_id.clone(),
            })?;
            let face = source.face.clone();
            let url = source.license_url.clone();
            let hash = source.license_sha256.clone();
            (face, FontSourceDetails::Distribution(source), url, hash)
        }
    };
    Ok(FontSource {
        face,
        details,
        license_url,
        license_sha256,
    })
}

#[cfg(test)]
mod synthesis_discovery_tests {
    use super::*;
    use opencut_contracts::scene::FontStyle;

    #[test]
    fn explicit_synthetic_italic_preserves_actual_bytes_and_normal_descriptor() {
        let sources = crate::font_synthesis::catalog().unwrap();
        assert!(
            sources.len() > 11,
            "All approved normal source routes participate"
        );
        for expected in sources {
            let response = source(&FontSourceRequest {
                catalog_id: expected.source.face.id.clone(),
            })
            .unwrap();
            assert_eq!(response.face.style, FontStyle::Normal);
            assert_eq!(response.face.requested_style, Some(FontStyle::Italic));
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
            let FontSourceDetails::Synthesis(details) = response.details else {
                panic!("wrong route")
            };
            assert_eq!(details.binding.binding_id, expected.binding.binding_id);
            assert_eq!(response.license_url, expected.source.license_url);
            let repeated = source(&FontSourceRequest {
                catalog_id: response.face.id,
            })
            .unwrap();
            let FontSourceDetails::Synthesis(repeated) = repeated.details else {
                panic!("wrong route")
            };
            assert_eq!(repeated.binding, details.binding);
        }
    }

    #[test]
    fn ordinary_catalog_faces_do_not_request_synthesis() {
        let faces = catalog().unwrap();
        assert!(faces.iter().any(|face| face.kind == FontKind::Synthesis));
        for face in faces {
            assert_eq!(
                face.requested_style,
                if face.kind == FontKind::Synthesis {
                    Some(FontStyle::Italic)
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
        for face in faces.iter().filter(|face| face.kind == FontKind::Synthesis) {
            let source = crate::font_synthesis::source(&face.id).unwrap();
            assert_eq!(
                face.display_family,
                crate::font_synthesis::display_family(&face.id).unwrap()
            );
            assert_eq!(
                face.render_profile,
                crate::font_synthesis::render_profile(&face.id).unwrap()
            );
            assert_eq!(face.family, source.binding.actual.family);
            assert_eq!(face.content_hash, source.binding.actual.content_hash);
            assert_eq!(face.style, FontStyle::Normal);
            assert_eq!(face.requested_style, Some(FontStyle::Italic));
            renamed |= face.family != face.display_family;
            colored |= face.render_profile != FontRenderProfile::StaticOutline;
            let priority = crate::font_synthesis::base_priority(&face.id).unwrap();
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
        let original = crate::font_synthesis::catalog().unwrap().remove(0);
        let mut changed = original.clone();
        changed.source.face.style = FontStyle::Italic;
        assert!(synthesis_face(&changed).is_err());
        let mut changed = original;
        changed.binding.requested.weight = 400;
        assert!(synthesis_face(&changed).is_err());
    }
}
