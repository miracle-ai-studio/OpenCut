//! Catalog policy and provenance. No HTTP, browser, filesystem or user URLs.
use opencut_contracts::{AssetKind, EditorError, catalog_resources_v1::*, project_v4::*};
use serde::Deserialize;
use std::sync::OnceLock;

pub const KENNEY_URL: &str =
    "https://kenney.nl/media/pages/assets/ui-audio/490d233f68-1677590494/kenney_ui-audio.zip";
pub const KENNEY_SHA256: &str = "946fc23a63d535d693eb31b2eabb80c8c28d6351e2186b344ceb71b2cb1d5eb6";
pub const KENNEY_BYTES: usize = 411949;
pub const KENNEY_LICENSE_SHA256: &str =
    "4f88ab3c885c87874834441a0d009cea8942f57461d7b870be65cf4e31362073";
#[derive(Deserialize)]
pub struct PinnedSoundV1 {
    pub name: String,
    pub path: String,
    pub size: usize,
    pub sha256: String,
    pub crc32: u32,
}
pub fn sound_sources() -> &'static [PinnedSoundV1] {
    static SOUNDS: OnceLock<Vec<PinnedSoundV1>> = OnceLock::new();
    SOUNDS.get_or_init(|| {
        serde_json::from_str(include_str!("catalog_resources_v1/sounds.json"))
            .expect("reviewed Kenney source inventory")
    })
}
pub fn sound_source(id: &str) -> Result<&'static PinnedSoundV1, EditorError> {
    sound_sources()
        .iter()
        .find(|s| id == format!("kenney-ui-audio:{}", s.name))
        .ok_or_else(|| invalid("Unknown pinned sound entry"))
}
pub const MAX_RESOURCE_BYTES: u64 = 1024 * 1024;
pub const MAX_SOUND_BYTES: usize = 262144;
#[derive(Deserialize)]
struct Flag {
    id: String,
    name: String,
    code: String,
    region: String,
    keywords: Vec<String>,
    sha256: String,
    width: f64,
    height: f64,
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
pub fn entries() -> &'static [CatalogResourceEntryV1] {
    static ENTRIES: OnceLock<Vec<CatalogResourceEntryV1>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        let license = ResourceLicenseV4 { identifier: Some("MIT".into()), source_url: Some("https://github.com/OpenCut-app/OpenCut/blob/main/LICENSE".into()), notice: Some(include_str!("catalog_resources_v1/LICENSE").into()), attribution: Some("OpenCut Classic bundled flag-icons assets".into()) };
        let mut entries = Vec::new();
        // Classic graphics/definitions/{rectangle,ellipse,polygon,star}.ts:
        // these are searchable names for four parametric definitions, not new presets.
        for (name, keywords) in [
            ("rectangle", &["rectangle", "square", "box"][..]),
            ("ellipse", &["ellipse", "circle", "oval"][..]),
            ("polygon", &["polygon", "triangle", "pentagon", "hexagon", "diamond"][..]),
            ("star", &["star", "sparkle", "burst"][..]),
        ] {
            entries.push(CatalogResourceEntryV1 { id: format!("shapes:{name}"), name: name.into(), category: CatalogResourceCategoryV1::Shapes, keywords: keywords.iter().map(|word| (*word).into()).collect(), source_url: "https://github.com/OpenCut-app/OpenCut".into(), source_sha256: None, license: license.clone(), width: None, height: None });
        }
        let flags: Vec<Flag> = serde_json::from_str(include_str!("catalog_resources_v1/flags.json")).expect("reviewed Classic flag catalog");
        for flag in flags {
            let mut keywords = flag.keywords; keywords.extend([flag.code.clone(), flag.region]);
            // Preserve aspect ratio while bounding browser rasterization to 640 px.
            let ratio = 640.0 / flag.width.max(flag.height);
            entries.push(CatalogResourceEntryV1 { id: flag.id, name: flag.name, category: CatalogResourceCategoryV1::Flags, keywords, source_url: format!("https://github.com/OpenCut-app/OpenCut/blob/main/apps/web/public/flags/{}.svg", flag.code), source_sha256: Some(flag.sha256), license: license.clone(), width: Some((flag.width * ratio).round() as u32), height: Some((flag.height * ratio).round() as u32) });
        }
        for sound in sound_sources() {
            let name = &sound.name;
            let group = name.trim_end_matches(|c: char| c.is_ascii_digit());
            entries.push(CatalogResourceEntryV1 { id: format!("kenney-ui-audio:{name}"), name: name.clone(), category: CatalogResourceCategoryV1::Sounds, keywords: vec!["interface".into(), "ui".into(), "effect".into(), name.clone(), group.into()], source_url: "https://kenney.nl/assets/ui-audio".into(), source_sha256: Some(sound.sha256.clone()), license: ResourceLicenseV4 { identifier: Some("CC0-1.0".into()), source_url: Some("https://creativecommons.org/publicdomain/zero/1.0/".into()), notice: Some(format!("{}\nPinned archive SHA-256 {KENNEY_SHA256}; License.txt SHA-256 {KENNEY_LICENSE_SHA256}.", include_str!("catalog_resources_v1/KENNEY-LICENSE.txt"))), attribution: Some("Kenney (www.kenney.nl)".into()) }, width: None, height: None });
        }
        entries
    })
}
pub fn entry(id: &str) -> Result<&'static CatalogResourceEntryV1, EditorError> {
    entries()
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| invalid("Unknown fixed resource catalog ID"))
}
pub fn validate_request(input: &ImportCatalogResourceV1) -> Result<(), EditorError> {
    crate::require_idempotency_key(&input.idempotency_key)?;
    for id in [&input.project_id, &input.expected_revision, &input.asset_id] {
        if id.trim().is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
            return Err(invalid("Catalog import needs bounded fixed identities"));
        }
    }
    match (entry(&input.catalog_id)?.category, &input.uploaded) {
        (CatalogResourceCategoryV1::Flags, Some(uploaded)) => {
            crate::uploads::validate_hash(&uploaded.version.content_hash)?;
            if uploaded.media_id.is_empty()
                || uploaded.media_id.len() > 1024
                || uploaded.media_id.chars().any(char::is_control)
                || uploaded.version.size == 0
                || uploaded.version.size > MAX_RESOURCE_BYTES
                || uploaded.version.id != format!("av_{}", uploaded.version.content_hash)
            {
                return Err(invalid("Invalid uploaded catalog image identity"));
            }
        }
        (CatalogResourceCategoryV1::Sounds, None) => {}
        _ => {
            return Err(invalid(
                "Flags require authorized PNG media; sounds require native acquisition; shapes use graphic authoring",
            ));
        }
    }
    Ok(())
}
pub fn bind_resource(
    input: &ImportCatalogResourceV1,
    mut asset: AssetV4,
) -> Result<AssetV4, EditorError> {
    validate_request(input)?;
    let source = entry(&input.catalog_id)?;
    if asset.version.size == 0 || asset.version.size > MAX_RESOURCE_BYTES {
        return Err(invalid("Catalog resource exceeds byte budget"));
    }
    match source.category {
        CatalogResourceCategoryV1::Flags => {
            if asset.kind != AssetKind::Image
                || asset.metadata.mime_type.as_deref() != Some("image/png")
                || asset.metadata.intrinsic_size
                    != Some(CanvasSizeV4 {
                        width: source.width.unwrap(),
                        height: source.height.unwrap(),
                    })
                || input
                    .uploaded
                    .as_ref()
                    .is_none_or(|u| u.version != asset.version)
            {
                return Err(invalid(
                    "Flag raster must be an uploaded PNG at the fixed catalog size",
                ));
            }
            asset.origin = ResourceOriginV4::Generated {
                provider: "classic-v31-flag-browser-raster".into(),
                generation_id: format!(
                    "{}:{}",
                    source.id,
                    source.source_sha256.as_deref().unwrap()
                ),
            };
        }
        CatalogResourceCategoryV1::Sounds => {
            let sound = sound_source(&input.catalog_id)?;
            if asset.version.content_hash != sound.sha256
                || asset.version.size != sound.size as u64
                || asset.version.id != format!("av_{}", sound.sha256)
            {
                return Err(invalid("Sound bytes differ from the pinned catalog entry"));
            }
            if asset.kind != AssetKind::Audio
                || asset.metadata.mime_type.as_deref() != Some("audio/ogg")
                || !matches!(asset.metadata.audio, AudioPresenceV4::Present { .. })
                || asset.duration.as_ref().is_none_or(|v| {
                    opencut_time::RationalTime::try_from(v).map_or(true, |t| {
                        t.ticks() <= 0 || i128::from(t.ticks()) > i128::from(t.timebase()) * 30
                    })
                })
            {
                return Err(invalid("Pinned sound needs observed bounded Ogg audio"));
            }
            asset.origin = ResourceOriginV4::Online {
                provider: "kenney-ui-audio-v1".into(),
                source_url: format!(
                    "{KENNEY_URL}#Audio/{}.ogg",
                    source.id.strip_prefix("kenney-ui-audio:").unwrap()
                ),
            };
        }
        CatalogResourceCategoryV1::Shapes => return Err(invalid("Shapes are authored graphics")),
    }
    asset.id = input.asset_id.clone();
    asset.name = source.name.clone();
    asset.license = Some(source.license.clone());
    Ok(asset)
}

/// Native implementation may fetch only the admitted catalog ID. It must verify
/// archive, entry, format and immutable CAS bytes before returning this resource.
pub struct AcquiredCatalogResourceV1<L> {
    pub resource: AssetV4,
    pub lease: L,
}
pub trait CatalogSoundAcquireV1 {
    type Lease;
    fn acquire(
        &self,
        catalog_id: &str,
        checkpoint: &mut dyn crate::font_import_v1::FontImportCheckpointV1,
    ) -> Result<AcquiredCatalogResourceV1<Self::Lease>, EditorError>;
}
pub trait CatalogSoundGenerateV1 {
    type Lease;
    fn generate(
        &self,
        recipe: &LocalSoundRecipeV1,
        checkpoint: &mut dyn crate::font_import_v1::FontImportCheckpointV1,
    ) -> Result<AcquiredCatalogResourceV1<Self::Lease>, EditorError>;
}
pub fn validate_recipe(recipe: &LocalSoundRecipeV1) -> Result<(), EditorError> {
    let (min, max) = match recipe.kind {
        LocalSoundKindV1::Whoosh => (100, 5000),
        LocalSoundKindV1::Click => (40, 500),
        LocalSoundKindV1::Impact => (100, 3000),
    };
    if !(min..=max).contains(&recipe.duration_ms)
        || !recipe.level_db.is_finite()
        || !(-60.0..=0.0).contains(&recipe.level_db)
    {
        return Err(invalid(
            "Local sound recipe duration or level exceeds its bounded range",
        ));
    }
    Ok(())
}
pub fn validate_generate(input: &GenerateCatalogSoundV1) -> Result<(), EditorError> {
    crate::require_idempotency_key(&input.idempotency_key)?;
    for id in [&input.project_id, &input.expected_revision, &input.asset_id] {
        if id.trim().is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
            return Err(invalid("Local sound needs bounded fixed identities"));
        }
    }
    validate_recipe(&input.recipe)
}
pub fn bind_generated(
    input: &GenerateCatalogSoundV1,
    mut asset: AssetV4,
) -> Result<AssetV4, EditorError> {
    validate_generate(input)?;
    crate::uploads::validate_hash(&asset.version.content_hash)?;
    let samples = u64::from(input.recipe.duration_ms) * 48;
    if asset.kind != AssetKind::Audio
        || asset.version.size != 44 + samples * 2
        || asset.version.id != format!("av_{}", asset.version.content_hash)
        || asset.duration
            != Some(opencut_contracts::Time {
                ticks: samples.to_string(),
                timebase: 48000,
            })
        || asset.metadata
            != (MediaMetadataV4 {
                mime_type: Some("audio/wav".into()),
                intrinsic_size: None,
                frame_rate: None,
                rotation_degrees: None,
                audio: AudioPresenceV4::Present {
                    metadata: Some(AudioMetadataV4 {
                        sample_rate: 48000,
                        channels: 1,
                        channel_layout: Some("mono".into()),
                    }),
                },
            })
    {
        return Err(invalid(
            "Generated sound differs from its fixed PCM recipe envelope",
        ));
    }
    let name = match input.recipe.kind {
        LocalSoundKindV1::Whoosh => "Whoosh",
        LocalSoundKindV1::Click => "Click",
        LocalSoundKindV1::Impact => "Impact",
    };
    asset.id = input.asset_id.clone();
    asset.name = format!("{name} · {} ms", input.recipe.duration_ms);
    asset.origin = ResourceOriginV4::Generated { provider: "opencut-local-sfx-v1".into(), generation_id: serde_json::to_string(&serde_json::json!({"schema_version":1,"recipe":input.recipe,"sample_rate":48000,"channels":1,"sample_format":"pcm_s16le"})).map_err(|_| invalid("Cannot encode local sound recipe"))? };
    asset.license = Some(ResourceLicenseV4 { identifier: None, source_url: None, notice: Some("Locally synthesized from the recorded OpenCut SFX v1 recipe. No downloaded source audio is used.".into()), attribution: None });
    Ok(asset)
}

pub fn observed_asset(
    asset: &opencut_contracts::Asset,
    version: &opencut_contracts::AssetVersion,
) -> Result<AssetV4, EditorError> {
    use opencut_editor_core::project_v4::legacy::*;
    let document = opencut_contracts::ProjectDocument {
        schema_version: 2,
        id: "catalog_observation_project".into(),
        name: "Catalog observation".into(),
        default_sequence_id: "catalog_observation_scene".into(),
        assets: vec![asset.clone()],
        sequences: vec![opencut_contracts::Sequence {
            id: "catalog_observation_scene".into(),
            name: "Observation".into(),
            width: 1,
            height: 1,
            fps: opencut_contracts::FrameRate {
                numerator: 1,
                denominator: 1,
            },
            tracks: vec![],
            caption_cues: vec![],
        }],
    };
    let inputs = LegacyConversionInputs {
        asset_versions: std::collections::BTreeMap::from([(asset.id.clone(), version.clone())]),
    };
    match convert_legacy_project(LegacyProjectInput::Base(&document), &inputs) {
        LegacyConversionResult::Converted { mut project, .. } => project
            .assets
            .pop()
            .ok_or_else(|| invalid("Missing catalog observation")),
        LegacyConversionResult::Rejected { .. } => {
            Err(invalid("Catalog probe cannot be represented by V4"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(id: &str) -> ImportCatalogResourceV1 {
        ImportCatalogResourceV1 {
            project_id: "p".into(),
            expected_revision: "r".into(),
            idempotency_key: "catalog-request".into(),
            catalog_id: id.into(),
            asset_id: "flag".into(),
            uploaded: None,
        }
    }
    #[test]
    fn catalog_has_real_classic_shapes_flags_and_only_cc0_sounds() {
        let all = entries();
        assert_eq!(
            all.iter()
                .filter(|e| e.category == CatalogResourceCategoryV1::Shapes)
                .count(),
            4
        );
        assert!(
            all.iter()
                .filter(|e| e.category == CatalogResourceCategoryV1::Flags)
                .count()
                > 200
        );
        let sounds = all
            .iter()
            .filter(|e| e.category == CatalogResourceCategoryV1::Sounds)
            .collect::<Vec<_>>();
        assert_eq!(sounds.len(), 51);
        assert!(sounds.iter().all(|e| {
            e.license.identifier.as_deref() == Some("CC0-1.0")
                && e.source_sha256
                    .as_ref()
                    .is_some_and(|hash| hash.len() == 64)
        }));
        assert!(entry("https://example.com/sound.ogg").is_err());
        assert!(entry("flags:../cn").is_err());
    }
    #[test]
    fn shape_search_preserves_classic_definition_keywords_without_inventing_presets() {
        for (id, words) in [
            ("shapes:rectangle", &["rectangle", "square", "box"][..]),
            ("shapes:ellipse", &["ellipse", "circle", "oval"][..]),
            (
                "shapes:polygon",
                &["polygon", "triangle", "pentagon", "hexagon", "diamond"][..],
            ),
            ("shapes:star", &["star", "sparkle", "burst"][..]),
        ] {
            assert_eq!(
                entry(id).unwrap().keywords,
                words
                    .iter()
                    .map(|word| (*word).to_owned())
                    .collect::<Vec<_>>()
            );
        }
        assert!(entry("shapes:circle").is_err());
        assert!(entry("shapes:triangle").is_err());
    }
    #[test]
    fn uploaded_flag_requires_exact_version_dimensions_and_png() {
        let source = entry("flags:fr").unwrap();
        let version = opencut_contracts::AssetVersion {
            id: format!("av_{}", "a".repeat(64)),
            content_hash: "a".repeat(64),
            size: 100,
        };
        let mut input = request("flags:fr");
        input.uploaded = Some(CatalogUploadedMediaV1 {
            media_id: "media".into(),
            version: version.clone(),
        });
        let asset = AssetV4 {
            id: "upload".into(),
            name: "original.png".into(),
            kind: AssetKind::Image,
            version,
            duration: None,
            origin: ResourceOriginV4::LocalImport {
                original_name: "original.png".into(),
            },
            license: None,
            metadata: MediaMetadataV4 {
                mime_type: Some("image/png".into()),
                intrinsic_size: Some(CanvasSizeV4 {
                    width: source.width.unwrap(),
                    height: source.height.unwrap(),
                }),
                frame_rate: None,
                rotation_degrees: None,
                audio: AudioPresenceV4::Absent {},
            },
        };
        let bound = bind_resource(&input, asset.clone()).unwrap();
        assert_eq!(bound.id, "flag");
        assert_eq!(bound.version, asset.version);
        assert!(matches!(bound.origin, ResourceOriginV4::Generated { .. }));
        let mut wrong = asset.clone();
        wrong.metadata.intrinsic_size.as_mut().unwrap().width += 1;
        assert!(bind_resource(&input, wrong).is_err());
        let mut wrong = asset.clone();
        wrong.metadata.mime_type = Some("image/svg+xml".into());
        assert!(bind_resource(&input, wrong).is_err());
        let mut wrong = asset;
        wrong.version.size += 1;
        assert!(bind_resource(&input, wrong).is_err());
        input.catalog_id = "kenney-ui-audio:click1".into();
        assert!(validate_request(&input).is_err());
    }
    #[test]
    fn sound_selector_cannot_carry_uploaded_bytes_or_unknown_names() {
        let input = request("kenney-ui-audio:click1");
        assert!(validate_request(&input).is_ok());
        for id in [
            "shapes:star",
            "kenney-ui-audio:click9",
            "kenney-ui-audio:../../License",
        ] {
            assert!(validate_request(&request(id)).is_err());
        }
    }
    #[test]
    fn local_recipe_bounds_are_kind_specific_and_do_not_accept_nonfinite_gain() {
        for (kind, min, max) in [
            (LocalSoundKindV1::Whoosh, 100, 5000),
            (LocalSoundKindV1::Click, 40, 500),
            (LocalSoundKindV1::Impact, 100, 3000),
        ] {
            let mut recipe = LocalSoundRecipeV1 {
                kind,
                duration_ms: min,
                seed: u32::MAX,
                level_db: -60.0,
            };
            assert!(validate_recipe(&recipe).is_ok());
            recipe.duration_ms = max;
            recipe.level_db = 0.0;
            assert!(validate_recipe(&recipe).is_ok());
            recipe.duration_ms = min - 1;
            assert!(validate_recipe(&recipe).is_err());
            recipe.duration_ms = max + 1;
            assert!(validate_recipe(&recipe).is_err());
            recipe.duration_ms = min;
            for level in [f64::NAN, f64::INFINITY, -60.01, 0.01] {
                recipe.level_db = level;
                assert!(validate_recipe(&recipe).is_err());
            }
        }
    }
}
