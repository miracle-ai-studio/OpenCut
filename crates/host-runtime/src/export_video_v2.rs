//! Public capability assembly; no project state or local paths cross this DTO.
use opencut_job_contracts::export_video_v2::*;
use opencut_native_services::export_video_v2::ExportEncoderSupportV2;
pub(crate) fn capability() -> ExportVideoCapabilityV2 {
    let result = (|| {
        crate::classic_visual_runtime::check_export_video_runtime_v2()?;
        let ffmpeg = opencut_native_services::classic_video::resolve_tool(
            std::env::var_os("OPENCUT_FFMPEG")
                .map(Into::into)
                .unwrap_or_else(|| "ffmpeg".into()),
        )?;
        opencut_native_services::classic_video::resolve_tool(
            std::env::var_os("OPENCUT_FFPROBE")
                .map(Into::into)
                .unwrap_or_else(|| "ffprobe".into()),
        )?;
        opencut_native_services::export_video_v2::probe_encoder_support_v2(
            &ffmpeg,
            &opencut_media::CancellationToken::new(),
        )
    })();
    let audio_runtime = result.is_ok() && crate::classic_audio_runtime::capability().available;
    report(result, audio_runtime)
}
fn report(
    result: Result<ExportEncoderSupportV2, opencut_contracts::EditorError>,
    audio_runtime: bool,
) -> ExportVideoCapabilityV2 {
    let (formats, audio_available, unavailable_reason) = match result {
        Ok(encoders) => {
            let formats = encoders.formats();
            // This DTO has one audio flag. Be conservative for every advertised
            // format; video-only remains usable when any audio encoder is absent.
            let audio = audio_runtime
                && !formats.is_empty()
                && formats.iter().all(|f| encoders.audio_for(*f));
            let reason = formats
                .is_empty()
                .then(|| "No supported H.264 or VP9 export encoder is installed.".into());
            (formats, audio, reason)
        }
        Err(error) => (vec![], false, Some(error.message)),
    };
    ExportVideoCapabilityV2 {
        schema_version: 2,
        available: !formats.is_empty(),
        audio_available,
        submit_tool: "opencut_job_export_video_v2".into(),
        formats,
        qualities: vec![
            ExportQualityV2::Low,
            ExportQualityV2::Medium,
            ExportQualityV2::High,
            ExportQualityV2::VeryHigh,
        ],
        unavailable_reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_audio_does_not_hide_video_but_single_audio_flag_covers_all_formats() {
        let encoders = ExportEncoderSupportV2 {
            h264: true,
            vp9: true,
            aac: true,
            opus: true,
        };
        let silent = report(Ok(encoders), false);
        assert!(silent.available);
        assert!(!silent.audio_available);
        assert_eq!(
            silent.formats,
            vec![ExportFormatV2::Mp4, ExportFormatV2::Webm]
        );
        assert!(
            !report(
                Ok(ExportEncoderSupportV2 {
                    opus: false,
                    ..encoders
                }),
                true
            )
            .audio_available
        );
        let mp4 = report(
            Ok(ExportEncoderSupportV2 {
                vp9: false,
                opus: false,
                ..encoders
            }),
            true,
        );
        assert_eq!(mp4.formats, vec![ExportFormatV2::Mp4]);
        assert!(mp4.audio_available);
        let missing = report(Ok(ExportEncoderSupportV2::default()), true);
        assert!(!missing.available);
        assert!(!missing.audio_available);
        assert!(missing.unavailable_reason.is_some());
    }
    #[test]
    fn visual_or_probe_failure_never_advertises_an_export_format() {
        let report = report(
            Err(opencut_contracts::EditorError::new(
                "CAPABILITY_UNAVAILABLE",
                "missing persistent runner",
            )),
            true,
        );
        assert!(!report.available);
        assert!(!report.audio_available);
        assert!(report.formats.is_empty());
        assert_eq!(
            report.unavailable_reason.as_deref(),
            Some("missing persistent runner")
        );
    }
}
