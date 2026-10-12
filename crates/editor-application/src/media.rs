//! Media identity and metadata policy, independent of files and codecs.
pub use opencut_api_contracts::media::{InspectMediaInput, RepairMediaInput};
use opencut_contracts::project_v4::AudioMetadataV4;
use opencut_contracts::{Asset, AssetKind, CatalogEntry, EditorError, Time};
use opencut_media_contracts::{MediaMetadata, StreamKind};
use serde_json::json;
use std::collections::BTreeMap;

pub struct DetectedMedia {
    pub metadata: MediaMetadata,
    pub uri: String,
    pub name: String,
    pub content_hash: String,
    pub size: u64,
}
/// Repair must compare expected metadata under the same per-content lock used
/// by import, verify and probe a private copy of the exact content, run classify,
/// and atomically publish its result. Any failure preserves previous metadata.
pub trait MediaCatalog {
    fn inspect(&self, hash: &str) -> Result<CatalogEntry, EditorError>;
    fn repair(
        &self,
        input: &RepairMediaInput,
        classify: &dyn Fn(DetectedMedia) -> Result<Asset, EditorError>,
    ) -> Result<CatalogEntry, EditorError>;
    fn publish(&self, hash: &str, asset: &Asset) -> Result<(), EditorError>;
}
fn identity(hash: &str) -> Result<(), EditorError> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Expected a lowercase SHA-256 media identity",
        ));
    }
    Ok(())
}
pub fn inspect(
    store: &impl MediaCatalog,
    input: InspectMediaInput,
) -> Result<CatalogEntry, EditorError> {
    identity(&input.content_hash)?;
    store.inspect(&input.content_hash)
}
pub fn repair(
    store: &impl MediaCatalog,
    input: RepairMediaInput,
) -> Result<CatalogEntry, EditorError> {
    identity(&input.content_hash)?;
    store.repair(&input, &|detected| {
        if detected.content_hash != input.content_hash {
            return Err(EditorError::new(
                "INTERNAL_ERROR",
                "Media observation belongs to another content identity",
            ));
        }
        describe(
            &detected.metadata,
            &detected.uri,
            &detected.name,
            &detected.content_hash,
            detected.size,
        )
    })
}
pub fn publish(store: &impl MediaCatalog, hash: &str, asset: &Asset) -> Result<(), EditorError> {
    identity(hash)?;
    store.publish(hash, asset)
}

pub fn describe<P>(
    metadata: &MediaMetadata<P>,
    uri: &str,
    name: &str,
    hash: &str,
    size: u64,
) -> Result<Asset, EditorError> {
    let video = metadata
        .streams
        .iter()
        .find(|s| s.codec_type == StreamKind::Video);
    let audio = metadata
        .streams
        .iter()
        .find(|s| s.codec_type == StreamKind::Audio);
    let format = metadata.format_name.as_deref().unwrap_or("");
    let format_brand = metadata.format_brand.as_deref().unwrap_or("").trim();
    let normalized_brand = format_brand.to_ascii_lowercase();
    let video_codec = video.and_then(|stream| stream.codec_name.as_deref());
    // A PNG/MJPEG codec inside a movie is still a video. Classify the
    // container detected from bytes, never the supplied filename or codec alone.
    let image = video.is_some()
        && format.split(',').any(|part| {
            matches!(
                part,
                "image2"
                    | "image2pipe"
                    | "png_pipe"
                    | "jpeg_pipe"
                    | "webp_pipe"
                    | "bmp_pipe"
                    | "tiff_pipe"
            )
        });
    let (kind, mime) = if image {
        (
            AssetKind::Image,
            match video_codec {
                Some("png") => "image/png",
                Some("webp") => "image/webp",
                Some("bmp") => "image/bmp",
                Some("tiff") => "image/tiff",
                _ => "image/jpeg",
            },
        )
    } else if video.is_some() {
        (
            AssetKind::Video,
            if format_brand.eq_ignore_ascii_case("qt") {
                "video/quicktime"
            } else if normalized_brand.starts_with("3g2") {
                "video/3gpp2"
            } else if normalized_brand.starts_with("3gp") {
                "video/3gpp"
            } else if format.split(',').any(|part| part == "mov") {
                if known_mp4_brand(&normalized_brand) {
                    "video/mp4"
                } else {
                    // FFprobe exposes MOV/MP4/3GP as one shared demuxer alias.
                    // Unknown or missing brand cannot safely distinguish them.
                    "application/octet-stream"
                }
            } else if format.split(',').any(|part| part == "matroska") {
                // ffprobe uses a shared Matroska/WebM demuxer. Do not assert
                // WebM codec restrictions from that ambiguous format name.
                "application/octet-stream"
            } else if format == "avi" {
                "video/x-msvideo"
            } else if format == "gif" {
                "image/gif"
            } else if format.split(',').any(|part| part == "mov")
                && !format.split(',').any(|part| part == "mp4")
            {
                "video/quicktime"
            } else if format.split(',').any(|part| part == "mp4") {
                "video/mp4"
            } else if format.split(',').any(|part| part == "mpegts") {
                "video/mp2t"
            } else if format.split(',').any(|part| part == "mpeg") {
                "video/mpeg"
            } else {
                "application/octet-stream"
            },
        )
    } else if audio.is_some() {
        (
            AssetKind::Audio,
            if format_brand.eq_ignore_ascii_case("qt") {
                "audio/quicktime"
            } else if normalized_brand.starts_with("3g2") {
                "audio/3gpp2"
            } else if normalized_brand.starts_with("3gp") {
                "audio/3gpp"
            } else if format.split(',').any(|part| part == "mov") {
                if known_mp4_brand(&normalized_brand) {
                    "audio/mp4"
                } else {
                    "application/octet-stream"
                }
            } else if format.split(',').any(|part| part == "wav") {
                "audio/wav"
            } else if format.split(',').any(|part| part == "mp3") {
                "audio/mpeg"
            } else if format.split(',').any(|part| part == "ogg") {
                "audio/ogg"
            } else if format == "flac" {
                "audio/flac"
            } else if format.split(',').any(|part| part == "aac") {
                "audio/aac"
            } else if format.split(',').any(|part| part == "mp4") {
                "audio/mp4"
            } else {
                "application/octet-stream"
            },
        )
    } else {
        return Err(EditorError::new(
            "CAPABILITY_UNAVAILABLE",
            "Upload contains no supported visual or audio stream",
        ));
    };
    let mut fields = BTreeMap::new();
    fields.insert("content_hash".into(), json!(hash));
    fields.insert("media_url".into(), json!(format!("/v1/media/{hash}")));
    fields.insert("mime_type".into(), json!(mime));
    fields.insert("size".into(), json!(size));
    if let Some(video) = video {
        if !image
            && let Some(rate) = &video.average_frame_rate
            && rate.numerator > 0
            && rate.denominator > 0
        {
            fields.insert("frame_rate".into(), json!(rate));
        }
        fields.insert("width".into(), json!(video.width));
        fields.insert("height".into(), json!(video.height));
    }
    fields.insert("has_audio".into(), json!(audio.is_some()));
    // Record the primary observed stream only. Missing format observations do
    // not imply mono/stereo or the output sample rate requested by a renderer.
    if let Some(audio) = audio
        && let (Some(sample_rate), Some(channels)) = (audio.sample_rate, audio.channels)
        && sample_rate > 0
        && channels > 0
    {
        fields.insert(
            "audio_metadata".into(),
            json!(AudioMetadataV4 {
                sample_rate,
                channels,
                channel_layout: audio.channel_layout.clone(),
            }),
        );
    }
    let asset = Asset {
        id: format!("asset_{hash}"),
        name: name.into(),
        kind,
        uri: uri.into(),
        duration: if image {
            None
        } else {
            metadata.duration.map(|t| Time {
                ticks: t.ticks.to_string(),
                timebase: t.timebase,
            })
        },
        version: None,
        metadata: fields,
    };
    Ok(asset)
}

fn known_mp4_brand(brand: &str) -> bool {
    matches!(
        brand,
        "isom" | "iso2" | "mp41" | "mp42" | "mp4v" | "avc1" | "dash" | "m4v" | "m4a"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencut_media_contracts::{MediaStream, StreamKind};

    fn visual(format_name: &str, format_brand: Option<&str>) -> MediaMetadata<String> {
        MediaMetadata {
            input: "/private/media".into(),
            format_name: Some(format_name.into()),
            format_brand: format_brand.map(str::to_owned),
            duration: None,
            bit_rate: None,
            streams: vec![MediaStream {
                index: 0,
                codec_type: StreamKind::Video,
                codec_name: Some("h264".into()),
                duration: None,
                time_base: None,
                width: Some(64),
                height: Some(64),
                sample_rate: None,
                channels: None,
                average_frame_rate: None,
                pixel_format: None,
                channel_layout: None,
                color_space: None,
                color_transfer: None,
                color_primaries: None,
            }],
        }
    }

    fn audio(format_name: &str, format_brand: Option<&str>) -> MediaMetadata<String> {
        MediaMetadata {
            input: "/private/media".into(),
            format_name: Some(format_name.into()),
            format_brand: format_brand.map(str::to_owned),
            duration: None,
            bit_rate: None,
            streams: vec![MediaStream {
                index: 0,
                codec_type: StreamKind::Audio,
                codec_name: Some("aac".into()),
                duration: None,
                time_base: None,
                width: None,
                height: None,
                sample_rate: Some(44_100),
                channels: Some(1),
                average_frame_rate: None,
                pixel_format: None,
                channel_layout: Some("mono".into()),
                color_space: None,
                color_transfer: None,
                color_primaries: None,
            }],
        }
    }

    #[test]
    fn primary_probe_audio_format_survives_description_and_v4_conversion() {
        use opencut_contracts::project_v4::AudioPresenceV4;
        use opencut_contracts::{AssetVersion, FrameRate, ProjectDocument, Sequence};
        use opencut_editor_core::project_v4::legacy::{
            LegacyConversionInputs, LegacyConversionResult, LegacyProjectInput,
            convert_legacy_project,
        };

        let mut probe = visual("mov,mp4,m4a,3gp,3g2,mj2", Some("isom"));
        let mut primary = audio("aac", None).streams.remove(0);
        primary.index = 1;
        primary.sample_rate = Some(48_000);
        let mut secondary = primary.clone();
        secondary.index = 2;
        secondary.sample_rate = Some(44_100);
        secondary.channels = Some(2);
        secondary.channel_layout = Some("stereo".into());
        probe.streams.extend([primary, secondary]);
        let hash = "a".repeat(64);
        let observed = describe(&probe, "private", "AAC video", &hash, 123).unwrap();
        let expected = AudioMetadataV4 {
            sample_rate: 48_000,
            channels: 1,
            channel_layout: Some("mono".into()),
        };
        assert_eq!(observed.metadata["audio_metadata"], json!(expected));
        assert_eq!(observed.metadata["has_audio"], json!(true));
        let mut old = ProjectDocument {
            id: "project".into(),
            name: "Project".into(),
            schema_version: 2,
            default_sequence_id: "scene".into(),
            sequences: vec![Sequence {
                id: "scene".into(),
                name: "Scene".into(),
                width: 640,
                height: 360,
                fps: FrameRate {
                    numerator: 24,
                    denominator: 1,
                },
                tracks: Vec::new(),
                caption_cues: Vec::new(),
            }],
            assets: vec![observed],
        };
        old.assets[0].version = Some(AssetVersion {
            id: format!("av_{hash}"),
            content_hash: hash,
            size: 123,
        });
        let LegacyConversionResult::Converted { project, .. } = convert_legacy_project(
            LegacyProjectInput::Base(&old),
            &LegacyConversionInputs::default(),
        ) else {
            panic!("native probe metadata must convert without losing audio format");
        };
        opencut_editor_core::project_v4::validate_project_v4(&project).unwrap();
        assert_eq!(
            project.assets[0].metadata.audio,
            AudioPresenceV4::Present {
                metadata: Some(expected)
            }
        );

        // Incomplete or invalid primary format cannot fall back to the second
        // stream or invent a layout; known presence remains independently true.
        for (sample_rate, channels) in [
            (None, Some(1)),
            (Some(48_000), None),
            (Some(0), Some(1)),
            (Some(48_000), Some(0)),
        ] {
            probe.streams[1].sample_rate = sample_rate;
            probe.streams[1].channels = channels;
            let unknown = describe(&probe, "private", "AAC video", "hash", 1).unwrap();
            assert_eq!(unknown.metadata["has_audio"], json!(true));
            assert!(!unknown.metadata.contains_key("audio_metadata"));
        }
        probe.streams[1].sample_rate = Some(48_000);
        probe.streams[1].channels = Some(1);
        probe.streams[1].channel_layout = None;
        assert_eq!(
            describe(&probe, "private", "AAC video", "hash", 1)
                .unwrap()
                .metadata["audio_metadata"]["channel_layout"],
            json!(null)
        );
    }

    #[test]
    fn native_video_rate_is_preserved_but_still_images_and_invalid_rates_have_none() {
        let mut metadata = visual("mov,mp4,m4a,3gp,3g2,mj2", Some("isom"));
        metadata.streams[0].average_frame_rate = Some(opencut_media_contracts::FrameRate {
            numerator: 60000,
            denominator: 1001,
        });
        let observed = describe(&metadata, "fixture", "video", "hash", 1).unwrap();
        assert_eq!(
            observed.metadata["frame_rate"],
            json!({"numerator":60000,"denominator":1001})
        );
        metadata.format_name = Some("png_pipe".into());
        assert!(
            !describe(&metadata, "fixture", "image", "hash", 1)
                .unwrap()
                .metadata
                .contains_key("frame_rate")
        );
        metadata.format_name = Some("mp4".into());
        metadata.streams[0]
            .average_frame_rate
            .as_mut()
            .unwrap()
            .denominator = 0;
        assert!(
            !describe(&metadata, "fixture", "invalid", "hash", 1)
                .unwrap()
                .metadata
                .contains_key("frame_rate")
        );
    }
    #[test]
    fn iso_bmff_major_brand_disambiguates_quicktime_mov_from_mp4() {
        let mov = describe(
            &visual("mov,mp4,m4a,3gp,3g2,mj2", Some("qt  ")),
            "uri",
            "renamed.bin",
            "a",
            1,
        )
        .unwrap();
        let mp4 = describe(
            &visual("mov,mp4,m4a,3gp,3g2,mj2", Some("isom")),
            "uri",
            "renamed.bin",
            "b",
            1,
        )
        .unwrap();
        assert_eq!(mov.kind, AssetKind::Video);
        assert_eq!(mov.metadata["mime_type"], "video/quicktime");
        assert_eq!(mp4.kind, AssetKind::Video);
        assert_eq!(mp4.metadata["mime_type"], "video/mp4");

        for brand in [None, Some("unknown")] {
            let ambiguous = describe(
                &visual("mov,mp4,m4a,3gp,3g2,mj2", brand),
                "uri",
                "movie.mp4",
                "ambiguous",
                1,
            )
            .unwrap();
            assert_eq!(ambiguous.metadata["mime_type"], "application/octet-stream");
        }

        let three_gp = describe(
            &visual("mov,mp4,m4a,3gp,3g2,mj2", Some("3gp6")),
            "uri",
            "renamed.bin",
            "c",
            1,
        )
        .unwrap();
        let three_g2 = describe(
            &visual("mov,mp4,m4a,3gp,3g2,mj2", Some("3g2b")),
            "uri",
            "renamed.bin",
            "d",
            1,
        )
        .unwrap();
        assert_eq!(three_gp.metadata["mime_type"], "video/3gpp");
        assert_eq!(three_g2.metadata["mime_type"], "video/3gpp2");
    }

    #[test]
    fn iso_bmff_brand_maps_audio_only_containers_without_filename_hints() {
        for (brand, expected) in [
            ("qt  ", "audio/quicktime"),
            ("3gp6", "audio/3gpp"),
            ("3g2b", "audio/3gpp2"),
        ] {
            let asset = describe(
                &audio("mov,mp4,m4a,3gp,3g2,mj2", Some(brand)),
                "uri",
                "misleading.wav",
                "a",
                1,
            )
            .unwrap();
            assert_eq!(asset.kind, AssetKind::Audio);
            assert_eq!(asset.metadata["mime_type"], expected);
        }

        let unknown = describe(
            &audio("mov,mp4,m4a,3gp,3g2,mj2", None),
            "uri",
            "misleading.mp3",
            "unknown-brand",
            1,
        )
        .unwrap();
        assert_eq!(unknown.metadata["mime_type"], "application/octet-stream");

        let m4a = describe(
            &audio("mov,mp4,m4a,3gp,3g2,mj2", Some("M4A ")),
            "uri",
            "misleading.wav",
            "m4a",
            1,
        )
        .unwrap();
        assert_eq!(m4a.metadata["mime_type"], "audio/mp4");
    }
}
