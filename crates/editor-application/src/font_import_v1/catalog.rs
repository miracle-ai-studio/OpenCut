//! Small reviewed catalog of unchanged, redistributable Google Fonts faces.
//! These pins describe source bytes; they are never a system-font fallback.
use super::*;

pub const GOOGLE_FONTS_COMMIT_V1: &str = "66928797f271fed5ee428544e7229685e2d90127";

#[derive(Clone, Copy, Debug)]
pub struct PinnedFontSourceV1 {
    pub catalog_id: &'static str,
    pub family: &'static str,
    pub weight: u16,
    pub style: FontStyleV4,
    pub source_commit: &'static str,
    pub source_url: &'static str,
    pub content_hash: &'static str,
    pub size_bytes: u64,
    pub license_url: &'static str,
    pub license_sha256: &'static str,
    pub license_notice: &'static str,
    pub attribution: &'static str,
}

static CATALOG: [PinnedFontSourceV1; 2] = [
    PinnedFontSourceV1 {
        catalog_id: "google-lato-regular-v1",
        family: "Lato",
        weight: 400,
        style: FontStyleV4::Normal,
        source_commit: GOOGLE_FONTS_COMMIT_V1,
        source_url: "https://raw.githubusercontent.com/google/fonts/66928797f271fed5ee428544e7229685e2d90127/ofl/lato/Lato-Regular.ttf",
        content_hash: "d636e4683231f931eda222d588e944d082bfd3bdba02f928bee461c0f185b251",
        size_bytes: 656568,
        license_url: "https://raw.githubusercontent.com/google/fonts/66928797f271fed5ee428544e7229685e2d90127/ofl/lato/OFL.txt",
        license_sha256: "74ba064d03f1f1c4a952da936c3eb71866c34404916734de3cae73b34357e59e",
        license_notice: include_str!("licenses/lato-OFL.txt"),
        attribution: "Copyright (c) 2010-2014 by tyPoland Lukasz Dziedzic (team@latofonts.com) with Reserved Font Name \"Lato\"",
    },
    PinnedFontSourceV1 {
        catalog_id: "google-mashanzheng-regular-v1",
        family: "Ma Shan Zheng",
        weight: 400,
        style: FontStyleV4::Normal,
        source_commit: GOOGLE_FONTS_COMMIT_V1,
        source_url: "https://raw.githubusercontent.com/google/fonts/66928797f271fed5ee428544e7229685e2d90127/ofl/mashanzheng/MaShanZheng-Regular.ttf",
        content_hash: "6d2546bb189c732a8ca29af9e22457b152387d158aa459e4ac2ce1e51788b7fb",
        size_bytes: 5857936,
        license_url: "https://raw.githubusercontent.com/google/fonts/66928797f271fed5ee428544e7229685e2d90127/ofl/mashanzheng/OFL.txt",
        license_sha256: "d7bdb1cee215b689e23c2f95672a6084c790542170648267a55114103d756a08",
        license_notice: include_str!("licenses/mashanzheng-OFL.txt"),
        attribution: "Copyright 2018 The Ma Shan Zheng Project Authors (https://github.com/googlefonts/mashanzheng)",
    },
];

pub fn font_catalog_v1() -> &'static [PinnedFontSourceV1] {
    &CATALOG
}
pub fn pinned_font_source_v1(id: &str) -> Result<&'static PinnedFontSourceV1, EditorError> {
    CATALOG
        .iter()
        .find(|source| source.catalog_id == id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Unknown pinned font catalog entry"))
}
impl PinnedFontSourceV1 {
    pub fn import_spec(&self, font_id: String, required_text: String) -> FontImportSpecV1 {
        FontImportSpecV1 {
            font_id,
            family: self.family.into(),
            weight: self.weight,
            style: self.style,
            origin: ResourceOriginV4::Online {
                provider: "google-fonts".into(),
                source_url: self.source_url.into(),
            },
            license: ResourceLicenseV4 {
                identifier: Some("OFL-1.1".into()),
                source_url: Some(self.license_url.into()),
                notice: Some(self.license_notice.into()),
                attribution: Some(self.attribution.into()),
            },
            required_text,
        }
    }
}

/// Host-injected permission to fetch a public resource. This is not a request
/// field that can override the task's network policy. Default is offline.
#[derive(Clone, Copy, Debug)]
pub struct FontFetchPolicyV1 {
    pub allow_public_font_download: bool,
    pub max_bytes: u64,
    pub deadline_ms: u64,
}
impl Default for FontFetchPolicyV1 {
    fn default() -> Self {
        Self {
            allow_public_font_download: false,
            max_bytes: MAX_FONT_BYTES_V1,
            deadline_ms: 30_000,
        }
    }
}
pub fn validate_font_fetch_v1(
    catalog_id: &str,
    policy: &FontFetchPolicyV1,
) -> Result<&'static PinnedFontSourceV1, EditorError> {
    let source = pinned_font_source_v1(catalog_id)?;
    if !policy.allow_public_font_download {
        return Err(EditorError::new(
            "FONT_NETWORK_DENIED",
            "Task policy does not allow public font downloads",
        ));
    }
    if !(1..=30_000).contains(&policy.deadline_ms)
        || !(1..=MAX_FONT_BYTES_V1).contains(&policy.max_bytes)
    {
        return Err(invalid("Font download policy exceeds its bounded envelope"));
    }
    if source.size_bytes > policy.max_bytes {
        return Err(EditorError::new(
            "FONT_BUDGET_EXCEEDED",
            "Pinned font exceeds the download byte budget",
        ));
    }
    Ok(source)
}

pub trait FontSourceFetchV1 {
    /// Download only the compiled catalog entry. Never send project content,
    /// text, application credentials or a caller-provided URL to the provider.
    fn fetch(
        &self,
        catalog_id: &str,
        policy: &FontFetchPolicyV1,
        checkpoint: &mut dyn FontImportCheckpointV1,
    ) -> Result<Vec<u8>, EditorError>;
}

/// Keeps downloaded bytes alive for the caller's existing CAS publication;
/// only the immutable resource is subsequently committed to the project.
pub struct PreparedCatalogFontV1 {
    pub font: PreparedFontV1,
    pub bytes: Vec<u8>,
}
pub fn prepare_catalog_font_v1(
    fetcher: &impl FontSourceFetchV1,
    inspector: &impl FontInspectorV1,
    catalog_id: &str,
    font_id: String,
    required_text: String,
    fetch_policy: &FontFetchPolicyV1,
    inspection_policy: &FontImportPolicyV1,
    checkpoint: &mut dyn FontImportCheckpointV1,
) -> Result<PreparedCatalogFontV1, EditorError> {
    checkpoint.check()?;
    let source = validate_font_fetch_v1(catalog_id, fetch_policy)?;
    let spec = source.import_spec(font_id, required_text);
    validate_font_spec_v1(&spec)?;
    validate_font_envelope_v1(source.size_bytes, &spec.required_text, inspection_policy)?;
    let bytes = fetcher.fetch(catalog_id, fetch_policy, checkpoint)?;
    let font = prepare_font_v1(inspector, &bytes, &spec, inspection_policy, checkpoint)?;
    if font.resource.content.content_hash != source.content_hash
        || font.resource.content.size != source.size_bytes
    {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Font download does not match the pinned source",
        ));
    }
    checkpoint.check()?;
    Ok(PreparedCatalogFontV1 { font, bytes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    #[test]
    fn catalog_preserves_complete_original_license_and_fixed_source_identity() {
        for source in font_catalog_v1() {
            assert_eq!(
                format!("{:x}", Sha256::digest(source.license_notice.as_bytes())),
                source.license_sha256
            );
            assert!(source.source_url.contains(GOOGLE_FONTS_COMMIT_V1));
            assert!(source.license_url.contains(GOOGLE_FONTS_COMMIT_V1));
            assert!(source.license_notice.starts_with(source.attribution));
            let spec = source.import_spec("font".into(), "AV To".into());
            validate_font_spec_v1(&spec).unwrap();
            assert_eq!(spec.license.notice.as_deref(), Some(source.license_notice));
        }
    }
    #[test]
    fn default_offline_unknown_source_and_smaller_budget_cannot_reach_network() {
        let id = font_catalog_v1()[0].catalog_id;
        assert_eq!(
            validate_font_fetch_v1(id, &FontFetchPolicyV1::default())
                .unwrap_err()
                .code,
            "FONT_NETWORK_DENIED"
        );
        let policy = FontFetchPolicyV1 {
            allow_public_font_download: true,
            max_bytes: 1,
            ..Default::default()
        };
        assert_eq!(
            validate_font_fetch_v1(id, &policy).unwrap_err().code,
            "FONT_BUDGET_EXCEEDED"
        );
        assert_eq!(
            validate_font_fetch_v1("https://attacker.test/font.ttf", &policy)
                .unwrap_err()
                .code,
            "NOT_FOUND"
        );
    }

    struct WrongBytes(Cell<usize>);
    impl FontSourceFetchV1 for WrongBytes {
        fn fetch(
            &self,
            _: &str,
            _: &FontFetchPolicyV1,
            _: &mut dyn FontImportCheckpointV1,
        ) -> Result<Vec<u8>, EditorError> {
            self.0.set(self.0.get() + 1);
            Ok(b"abc".to_vec())
        }
    }
    struct ParsedLato;
    impl FontInspectorV1 for ParsedLato {
        fn inspect(
            &self,
            _: &[u8],
            _: &str,
            _: &FontImportPolicyV1,
            _: &mut dyn FontImportCheckpointV1,
        ) -> Result<ParsedFontFaceV1, EditorError> {
            Ok(ParsedFontFaceV1 {
                family: "Lato".into(),
                weight: 400,
                style: FontStyleV4::Normal,
                glyph_count: 3,
                checked_codepoints: 1,
            })
        }
    }
    #[test]
    fn preparation_rechecks_download_identity_and_validates_text_before_network() {
        let source = WrongBytes(Cell::new(0));
        let policy = FontFetchPolicyV1 {
            allow_public_font_download: true,
            ..Default::default()
        };
        let prepare = |text: &str| {
            prepare_catalog_font_v1(
                &source,
                &ParsedLato,
                "google-lato-regular-v1",
                "font".into(),
                text.into(),
                &policy,
                &FontImportPolicyV1::default(),
                &mut || Ok(()),
            )
        };
        assert_eq!(prepare("A\0").err().unwrap().code, "INVALID_ARGUMENT");
        assert_eq!(source.0.get(), 0);
        assert_eq!(
            prepare("A").err().unwrap().code,
            "FONT_SOURCE_INTEGRITY_ERROR"
        );
        assert_eq!(source.0.get(), 1);
    }
}
