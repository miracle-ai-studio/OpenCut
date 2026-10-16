//! Finite acquisition routes over existing compiled sources. No catalog alias
//! or caller-authored proof can introduce a URL, recipe or native face.
use super::*;
use crate::font_catalog_v3::{ApprovedFontSourceV3, static_sources_v3};
use opencut_contracts::font_catalog_v3::FontRenderProfileV3;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub enum SynthesisBaseSourceV1 {
    StaticV1(&'static PinnedFontSourceV1),
    StaticV3(&'static ApprovedFontSourceV3),
    DerivedV1(PinnedDerivedFontV1),
    DerivedV2(PinnedDerivedFontV1),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SynthesisAcquisitionLimitsV1 {
    pub max_source_bytes: u64,
    pub max_output_bytes: u64,
    pub max_deadline_ms: u64,
}
pub(super) struct Record {
    pub source: FontSynthesisSourceV1,
    pub base: SynthesisBaseSourceV1,
    pub display: String,
    pub render_profile: FontRenderProfileV3,
    pub license: ResourceLicenseV4,
}
struct Catalog {
    records: Vec<Record>,
    ids: BTreeMap<String, usize>,
    bindings: BTreeMap<String, usize>,
}
fn eligible(style: FontStyleV4, weight: u16) -> bool {
    style == FontStyleV4::Normal && matches!(weight, 400 | 700)
}
fn entry(pin: &PinnedFontSourceV1) -> FontCatalogEntryV1 {
    FontCatalogEntryV1 {
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
    base: SynthesisBaseSourceV1,
    pin: &PinnedFontSourceV1,
    display: &str,
    profile: FontRenderProfileV3,
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
fn derived_record(pin: PinnedDerivedFontV1, v2: bool) -> Result<Record, EditorError> {
    let source = &pin.source;
    let actual = FontCatalogEntryV1 {
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
    let display = if v2 {
        crate::font_derived_v2::display_family(&source.face.id)?
    } else {
        source.face.family.clone()
    };
    let profile = if v2 {
        serde_json::from_value(Value::String(crate::font_derived_v2::render_profile(
            &source.face.id,
        )?))
        .map_err(|_| invalid("Unknown approved base render profile"))?
    } else {
        FontRenderProfileV3::StaticOutline
    };
    let license = ResourceLicenseV4 {
        identifier: Some(source.license_id.clone()),
        source_url: Some(source.license_url.clone()),
        notice: Some(source.license_notice.clone()),
        attribution: None,
    };
    let provenance =
        serde_json::to_value(source).map_err(|_| invalid("Invalid base derivation proof"))?;
    let route = if v2 { "derived_v2" } else { "derived_v1" };
    let base = if v2 {
        SynthesisBaseSourceV1::DerivedV2(pin)
    } else {
        SynthesisBaseSourceV1::DerivedV1(pin)
    };
    make(base, actual, &display, profile, route, provenance, license)
}
fn make(
    base: SynthesisBaseSourceV1,
    actual: FontCatalogEntryV1,
    display: &str,
    render_profile: FontRenderProfileV3,
    route: &str,
    provenance: Value,
    license: ResourceLicenseV4,
) -> Result<Record, EditorError> {
    let size = actual
        .size_bytes
        .parse::<u64>()
        .map_err(|_| invalid("Base output size is invalid"))?;
    if !eligible(actual.style, actual.weight)
        || size == 0
        || size > MAX_FONT_BYTES_V1
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
    let requested = FontSynthesisRequestV1 {
        weight: actual.weight,
        style: FontStyleV4::Italic,
    };
    let profile = FontSynthesisProfileV1::ClassicChromiumItalicV1;
    let license_url = license
        .source_url
        .clone()
        .ok_or_else(|| invalid("Base license URL is missing"))?;
    let mut proof = json!({"schema_version":3,"kind":"browser_font_synthesis","provider":FONT_SYNTHESIS_PROVIDER_V1,
        "profile":profile,"catalog_id":id,"actual":actual,"requested":requested,"license_url":license_url,
        "license_sha256":format!("{:x}",Sha256::digest(actual.license_notice.as_bytes())),"attribution":license.attribution,
        "source_bytes_unchanged":true,
        "base":{"route":route,"catalog_id":actual.id,"display_family":display,"render_profile":render_profile,"source":provenance}});
    let binding_id = format!(
        "{FONT_SYNTHESIS_BINDING_ID_PREFIX_V1}{:x}",
        Sha256::digest(canonical(&proof)?.as_bytes())
    );
    proof["derivation_id"] = Value::String(binding_id.clone());
    let source = FontSynthesisSourceV1 {
        source: FontDerivedSourceV1 {
            face: FontDerivedFaceV1 {
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
        binding: FontSynthesisBindingV1 {
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
                let pin = pinned_font_source_v1(id)?;
                records.push(static_record(
                    SynthesisBaseSourceV1::StaticV1(pin),
                    pin,
                    pin.family,
                    FontRenderProfileV3::StaticOutline,
                    "static_v1",
                )?);
            }
            for pin in crate::font_import_v1::catalog::font_catalog_v1() {
                if eligible(pin.style, pin.weight) && !ACTUAL_SOURCE_IDS.contains(&pin.catalog_id) {
                    records.push(static_record(
                        SynthesisBaseSourceV1::StaticV1(pin),
                        pin,
                        pin.family,
                        FontRenderProfileV3::StaticOutline,
                        "static_v1",
                    )?);
                }
            }
            for entry in static_sources_v3() {
                let pin = &entry.approved.source;
                if eligible(pin.style, pin.weight) {
                    records.push(static_record(
                        SynthesisBaseSourceV1::StaticV3(&entry.approved),
                        pin,
                        entry.display_family,
                        entry.approved.render_profile,
                        "static_v3",
                    )?);
                }
            }
            for pin in crate::font_derived_v1::catalog()? {
                if eligible(pin.source.face.style, pin.source.face.weight) {
                    records.push(derived_record(pin, false)?);
                }
            }
            for pin in crate::font_derived_v2::compiled_sources_v2()? {
                if eligible(pin.source.face.style, pin.source.face.weight) {
                    records.push(derived_record(pin.clone(), true)?);
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
pub fn base_source(id: &str) -> Result<SynthesisBaseSourceV1, EditorError> {
    Ok(by_id(id)?.base.clone())
}
pub fn display_family(id: &str) -> Result<String, EditorError> {
    Ok(by_id(id)?.display.clone())
}
pub fn render_profile(id: &str) -> Result<FontRenderProfileV3, EditorError> {
    Ok(by_id(id)?.render_profile)
}
pub fn acquisition_limits(id: &str) -> Result<SynthesisAcquisitionLimitsV1, EditorError> {
    let base = &by_id(id)?.base;
    Ok(SynthesisAcquisitionLimitsV1 {
        max_source_bytes: if matches!(base, SynthesisBaseSourceV1::DerivedV2(_)) {
            64 * 1024 * 1024
        } else {
            MAX_FONT_BYTES_V1
        },
        max_output_bytes: MAX_FONT_BYTES_V1,
        max_deadline_ms: if matches!(
            base,
            SynthesisBaseSourceV1::DerivedV1(_) | SynthesisBaseSourceV1::DerivedV2(_)
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
        SynthesisBaseSourceV1::StaticV1(_) => 0,
        SynthesisBaseSourceV1::StaticV3(_) => 1,
        SynthesisBaseSourceV1::DerivedV1(_) => 2,
        SynthesisBaseSourceV1::DerivedV2(_) => 3,
    })
}
