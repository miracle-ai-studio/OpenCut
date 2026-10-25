//! Finite acquisition routes over existing compiled sources. No catalog alias
//! or caller-authored proof can introduce a URL, recipe or native face.
use super::*;
use crate::font_catalog::{ApprovedFontSource, static_sources};
use opencut_contracts::font_catalog::FontRenderProfile;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub enum SynthesisBaseSource {
    RepositoryStatic(&'static PinnedFontSource),
    ApprovedStatic(&'static ApprovedFontSource),
    Outline(PinnedDerivedFont),
    RenamedOutline(PinnedDerivedFont),
    CompleteColor(PinnedDerivedFont),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SynthesisAcquisitionLimits {
    pub max_source_bytes: u64,
    pub max_output_bytes: u64,
    pub max_deadline_ms: u64,
}
pub(super) struct Record {
    pub source: FontSynthesisSource,
    pub base: SynthesisBaseSource,
    pub display: String,
    pub render_profile: FontRenderProfile,
    pub license: ResourceLicense,
}
struct Catalog {
    records: Vec<Record>,
    ids: BTreeMap<String, usize>,
    bindings: BTreeMap<String, usize>,
}
fn eligible(style: FontStyle, weight: u16) -> bool {
    style == FontStyle::Normal && matches!(weight, 400 | 700)
}
fn entry(pin: &PinnedFontSource) -> FontCatalogEntry {
    FontCatalogEntry {
        id: pin.catalog_id.into(),
        family: pin.family.into(),
        weight: pin.weight,
        style: pin.style,
        source_url: pin.source_url.into(),
        source_commit: pin.source_commit.into(),
        content_hash: pin.content_hash.into(),
        size_bytes: pin.size_bytes.to_string(),
        license_id: pin.license_identifier.into(),
        license_notice: pin.license_notice.into(),
    }
}
fn static_record(
    base: SynthesisBaseSource,
    pin: &PinnedFontSource,
    display: &str,
    profile: FontRenderProfile,
    route: &str,
) -> Result<Record, EditorError> {
    if format!("{:x}", Sha256::digest(pin.license_notice.as_bytes())) != pin.license_sha256 {
        return Err(invalid(
            "Compiled base license differs from its pinned digest",
        ));
    }
    let license = pin.import_spec("source".into(), String::new()).license;
    let actual = entry(pin);
    if route == "static_v1" && ACTUAL_SOURCE_IDS.contains(&pin.catalog_id) {
        return Ok(Record {
            source: from_actual(pin)?,
            base,
            display: display.into(),
            render_profile: profile,
            license,
        });
    }
    let provenance = json!({"face":actual,"license_url":pin.license_url,"license_sha256":pin.license_sha256,"attribution":pin.attribution});
    make(base, actual, display, profile, route, provenance, license)
}
#[derive(Clone, Copy)]
enum DerivedRoute {
    V1,
    V2,
    ColorStaticV1,
}
fn derived_record(pin: PinnedDerivedFont, route: DerivedRoute) -> Result<Record, EditorError> {
    let v2 = matches!(route, DerivedRoute::V2);
    let color = matches!(route, DerivedRoute::ColorStaticV1);
    let source = &pin.source;
    let actual = FontCatalogEntry {
        id: source.face.id.clone(),
        family: source.face.family.clone(),
        weight: source.face.weight,
        style: source.face.style,
        source_url: source.source_url.clone(),
        source_commit: source.source_commit.clone(),
        content_hash: source.face.content_hash.clone(),
        size_bytes: source.face.size_bytes.clone(),
        license_id: source.license_id.clone(),
        license_notice: source.license_notice.clone(),
    };
    let display = if color {
        crate::font_color_static::display_family(&source.face.id)?
    } else if v2 {
        crate::font_named::display_family(&source.face.id)?
    } else {
        source.face.family.clone()
    };
    let profile = if color {
        FontRenderProfile::ColrV1
    } else if v2 {
        serde_json::from_value(Value::String(crate::font_named::render_profile(
            &source.face.id,
        )?))
        .map_err(|_| invalid("Unknown approved base render profile"))?
    } else {
        FontRenderProfile::StaticOutline
    };
    let license = ResourceLicense {
        identifier: Some(source.license_id.clone()),
        source_url: Some(source.license_url.clone()),
        notice: Some(source.license_notice.clone()),
        attribution: None,
    };
    let provenance =
        serde_json::to_value(source).map_err(|_| invalid("Invalid base derivation proof"))?;
    let route = if color {
        "color_static_v1"
    } else if v2 {
        "derived_v2"
    } else {
        "derived_v1"
    };
    let base = if color {
        SynthesisBaseSource::CompleteColor(pin)
    } else if v2 {
        SynthesisBaseSource::RenamedOutline(pin)
    } else {
        SynthesisBaseSource::Outline(pin)
    };
    make(base, actual, &display, profile, route, provenance, license)
}
fn make(
    base: SynthesisBaseSource,
    actual: FontCatalogEntry,
    display: &str,
    render_profile: FontRenderProfile,
    route: &str,
    provenance: Value,
    license: ResourceLicense,
) -> Result<Record, EditorError> {
    let size = actual
        .size_bytes
        .parse::<u64>()
        .map_err(|_| invalid("Base output size is invalid"))?;
    if !eligible(actual.style, actual.weight)
        || size == 0
        || size > MAX_FONT_BYTES
        || size.to_string() != actual.size_bytes
        || license.notice.as_deref() != Some(actual.license_notice.as_str())
        || license.identifier.as_deref() != Some(actual.license_id.as_str())
    {
        return Err(invalid(
            "Synthesis base is not an approved bounded normal output",
        ));
    }
    let id = format!(
        "{}-{}-synthetic-italic-v1",
        actual.id,
        route.replace('_', "-")
    );
    let requested = FontSynthesisRequest {
        weight: actual.weight,
        style: FontStyle::Italic,
    };
    let profile = FontSynthesisProfile::ClassicChromiumItalicV1;
    let license_url = license
        .source_url
        .clone()
        .ok_or_else(|| invalid("Base license URL is missing"))?;
    let mut proof = json!({"schema_version":3,"kind":"browser_font_synthesis","provider":FONT_SYNTHESIS_PROVIDER,
        "profile":profile,"catalog_id":id,"actual":actual,"requested":requested,"license_url":license_url,
        "license_sha256":format!("{:x}",Sha256::digest(actual.license_notice.as_bytes())),"attribution":license.attribution,
        "source_bytes_unchanged":true,
        "base":{"route":route,"catalog_id":actual.id,"display_family":display,"render_profile":render_profile,"source":provenance}});
    let binding_id = format!(
        "{FONT_SYNTHESIS_BINDING_ID_PREFIX}{:x}",
        Sha256::digest(canonical(&proof)?.as_bytes())
    );
    proof["derivation_id"] = Value::String(binding_id.clone());
    let source = FontSynthesisSource {
        source: FontDerivedSource {
            face: FontDerivedFace {
                id,
                family: actual.family.clone(),
                weight: actual.weight,
                style: actual.style,
                derivation_id: binding_id.clone(),
                content_hash: actual.content_hash.clone(),
                size_bytes: actual.size_bytes.clone(),
            },
            source_commit: actual.source_commit.clone(),
            source_url: actual.source_url.clone(),
            source_sha256: actual.content_hash.clone(),
            source_size_bytes: actual.size_bytes.clone(),
            license_url,
            license_id: actual.license_id.clone(),
            license_notice: actual.license_notice.clone(),
            axes: vec![],
            proof_json: canonical(&proof)?,
        },
        binding: FontSynthesisBinding {
            schema_version: 1,
            binding_id,
            profile,
            actual,
            requested,
        },
    };
    Ok(Record {
        source,
        base,
        display: display.into(),
        render_profile,
        license,
    })
}
fn compiled() -> Result<&'static Catalog, EditorError> {
    static CATALOG: std::sync::OnceLock<Result<Catalog, EditorError>> = std::sync::OnceLock::new();
    CATALOG
        .get_or_init(|| {
            let mut records = Vec::new();
            // Preserve the original eleven's ordering, IDs and canonical proof bytes.
            for id in ACTUAL_SOURCE_IDS {
                let pin = pinned_font_source(id)?;
                records.push(static_record(
                    SynthesisBaseSource::RepositoryStatic(pin),
                    pin,
                    pin.family,
                    FontRenderProfile::StaticOutline,
                    "static_v1",
                )?);
            }
            for pin in crate::font_import::catalog::static_font_inventory() {
                if eligible(pin.style, pin.weight) && !ACTUAL_SOURCE_IDS.contains(&pin.catalog_id) {
                    records.push(static_record(
                        SynthesisBaseSource::RepositoryStatic(pin),
                        pin,
                        pin.family,
                        FontRenderProfile::StaticOutline,
                        "static_v1",
                    )?);
                }
            }
            for entry in static_sources() {
                let pin = &entry.approved.source;
                if eligible(pin.style, pin.weight) {
                    records.push(static_record(
                        SynthesisBaseSource::ApprovedStatic(&entry.approved),
                        pin,
                        entry.display_family,
                        entry.approved.render_profile,
                        "static_v3",
                    )?);
                }
            }
            for pin in crate::font_derived::catalog()? {
                if eligible(pin.source.face.style, pin.source.face.weight) {
                    records.push(derived_record(pin, DerivedRoute::V1)?);
                }
            }
            for pin in crate::font_color_static::compiled_sources()? {
                if eligible(pin.source.face.style, pin.source.face.weight) {
                    records.push(derived_record(pin.clone(), DerivedRoute::ColorStaticV1)?);
                }
            }
            for pin in crate::font_named::compiled_sources()? {
                if eligible(pin.source.face.style, pin.source.face.weight) {
                    records.push(derived_record(pin.clone(), DerivedRoute::V2)?);
                }
            }
            let mut ids = BTreeMap::new();
            let mut bindings = BTreeMap::new();
            for (index, record) in records.iter().enumerate() {
                if ids
                    .insert(record.source.source.face.id.clone(), index)
                    .is_some()
                    || bindings
                        .insert(record.source.binding.binding_id.clone(), index)
                        .is_some()
                {
                    return Err(invalid("Duplicate compiled synthesis identity"));
                }
            }
            Ok(Catalog {
                records,
                ids,
                bindings,
            })
        })
        .as_ref()
        .map_err(Clone::clone)
}
pub(super) fn records() -> Result<&'static [Record], EditorError> {
    Ok(&compiled()?.records)
}
pub(super) fn by_id(id: &str) -> Result<&'static Record, EditorError> {
    let catalog = compiled()?;
    catalog
        .ids
        .get(id)
        .map(|index| &catalog.records[*index])
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Unknown approved font synthesis binding"))
}
pub(super) fn by_binding(id: &str) -> Result<&'static Record, EditorError> {
    let catalog = compiled()?;
    catalog
        .bindings
        .get(id)
        .map(|index| &catalog.records[*index])
        .ok_or_else(|| invalid("Font resource has no approved synthesis binding"))
}
pub fn base_source(id: &str) -> Result<SynthesisBaseSource, EditorError> {
    Ok(by_id(id)?.base.clone())
}
pub fn display_family(id: &str) -> Result<String, EditorError> {
    Ok(by_id(id)?.display.clone())
}
pub fn render_profile(id: &str) -> Result<FontRenderProfile, EditorError> {
    Ok(by_id(id)?.render_profile)
}
pub fn acquisition_limits(id: &str) -> Result<SynthesisAcquisitionLimits, EditorError> {
    let base = &by_id(id)?.base;
    Ok(SynthesisAcquisitionLimits {
        max_source_bytes: if matches!(
            base,
            SynthesisBaseSource::RenamedOutline(_) | SynthesisBaseSource::CompleteColor(_)
        ) {
            64 * 1024 * 1024
        } else {
            MAX_FONT_BYTES
        },
        max_output_bytes: MAX_FONT_BYTES,
        max_deadline_ms: if matches!(
            base,
            SynthesisBaseSource::Outline(_)
                | SynthesisBaseSource::RenamedOutline(_)
                | SynthesisBaseSource::CompleteColor(_)
        ) {
            120_000
        } else {
            30_000
        },
    })
}
/// Stable discovery preference when multiple approved routes describe one
/// Classic family/weight slot. This never silently changes a chosen source ID.
pub fn base_priority(id: &str) -> Result<u8, EditorError> {
    Ok(match &by_id(id)?.base {
        SynthesisBaseSource::RepositoryStatic(_) => 0,
        SynthesisBaseSource::ApprovedStatic(_) => 1,
        SynthesisBaseSource::Outline(_) => 2,
        SynthesisBaseSource::RenamedOutline(_) => 3,
        SynthesisBaseSource::CompleteColor(_) => 3,
    })
}
