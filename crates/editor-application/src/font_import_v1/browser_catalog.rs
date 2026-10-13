//! Classic family/style browsing with explicit import availability. This is
//! application metadata, not a substitute resource authority or public DTO.
use opencut_contracts::project_v4::FontStyleV4;
use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Clone, Debug, Deserialize)]
pub struct FontBrowserCatalogV2 {
    pub source_commit: String,
    pub classic_atlas_sha256: String,
    pub classic_family_count: usize,
    pub pinned_face_count: usize,
    pub families: Vec<FontBrowserFamilyV2>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct FontBrowserFamilyV2 {
    pub family: String,
    pub category: FontBrowserCategoryV2,
    pub faces: Vec<FontBrowserFaceV2>,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FontBrowserCategoryV2 {
    Google,
    System,
}
#[derive(Clone, Debug, Deserialize)]
pub struct FontBrowserFaceV2 {
    pub weight: u16,
    pub style: FontStyleV4,
    pub catalog_id: Option<String>,
    pub unavailable_reason: Option<FontBrowserUnavailableReasonV2>,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FontBrowserUnavailableReasonV2 {
    WeightNotExpressedByTextV4,
    NoPinnedStaticSource,
    SystemFontNotRedistributable,
}
static BROWSER_CATALOG: LazyLock<FontBrowserCatalogV2> = LazyLock::new(|| {
    serde_json::from_str(include_str!("browser_catalog.json"))
        .expect("compiled font browser metadata must pass catalog validation")
});
pub fn font_browser_catalog_v2() -> &'static FontBrowserCatalogV2 {
    &BROWSER_CATALOG
}

#[cfg(test)]
mod tests {
    use super::super::catalog::{GOOGLE_FONTS_COMMIT_V1, font_catalog_v1, pinned_font_source_v1};
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_classic_family_has_explicit_face_availability() {
        let catalog = font_browser_catalog_v2();
        assert_eq!(catalog.source_commit, GOOGLE_FONTS_COMMIT_V1);
        assert_eq!(catalog.classic_atlas_sha256.len(), 64);
        assert_eq!(catalog.classic_family_count, 1920);
        assert_eq!(catalog.pinned_face_count, font_catalog_v1().len());
        assert_eq!(catalog.families.len(), catalog.classic_family_count + 9);
        let mut families = BTreeSet::new();
        for family in &catalog.families {
            assert!(families.insert(&family.family));
            assert!(!family.faces.is_empty());
            let mut styles = BTreeSet::new();
            for face in &family.faces {
                assert!(styles.insert((face.weight, format!("{:?}", face.style))));
                match (&face.catalog_id, face.unavailable_reason) {
                    (Some(id), None) => {
                        assert_eq!(family.category, FontBrowserCategoryV2::Google);
                        assert!(matches!(face.weight, 400 | 700));
                        let source = pinned_font_source_v1(id).unwrap();
                        assert_eq!(source.family, family.family);
                        assert_eq!(source.weight, face.weight);
                        assert_eq!(source.style, face.style);
                    }
                    (None, Some(reason)) => {
                        if family.category == FontBrowserCategoryV2::System {
                            assert_eq!(
                                reason,
                                FontBrowserUnavailableReasonV2::SystemFontNotRedistributable
                            );
                        } else if !matches!(face.weight, 400 | 700) {
                            assert_eq!(
                                reason,
                                FontBrowserUnavailableReasonV2::WeightNotExpressedByTextV4
                            );
                        } else {
                            assert_eq!(
                                reason,
                                FontBrowserUnavailableReasonV2::NoPinnedStaticSource
                            );
                        }
                    }
                    _ => panic!("each face must have one explicit availability result"),
                }
            }
        }
    }

    #[test]
    fn pinned_static_catalog_has_unique_ids_and_face_identities() {
        let mut ids = BTreeSet::new();
        let mut faces = BTreeSet::new();
        for source in font_catalog_v1() {
            assert!(ids.insert(source.catalog_id));
            assert!(faces.insert((source.family, source.weight, format!("{:?}", source.style))));
            assert!(matches!(source.weight, 400 | 700));
            assert!(source.size_bytes > 0 && source.size_bytes <= super::super::MAX_FONT_BYTES_V1);
            assert_eq!(source.content_hash.len(), 64);
        }
        for id in [
            "google-lato-regular-v1",
            "google-mashanzheng-regular-v1",
            "google-lato-bold-v1",
            "google-lato-italic-v1",
            "google-lato-bold-italic-v1",
        ] {
            assert!(pinned_font_source_v1(id).is_ok(), "missing {id}");
        }
    }
}
