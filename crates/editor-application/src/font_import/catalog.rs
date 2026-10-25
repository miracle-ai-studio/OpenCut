//! Pinned static Classic Google Fonts faces with complete original licenses.
//! These pins describe source bytes; they are never a system-font fallback.
use super::*;

pub const GOOGLE_FONTS_COMMIT: &str = "66928797f271fed5ee428544e7229685e2d90127";

#[derive(Clone, Copy, Debug)]
pub struct PinnedFontSource {
    pub catalog_id: &'static str,
    pub family: &'static str,
    pub weight: u16,
    pub style: FontStyle,
    pub source_commit: &'static str,
    pub source_url: &'static str,
    pub content_hash: &'static str,
    pub size_bytes: u64,
    pub license_url: &'static str,
    pub license_sha256: &'static str,
    pub license_identifier: &'static str,
    pub license_notice: &'static str,
    pub attribution: &'static str,
}

static CATALOG: &[PinnedFontSource] = include!("catalog_generated.rs");

pub fn static_font_inventory() -> &'static [PinnedFontSource] {
    CATALOG
}
pub fn pinned_font_source(id: &str) -> Result<&'static PinnedFontSource, EditorError> {
    CATALOG
        .iter()
        .find(|source| source.catalog_id == id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Unknown pinned font catalog entry"))
}
/// Resolve the requested static face exactly; never synthesize bold/italic or
/// replace a missing family with a system/default face.
pub fn pinned_font_face(
    family: &str,
    weight: u16,
    style: FontStyle,
) -> Result<&'static PinnedFontSource, EditorError> {
    CATALOG
        .iter()
        .find(|source| source.family == family && source.weight == weight && source.style == style)
        .ok_or_else(|| {
            EditorError::new(
                "FONT_FACE_UNAVAILABLE",
                "Requested exact static font face is not pinned",
            )
        })
}

impl PinnedFontSource {
    pub fn import_spec(&self, font_id: String, required_text: String) -> FontImportSpec {
        FontImportSpec {
            font_id,
            family: self.family.into(),
            weight: self.weight,
            style: self.style,
            origin: ResourceOrigin::Online {
                provider: "google-fonts".into(),
                source_url: self.source_url.into(),
            },
            license: ResourceLicense {
                identifier: Some(self.license_identifier.into()),
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
pub struct FontFetchPolicy {
    pub allow_public_font_download: bool,
    pub max_bytes: u64,
    pub deadline_ms: u64,
}
impl Default for FontFetchPolicy {
    fn default() -> Self {
        Self {
            allow_public_font_download: false,
            max_bytes: MAX_FONT_BYTES,
            deadline_ms: 30_000,
        }
    }
}
pub fn validate_font_fetch(
    catalog_id: &str,
    policy: &FontFetchPolicy,
) -> Result<&'static PinnedFontSource, EditorError> {
    let source = pinned_font_source(catalog_id)?;
    if !policy.allow_public_font_download {
        return Err(EditorError::new(
            "FONT_NETWORK_DENIED",
            "Task policy does not allow public font downloads",
        ));
    }
    if !(1..=30_000).contains(&policy.deadline_ms)
        || !(1..=MAX_FONT_BYTES).contains(&policy.max_bytes)
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

pub trait FontSourceFetch {
    /// Download only the compiled catalog entry. Never send project content,
    /// text, application credentials or a caller-provided URL to the provider.
    fn fetch(
        &self,
        catalog_id: &str,
        policy: &FontFetchPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError>;
}

/// Keeps downloaded bytes alive for the caller's existing CAS publication;
/// only the immutable resource is subsequently committed to the project.
pub struct PreparedCatalogFont {
    pub font: PreparedFont,
    pub bytes: Vec<u8>,
}
pub fn prepare_catalog_font(
    fetcher: &impl FontSourceFetch,
    inspector: &impl FontInspector,
    catalog_id: &str,
    font_id: String,
    required_text: String,
    fetch_policy: &FontFetchPolicy,
    inspection_policy: &FontImportPolicy,
    checkpoint: &mut dyn FontImportCheckpoint,
) -> Result<PreparedCatalogFont, EditorError> {
    checkpoint.check()?;
    let source = validate_font_fetch(catalog_id, fetch_policy)?;
    let spec = source.import_spec(font_id, required_text);
    validate_font_spec(&spec)?;
    validate_font_envelope(source.size_bytes, &spec.required_text, inspection_policy)?;
    let bytes = fetcher.fetch(catalog_id, fetch_policy, checkpoint)?;
    let font = prepare_font(inspector, &bytes, &spec, inspection_policy, checkpoint)?;
    if font.resource.content.content_hash != source.content_hash
        || font.resource.content.size != source.size_bytes
    {
        return Err(EditorError::new(
            "FONT_SOURCE_INTEGRITY_ERROR",
            "Font download does not match the pinned source",
        ));
    }
    checkpoint.check()?;
    Ok(PreparedCatalogFont { font, bytes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    #[test]
    fn catalog_preserves_complete_original_license_and_fixed_source_identity() {
        for source in static_font_inventory() {
            assert_eq!(
                format!("{:x}", Sha256::digest(source.license_notice.as_bytes())),
                source.license_sha256
            );
            assert!(source.source_url.contains(GOOGLE_FONTS_COMMIT));
            assert!(source.license_url.contains(GOOGLE_FONTS_COMMIT));
            assert!(source.license_notice.starts_with(source.attribution));
            assert!(matches!(
                source.license_identifier,
                "OFL-1.1" | "Apache-2.0"
            ));
            let spec = source.import_spec("font".into(), "AV To".into());
            validate_font_spec(&spec).unwrap();
            assert_eq!(spec.license.notice.as_deref(), Some(source.license_notice));
        }
    }
    #[test]
    fn default_offline_unknown_source_and_smaller_budget_cannot_reach_network() {
        let id = static_font_inventory()[0].catalog_id;
        assert_eq!(
            validate_font_fetch(id, &FontFetchPolicy::default())
                .unwrap_err()
                .code,
            "FONT_NETWORK_DENIED"
        );
        let policy = FontFetchPolicy {
            allow_public_font_download: true,
            max_bytes: 1,
            ..Default::default()
        };
        assert_eq!(
            validate_font_fetch(id, &policy).unwrap_err().code,
            "FONT_BUDGET_EXCEEDED"
        );
        assert_eq!(
            validate_font_fetch("https://attacker.test/font.ttf", &policy)
                .unwrap_err()
                .code,
            "NOT_FOUND"
        );
    }

    #[test]
    fn exact_face_resolution_never_synthesizes_or_uses_a_system_fallback() {
        for (weight, style, id) in [
            (400, FontStyle::Normal, "google-lato-regular-v1"),
            (700, FontStyle::Normal, "google-lato-bold-v1"),
            (400, FontStyle::Italic, "google-lato-italic-v1"),
            (700, FontStyle::Italic, "google-lato-bold-italic-v1"),
        ] {
            assert_eq!(
                pinned_font_face("Lato", weight, style).unwrap().catalog_id,
                id
            );
        }
        for (family, weight, style) in [
            ("Lato", 600, FontStyle::Normal),
            ("Ma Shan Zheng", 700, FontStyle::Normal),
            ("Arial", 400, FontStyle::Normal),
            ("lato", 400, FontStyle::Normal),
        ] {
            assert_eq!(
                pinned_font_face(family, weight, style).unwrap_err().code,
                "FONT_FACE_UNAVAILABLE"
            );
        }
    }

    struct WrongBytes(Cell<usize>);
    impl FontSourceFetch for WrongBytes {
        fn fetch(
            &self,
            _: &str,
            _: &FontFetchPolicy,
            _: &mut dyn FontImportCheckpoint,
        ) -> Result<Vec<u8>, EditorError> {
            self.0.set(self.0.get() + 1);
            Ok(b"abc".to_vec())
        }
    }
    struct ParsedLato;
    impl FontInspector for ParsedLato {
        fn inspect(
            &self,
            _: &[u8],
            _: &str,
            _: &FontImportPolicy,
            _: &mut dyn FontImportCheckpoint,
        ) -> Result<ParsedFontFace, EditorError> {
            Ok(ParsedFontFace {
                family: "Lato".into(),
                weight: 400,
                style: FontStyle::Normal,
                glyph_count: 3,
                checked_codepoints: 1,
            })
        }
    }
    #[test]
    fn preparation_rechecks_download_identity_and_validates_text_before_network() {
        let source = WrongBytes(Cell::new(0));
        let policy = FontFetchPolicy {
            allow_public_font_download: true,
            ..Default::default()
        };
        let prepare = |text: &str| {
            prepare_catalog_font(
                &source,
                &ParsedLato,
                "google-lato-regular-v1",
                "font".into(),
                text.into(),
                &policy,
                &FontImportPolicy::default(),
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
