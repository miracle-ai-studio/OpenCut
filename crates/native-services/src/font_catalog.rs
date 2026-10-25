//! Transport for the additive compiled static-font inventory. The legacy
//! transport/source selector is unchanged and cannot select these IDs.
use opencut_contracts::EditorError;
use opencut_editor_application::font_import::{
    FontImportCheckpoint,
    catalog::{FontFetchPolicy, FontSourceFetch},
};

pub struct GoogleFontsFetch;
impl FontSourceFetch for GoogleFontsFetch {
    fn fetch(
        &self,
        catalog_id: &str,
        policy: &FontFetchPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        let pin = opencut_editor_application::font_catalog::validate_fetch(catalog_id, policy)?;
        crate::font_import::fetch::fetch_pinned_source(&pin.source, policy, checkpoint)
    }
}

pub struct GoogleFontsDistributionFetch;
impl FontSourceFetch for GoogleFontsDistributionFetch {
    fn fetch(
        &self,
        catalog_id: &str,
        policy: &FontFetchPolicy,
        checkpoint: &mut dyn FontImportCheckpoint,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        let pin = opencut_editor_application::font_catalog::distribution::validate_fetch(
            catalog_id, policy,
        )?;
        crate::font_import::fetch::fetch_pinned_source(&pin.source, policy, checkpoint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_unknown_and_cancelled_requests_stop_before_transport() {
        let mut check = || Ok(());
        assert_eq!(
            GoogleFontsFetch
                .fetch(
                    "google-fjordone-regular-alias-v3",
                    &FontFetchPolicy::default(),
                    &mut check
                )
                .unwrap_err()
                .code,
            "FONT_NETWORK_DENIED"
        );
        let allowed = FontFetchPolicy {
            allow_public_font_download: true,
            ..FontFetchPolicy::default()
        };
        assert_eq!(
            GoogleFontsFetch
                .fetch("google-lato-regular-v1", &allowed, &mut check)
                .unwrap_err()
                .code,
            "NOT_FOUND"
        );
        let mut cancelled = || Err(EditorError::new("CANCELLED", "cancelled"));
        assert_eq!(
            GoogleFontsFetch
                .fetch("google-fjordone-regular-alias-v3", &allowed, &mut cancelled)
                .unwrap_err()
                .code,
            "CANCELLED"
        );
    }
}
