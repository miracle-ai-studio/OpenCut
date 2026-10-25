//! Fixed catalog policy and provenance; no URL fetch, filesystem or browser authority.
use opencut_contracts::{AssetKind, EditorError, brand_resources::*, scene::*};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::OnceLock};

pub const RASTER_SIZE: u32 = 512;
pub const MAX_RASTER_BYTES: u64 = 1024 * 1024;

// The original receipt hash includes this exact field order. It is private
// persistence identity, never an alternate public request or a current writer.
#[derive(Serialize)]
pub(crate) struct OriginalRequest<'a> {
    schema_version: u32,
    #[serde(flatten)]
    request: &'a ImportBrandResource,
}
pub(crate) fn original_request(input: &ImportBrandResource) -> OriginalRequest<'_> {
    OriginalRequest {
        schema_version: 1,
        request: input,
    }
}
pub(crate) fn request_identity(
    input: &ImportBrandResource,
) -> Result<crate::identity::RequestIdentity, EditorError> {
    crate::identity::request_identity(
        "brand_resources_import_v1",
        &input.project_id,
        &original_request(input),
    )
}
#[derive(Deserialize)]
struct Record {
    entry: BrandCatalogEntry,
    metadata: serde_json::Value,
}
#[derive(Deserialize)]
struct License {
    text: String,
    url: String,
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn records() -> &'static [Record] {
    static RECORDS: OnceLock<Vec<Record>> = OnceLock::new();
    RECORDS.get_or_init(|| {
        serde_json::from_str(include_str!("brand_resources/inventory.json"))
            .expect("compiled fixed brand inventory")
    })
}
pub fn collection() -> &'static BrandCollection {
    static COLLECTION: OnceLock<BrandCollection> = OnceLock::new();
    COLLECTION.get_or_init(|| {
        serde_json::from_str(include_str!("brand_resources/collection.json"))
            .expect("compiled fixed brand collection")
    })
}
fn verify_inventory() -> Result<(), EditorError> {
    static VALID: OnceLock<bool> = OnceLock::new();
    let valid = VALID.get_or_init(|| {
        format!(
            "{:x}",
            Sha256::digest(include_bytes!("brand_resources/inventory.json"))
        ) == collection().inventory_sha256
            && records().len() == 3466
            && records()
                .windows(2)
                .all(|pair| pair[0].entry.id < pair[1].entry.id)
    });
    if !valid {
        return Err(EditorError::new(
            "RESOURCE_SOURCE_INVALID",
            "Compiled brand inventory differs from its fixed collection identity",
        ));
    }
    Ok(())
}
pub fn catalog(input: &ListBrandResources) -> Result<BrandCatalog, EditorError> {
    verify_inventory()?;
    let query = input.query.as_deref().unwrap_or("");
    if query.len() > 256 || query.chars().any(char::is_control) || !(1..=100).contains(&input.limit)
    {
        return Err(invalid(
            "Brand search needs a bounded query and page size of 1 to 100",
        ));
    }
    let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
    let matches: Vec<_> = records()
        .iter()
        .filter(|r| {
            let text = format!(
                "{} {} {}",
                r.entry.name,
                r.entry.slug,
                r.entry.keywords.join(" ")
            )
            .to_lowercase();
            terms.iter().all(|t| text.contains(t))
        })
        .collect();
    let matched = matches.len() as u32;
    if input.offset > matched {
        return Err(invalid(
            "Brand page offset exceeds the fixed search results",
        ));
    }
    let entries: Vec<_> = matches
        .into_iter()
        .skip(input.offset as usize)
        .take(input.limit as usize)
        .map(|r| r.entry.clone())
        .collect();
    let end = input.offset + entries.len() as u32;
    Ok(BrandCatalog {
        collection: collection().clone(),
        total: records().len() as u32,
        matched,
        offset: input.offset,
        next_offset: (end < matched).then_some(end),
        entries,
    })
}
fn record(id: &str) -> Result<&'static Record, EditorError> {
    verify_inventory()?;
    if id.len() > 160 {
        return Err(invalid("Brand catalog identity exceeds its bound"));
    }
    records()
        .binary_search_by(|r| r.entry.id.as_str().cmp(id))
        .map(|i| &records()[i])
        .map_err(|_| invalid("Unknown pinned brand catalog ID"))
}
pub fn entry(id: &str) -> Result<&'static BrandCatalogEntry, EditorError> {
    Ok(&record(id)?.entry)
}
fn admitted(id: &str) -> Result<&'static Record, EditorError> {
    let value = record(id)?;
    if !value.entry.import_available {
        return Err(EditorError::new(
            "RESOURCE_LICENSE_UNAVAILABLE",
            value
                .entry
                .unavailable_reason
                .as_deref()
                .unwrap_or("Brand license has not been admitted"),
        ));
    }
    Ok(value)
}
pub fn license(id: &str) -> Result<ResourceLicense, EditorError> {
    let r = admitted(id)?;
    static LICENSES: OnceLock<BTreeMap<String, License>> = OnceLock::new();
    let licenses = LICENSES.get_or_init(|| {
        serde_json::from_str(include_str!("brand_resources/licenses.json"))
            .expect("compiled complete license texts")
    });
    let identifier = r
        .entry
        .license_identifier
        .as_ref()
        .ok_or_else(|| invalid("Missing admitted icon license"))?;
    let license = licenses
        .get(identifier)
        .ok_or_else(|| invalid("Missing admitted icon license text"))?;
    // Preserve every supplied attribution/source/guideline field verbatim. This
    // identifies the collection adaptation, not an invented original copyright holder.
    let credit = format!(
        "{} — SVG adaptation from Simple Icons {} contributors. Original artwork source: {}",
        r.entry.name,
        collection().release,
        r.entry.source_url
    );
    let notice = format!(
        "{credit}\nPinned collection commit: {}\nOriginal SVG SHA-256: {}\nTransformation: path-only SVG rasterized to a transparent 512×512 PNG; monochrome artwork preserved.\nThis icon retains {identifier}; ShareAlike conditions, where applicable, remain attached to the raster. No trademark rights or endorsement are granted.\nAll upstream per-icon metadata and supplied notices:\n{}\n\n{}\n\nCollection disclaimer:\n{}",
        collection().commit,
        r.entry.svg_sha256,
        serde_json::to_string_pretty(&r.metadata)
            .map_err(|_| invalid("Cannot encode brand provenance"))?,
        license.text,
        collection().disclaimer
    );
    Ok(ResourceLicense {
        identifier: Some(identifier.clone()),
        source_url: Some(
            r.entry
                .license_url
                .clone()
                .unwrap_or_else(|| license.url.clone()),
        ),
        notice: Some(notice),
        attribution: Some(credit),
    })
}
pub fn source(input: &GetBrandSource) -> Result<BrandSource, EditorError> {
    let entry = admitted(&input.catalog_id)?.entry.clone();
    static SOURCES: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    let sources = SOURCES.get_or_init(|| {
        serde_json::from_str(include_str!("brand_resources/sources.json"))
            .expect("compiled admitted brand SVG sources")
    });
    let svg = sources.get(&input.catalog_id).ok_or_else(|| {
        EditorError::new(
            "CAPABILITY_UNAVAILABLE",
            "Pinned brand SVG is absent from this host",
        )
    })?;
    if svg.len() as u64 != entry.svg_size
        || svg.len() > 262144
        || format!("{:x}", Sha256::digest(svg.as_bytes())) != entry.svg_sha256
    {
        return Err(EditorError::new(
            "RESOURCE_SOURCE_INVALID",
            "Compiled brand SVG differs from its fixed inventory",
        ));
    }
    Ok(BrandSource {
        collection: collection().clone(),
        license: license(&input.catalog_id)?,
        entry,
        svg: svg.clone(),
        raster_width: RASTER_SIZE,
        raster_height: RASTER_SIZE,
    })
}
pub fn validate_request(input: &ImportBrandResource) -> Result<(), EditorError> {
    crate::require_idempotency_key(&input.idempotency_key)?;
    for id in [
        &input.project_id,
        &input.expected_revision,
        &input.asset_id,
        &input.uploaded.media_id,
    ] {
        if id.trim().is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
            return Err(invalid("Brand import needs bounded fixed identities"));
        }
    }
    admitted(&input.catalog_id)?;
    let version = &input.uploaded.version;
    crate::uploads::validate_hash(&version.content_hash)?;
    if version.id != format!("av_{}", version.content_hash)
        || version.size == 0
        || version.size > MAX_RASTER_BYTES
    {
        return Err(invalid(
            "Brand raster exceeds its immutable upload envelope",
        ));
    }
    Ok(())
}
pub fn bind_resource(
    input: &ImportBrandResource,
    mut asset: ProjectAsset,
) -> Result<ProjectAsset, EditorError> {
    validate_request(input)?;
    let entry = entry(&input.catalog_id)?;
    if asset.kind != AssetKind::Image
        || asset.version != input.uploaded.version
        || asset.metadata.mime_type.as_deref() != Some("image/png")
        || asset.metadata.intrinsic_size
            != Some(CanvasSize {
                width: RASTER_SIZE,
                height: RASTER_SIZE,
            })
        || asset.metadata.audio != (AudioPresence::Absent {})
    {
        return Err(invalid(
            "Brand raster must be authorized PNG media at the fixed source size",
        ));
    }
    asset.id = input.asset_id.clone();
    asset.name = entry.name.clone();
    asset.origin = ResourceOrigin::Generated {
        provider: "simple-icons-browser-raster-v1".into(),
        generation_id: format!("{}:{}:{}", entry.id, collection().commit, entry.svg_sha256),
    };
    asset.license = Some(license(&input.catalog_id)?);
    Ok(asset)
}

#[cfg(test)]
mod tests;
