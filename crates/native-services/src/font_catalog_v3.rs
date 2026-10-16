//! Transport for the additive compiled static-font inventory. The legacy
//! transport/source selector is unchanged and cannot select these IDs.
use opencut_contracts::EditorError;
use opencut_editor_application::font_import_v1::{
    FontImportCheckpointV1,
    catalog::{FontFetchPolicyV1, FontSourceFetchV1},
};

pub struct GoogleFontsFetchV3;
impl FontSourceFetchV1 for GoogleFontsFetchV3 {
    fn fetch(
        &self,
        catalog_id: &str,
        policy: &FontFetchPolicyV1,
        checkpoint: &mut dyn FontImportCheckpointV1,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        let pin =
            opencut_editor_application::font_catalog_v3::validate_fetch_v3(catalog_id, policy)?;
        crate::font_import_v1::fetch::fetch_pinned_source(&pin.source, policy, checkpoint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_unknown_and_cancelled_requests_stop_before_transport() {
        let mut check = || Ok(());
        assert_eq!(
            GoogleFontsFetchV3
                .fetch(
                    "google-fjordone-regular-alias-v3",
                    &FontFetchPolicyV1::default(),
                    &mut check
                )
                .unwrap_err()
                .code,
            "FONT_NETWORK_DENIED"
        );
        let allowed = FontFetchPolicyV1 {
            allow_public_font_download: true,
            ..FontFetchPolicyV1::default()
        };
        assert_eq!(
            GoogleFontsFetchV3
                .fetch("google-lato-regular-v1", &allowed, &mut check)
                .unwrap_err()
                .code,
            "NOT_FOUND"
        );
        let mut cancelled = || Err(EditorError::new("CANCELLED", "cancelled"));
        assert_eq!(
            GoogleFontsFetchV3
                .fetch("google-fjordone-regular-alias-v3", &allowed, &mut cancelled)
                .unwrap_err()
                .code,
            "CANCELLED"
        );
    }
}
